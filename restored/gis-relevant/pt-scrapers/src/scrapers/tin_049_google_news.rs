//! TIN_049 — Google News RSS search.
//!
//! Rust port of `TIN_049_google_news.py` (feed archetype). Searches Google News
//! RSS for mentions of an entity. Free, no auth. CA/en locale like the original.

use crate::controls::{FeedConfig, LaneConfig, ScraperLimits};
use crate::lanes::feed;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct GoogleNews;

#[async_trait]
impl Scraper for GoogleNews {
    fn id(&self) -> &'static str {
        "TIN_049"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_049".to_string(),
            source_label: "Google News (RSS)".to_string(),
            domain: Domain::General,
            archetype: Archetype::Feed,
            auth_required: false,
            auth_hint: None,
            description: "Search Google News RSS for recent articles mentioning an entity."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("news.google.com")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(2),
            min_delay_ms: Some(1000),
            max_results: None,
            max_pages: None,
            notes: "Google News RSS — free, no published rate limit; be polite.".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        LaneConfig::Feed(FeedConfig {
            url_template:
                "https://news.google.com/rss/search?q={{query}}&hl=en-CA&gl=CA&ceid=CA:en"
                    .to_string(),
            feed_type: Some("rss".to_string()),
            entry_fields: Default::default(),
            entry_filters: Vec::new(),
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

        let url = format!(
            "https://news.google.com/rss/search?q={}&hl=en-CA&gl=CA&ceid=CA:en",
            query.replace(' ', "+")
        );
        let fetch = feed::get_feed(ctx, &url).await?;
        record.raw_capture = Some(fetch.blob.hash);

        if fetch.status != 200 {
            record.status = if fetch.status == 429 {
                ScrapeStatus::RateLimited
            } else {
                ScrapeStatus::Error(format!("HTTP {}", fetch.status))
            };
            return Ok(record);
        }

        record.candidates = fetch.candidates;
        record.notes = format!("{} articles", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
