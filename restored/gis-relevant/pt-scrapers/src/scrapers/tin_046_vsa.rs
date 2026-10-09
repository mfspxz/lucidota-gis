//! TIN_046 — Vehicle Sales Authority of BC dealer registry.
//!
//! Rust port of `TIN_046_vsa.py` (static-html archetype). The Python did a
//! GET (to grab hidden form tokens) then a POST to submit the search; it parsed
//! the results `<table>` into rows. The static-html lane is GET-only, so this
//! port GETs the search page and extracts table rows from whatever the GET
//! returns. A full POST-form submit is a follow-up (browser/form lane).

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

const VSA_SEARCH_URL: &str = "https://www.vsa.bc.ca/dealer-search";

pub struct Vsa;

#[async_trait]
impl Scraper for Vsa {
    fn id(&self) -> &'static str {
        "TIN_046"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_046".to_string(),
            source_label: "Vehicle Sales Authority of BC (dealer registry)".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Search VSA public dealer/salesperson registry (name/licence/status)."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.vsa.bc.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(2),
            min_delay_ms: Some(1500),
            max_results: Some(50),
            max_pages: None,
            notes: "VSA — public HTML; source used 1.5s inter-query spacing. Real search is a \
                    POST form (GET-only lane extracts table if present)."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        selectors.insert("row".to_string(), "table tr".to_string());
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://www.vsa.bc.ca/dealer-search?search_name={{query}}".to_string(),
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
                record.status = ScrapeStatus::Error("missing 'query' param (dealer name)".to_string());
                return Ok(record);
            }
        };

        let url = format!("{}?search_name={}", VSA_SEARCH_URL, query.replace(' ', "+"));
        let fetch = html::get_html(ctx, &url).await?;
        record.raw_capture = Some(fetch.blob.hash);

        if fetch.status != 200 {
            record.status = match fetch.status {
                429 => ScrapeStatus::RateLimited,
                403 => ScrapeStatus::Blocked("VSA returned 403".to_string()),
                s => ScrapeStatus::Error(format!("HTTP {s}")),
            };
            return Ok(record);
        }

        let cfg_selectors: &[(&str, &str)] = &[("row", "table tr")];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.8);
        record.notes = format!(
            "{} table row(s); note: full VSA search is a POST form, GET-only extraction",
            record.candidates.len()
        );
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
