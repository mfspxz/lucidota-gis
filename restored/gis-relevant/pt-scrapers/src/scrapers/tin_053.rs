//! TIN_053 — PERB (Pacific Escort Review Board), XenForo 2.x forum.
//!
//! Rust port of `TIN_053_perb.py` (browser archetype). The Python drives a
//! XenForo scraper over `https://perb.cc/xenforo`, indexing forum sections
//! (e.g. `forums/vancouver-escort-reviews.14/`). robots.txt allows public read;
//! Cloudflare may challenge. Optional cookies PERB_XF_SESSION / PERB_XF_USER
//! bypass login-gated content. This port fetches a forum section on the cheap
//! lane and extracts thread titles, then escalates to the browser lane —
//! honest Blocked on Cloudflare / when the browser is unavailable.

use crate::controls::{BrowserConfig, LaneConfig, ScraperLimits};
use crate::lanes::{browser, html};
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct Perb;

const PERB_BASE: &str = "https://perb.cc/xenforo";

fn section_url(query: &str) -> String {
    // Accept a full URL, a "slug.id" forum path, or a bare slug.
    if query.starts_with("http") {
        query.to_string()
    } else {
        let path = query.trim_start_matches('/');
        format!("{PERB_BASE}/forums/{path}/")
    }
}

fn looks_like_cloudflare(body: &str) -> bool {
    let b = body.to_lowercase();
    b.contains("just a moment")
        || b.contains("cf-challenge")
        || b.contains("checking your browser")
        || b.contains("cf-browser-verification")
}

#[async_trait]
impl Scraper for Perb {
    fn id(&self) -> &'static str {
        "TIN_053"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_053".to_string(),
            source_label: "PERB (Pacific Escort Review Board) forum".to_string(),
            domain: Domain::Social,
            archetype: Archetype::Browser,
            auth_required: false,
            auth_hint: Some("PERB_XF_SESSION / PERB_XF_USER".to_string()),
            description: "Index a PERB XenForo forum section (thread titles/links) by slug.id; Cloudflare-gated.".to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("perb.cc")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(3000),
            max_results: None,
            max_pages: None,
            notes: "XenForo 2.x; robots allows public read, 3-5s intervals. Cloudflare may challenge. Optional PERB_XF_SESSION/PERB_XF_USER cookies (UNKNOWN whether injected in this build).".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        LaneConfig::Browser(BrowserConfig {
            url_template: "https://perb.cc/xenforo/forums/{{query}}/".to_string(),
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
                record.status = ScrapeStatus::Error(
                    "missing 'query' param (forum section slug.id, e.g. 'vancouver-escort-reviews.14')"
                        .to_string(),
                );
                return Ok(record);
            }
        };
        let url = section_url(&query);

        // XenForo thread list selectors.
        let sels: &[(&str, &str)] = &[
            ("thread", "div.structItem-title a, .structItem-title a[data-tp-primary]"),
            ("title", "meta[property=\"og:title\"]"),
        ];

        // Cheap lane first.
        if let Ok(f) = html::get_html(ctx, &url).await {
            record.raw_capture = Some(f.blob.hash);
            match f.status {
                200 if !f.body.is_empty() => {
                    if looks_like_cloudflare(&f.body) {
                        record.status =
                            ScrapeStatus::Blocked("PERB Cloudflare challenge (cookies needed)".to_string());
                        return Ok(record);
                    }
                    record.candidates = html::extract(&f.body, sels, 0.7);
                    if !record.candidates.is_empty() {
                        record.status = ScrapeStatus::Ok;
                        record.notes = format!("via html: {} thread link(s)", record.candidates.len());
                        return Ok(record);
                    }
                }
                403 => {
                    record.status = ScrapeStatus::Blocked("PERB returned 403 (Cloudflare/anti-bot)".to_string());
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
                if looks_like_cloudflare(&page.html) {
                    record.status =
                        ScrapeStatus::Blocked("PERB Cloudflare challenge (cookies needed)".to_string());
                    return Ok(record);
                }
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
