//! TIN_034 — BC Gazette incorporation/name-change/dissolution search.
//!
//! Rust port of `TIN_034_bc_gazette.py` (static-html archetype). Searches the
//! BC Gazette full-text search and parses `.gazette-result` blocks. Uses
//! collision-safe id "TIN_034_gazette".

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct BcGazette;

#[async_trait]
impl Scraper for BcGazette {
    fn id(&self) -> &'static str {
        "TIN_034_gazette"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_034_gazette".to_string(),
            source_label: "BC Gazette (notices)".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Search BC Gazette for incorporations, name changes, and dissolutions."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.bcgazette.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(2000),
            max_results: Some(20),
            max_pages: None,
            notes: "Gazette full-text search; result blocks are .gazette-result.".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        selectors.insert("result".to_string(), ".gazette-result".to_string());
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://www.bcgazette.ca/content/gazette-search?search={{query}}"
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
            "https://www.bcgazette.ca/content/gazette-search?search={}",
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

        let cfg_selectors: &[(&str, &str)] = &[("result", ".gazette-result")];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.85);
        record.notes = format!("{} gazette notice(s) extracted", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
