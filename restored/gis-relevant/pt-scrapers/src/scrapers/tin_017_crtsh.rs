//! TIN_017 — Certificate Transparency (crt.sh).
//!
//! Rust port of `TIN_017_crtsh.py` (api-json archetype). Queries crt.sh for all
//! SSL/TLS certificates issued for a domain, revealing subdomains and issuers.
//! Free, no auth.
//!
//! Parity target: `search_certs()` → `https://crt.sh/?q=%.<domain>&output=json`.
//! The response is a top-level JSON array of certificate objects.

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

const CRTSH_URL: &str = "https://crt.sh/";

pub struct Crtsh;

#[async_trait]
impl Scraper for Crtsh {
    fn id(&self) -> &'static str {
        "TIN_017"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_017".to_string(),
            source_label: "Certificate Transparency (crt.sh)".to_string(),
            domain: Domain::General,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Query crt.sh for SSL/TLS certificates issued for a domain (subdomains, issuers)."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("crt.sh")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: None,
            min_delay_ms: None,
            max_results: None,
            max_pages: None,
            notes: "crt.sh — free, public, no auth; no published hard rate limit, \
                    but be polite (the Python port spaces batch requests). May return \
                    HTML instead of JSON under load."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut query_params = std::collections::BTreeMap::new();
        query_params.insert("q".to_string(), "%.{{query}}".to_string());
        query_params.insert("output".to_string(), "json".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template: CRTSH_URL.to_string(),
            method: "GET".to_string(),
            query_params,
            // Top-level JSON array — no nested json_path.
            json_path: None,
            field_mappings: Default::default(),
            pagination: Default::default(),
            auth: Default::default(),
        })
    }

    async fn run(&self, ctx: &ScrapeCtx, params: ScrapeParams) -> anyhow::Result<ScrapeRecord> {
        let meta = self.meta();
        let mut record = ScrapeRecord::start(&meta, &params);

        let domain = match params.query() {
            Some(q) if !q.is_empty() => q.to_string(),
            _ => {
                record.status = ScrapeStatus::Error("missing 'query' param (domain)".to_string());
                return Ok(record);
            }
        };

        let q = format!("%.{domain}");
        let fetch = api::get_json(ctx, CRTSH_URL, &[("q", q.as_str()), ("output", "json")]).await?;
        record.raw_capture = Some(fetch.hash());

        if fetch.status != 200 {
            record.status = if fetch.status == 429 {
                ScrapeStatus::RateLimited
            } else {
                ScrapeStatus::Error(format!("HTTP {}", fetch.status))
            };
            return Ok(record);
        }
        if let Some(note) = &fetch.parse_note {
            // crt.sh sometimes returns HTML instead of JSON under load.
            record.status = ScrapeStatus::Error(format!("non-JSON response from crt.sh: {note}"));
            record.notes = note.clone();
            return Ok(record);
        }

        // Response is a top-level JSON array of certificate objects.
        let certs = match fetch.json.as_array() {
            Some(a) => a,
            None => {
                record.status = ScrapeStatus::NotFound;
                record.notes = "no certificates (non-array response)".to_string();
                return Ok(record);
            }
        };

        for cert in certs {
            let data = serde_json::json!({
                "type": "certificate",
                "certificate": cert,
            });
            record.candidates.push(ScrapeCandidate::new(data, 0.9));
        }

        record.notes = format!("{} certificate(s)", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
