//! TIN_033_indeed — Indeed Job Postings.
//!
//! Rust port of `TIN_033_indeed.py` (browser archetype). Indeed requires Selenium
//! + CAPTCHA bypass. The Python fetches `ca.indeed.com/jobs?q=...&l=Vancouver, BC`
//! and selects `.jobsearch-ResultsList li` -> `.jobTitle` / `.companyName`. This
//! port fetches on the cheap lane and extracts those selectors, then escalates to
//! the browser lane — honest Blocked (CAPTCHA/403) when unavailable.

use crate::controls::{BrowserConfig, LaneConfig, ScraperLimits};
use crate::lanes::{browser, html};
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct Indeed;

#[async_trait]
impl Scraper for Indeed {
    fn id(&self) -> &'static str {
        "TIN_033_indeed"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_033_indeed".to_string(),
            source_label: "Indeed (job postings)".to_string(),
            domain: Domain::Social,
            archetype: Archetype::Browser,
            auth_required: false,
            auth_hint: None,
            description: "Search Indeed (ca.indeed.com) for job postings by keyword/company (Selenium + CAPTCHA-gated).".to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("ca.indeed.com")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(5000),
            max_results: Some(10),
            max_pages: None,
            notes: "Indeed requires Selenium + CAPTCHA bypass. Location fixed to 'Vancouver, BC' per the original.".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        LaneConfig::Browser(BrowserConfig {
            url_template: "https://ca.indeed.com/jobs?q={{query}}&l=Vancouver%2C+BC".to_string(),
            wait_ms: Some(5000),
            stealth_user_agent: Some(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
                    .to_string(),
            ),
            stealth_patches: true,
            fallback_chain: vec!["html".to_string(), "browser".to_string()],
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
        let url = format!(
            "https://ca.indeed.com/jobs?q={}&l=Vancouver%2C+BC",
            query.replace(' ', "+")
        );

        let sels: &[(&str, &str)] = &[
            ("title", ".jobTitle, h2.jobTitle span"),
            ("company", ".companyName, [data-testid=\"company-name\"]"),
        ];

        // Cheap lane first.
        if let Ok(f) = html::get_html(ctx, &url).await {
            record.raw_capture = Some(f.blob.hash);
            match f.status {
                200 if !f.body.is_empty() => {
                    record.candidates = html::extract(&f.body, sels, 0.7);
                    if !record.candidates.is_empty() {
                        record.status = ScrapeStatus::Ok;
                        record.notes = "via html".to_string();
                        return Ok(record);
                    }
                }
                403 => {
                    record.status = ScrapeStatus::Blocked("Indeed returned 403 (anti-bot/CAPTCHA)".to_string());
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
                record.candidates = html::extract(&page.html, sels, 0.75);
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
