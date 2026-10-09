//! TIN_032_glassdoor — Glassdoor Employer Reviews.
//!
//! Rust port of `TIN_032_glassdoor.py` (browser archetype). Glassdoor requires
//! Selenium/JS and blocks scraping. The Python fetches a `/Reviews/{slug}-Reviews-E.htm`
//! page and selects `.reviewText`. This port fetches that page on the cheap lane,
//! extracts review text, then escalates to the browser lane — honest Blocked when
//! unavailable.

use crate::controls::{BrowserConfig, LaneConfig, ScraperLimits};
use crate::lanes::{browser, html};
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct Glassdoor;

#[async_trait]
impl Scraper for Glassdoor {
    fn id(&self) -> &'static str {
        "TIN_032_glassdoor"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_032_glassdoor".to_string(),
            source_label: "Glassdoor (employer reviews)".to_string(),
            domain: Domain::Social,
            archetype: Archetype::Browser,
            auth_required: false,
            auth_hint: None,
            description: "Search Glassdoor employer reviews for a company (Selenium/JS-gated; anti-bot).".to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.glassdoor.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(5000),
            max_results: Some(10),
            max_pages: None,
            notes: "Glassdoor requires Selenium and blocks scrapers. Review-page slug is best-effort; UNKNOWN exact EI_IE id.".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        LaneConfig::Browser(BrowserConfig {
            url_template: "https://www.glassdoor.ca/Reviews/{{query}}-Reviews-E.htm".to_string(),
            wait_ms: Some(5000),
            stealth_user_agent: Some(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
                    .to_string(),
            ),
            stealth_patches: true,
            fallback_chain: vec!["html_meta".to_string(), "browser".to_string()],
            selectors: Default::default(),
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
        let slug = query.to_lowercase().replace(' ', "-");
        let url = format!("https://www.glassdoor.ca/Reviews/{}-Reviews-E.htm", slug);

        // Cheap lane: pull review text + OG meta from the static HTML.
        if let Ok(f) = html::get_html(ctx, &url).await {
            record.raw_capture = Some(f.blob.hash);
            match f.status {
                200 if !f.body.is_empty() => {
                    let sels: &[(&str, &str)] = &[
                        ("review", ".reviewText, [data-test=\"review-text\"]"),
                        ("title", "meta[property=\"og:title\"]"),
                        ("description", "meta[property=\"og:description\"]"),
                    ];
                    record.candidates = html::extract(&f.body, sels, 0.6);
                    if !record.candidates.is_empty() {
                        record.status = ScrapeStatus::Ok;
                        record.notes = "via html".to_string();
                        return Ok(record);
                    }
                }
                403 => {
                    record.status = ScrapeStatus::Blocked("Glassdoor returned 403 (anti-bot)".to_string());
                    return Ok(record);
                }
                429 => {
                    record.status = ScrapeStatus::RateLimited;
                    return Ok(record);
                }
                _ => {}
            }
        }

        // Escalate to the browser lane — honest Blocked if unavailable.
        match browser::render_via_ctx(ctx, &url, 5000).await {
            Ok(page) => {
                let sels: &[(&str, &str)] = &[
                    ("review", ".reviewText, [data-test=\"review-text\"]"),
                    ("title", "meta[property=\"og:title\"]"),
                ];
                record.candidates = html::extract(&page.html, sels, 0.7);
                record.status = if record.candidates.is_empty() {
                    ScrapeStatus::NotFound
                } else {
                    ScrapeStatus::Ok
                };
                record.notes = "via browser".to_string();
            }
            Err(e) => {
                record.status = ScrapeStatus::Blocked(e.to_string());
            }
        }
        Ok(record)
    }
}
