//! SCI_005 — ORCID public API.
//!
//! Rust port of `SCI_005_orcid.py` (api-json archetype). Searches the ORCID
//! public API for researcher records by name/keyword. No key required for
//! public data. Result array: `result` (each element carries an
//! `orcid-identifier`).

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct Orcid;

#[async_trait]
impl Scraper for Orcid {
    fn id(&self) -> &'static str {
        "SCI_005"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "SCI_005".to_string(),
            source_label: "ORCID Public API v3.0".to_string(),
            domain: Domain::AcademicIntegrity,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Search ORCID public records (researcher identifiers) by name/keyword."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("pub.orcid.org")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: None,
            min_delay_ms: Some(50),
            max_results: None,
            max_pages: None,
            notes: "ORCID public API v3.0: no key required for public data. Polite spacing ~50ms \
                    (min_delay 50ms) per source guidance; no confirmed hard rate cap."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut field_mappings = std::collections::BTreeMap::new();
        field_mappings.insert("orcid".to_string(), "orcid-identifier.path".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template: "https://pub.orcid.org/v3.0/search/?q={{query}}&rows=20".to_string(),
            method: "GET".to_string(),
            query_params: Default::default(),
            json_path: Some("result".to_string()),
            field_mappings,
            pagination: Default::default(),
            auth: Default::default(),
        })
    }

    async fn run(&self, ctx: &ScrapeCtx, params: ScrapeParams) -> anyhow::Result<ScrapeRecord> {
        let meta = self.meta();
        let mut record = ScrapeRecord::start(&meta, &params);

        let query = match params.query() {
            Some(q) if !q.is_empty() => q.to_string(),
            _ => {
                record.status = ScrapeStatus::Error("missing 'query' param".to_string());
                return Ok(record);
            }
        };

        let url = "https://pub.orcid.org/v3.0/search/";
        let fetch =
            api::get_json(ctx, url, &[("q", query.as_str()), ("rows", "20")]).await?;
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
            record.status = ScrapeStatus::Error(format!("malformed JSON: {note}"));
            record.notes = note.clone();
            return Ok(record);
        }

        // Result array: result — each element carries an orcid-identifier.path
        let empty = Vec::new();
        let results = fetch
            .json
            .get("result")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        for item in results {
            // Only keep entries that actually carry an ORCID path (Python skips
            // the rest).
            let has_path = item
                .get("orcid-identifier")
                .and_then(|o| o.get("path"))
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            if has_path {
                record
                    .candidates
                    .push(ScrapeCandidate::new(serde_json::json!({ "raw": item }), 0.6));
            }
        }

        record.notes = format!("{} ORCID identifiers", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
