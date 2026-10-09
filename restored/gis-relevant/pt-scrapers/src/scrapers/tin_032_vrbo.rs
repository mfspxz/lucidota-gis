//! TIN_032_vrbo — VRBO / HomeAway Listing Search.
//!
//! Rust port of `TIN_032_vrbo.py` (browser archetype). VRBO renders client-side
//! and blocks scraping (403). The Python fetches `/search?q=...` and counts
//! listing markers (`"listingId"`, `"propertyId"`, `data-stid="property-listing"`).
//! This port counts markers on the cheap lane, then escalates to the browser lane
//! — honest Blocked on 403 / when the browser is unavailable.

use crate::controls::{BrowserConfig, LaneConfig, ScraperLimits};
use crate::lanes::{browser, html};
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct Vrbo;

fn count_markers(body: &str) -> usize {
    body.matches("\"listingId\"").count()
        + body.matches("\"propertyId\"").count()
        + body.matches("data-stid=\"property-listing\"").count()
}

#[async_trait]
impl Scraper for Vrbo {
    fn id(&self) -> &'static str {
        "TIN_032_vrbo"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_032_vrbo".to_string(),
            source_label: "VRBO / HomeAway (listing search)".to_string(),
            domain: Domain::Social,
            archetype: Archetype::Browser,
            auth_required: false,
            auth_hint: None,
            description: "Search VRBO/HomeAway for vacation-rental listings by location (aggressive anti-bot; JS-rendered).".to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.vrbo.com")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(5000),
            max_results: None,
            max_pages: None,
            notes: "VRBO blocks scraping aggressively (403). Client-side rendering; static HTML only exposes listing markers.".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        LaneConfig::Browser(BrowserConfig {
            url_template: "https://www.vrbo.com/search?q={{query}}".to_string(),
            wait_ms: Some(6000),
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
        let url = format!("https://www.vrbo.com/search?q={}", query.replace(' ', "%20"));

        // Cheap lane: count listing markers in the static HTML.
        if let Ok(f) = html::get_html(ctx, &url).await {
            record.raw_capture = Some(f.blob.hash);
            match f.status {
                200 if !f.body.is_empty() => {
                    let n = count_markers(&f.body);
                    if n > 0 {
                        record.candidates.push(ScrapeCandidate::new(
                            serde_json::json!({ "listing_markers": n, "query": query }),
                            0.5,
                        ));
                        record.status = ScrapeStatus::Ok;
                        record.notes =
                            format!("via html: ~{n} listing marker(s) (full extraction needs JS render)");
                        return Ok(record);
                    }
                }
                403 => {
                    record.status = ScrapeStatus::Blocked("VRBO returned 403 (anti-bot)".to_string());
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
        match browser::render_via_ctx(ctx, &url, 6000).await {
            Ok(page) => {
                let n = count_markers(&page.html);
                if n > 0 {
                    record.candidates.push(ScrapeCandidate::new(
                        serde_json::json!({ "listing_markers": n, "query": query }),
                        0.6,
                    ));
                    record.status = ScrapeStatus::Ok;
                    record.notes = format!("via browser: ~{n} listing marker(s)");
                } else {
                    record.status = ScrapeStatus::NotFound;
                    record.notes = "via browser: no listing markers".to_string();
                }
            }
            Err(e) => {
                record.status = ScrapeStatus::Blocked(e.to_string());
            }
        }
        Ok(record)
    }
}
