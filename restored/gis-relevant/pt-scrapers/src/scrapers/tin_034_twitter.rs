//! TIN_034_twitter — Twitter / X Profile Search.
//!
//! Rust port of `TIN_034_twitter.py` (browser archetype). Twitter/X has no free
//! public API since 2023 and heavily JS-renders. The Python fetches
//! `https://x.com/{handle}`, reads OG meta, and tries a Nitter mirror fallback.
//! This port fetches the profile page on the cheap lane, extracts OG meta, tries
//! a Nitter mirror, then escalates to the browser lane — honest Blocked when
//! unavailable.

use crate::controls::{BrowserConfig, LaneConfig, ScraperLimits};
use crate::lanes::{browser, html};
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct TwitterScrape;

#[async_trait]
impl Scraper for TwitterScrape {
    fn id(&self) -> &'static str {
        "TIN_034_twitter"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_034_twitter".to_string(),
            source_label: "Twitter / X (public profile)".to_string(),
            domain: Domain::Social,
            archetype: Archetype::Browser,
            auth_required: false,
            auth_hint: None,
            description: "Look up a Twitter/X public profile by handle via OG meta + Nitter mirror (no free API since 2023).".to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("x.com")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(5000),
            max_results: None,
            max_pages: None,
            notes: "No free Twitter/X API since 2023; heavy JS render, 403 common. Nitter mirror availability is UNKNOWN/unstable.".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        LaneConfig::Browser(BrowserConfig {
            url_template: "https://x.com/{{query}}".to_string(),
            wait_ms: Some(5000),
            stealth_user_agent: Some(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
                    .to_string(),
            ),
            stealth_patches: true,
            fallback_chain: vec![
                "html_meta".to_string(),
                "nitter".to_string(),
                "browser".to_string(),
            ],
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
        let handle = query.trim_start_matches('@').replace(' ', "");
        let profile_url = format!("https://x.com/{}", handle);

        let og_sels: &[(&str, &str)] = &[
            ("title", "meta[property=\"og:title\"]"),
            ("description", "meta[property=\"og:description\"]"),
        ];

        // Cheap lane: OG meta on the x.com profile page.
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
                    record.status = ScrapeStatus::Blocked("x.com returned 403 (anti-bot)".to_string());
                    // continue to Nitter / browser attempts below
                }
                429 => {
                    record.status = ScrapeStatus::RateLimited;
                    return Ok(record);
                }
                _ => {}
            }
        }

        // Nitter mirror fallback (open-source Twitter frontend).
        let nitter_url = format!("https://nitter.net/{}", handle);
        if let Ok(f) = html::get_html(ctx, &nitter_url).await {
            if f.status == 200 && !f.body.is_empty() {
                let sels: &[(&str, &str)] = &[
                    ("bio", ".profile-bio"),
                    ("name", ".profile-card-fullname"),
                ];
                let cands = html::extract(&f.body, sels, 0.55);
                if !cands.is_empty() {
                    record.candidates = cands;
                    record.status = ScrapeStatus::Ok;
                    record.notes = "via nitter mirror".to_string();
                    return Ok(record);
                }
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
                // Preserve an earlier Blocked reason if we already have one.
                if !matches!(record.status, ScrapeStatus::Blocked(_)) {
                    record.status = ScrapeStatus::Blocked(e.to_string());
                }
            }
        }
        Ok(record)
    }
}
