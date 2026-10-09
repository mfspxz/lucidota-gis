//! TIN_014 — LinkedIn Public Profile OSINT.
//!
//! Rust port of `TIN_014_linkedin.py` (browser archetype). The Python discovers
//! profiles via Google dorking (`site:linkedin.com/in/ "query"`), preferring
//! SerpAPI when SERPAPI_API_KEY is set, then fetches each profile's public OG
//! meta (requests, then stealth browser). LinkedIn auth-walls most content. This
//! port fetches the profile page directly when the query is a slug/URL, extracts
//! OG meta on the cheap lane, then escalates to the browser lane — honest Blocked
//! when unavailable.

use crate::controls::{BrowserConfig, LaneConfig, ScraperLimits};
use crate::lanes::{browser, html};
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct LinkedIn;

#[async_trait]
impl Scraper for LinkedIn {
    fn id(&self) -> &'static str {
        "TIN_014"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_014".to_string(),
            source_label: "LinkedIn (public profile)".to_string(),
            domain: Domain::Social,
            archetype: Archetype::Browser,
            auth_required: false,
            auth_hint: Some("SERPAPI_API_KEY".to_string()),
            description: "Fetch a LinkedIn public profile's OG meta by slug or URL; SerpAPI/stealth discovery in the original.".to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.linkedin.com")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(5000),
            max_results: None,
            max_pages: None,
            notes: "LinkedIn auth-walls most content; direct fetches often redirect to /authwall. Discovery uses Google dorking (SerpAPI).".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        LaneConfig::Browser(BrowserConfig {
            url_template: "https://www.linkedin.com/in/{{query}}/".to_string(),
            wait_ms: Some(4000),
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

        // Accept a full URL, or treat the query as an /in/ slug.
        let url = if query.starts_with("http") {
            query.clone()
        } else {
            format!("https://www.linkedin.com/in/{}/", query.replace(' ', "-"))
        };

        // Cheap lane: OG meta (LinkedIn serves some OG tags pre-authwall).
        if let Ok(f) = html::get_html(ctx, &url).await {
            record.raw_capture = Some(f.blob.hash);
            if f.status == 200 && !f.body.is_empty() {
                let sels: &[(&str, &str)] = &[
                    ("title", "meta[property=\"og:title\"]"),
                    ("description", "meta[property=\"og:description\"]"),
                ];
                record.candidates = html::extract(&f.body, sels, 0.6);
                if !record.candidates.is_empty() {
                    record.status = ScrapeStatus::Ok;
                    record.notes = "via html_meta".to_string();
                    return Ok(record);
                }
            } else if f.status == 429 {
                record.status = ScrapeStatus::RateLimited;
                return Ok(record);
            }
        }

        // Escalate to the browser lane — honest Blocked if unavailable.
        match browser::render_via_ctx(ctx, &url, 4000).await {
            Ok(page) => {
                if page.final_url.to_lowercase().contains("authwall")
                    || page.final_url.to_lowercase().contains("/login")
                {
                    record.status = ScrapeStatus::Blocked("LinkedIn auth wall".to_string());
                    return Ok(record);
                }
                let sels: &[(&str, &str)] = &[
                    ("title", "meta[property=\"og:title\"]"),
                    ("description", "meta[property=\"og:description\"]"),
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
