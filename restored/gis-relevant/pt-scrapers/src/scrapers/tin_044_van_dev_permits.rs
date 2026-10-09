//! TIN_044 — City of Vancouver development permits portal.
//!
//! Rust port of `TIN_044_van_dev_permits.py` (static-html archetype). Searches
//! development.vancouver.ca for permit status by address. Selectors mirror the
//! bs4 `find_all(class_=[...])` in the Python source.

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

const BASE_URL: &str = "https://development.vancouver.ca";

pub struct VanDevPermits;

#[async_trait]
impl Scraper for VanDevPermits {
    fn id(&self) -> &'static str {
        "TIN_044"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_044".to_string(),
            source_label: "City of Vancouver Development Permits".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Search Vancouver development permits by address (permit number + status)."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("development.vancouver.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(2),
            min_delay_ms: Some(1500),
            max_results: None,
            max_pages: None,
            notes: "development.vancouver.ca — public HTML; source used 1.5s spacing.".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        selectors.insert(
            "result".to_string(),
            ".permit-item, .permit-row, .result-item".to_string(),
        );
        selectors.insert("permit_number".to_string(), ".permit-number".to_string());
        selectors.insert("status".to_string(), ".status, .permit-status".to_string());
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://development.vancouver.ca/permits?address={{query}}".to_string(),
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
                record.status = ScrapeStatus::Error("missing 'query' param (address)".to_string());
                return Ok(record);
            }
        };

        let url = format!("{}/permits?address={}", BASE_URL, query.replace(' ', "+"));
        let fetch = html::get_html(ctx, &url).await?;
        record.raw_capture = Some(fetch.blob.hash);

        if fetch.status != 200 {
            record.status = match fetch.status {
                429 => ScrapeStatus::RateLimited,
                403 => ScrapeStatus::Blocked("dev permits returned 403".to_string()),
                s => ScrapeStatus::Error(format!("HTTP {s}")),
            };
            return Ok(record);
        }

        let cfg_selectors: &[(&str, &str)] = &[
            ("result", ".permit-item, .permit-row, .result-item"),
            ("permit_number", ".permit-number"),
            ("status", ".status, .permit-status"),
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
