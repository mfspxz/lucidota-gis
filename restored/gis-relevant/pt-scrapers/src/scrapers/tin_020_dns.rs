//! TIN_020 — DNS record intelligence.
//!
//! Rust port of `TIN_020_dns.py` (non-harvest protocol module, General domain).
//! Resolves A / AAAA / MX / NS / TXT records for a domain to reveal hosting and
//! mail infrastructure and the connections it implies between domains.
//!
//! DMARC / SPF TXT records matter (the Python highlighted both): SPF lives in the
//! apex TXT records (`v=spf1 ...`); DMARC lives in the `_dmarc.<domain>` TXT
//! record (`v=DMARC1 ...`). We query both and flag them on the emitted candidates.
//!
//! Uses the `hickory-resolver` crate (pure-Rust async DNS; no API key, no
//! published rate limit). Each resolved record is emitted as one candidate.

use crate::controls::{LaneConfig, ScraperLimits};
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;
use hickory_resolver::config::{ResolverConfig, ResolverOpts};
use hickory_resolver::TokioAsyncResolver;

pub struct DnsLookup;

#[async_trait]
impl Scraper for DnsLookup {
    fn id(&self) -> &'static str {
        "TIN_020"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_020".to_string(),
            source_label: "DNS".to_string(),
            domain: Domain::General,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "DNS record intelligence (A/AAAA/MX/NS/TXT + DMARC) via \
                          hickory-resolver. Reveals hosting, mail servers, SPF/DMARC."
                .to_string(),
        }
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(4),
            min_delay_ms: Some(0),
            max_results: None,
            max_pages: None,
            notes: "DNS over the system/default resolver; no published rate limit. \
                    Resolver is queried directly, not via the HTTP governor."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        LaneConfig::NonHarvest(serde_json::json!({
            "protocol": "dns",
            "record_types": ["A", "AAAA", "MX", "NS", "TXT", "DMARC"]
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

        // Fail-closed: DNS resolver egress must honor StealthPolicy like every other
        // lane. Under StealthPolicy::Always with no persona injected, refuse rather
        // than egress bare (audit #3). DNS is UDP/TCP resolver traffic with no UA to
        // wear, but it MUST still respect the policy gate.
        if let Err(e) = ctx.net_guard(&domain) {
            record.status = ScrapeStatus::Blocked(e.to_string());
            return Ok(record);
        }

        // hickory-resolver 0.24: `tokio(..)` is infallible and returns the resolver.
        let resolver =
            TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default());

        let mut notes: Vec<String> = Vec::new();

        // A / AAAA — one lookup, split by address family.
        match resolver.lookup_ip(domain.as_str()).await {
            Ok(ips) => {
                for ip in ips.iter() {
                    let rtype = if ip.is_ipv4() { "A" } else { "AAAA" };
                    record.candidates.push(ScrapeCandidate::new(
                        serde_json::json!({
                            "domain": domain,
                            "record_type": rtype,
                            "value": ip.to_string(),
                        }),
                        0.95,
                    ));
                }
            }
            Err(e) => notes.push(format!("A/AAAA: {e}")),
        }

        // NS.
        match resolver.ns_lookup(domain.as_str()).await {
            Ok(ns) => {
                for r in ns.iter() {
                    record.candidates.push(ScrapeCandidate::new(
                        serde_json::json!({
                            "domain": domain,
                            "record_type": "NS",
                            "value": r.to_string(),
                        }),
                        0.95,
                    ));
                }
            }
            Err(e) => notes.push(format!("NS: {e}")),
        }

        // MX.
        match resolver.mx_lookup(domain.as_str()).await {
            Ok(mx) => {
                for r in mx.iter() {
                    record.candidates.push(ScrapeCandidate::new(
                        serde_json::json!({
                            "domain": domain,
                            "record_type": "MX",
                            "preference": r.preference(),
                            "exchange": r.exchange().to_string(),
                            "value": r.to_string(),
                        }),
                        0.95,
                    ));
                }
            }
            Err(e) => notes.push(format!("MX: {e}")),
        }

        // TXT (apex) — SPF lives here.
        match resolver.txt_lookup(domain.as_str()).await {
            Ok(txt) => {
                for r in txt.iter() {
                    let value = r.to_string();
                    let is_spf = value.to_ascii_lowercase().contains("v=spf1");
                    record.candidates.push(ScrapeCandidate::new(
                        serde_json::json!({
                            "domain": domain,
                            "record_type": "TXT",
                            "value": value,
                            "is_spf": is_spf,
                        }),
                        0.95,
                    ));
                }
            }
            Err(e) => notes.push(format!("TXT: {e}")),
        }

        // DMARC — TXT at _dmarc.<domain>.
        let dmarc_name = format!("_dmarc.{domain}");
        match resolver.txt_lookup(dmarc_name.as_str()).await {
            Ok(txt) => {
                for r in txt.iter() {
                    let value = r.to_string();
                    let is_dmarc = value.to_ascii_lowercase().contains("v=dmarc1");
                    record.candidates.push(ScrapeCandidate::new(
                        serde_json::json!({
                            "domain": domain,
                            "record_type": "DMARC",
                            "name": dmarc_name,
                            "value": value,
                            "is_dmarc": is_dmarc,
                        }),
                        0.95,
                    ));
                }
            }
            Err(e) => notes.push(format!("DMARC: {e}")),
        }

        record.notes = if notes.is_empty() {
            format!("{} DNS record(s)", record.candidates.len())
        } else {
            format!(
                "{} DNS record(s); lookups with no data/errors: {}",
                record.candidates.len(),
                notes.join(", ")
            )
        };

        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
