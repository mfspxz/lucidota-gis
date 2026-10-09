//! TIN_026 — BC Legislature bill monitor.
//!
//! Rust port of `TIN_026_bc_legislature.py` (static-html archetype). Scrapes the
//! BC Legislature first-reading bills list for links (anchor tags) matching
//! housing/tenancy keywords. Python uses `soup.find_all("a", href=True)`.

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct BcLegislature;

#[async_trait]
impl Scraper for BcLegislature {
    fn id(&self) -> &'static str {
        "TIN_026"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_026".to_string(),
            source_label: "BC Legislature (bills)".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Scrape BC Legislature first-reading bills list for housing/tenancy-related bills."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.leg.bc.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(2000),
            max_results: None,
            max_pages: None,
            notes: "Bills list page; extract anchors, filter by keyword downstream. Python scans all <a href>."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        // Python iterates all anchors; capture links + page content.
        selectors.insert("link".to_string(), "a[href]".to_string());
        LaneConfig::StaticHtml(HtmlConfig {
            url_template:
                "https://www.leg.bc.ca/parliamentary-business/overview/43rd-parliament/2nd-session/bills/first-reading"
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

        // Fixed bills-list URL; query optional (used only for downstream filtering).
        let url = "https://www.leg.bc.ca/parliamentary-business/overview/43rd-parliament/2nd-session/bills/first-reading";
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

        let cfg_selectors: &[(&str, &str)] = &[("link", "a[href]")];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.85);
        record.notes = format!("{} anchor links extracted", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
