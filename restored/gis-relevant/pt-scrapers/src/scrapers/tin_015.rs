//! TIN_015 — Facebook Public Pages / Marketplace OSINT.
//!
//! Rust port of `TIN_015_facebook.py` (browser archetype). The Python discovers
//! pages via Google dorking (`site:facebook.com "query"`), preferring SerpAPI
//! when SERPAPI_API_KEY is set, then fetches each page's OG meta (requests, then
//! stealth browser). Facebook auth-walls most content. This port fetches a page
//! by URL/slug, extracts OG meta on the cheap lane, then escalates to the browser
//! lane — honest Blocked when unavailable.

use crate::controls::{BrowserConfig, LaneConfig, ScraperLimits};
use crate::lanes::{browser, html};
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct Facebook;

#[async_trait]
impl Scraper for Facebook {
    fn id(&self) -> &'static str {
        "TIN_015"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_015".to_string(),
            source_label: "Facebook (public pages / marketplace)".to_string(),
            domain: Domain::Social,
            archetype: Archetype::Browser,
            auth_required: false,
            auth_hint: Some("SERPAPI_API_KEY".to_string()),
            description: "Fetch a Facebook public page's OG meta by URL/slug; Google-search/stealth discovery in the original.".to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.facebook.com")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(3000),
            max_results: None,
            max_pages: None,
            notes: "Facebook auth-walls most content; direct fetches often redirect to /login or /checkpoint. Discovery uses Google dorking (SerpAPI).".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        LaneConfig::Browser(BrowserConfig {
            url_template: "https://www.facebook.com/{{query}}".to_string(),
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

        let url = if query.starts_with("http") {
            query.clone()
        } else {
            format!("https://www.facebook.com/{}", query.trim_start_matches('/').replace(' ', ""))
        };

        // Cheap lane: OG meta (Facebook serves OG tags for many public pages).
        if let Ok(f) = html::get_html(ctx, &url).await {
            record.raw_capture = Some(f.blob.hash);
            if f.status == 200 && !f.body.is_empty() {
                let sels: &[(&str, &str)] = &[
                    ("title", "meta[property=\"og:title\"]"),
                    ("description", "meta[property=\"og:description\"]"),
                    ("site_name", "meta[property=\"og:site_name\"]"),
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
                let final_lower = page.final_url.to_lowercase();
                if final_lower.contains("/login") || final_lower.contains("checkpoint") {
                    record.status = ScrapeStatus::Blocked("Facebook auth wall".to_string());
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
