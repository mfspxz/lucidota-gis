//! TIN_045 — Craigslist deep archive (historical rental listings).
//!
//! Rust port of `TIN_045_craigslist_archive.py` (static-html archetype). Searches
//! Vancouver Craigslist housing sections for a query. This port fetches the first
//! page of the apartments (`/apa`) section; the Python iterated /apa,/roo,/sub and
//! paginated (s=offset) — pagination is left to the operator via controls.

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

const CL_BASE: &str = "https://vancouver.craigslist.org";

pub struct CraigslistArchive;

#[async_trait]
impl Scraper for CraigslistArchive {
    fn id(&self) -> &'static str {
        "TIN_045"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_045".to_string(),
            source_label: "Craigslist Vancouver (housing archive)".to_string(),
            domain: Domain::General,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Search Vancouver Craigslist housing listings by query (title/price/date)."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("vancouver.craigslist.org")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(1000),
            max_results: None,
            max_pages: None,
            notes: "Craigslist — 1 req/s per source; sections /apa,/roo,/sub, 120/page."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        selectors.insert(
            "result".to_string(),
            "li.result-row, li.cl-search-result".to_string(),
        );
        selectors.insert(
            "title".to_string(),
            ".result-title, .posting-title".to_string(),
        );
        selectors.insert(
            "price".to_string(),
            ".result-price, .priceinfo".to_string(),
        );
        selectors.insert("date".to_string(), "time".to_string());
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://vancouver.craigslist.org/apa/search?query={{query}}&s=0"
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
            "{}/apa/search?query={}&s=0",
            CL_BASE,
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
            ("result", "li.result-row, li.cl-search-result"),
            ("title", ".result-title, .posting-title"),
            ("price", ".result-price, .priceinfo"),
            ("date", "time"),
        ];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.7);
        record.notes = format!("{} extracted fields (section /apa page 0)", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
