//! TIN_042 — Elections BC donation records scraper.
//!
//! Rust port of `TIN_042_elections_bc_donations.py` (static-html archetype).
//! Fetches the Elections BC public disclosure search page and parses donation
//! tables. Python iterates `soup.find_all("table")` then rows/cells.

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct ElectionsBcDonations;

#[async_trait]
impl Scraper for ElectionsBcDonations {
    fn id(&self) -> &'static str {
        "TIN_042"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_042".to_string(),
            source_label: "Elections BC (donations)".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Scrape Elections BC public disclosure donation tables by party."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("ems.electionsbc.gov.bc.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(3000),
            max_results: None,
            max_pages: None,
            notes: "Public portal; 3s spacing. Python parses generic <table> rows; selector is table."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        // Python parses all <table> rows/cells; no specific class.
        selectors.insert("table".to_string(), "table".to_string());
        selectors.insert("row".to_string(), "table tr".to_string());
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://ems.electionsbc.gov.bc.ca/prs/PRS_PublicSearchDisclosure.aspx"
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

        // Python GETs the disclosure page with no query params; query optional.
        let url = "https://ems.electionsbc.gov.bc.ca/prs/PRS_PublicSearchDisclosure.aspx";
        let fetch = html::get_html(ctx, url).await?;
        record.raw_capture = Some(fetch.blob.hash);

        if fetch.status != 200 {
            record.status = match fetch.status {
                429 => ScrapeStatus::RateLimited,
                403 => ScrapeStatus::Blocked("403".to_string()),
                s => ScrapeStatus::Error(format!("HTTP {s}")),
            };
            return Ok(record);
        }

        let cfg_selectors: &[(&str, &str)] = &[("table", "table"), ("row", "table tr")];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.85);
        record.notes = format!("{} table row(s) extracted", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
