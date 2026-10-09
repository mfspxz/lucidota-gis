//! TIN_018 — WHOIS domain registration lookup.
//!
//! Rust port of `TIN_018_whois.py` (non-harvest protocol module, General domain).
//! Reveals registrant info, creation/expiry dates, registrar, and nameservers —
//! the raw material for spotting shared infrastructure between domains.
//!
//! The Python used the `python-whois` library. This is a dependency-free,
//! pure-Rust WHOIS client over raw TCP port 43 (skeleton B):
//!   1. Query `whois.iana.org:43` for the domain to discover the authoritative
//!      TLD WHOIS server (the `refer:` / `whois:` line).
//!   2. Query that referred server for the full registration record.
//! The raw text is emitted as a candidate (WHOIS grammar is registrar-specific
//! and not reliably machine-parseable; downstream extractors read the raw blob).

use crate::controls::{LaneConfig, ScraperLimits};
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct Whois;

#[async_trait]
impl Scraper for Whois {
    fn id(&self) -> &'static str {
        "TIN_018"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_018".to_string(),
            source_label: "WHOIS".to_string(),
            domain: Domain::General,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "WHOIS domain registration lookup over raw TCP port 43 \
                          (IANA referral then TLD server). Registrant, registrar, \
                          dates, nameservers."
                .to_string(),
        }
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(1000),
            max_results: None,
            max_pages: None,
            notes: "WHOIS protocol (TCP/43); registrar rate limits vary and are \
                    often unpublished — be conservative (serial, 1s spacing)."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        LaneConfig::NonHarvest(serde_json::json!({
            "protocol": "whois",
            "port": 43,
            "referral_root": "whois.iana.org"
        }))
    }

    async fn run(&self, ctx: &ScrapeCtx, params: ScrapeParams) -> anyhow::Result<ScrapeRecord> {
        let meta = self.meta();
        let mut record = ScrapeRecord::start(&meta, &params);

        let domain = match params.query() {
            Some(q) if !q.is_empty() => q.to_string(),
            _ => {
                record.status =
                    ScrapeStatus::Error("missing 'query' param (domain)".to_string());
                return Ok(record);
            }
        };

        // Fail-closed: raw TCP/43 WHOIS egress must honor StealthPolicy like every
        // other lane. Under StealthPolicy::Always with no persona injected, refuse
        // rather than egress bare (audit #2). WHOIS is a raw socket so it can't wear
        // a UA fingerprint, but it MUST still respect the policy gate.
        if let Err(e) = ctx.net_guard(&domain) {
            record.status = ScrapeStatus::Blocked(e.to_string());
            return Ok(record);
        }

        match tokio::time::timeout(
            std::time::Duration::from_secs(20),
            whois_lookup(&domain),
        )
        .await
        {
            Ok(Ok((server, text))) => {
                if text.trim().is_empty() {
                    record.status = ScrapeStatus::NotFound;
                    record.notes = format!("empty WHOIS response from {server}");
                } else {
                    record.candidates.push(ScrapeCandidate::new(
                        serde_json::json!({
                            "domain": domain,
                            "whois_server": server,
                            "whois_raw": text,
                        }),
                        0.9,
                    ));
                    record.notes = format!("WHOIS via {server}, {} bytes", text.len());
                    record.status = ScrapeStatus::Ok;
                }
            }
            Ok(Err(e)) => {
                record.status = ScrapeStatus::Error(e.to_string());
            }
            Err(_) => {
                record.status = ScrapeStatus::Error("whois timeout".to_string());
            }
        }

        Ok(record)
    }
}

/// One raw WHOIS query: connect TCP/43, send `domain\r\n`, read the whole reply.
async fn whois_query(server: &str, query: &str) -> anyhow::Result<String> {
    let addr = format!("{server}:43");
    let mut stream = TcpStream::connect(&addr).await?;
    stream.write_all(format!("{query}\r\n").as_bytes()).await?;
    stream.flush().await?;

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Two-step WHOIS: ask IANA for the referral server, then query it.
/// Returns (server_actually_answered, raw_text).
async fn whois_lookup(domain: &str) -> anyhow::Result<(String, String)> {
    // Step 1: IANA tells us which TLD WHOIS server is authoritative.
    let iana = whois_query("whois.iana.org", domain).await?;
    let referred = parse_refer(&iana);

    match referred {
        Some(server) => {
            // Step 2: query the authoritative registry/registrar server.
            let text = whois_query(&server, domain).await?;
            if text.trim().is_empty() {
                // Fall back to the IANA body if the referral yields nothing.
                Ok(("whois.iana.org".to_string(), iana))
            } else {
                Ok((server, text))
            }
        }
        // No referral (some TLDs) — the IANA body is the best we have.
        None => Ok(("whois.iana.org".to_string(), iana)),
    }
}

/// Extract the referral WHOIS server from an IANA response.
/// IANA uses `refer:`; some servers use `whois:`. Case-insensitive.
fn parse_refer(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        for prefix in ["refer:", "whois:"] {
            if let Some(rest) = lower.strip_prefix(prefix) {
                // Preserve original-case value (hostnames are case-insensitive
                // anyway, but avoid mangling anything unexpected).
                let start = trimmed.len() - rest.len();
                let value = trimmed[start..].trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}
