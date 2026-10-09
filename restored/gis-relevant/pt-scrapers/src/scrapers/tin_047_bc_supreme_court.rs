//! TIN_047 — BC Supreme Court eCourt civil party search.
//!
//! Rust port of `TIN_047_bc_supreme_court.py` (static-html archetype). Searches
//! the BC court services online civil party registry. The Python selected
//! `table.results tr` and read the first three `<td>` cells (case #, type, status).

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

const COURT_SEARCH_URL: &str = "https://www.justice.gov.bc.ca/cso/esearch/civil/partySearch.do";

pub struct BcSupremeCourt;

#[async_trait]
impl Scraper for BcSupremeCourt {
    fn id(&self) -> &'static str {
        "TIN_047"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_047".to_string(),
            source_label: "BC Supreme Court (eCourt civil registry)".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Search BC Supreme Court civil party registry (case number/type/status)."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.justice.gov.bc.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(2),
            min_delay_ms: Some(1000),
            max_results: None,
            max_pages: None,
            notes: "BC CSO eSearch — public HTML; be polite. May require a session/interstitial."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        selectors.insert("result_row".to_string(), "table.results tr".to_string());
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://www.justice.gov.bc.ca/cso/esearch/civil/partySearch.do?partyName={{query}}&courtClass=Civil&courtLevel=S".to_string(),
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
            "{}?partyName={}&courtClass=Civil&courtLevel=S",
            COURT_SEARCH_URL,
            query.replace(' ', "+")
        );
        let fetch = html::get_html(ctx, &url).await?;
        record.raw_capture = Some(fetch.blob.hash);

        if fetch.status != 200 {
            record.status = match fetch.status {
                429 => ScrapeStatus::RateLimited,
                403 => ScrapeStatus::Blocked("BC court search returned 403".to_string()),
                s => ScrapeStatus::Error(format!("HTTP {s}")),
            };
            return Ok(record);
        }

        let cfg_selectors: &[(&str, &str)] = &[("result_row", "table.results tr")];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.85);
        record.notes = format!("{} result row(s)", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
