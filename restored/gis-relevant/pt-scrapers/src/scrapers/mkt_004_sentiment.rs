//! MKT_004 — Social Sentiment Scanner.
//!
//! Rust port of `MKT_004_sentiment.py` (api-json archetype). Unlike the other
//! MKT scrapers this hits Reddit's public search JSON API
//! (www.reddit.com/search.json) rather than SEC EDGAR. No auth. Reddit throttles
//! anonymous JSON access; be polite. A descriptive User-Agent is required
//! (set by the ScrapeCtx http client).

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct SocialSentiment;

#[async_trait]
impl Scraper for SocialSentiment {
    fn id(&self) -> &'static str {
        "MKT_004"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "MKT_004".to_string(),
            source_label: "Reddit Search (social sentiment)".to_string(),
            domain: Domain::Finance,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Scan Reddit's public search JSON API for recent posts mentioning a \
                          stock ticker, to gauge aggregate social sentiment."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.reddit.com")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(2),
            min_delay_ms: Some(1000),
            max_results: Some(25),
            max_pages: None,
            notes: "Reddit public JSON API — no published anonymous rate limit but aggressive \
                    throttling; be polite (max ~1 req/s). Reddit caps 25 results per page. A \
                    descriptive User-Agent is required (set by ScrapeCtx http client)."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut field_mappings = std::collections::BTreeMap::new();
        field_mappings.insert("title".to_string(), "data.title".to_string());
        field_mappings.insert("author".to_string(), "data.author".to_string());
        field_mappings.insert("permalink".to_string(), "data.permalink".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template:
                "https://www.reddit.com/search.json?q={{query}}+stock&sort=new&limit=25&t=week"
                    .to_string(),
            method: "GET".to_string(),
            query_params: Default::default(),
            json_path: Some("data.children".to_string()),
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

        let url = "https://www.reddit.com/search.json";
        let q = format!("{query} stock");
        let fetch = api::get_json(
            ctx,
            url,
            &[
                ("q", q.as_str()),
                ("sort", "new"),
                ("limit", "25"),
                ("t", "week"),
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

        let empty = Vec::new();
        let children = fetch
            .json
            .get("data")
            .and_then(|d| d.get("children"))
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);
        for child in children {
            let data = serde_json::json!({ "type": "reddit_post", "raw": child });
            record.candidates.push(ScrapeCandidate::new(data, 0.6));
        }

        record.notes = format!("{} mentions", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
