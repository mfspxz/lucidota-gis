//! TIN_029 — BC Courts Online case search (eSearch variant).
//!
//! Rust port of `TIN_029_bc_courts.py` (static-html archetype). Searches CSO
//! eSearch by partyName and parses result rows. Python selects
//! `table tr, .search-result, .case-row`. Uses collision-safe id "TIN_029_courts".

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct BcCourtsEsearch;

#[async_trait]
impl Scraper for BcCourtsEsearch {
    fn id(&self) -> &'static str {
        "TIN_029_courts"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_029_courts".to_string(),
            source_label: "BC Courts Online (CSO eSearch)".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: Some("CSO eSearch may restrict full results behind a paid account.".to_string()),
            description: "Search BC Courts Online (CSO eSearch) civil/criminal case index by party name."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("justice.gov.bc.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(2000),
            max_results: Some(50),
            max_pages: None,
            notes: "CSO eSearch; some results restricted behind login. Row selectors from Python bs4 .select."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        selectors.insert(
            "row".to_string(),
            "table tr, .search-result, .case-row".to_string(),
        );
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://justice.gov.bc.ca/cso/esearch/?partyName={{query}}".to_string(),
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
                record.status = ScrapeStatus::Error("missing 'query' param (party name)".to_string());
                return Ok(record);
            }
        };

        let url = format!(
            "https://justice.gov.bc.ca/cso/esearch/?partyName={}",
            query.replace(' ', "+")
        );
        let fetch = html::get_html(ctx, &url).await?;
        record.raw_capture = Some(fetch.blob.hash);

        if fetch.status != 200 {
            record.status = match fetch.status {
                429 => ScrapeStatus::RateLimited,
                403 => ScrapeStatus::Blocked("CSO 403 — authenticated session required".to_string()),
                s => ScrapeStatus::Error(format!("HTTP {s}")),
            };
            return Ok(record);
        }

        let cfg_selectors: &[(&str, &str)] = &[("row", "table tr, .search-result, .case-row")];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.85);
        record.notes = format!("{} case row(s) extracted", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
