//! TIN_022 — BC Assessment property lookup.
//!
//! Rust port of `TIN_022_bc_assessment.py` (static-html archetype). Queries the
//! BC Assessment public GetByAddress endpoint (JSON-over-HTTP) for property
//! records. The Python parses JSON keys; here the html lane captures the body
//! and downstream JSON/text extraction runs. Selectors UNKNOWN (JSON API).

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct BcAssessment;

#[async_trait]
impl Scraper for BcAssessment {
    fn id(&self) -> &'static str {
        "TIN_022"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_022".to_string(),
            source_label: "BC Assessment (property lookup)".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Look up BC Assessment property records by address via GetByAddress endpoint."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.bcassessment.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(2),
            min_delay_ms: Some(2000),
            max_results: None,
            max_pages: None,
            notes: "GetByAddress returns JSON (Address/PID/RollNumber/...). Selectors UNKNOWN — JSON API, parse downstream."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let selectors = std::collections::BTreeMap::new();
        // UNKNOWN: source is a JSON typeahead endpoint, not HTML with CSS selectors.
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://www.bcassessment.ca/Property/Search/GetByAddress?addr={{query}}"
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
                record.status = ScrapeStatus::Error("missing 'query' param (address)".to_string());
                return Ok(record);
            }
        };

        let url = format!(
            "https://www.bcassessment.ca/Property/Search/GetByAddress?addr={}",
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

        // UNKNOWN selectors — JSON API. Capture raw body; downstream parses JSON.
        let cfg_selectors: &[(&str, &str)] = &[];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.85);
        record.notes = format!(
            "JSON body captured ({} bytes); {} css fields (UNKNOWN — JSON API)",
            fetch.body.len(),
            record.candidates.len()
        );
        record.status = ScrapeStatus::Ok;
        Ok(record)
    }
}
