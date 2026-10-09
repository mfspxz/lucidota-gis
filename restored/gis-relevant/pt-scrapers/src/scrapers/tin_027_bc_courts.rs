//! TIN_027 — BC Supreme/Provincial Court public dockets.
//!
//! Rust port of `TIN_027_bc_courts.py` (static-html archetype). Hits BC Court
//! Services Online (CSO) csoAccess with name/address params. The Python's
//! `_parse_response` is a placeholder returning [] (full docket access needs
//! auth), so extraction selectors are UNKNOWN.

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct BcCourts;

#[async_trait]
impl Scraper for BcCourts {
    fn id(&self) -> &'static str {
        "TIN_027"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_027".to_string(),
            source_label: "BC Court Services Online (dockets)".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: Some("Full docket access may require CSO authenticated session.".to_string()),
            description: "Search BC Court Services Online public dockets by name/address."
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
            max_results: None,
            max_pages: None,
            notes: "CSO csoAccess; Python parser is a placeholder ([]). Selectors UNKNOWN — full access needs auth."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let selectors = std::collections::BTreeMap::new();
        // UNKNOWN: Python _parse_response is a placeholder returning [].
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://justice.gov.bc.ca/cso/csoAccess.do?name={{query}}&court_level=all&jurisdiction=BC"
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
                record.status = ScrapeStatus::Error("missing 'query' param (name/address)".to_string());
                return Ok(record);
            }
        };

        let url = format!(
            "https://justice.gov.bc.ca/cso/csoAccess.do?name={}&court_level=all&jurisdiction=BC",
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

        // UNKNOWN selectors — Python parser is a stub. Capture raw body.
        let cfg_selectors: &[(&str, &str)] = &[];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.85);
        record.notes = format!(
            "raw body captured ({} bytes); selectors UNKNOWN (Python parser is placeholder)",
            fetch.body.len()
        );
        record.status = ScrapeStatus::Ok;
        Ok(record)
    }
}
