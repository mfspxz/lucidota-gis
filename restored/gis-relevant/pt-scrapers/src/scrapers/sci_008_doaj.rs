//! SCI_008 — DOAJ (Directory of Open Access Journals) API.
//!
//! Rust port of `SCI_008_doaj.py` (api-json archetype). Searches the DOAJ
//! journals registry. The query term is a PATH segment
//! (`/api/search/journals/<query>`), so the URL is built with the query
//! percent-encoded into the path. No auth; be polite (~1 req/s). Result array:
//! `results`.

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct Doaj;

#[async_trait]
impl Scraper for Doaj {
    fn id(&self) -> &'static str {
        "SCI_008"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "SCI_008".to_string(),
            source_label: "DOAJ API".to_string(),
            domain: Domain::AcademicIntegrity,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Search DOAJ journals registry (open-access legitimacy, APC, seal) by ISSN/title."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("doaj.org")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: None,
            min_delay_ms: Some(1000),
            max_results: None,
            max_pages: None,
            notes: "DOAJ API: no key required; documented politeness ~1 req/s (min_delay 1000ms). \
                    Query term is a URL path segment, not a query param."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut field_mappings = std::collections::BTreeMap::new();
        field_mappings.insert("title".to_string(), "bibjson.title".to_string());
        field_mappings.insert("publisher".to_string(), "bibjson.publisher.name".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template: "https://doaj.org/api/search/journals/{{query}}?pageSize=25".to_string(),
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
                record.status = ScrapeStatus::Error("missing 'query' param (ISSN/title)".to_string());
                return Ok(record);
            }
        };

        // DOAJ carries the query in the PATH, not a query param. Build the URL
        // with the query percent-encoded into the path segment.
        let mut u = reqwest::Url::parse("https://doaj.org/api/search/journals/")?;
        u.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("cannot build DOAJ url"))?
            .pop_if_empty()
            .push(&query);
        let url = u.to_string();

        let fetch = api::get_json(ctx, &url, &[("pageSize", "25")]).await?;
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

        // Result array: results — each element is a journal record (with bibjson).
        let empty = Vec::new();
        let results = fetch
            .json
            .get("results")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        for j in results {
            record
                .candidates
                .push(ScrapeCandidate::new(serde_json::json!({ "raw": j }), 0.85));
        }

        record.notes = format!("{} journals", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
