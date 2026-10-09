//! SCI_003 — PubPeer post-publication review.
//!
//! Rust port of `SCI_003_pubpeer.py` (api-json archetype). Queries the PubPeer
//! API for publications matching a DOI; each publication carries a nested
//! `comments` array. No auth; be polite (~1 req/s). Result array: `data`.

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct PubPeer;

#[async_trait]
impl Scraper for PubPeer {
    fn id(&self) -> &'static str {
        "SCI_003"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "SCI_003".to_string(),
            source_label: "PubPeer API v3".to_string(),
            domain: Domain::AcademicIntegrity,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Look up PubPeer publications (with nested review comments) by DOI."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("pubpeer.com")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: None,
            min_delay_ms: Some(1000),
            max_results: None,
            max_pages: None,
            notes: "PubPeer API v3: no key required; documented politeness ~1 req/s \
                    (min_delay 1000ms). No published hard rate cap confirmed."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut field_mappings = std::collections::BTreeMap::new();
        field_mappings.insert("doi".to_string(), "doi".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template: "https://pubpeer.com/api/v3/publications?doi={{query}}".to_string(),
            method: "GET".to_string(),
            query_params: Default::default(),
            json_path: Some("data".to_string()),
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
                record.status = ScrapeStatus::Error("missing 'query' param (DOI)".to_string());
                return Ok(record);
            }
        };

        let url = "https://pubpeer.com/api/v3/publications";
        let fetch = api::get_json(ctx, url, &[("doi", query.as_str())]).await?;
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

        // Result array: data — each element is a publication carrying a nested
        // `comments` array.
        let empty = Vec::new();
        let pubs = fetch
            .json
            .get("data")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        for p in pubs {
            record
                .candidates
                .push(ScrapeCandidate::new(serde_json::json!({ "raw": p }), 0.85));
        }

        record.notes = format!("{} publications", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
