//! TIN_013 — Instagram Public Profile OSINT.
//!
//! Rust port of `TIN_013_instagram.py` (browser archetype). The Python fallback
//! chain is: JSON endpoint (`/api/v1/users/web_profile_info/`) -> HTML meta tag
//! parsing (`/{username}/` OG tags) -> stealth browser. This port preserves that
//! order: JSON endpoint on the api lane, then HTML meta (OG tags) on the cheap
//! lane, then the headless browser lane — honest Blocked when unavailable.
//! Optional SERPAPI_API_KEY is noted as an auth hint (used by the Python google
//! discovery path; not required to run a direct username lookup).

use crate::controls::{BrowserConfig, LaneConfig, ScraperLimits};
use crate::lanes::{browser, html};
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct Instagram;

#[async_trait]
impl Scraper for Instagram {
    fn id(&self) -> &'static str {
        "TIN_013"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_013".to_string(),
            source_label: "Instagram (public profile)".to_string(),
            domain: Domain::Social,
            archetype: Archetype::Browser,
            auth_required: false,
            auth_hint: Some("SERPAPI_API_KEY".to_string()),
            description: "Look up an Instagram public profile by username: JSON endpoint -> HTML meta -> browser fallback chain.".to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.instagram.com")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(5000),
            max_results: None,
            max_pages: None,
            notes: "Instagram aggressively blocks scrapers (429/redirect). 5s min spacing. Only public profiles.".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        LaneConfig::Browser(BrowserConfig {
            url_template: "https://www.instagram.com/{{query}}/".to_string(),
            wait_ms: Some(5000),
            stealth_user_agent: Some(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
                    .to_string(),
            ),
            stealth_patches: true,
            fallback_chain: vec![
                "json_endpoint".to_string(),
                "html_meta".to_string(),
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
        let username = query.trim_start_matches('@').replace(' ', "");

        // Method 1: semi-public JSON endpoint the web app uses.
        let json_url = format!(
            "https://www.instagram.com/api/v1/users/web_profile_info/?username={}",
            username
        );
        if let Ok(f) = html::get_html(ctx, &json_url).await {
            if f.status == 200 && !f.body.is_empty() {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&f.body) {
                    if let Some(user) = json.get("data").and_then(|d| d.get("user")) {
                        if !user.is_null() {
                            record.raw_capture = Some(f.blob.hash);
                            record.candidates.push(ScrapeCandidate::new(user.clone(), 0.75));
                            record.status = ScrapeStatus::Ok;
                            record.notes = "via json_endpoint".to_string();
                            return Ok(record);
                        }
                    }
                }
            } else if f.status == 429 {
                record.status = ScrapeStatus::RateLimited;
                return Ok(record);
            }
        }

        // Method 2: HTML meta (OG tags) on the profile page.
        let profile_url = format!("https://www.instagram.com/{}/", username);
        if let Ok(f) = html::get_html(ctx, &profile_url).await {
            if f.status == 200 && !f.body.is_empty() {
                record.raw_capture = Some(f.blob.hash);
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

        // Method 3: headless browser — honest Blocked if unavailable.
        match browser::render_via_ctx(ctx, &profile_url, 5000).await {
            Ok(page) => {
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
