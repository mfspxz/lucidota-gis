//! SCI_004 — OpenAlex bibliometric API.
//!
//! Rust port of `SCI_004_openalex.py` (api-json archetype). Searches OpenAlex
//! works by free-text query. No auth; polite pool via `mailto`. Result array:
//! `results`.

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct OpenAlex;

#[async_trait]
impl Scraper for OpenAlex {
    fn id(&self) -> &'static str {
        "SCI_004"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "SCI_004".to_string(),
            source_label: "OpenAlex API".to_string(),
            domain: Domain::AcademicIntegrity,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Search OpenAlex works (bibliometrics: citations, authors, concepts)."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("api.openalex.org")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: None,
            min_delay_ms: Some(100),
            max_results: Some(200),
            max_pages: None,
            notes: "OpenAlex: ~10 req/s (min_delay 100ms), 100k requests/day, no key required. \
                    Recommends a mailto (polite pool); passed as mailto query param. per_page max 200."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut field_mappings = std::collections::BTreeMap::new();
        field_mappings.insert("title".to_string(), "title".to_string());
        field_mappings.insert("doi".to_string(), "doi".to_string());
        field_mappings.insert("cited_by_count".to_string(), "cited_by_count".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template: "https://api.openalex.org/works?search={{query}}&per_page=25&mailto=contact@lucidota.dev"
                .to_string(),
            method: "GET".to_string(),
            query_params: Default::default(),
            json_path: Some("results".to_string()),
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

        let url = "https://api.openalex.org/works";
        let fetch = api::get_json(
            ctx,
            url,
            &[
                ("search", query.as_str()),
                ("per_page", "25"),
                ("mailto", "contact@lucidota.dev"),
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

        // Result array: results
        let empty = Vec::new();
        let results = fetch
            .json
            .get("results")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        for w in results {
            record
                .candidates
                .push(ScrapeCandidate::new(serde_json::json!({ "raw": w }), 0.85));
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
