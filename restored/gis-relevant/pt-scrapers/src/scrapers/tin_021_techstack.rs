//! TIN_021 — Web technology fingerprinting.
//!
//! Rust port of `TIN_021_techstack.py` (static-html archetype). Fetches a
//! domain's homepage and captures the raw HTML for tech-stack fingerprinting
//! (CMS markers, analytics IDs, meta generator). The Python does regex/string
//! matching on the body; here the html lane captures the page and the meta
//! generator selector, and downstream extraction runs the fingerprint regexes.

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct TechStack;

#[async_trait]
impl Scraper for TechStack {
    fn id(&self) -> &'static str {
        "TIN_021"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_021".to_string(),
            source_label: "HTTP tech fingerprint".to_string(),
            domain: Domain::General,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Fetch a domain homepage and capture HTML for CMS/analytics/tech fingerprinting."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        // Target domain is supplied by query; no fixed primary domain.
        None
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(2),
            min_delay_ms: Some(2000),
            max_results: None,
            max_pages: None,
            notes: "Fetches https://{query}; body regex-fingerprinted downstream. Selectors limited to meta generator."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        // UNKNOWN: Python fingerprints via body regex, not CSS. Capture meta generator only.
        selectors.insert(
            "generator".to_string(),
            "meta[name=generator]".to_string(),
        );
        selectors.insert("title".to_string(), "title".to_string());
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://{{query}}".to_string(),
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
                record.status = ScrapeStatus::Error("missing 'query' param (domain)".to_string());
                return Ok(record);
            }
        };

        // Query is a bare domain; prefix https:// if not already a URL.
        let url = if query.starts_with("http://") || query.starts_with("https://") {
            query.clone()
        } else {
            format!("https://{}", query)
        };
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
            &[("generator", "meta[name=generator]"), ("title", "title")];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.85);
        record.notes = format!("{} extracted fields", record.candidates.len());
        record.status = ScrapeStatus::Ok;
        Ok(record)
    }
}
