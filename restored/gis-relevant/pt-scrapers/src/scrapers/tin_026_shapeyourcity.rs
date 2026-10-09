//! TIN_026 (id `TIN_026_syc`) — Shape Your City / Vancouver rezoning portal.
//!
//! Rust port of `TIN_026_shapeyourcity.py` (static-html archetype). Fetches a
//! shapeyourcity.ca project page by an address slug and extracts project title +
//! content blocks. The Python normalized the address to a slug
//! (`lower`, spaces→`-`, drop `.`) under `/en-US/projects/<slug>`.
//!
//! Collision-safe id `TIN_026_syc` (the sibling ports TIN_026_bc_legislature).

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

const SYC_BASE: &str = "https://shapeyourcity.ca";

pub struct ShapeYourCity;

#[async_trait]
impl Scraper for ShapeYourCity {
    fn id(&self) -> &'static str {
        "TIN_026_syc"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_026_syc".to_string(),
            source_label: "Shape Your City Vancouver (project pages)".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Fetch a Shape Your City project page by address slug (title + content)."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("shapeyourcity.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(2),
            min_delay_ms: Some(500),
            max_results: None,
            max_pages: None,
            notes: "Shape Your City — source noted 2 req/s (0.5s spacing).".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        selectors.insert("title".to_string(), "h1, h2".to_string());
        selectors.insert(
            "content".to_string(),
            "div[class*=content], section[class*=project], article".to_string(),
        );
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://shapeyourcity.ca/en-US/projects/{{query}}".to_string(),
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

        // Mirror the Python slug: lowercase, spaces->'-', drop '.'.
        let slug = query.to_lowercase().replace(' ', "-").replace('.', "");
        let url = format!("{}/en-US/projects/{}", SYC_BASE, slug);
        let fetch = html::get_html(ctx, &url).await?;
        record.raw_capture = Some(fetch.blob.hash);

        if fetch.status != 200 {
            record.status = match fetch.status {
                429 => ScrapeStatus::RateLimited,
                403 => ScrapeStatus::Blocked("Shape Your City returned 403".to_string()),
                404 => ScrapeStatus::NotFound,
                s => ScrapeStatus::Error(format!("HTTP {s}")),
            };
            return Ok(record);
        }

        let cfg_selectors: &[(&str, &str)] = &[
            ("title", "h1, h2"),
            (
                "content",
                "div[class*=content], section[class*=project], article",
            ),
        ];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.8);
        record.notes = format!("{} extracted fields (slug={slug})", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
