//! TIN_031_airbnb — Airbnb Listing Search.
//!
//! Rust port of `TIN_031_airbnb.py` (browser archetype). Airbnb renders listings
//! client-side and aggressively blocks scrapers (403). The Python fetches the
//! search page and counts `"listingId"` markers in the static HTML. This port
//! fetches the search page on the cheap lane and counts listing markers, then
//! escalates to the browser lane for a JS render — honest Blocked on 403 / when
//! the browser is unavailable.

use crate::controls::{BrowserConfig, LaneConfig, ScraperLimits};
use crate::lanes::{browser, html};
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct Airbnb;

fn count_markers(body: &str) -> usize {
    body.matches("\"listingId\"").count()
}

#[async_trait]
impl Scraper for Airbnb {
    fn id(&self) -> &'static str {
        "TIN_031_airbnb"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_031_airbnb".to_string(),
            source_label: "Airbnb (listing search)".to_string(),
            domain: Domain::Social,
            archetype: Archetype::Browser,
            auth_required: false,
            auth_hint: None,
            description: "Search Airbnb for short-term rental listings by location/address (aggressive anti-bot; JS-rendered).".to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.airbnb.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(5000),
            max_results: None,
            max_pages: None,
            notes: "Airbnb aggressively blocks scraping (403). Client-side rendering; static HTML only exposes listing markers.".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        LaneConfig::Browser(BrowserConfig {
            url_template: "https://www.airbnb.ca/s/{{query}}/homes".to_string(),
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
        let url = format!(
            "https://www.airbnb.ca/s/{}/homes",
            query.replace(' ', "%20")
        );

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
                    record.status = ScrapeStatus::Blocked("Airbnb returned 403 (anti-bot)".to_string());
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
