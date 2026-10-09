//! TIN_009 — Craigslist Vancouver rental listings.
//!
//! Rust port of `TIN_009_craigslist.py` (static-html archetype). Searches
//! Craigslist Vancouver housing/rooms sections for listings matching an
//! address/area/keyword query. Selectors from the Python bs4 .select calls.

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct Craigslist;

#[async_trait]
impl Scraper for Craigslist {
    fn id(&self) -> &'static str {
        "TIN_009"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_009".to_string(),
            source_label: "Craigslist Vancouver (rentals)".to_string(),
            domain: Domain::General,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Search Craigslist Vancouver apartments/rooms for rental listings by query."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("vancouver.craigslist.org")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(2000),
            max_results: Some(50),
            max_pages: None,
            notes: "Craigslist apa section; be polite (2s). Room section (roo) is a sibling endpoint."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        selectors.insert(
            "result".to_string(),
            "li.cl-search-result, .result-row, .cl-static-search-result".to_string(),
        );
        selectors.insert("title".to_string(), "div.title, a.titlestring".to_string());
        selectors.insert(
            "price".to_string(),
            "span.priceinfo, span.result-price".to_string(),
        );
        selectors.insert(
            "location".to_string(),
            "div.location, span.result-hood".to_string(),
        );
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://vancouver.craigslist.org/search/apa?query={{query}}&sort=date"
                .to_string(),
            selectors,
            regex_fallback: Default::default(),
            encoding: None,
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

        let url = format!(
            "https://vancouver.craigslist.org/search/apa?query={}&sort=date",
            query.replace(' ', "+")
        );
        let fetch = html::get_html(ctx, &url).await?;
        record.raw_capture = Some(fetch.blob.hash);

        if fetch.status != 200 {
            record.status = match fetch.status {
                429 => ScrapeStatus::RateLimited,
                403 => ScrapeStatus::Blocked("Craigslist returned 403".to_string()),
                s => ScrapeStatus::Error(format!("HTTP {s}")),
            };
            return Ok(record);
        }

        let cfg_selectors: &[(&str, &str)] = &[
            (
                "result",
                "li.cl-search-result, .result-row, .cl-static-search-result",
            ),
            ("title", "div.title, a.titlestring"),
            ("price", "span.priceinfo, span.result-price"),
            ("location", "div.location, span.result-hood"),
        ];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.85);
        record.notes = format!("{} extracted fields", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
