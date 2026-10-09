//! SCI_001 — Crossref REST API.
//!
//! Rust port of `SCI_001_crossref.py` (api-json archetype). Searches Crossref
//! works by query string. No auth. Polite pool via `mailto` (~50 req/s).
//! Result array: `message.items`.

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct Crossref;

#[async_trait]
impl Scraper for Crossref {
    fn id(&self) -> &'static str {
        "SCI_001"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "SCI_001".to_string(),
            source_label: "Crossref REST API".to_string(),
            domain: Domain::AcademicIntegrity,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Search Crossref works (DOI, title, authors, journal, citations) by query."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("api.crossref.org")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: None,
            min_delay_ms: Some(20),
            max_results: None,
            max_pages: None,
            notes: "Crossref polite pool ~50 req/s (min_delay 20ms). Recommends a mailto in the \
                    User-Agent / query; passed as mailto query param. No API key required."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut field_mappings = std::collections::BTreeMap::new();
        field_mappings.insert("title".to_string(), "title.0".to_string());
        field_mappings.insert("doi".to_string(), "DOI".to_string());
        field_mappings.insert("publisher".to_string(), "publisher".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template: "https://api.crossref.org/works?query={{query}}&rows=20&mailto=research@lucidota.dev"
                .to_string(),
            method: "GET".to_string(),
            query_params: Default::default(),
            json_path: Some("message.items".to_string()),
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

        let url = "https://api.crossref.org/works";
        let fetch = api::get_json(
            ctx,
            url,
            &[
                ("query", query.as_str()),
                ("rows", "20"),
                ("mailto", "research@lucidota.dev"),
            ],
        )
        .await?;
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

        // Result array: message.items
        let empty = Vec::new();
        let items = fetch
            .json
            .get("message")
            .and_then(|m| m.get("items"))
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        for it in items {
            record
                .candidates
                .push(ScrapeCandidate::new(serde_json::json!({ "raw": it }), 0.85));
        }

        record.notes = format!("{} works", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
