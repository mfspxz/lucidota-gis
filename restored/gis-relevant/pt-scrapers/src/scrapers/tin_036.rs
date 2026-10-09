//! TIN_036 — TikTok Profile Search.
//!
//! Rust port of `TIN_036_tiktok.py` (browser archetype). TikTok has no public
//! search API and JS-renders profiles. The Python fetches
//! `https://www.tiktok.com/@{handle}`, tries to parse the embedded
//! `__UNIVERSAL_DATA_FOR_REHYDRATION__` JSON, then OG meta. This port fetches the
//! profile page on the cheap lane, extracts OG meta, then escalates to the browser
//! lane — honest Blocked when unavailable.

use crate::controls::{BrowserConfig, LaneConfig, ScraperLimits};
use crate::lanes::{browser, html};
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct TikTok;

#[async_trait]
impl Scraper for TikTok {
    fn id(&self) -> &'static str {
        "TIN_036"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_036".to_string(),
            source_label: "TikTok (public profile)".to_string(),
            domain: Domain::Social,
            archetype: Archetype::Browser,
            auth_required: false,
            auth_hint: None,
            description: "Look up a TikTok public profile by handle via embedded state / OG meta (no public search API).".to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.tiktok.com")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(5000),
            max_results: None,
            max_pages: None,
            notes: "No public search API. Profiles JS-render; embedded __UNIVERSAL_DATA_FOR_REHYDRATION__ needs a live render. Handle-only lookup.".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        LaneConfig::Browser(BrowserConfig {
            url_template: "https://www.tiktok.com/@{{query}}".to_string(),
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
        let handle = query.trim_start_matches('@').replace(' ', "").to_lowercase();
        let profile_url = format!("https://www.tiktok.com/@{}", handle);

        let og_sels: &[(&str, &str)] = &[
            ("title", "meta[property=\"og:title\"]"),
            ("description", "meta[property=\"og:description\"]"),
        ];

        // Cheap lane: OG meta on the profile page.
        if let Ok(f) = html::get_html(ctx, &profile_url).await {
            record.raw_capture = Some(f.blob.hash);
            match f.status {
                200 if !f.body.is_empty() => {
                    record.candidates = html::extract(&f.body, og_sels, 0.6);
                    if !record.candidates.is_empty() {
                        record.status = ScrapeStatus::Ok;
                        record.notes = "via html_meta".to_string();
                        return Ok(record);
                    }
                }
                403 => {
                    record.status = ScrapeStatus::Blocked("TikTok returned 403 (anti-bot)".to_string());
                    return Ok(record);
                }
                404 => {
                    record.status = ScrapeStatus::NotFound;
                    record.notes = format!("no TikTok profile at @{handle}");
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
        match browser::render_via_ctx(ctx, &profile_url, 5000).await {
            Ok(page) => {
                record.candidates = html::extract(&page.html, og_sels, 0.7);
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
