//! TIN_011 — Reddit public JSON search.
//!
//! Rust port of `TIN_011_reddit.py` (api-json archetype). Searches Reddit's
//! public JSON search endpoint for mentions of an entity. No auth required.
//!
//! Parity target: `search_reddit()` → `https://www.reddit.com/search.json` with
//! `q`, `limit`, `sort`, `t`. Posts live at `data.children[]`, each `.data` a
//! post (kind `t3`). Alert keywords from the Python are flagged per post.

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

const REDDIT_SEARCH: &str = "https://www.reddit.com/search.json";

/// Keywords that flag a post as relevant (from the Python ALERT_KEYWORDS set).
const ALERT_KEYWORDS: &[&str] = &[
    "unlicensed",
    "eviction",
    "renoviction",
    "displacement",
    "illegal",
    "no licence",
    "not licensed",
    "property manager",
    "property management",
    "bcfsa",
    "rtb",
    "tenancy branch",
    "fixed term",
    "bad landlord",
    "cash only",
    "no lease",
    "room by room",
    "slumlord",
    "scam landlord",
];

pub struct Reddit;

#[async_trait]
impl Scraper for Reddit {
    fn id(&self) -> &'static str {
        "TIN_011"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_011".to_string(),
            source_label: "Reddit (public JSON search)".to_string(),
            domain: Domain::General,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Search Reddit's public JSON API for posts mentioning an entity."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.reddit.com")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: None,
            min_delay_ms: None,
            max_results: None,
            max_pages: None,
            notes: "Reddit public JSON — unauthenticated access is limited to ~60 \
                    requests/minute per Reddit's published API rules; returns HTTP 429 \
                    when throttled. Be polite; a proper User-Agent is required."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut field_mappings = std::collections::BTreeMap::new();
        field_mappings.insert("title".to_string(), "data.title".to_string());
        field_mappings.insert("subreddit".to_string(), "data.subreddit".to_string());
        field_mappings.insert("author".to_string(), "data.author".to_string());
        field_mappings.insert("permalink".to_string(), "data.permalink".to_string());
        let mut query_params = std::collections::BTreeMap::new();
        query_params.insert("q".to_string(), "{{query}}".to_string());
        query_params.insert("limit".to_string(), "25".to_string());
        query_params.insert("sort".to_string(), "relevance".to_string());
        query_params.insert("t".to_string(), "all".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template: REDDIT_SEARCH.to_string(),
            method: "GET".to_string(),
            query_params,
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

        let fetch = api::get_json(
            ctx,
            REDDIT_SEARCH,
            &[
                ("q", query.as_str()),
                ("limit", "25"),
                ("sort", "relevance"),
                ("t", "all"),
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
            record.notes = format!("search returned HTTP {}", fetch.status);
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
            // Only link posts (kind t3).
            if child.get("kind").and_then(|k| k.as_str()) != Some("t3") {
                continue;
            }
            let d = match child.get("data") {
                Some(v) => v,
                None => continue,
            };
            let title = d.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let selftext = d.get("selftext").and_then(|v| v.as_str()).unwrap_or("");
            let combined = format!("{title} {selftext}").to_lowercase();
            let alerts: Vec<&str> = ALERT_KEYWORDS
                .iter()
                .filter(|kw| combined.contains(**kw))
                .copied()
                .collect();
            let permalink = d.get("permalink").and_then(|v| v.as_str()).unwrap_or("");

            let data = serde_json::json!({
                "type": "reddit_post",
                "title": title,
                "subreddit": d.get("subreddit"),
                "author": d.get("author"),
                "score": d.get("score"),
                "num_comments": d.get("num_comments"),
                "created_utc": d.get("created_utc"),
                "url": format!("https://reddit.com{permalink}"),
                "link_url": d.get("url"),
                "upvote_ratio": d.get("upvote_ratio"),
                "alert_keywords": alerts,
            });
            // Posts hitting an alert keyword are the target signal → higher confidence.
            let confidence = if alerts.is_empty() { 0.7 } else { 0.85 };
            record.candidates.push(ScrapeCandidate::new(data, confidence));
        }

        record.notes = format!("{} post(s)", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
