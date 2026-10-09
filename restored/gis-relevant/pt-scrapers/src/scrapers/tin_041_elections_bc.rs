//! TIN_041 — Elections BC campaign finance / donor search.
//!
//! Rust port of `TIN_041_elections_bc.py` (static-html archetype). Queries the
//! Elections BC electoral finance search portal and parses result rows. Python
//! selects `table tr, .result-row, .contribution-row`.

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct ElectionsBc;

#[async_trait]
impl Scraper for ElectionsBc {
    fn id(&self) -> &'static str {
        "TIN_041"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_041".to_string(),
            source_label: "Elections BC (campaign finance)".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Search Elections BC campaign finance disclosures for donors/parties."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("electoralfinancesearch.elections.bc.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(3000),
            max_results: Some(50),
            max_pages: None,
            notes: "Public portal; 3s spacing. May return JSON or HTML; row selectors from Python bs4 .select."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        selectors.insert(
            "row".to_string(),
            "table tr, .result-row, .contribution-row".to_string(),
        );
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://electoralfinancesearch.elections.bc.ca/efs/search?q={{query}}&type=entity"
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
                record.status = ScrapeStatus::Error("missing 'query' param (party/donor)".to_string());
                return Ok(record);
            }
        };

        let url = format!(
            "https://electoralfinancesearch.elections.bc.ca/efs/search?q={}&type=entity",
            query.replace(' ', "+")
        );
        let fetch = html::get_html(ctx, &url).await?;
        record.raw_capture = Some(fetch.blob.hash);

        if fetch.status != 200 {
            record.status = match fetch.status {
                429 => ScrapeStatus::RateLimited,
                403 => ScrapeStatus::Blocked("403".to_string()),
                s => ScrapeStatus::Error(format!("HTTP {s}")),
            };
            return Ok(record);
        }

        let cfg_selectors: &[(&str, &str)] =
            &[("row", "table tr, .result-row, .contribution-row")];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.85);
        record.notes = format!("{} contribution row(s) extracted", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
