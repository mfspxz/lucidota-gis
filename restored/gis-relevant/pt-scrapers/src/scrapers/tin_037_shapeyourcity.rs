//! TIN_037 (id `TIN_037`) — ShapeYourCity + Vancouver rezoning (newer/larger).
//!
//! Rust port of `TIN_037_shapeyourcity.py` (static-html archetype). This is the
//! newer, richer ShapeYourCity scraper with rezoning support. The Python fetched
//! `shapeyourcity.ca/<slug>` (no /en-US/projects prefix) and extracted title,
//! status/stage badges, timeline items, document links, and applicant/contact
//! blocks. It also had a rezoning JSON API + DP-portal path; this port covers the
//! primary static-html SYC project page (the other paths are api/browser lanes).

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

const SYC_BASE: &str = "https://shapeyourcity.ca";

pub struct ShapeYourCityRezoning;

#[async_trait]
impl Scraper for ShapeYourCityRezoning {
    fn id(&self) -> &'static str {
        "TIN_037"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_037".to_string(),
            source_label: "ShapeYourCity Vancouver + rezoning".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Fetch a ShapeYourCity project page by slug (title/status/timeline/docs/applicant)."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("shapeyourcity.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(2),
            min_delay_ms: Some(1000),
            max_results: None,
            max_pages: None,
            notes: "ShapeYourCity — public HTML; source used 25s timeout, polite spacing."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        selectors.insert("project_title".to_string(), "h1".to_string());
        selectors.insert(
            "status_badge".to_string(),
            "[class*=status], [class*=stage], [class*=badge]".to_string(),
        );
        selectors.insert(
            "timeline_item".to_string(),
            "[class*=timeline], [class*=milestone], [class*=date]".to_string(),
        );
        selectors.insert("document_link".to_string(), "a[href]".to_string());
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://shapeyourcity.ca/{{query}}".to_string(),
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
                record.status = ScrapeStatus::Error("missing 'query' param (project slug)".to_string());
                return Ok(record);
            }
        };

        let url = format!("{}/{}", SYC_BASE, query.trim_start_matches('/'));
        let fetch = html::get_html(ctx, &url).await?;
        record.raw_capture = Some(fetch.blob.hash);

        if fetch.status != 200 {
            record.status = match fetch.status {
                429 => ScrapeStatus::RateLimited,
                403 => ScrapeStatus::Blocked("ShapeYourCity returned 403".to_string()),
                404 => ScrapeStatus::NotFound,
                s => ScrapeStatus::Error(format!("HTTP {s}")),
            };
            return Ok(record);
        }

        let cfg_selectors: &[(&str, &str)] = &[
            ("project_title", "h1"),
            (
                "status_badge",
                "[class*=status], [class*=stage], [class*=badge]",
            ),
            (
                "timeline_item",
                "[class*=timeline], [class*=milestone], [class*=date]",
            ),
            ("document_link", "a[href]"),
        ];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.8);
        record.notes = format!("{} extracted fields", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
