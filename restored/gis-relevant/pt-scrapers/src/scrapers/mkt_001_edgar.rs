//! MKT_001 — SEC EDGAR Filing Scraper.
//!
//! Rust port of `MKT_001_edgar.py` (api-json archetype). Hits the SEC EDGAR
//! full-text-search JSON API (efts.sec.gov/LATEST/search-index) and extracts
//! filing hits. No auth. SEC fair-access: 10 req/s max with a descriptive
//! User-Agent (the ScrapeCtx http client already sets one).

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct EdgarFilings;

#[async_trait]
impl Scraper for EdgarFilings {
    fn id(&self) -> &'static str {
        "MKT_001"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "MKT_001".to_string(),
            source_label: "SEC EDGAR Full-Text Search".to_string(),
            domain: Domain::Finance,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Search SEC EDGAR full-text search (EFTS) for company filings \
                          (10-K, 8-K, Form 4, 13F, etc.) matching a company name, ticker, or CIK."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("efts.sec.gov")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(5),
            min_delay_ms: Some(120),
            max_results: Some(20),
            max_pages: None,
            notes: "SEC EDGAR fair-access policy: max 10 requests/second. SEC requires a \
                    descriptive User-Agent with contact info (set by ScrapeCtx http client)."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut field_mappings = std::collections::BTreeMap::new();
        field_mappings.insert("company_name".to_string(), "_source.entity_name".to_string());
        field_mappings.insert("filing_type".to_string(), "_source.file_type".to_string());
        field_mappings.insert("filed_date".to_string(), "_source.file_date".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template:
                "https://efts.sec.gov/LATEST/search-index?q={{query}}&from=0&size=20".to_string(),
            method: "GET".to_string(),
            query_params: Default::default(),
            json_path: Some("hits.hits".to_string()),
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

        let url = "https://efts.sec.gov/LATEST/search-index";
        let fetch = api::get_json(
            ctx,
            url,
            &[("q", query.as_str()), ("from", "0"), ("size", "20")],
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

        let empty = Vec::new();
        let hits = fetch
            .json
            .get("hits")
            .and_then(|h| h.get("hits"))
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);
        for hit in hits {
            let data = serde_json::json!({ "type": "filing", "raw": hit });
            record.candidates.push(ScrapeCandidate::new(data, 0.8));
        }

        record.notes = format!("{} filings", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
