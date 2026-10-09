//! SCI_002 — Retraction Watch (via Crossref).
//!
//! Rust port of `SCI_002_retraction_watch.py` (api-json archetype). Uses the
//! Crossref API as the retraction-metadata source (the Retraction Watch DB
//! proper requires institutional access). Filters Crossref works whose title
//! mentions "retract" or that carry an `update-to` relation. No auth; polite
//! pool via `mailto`. Result array: `message.items`.

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct RetractionWatch;

#[async_trait]
impl Scraper for RetractionWatch {
    fn id(&self) -> &'static str {
        "SCI_002"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "SCI_002".to_string(),
            source_label: "Retraction Watch (via Crossref)".to_string(),
            domain: Domain::AcademicIntegrity,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Search retraction notices via Crossref (title 'retract' / update-to filter)."
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
            notes: "Crossref polite pool ~50 req/s (min_delay 20ms), mailto recommended. \
                    Retraction Watch DB proper requires institutional access; this uses Crossref \
                    metadata. No API key required."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut field_mappings = std::collections::BTreeMap::new();
        field_mappings.insert("title".to_string(), "title.0".to_string());
        field_mappings.insert("doi".to_string(), "DOI".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template: "https://api.crossref.org/works?query={{query}}&filter=type:journal-article&query.bibliographic=retract&rows=50&mailto=research@lucidota.dev"
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
                ("filter", "type:journal-article"),
                ("query.bibliographic", "retract"),
                ("rows", "50"),
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

        // Result array: message.items — keep only items that look like retractions
        // (title contains "retract" OR carries an update-to relation), matching the
        // Python filter.
        let empty = Vec::new();
        let items = fetch
            .json
            .get("message")
            .and_then(|m| m.get("items"))
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        for it in items {
            let title = it
                .get("title")
                .and_then(|t| t.get(0))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let is_retraction =
                title.to_lowercase().contains("retract") || it.get("update-to").is_some();
            if is_retraction {
                record
                    .candidates
                    .push(ScrapeCandidate::new(serde_json::json!({ "raw": it }), 0.8));
            }
        }

        record.notes = format!("{} retraction candidates", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
