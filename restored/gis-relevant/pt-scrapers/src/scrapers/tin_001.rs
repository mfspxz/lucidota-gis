//! TIN_001 — BCFSA Licensee Lookup (BC Financial Services Authority).
//!
//! Rust port of `TIN_001_bcfsa.py` (browser archetype). BCFSA's licensee lookup
//! is a React SPA; the Python hits the underlying `/api/licensee-lookup` JSON
//! endpoint first, then notes Playwright is required. This port tries the JSON
//! endpoint on the cheap lane, then escalates to the headless browser lane on the
//! SPA page — honest Blocked when the browser is unavailable.

use crate::controls::{BrowserConfig, LaneConfig, ScraperLimits};
use crate::lanes::{browser, html};
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct Bcfsa;

#[async_trait]
impl Scraper for Bcfsa {
    fn id(&self) -> &'static str {
        "TIN_001"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_001".to_string(),
            source_label: "BCFSA Licensee Lookup".to_string(),
            domain: Domain::Social,
            archetype: Archetype::Browser,
            auth_required: false,
            auth_hint: None,
            description: "Check BC Financial Services Authority licensee database for a name/company (React SPA + JSON API).".to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.bcfsa.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(3000),
            max_results: None,
            max_pages: None,
            notes: "React SPA; underlying /api/licensee-lookup JSON endpoint may 4xx and require browser rendering. Falls back to browser lane.".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        LaneConfig::Browser(BrowserConfig {
            url_template: "https://www.bcfsa.ca/en/licensee-lookup".to_string(),
            wait_ms: Some(5000),
            stealth_user_agent: Some(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
                    .to_string(),
            ),
            stealth_patches: true,
            fallback_chain: vec![
                "json_endpoint".to_string(),
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

        // Cheap lane: the semi-public JSON endpoint the SPA calls.
        let api_url = format!(
            "https://www.bcfsa.ca/api/licensee-lookup?q={}",
            query.replace(' ', "%20")
        );
        if let Ok(f) = html::get_html(ctx, &api_url).await {
            record.raw_capture = Some(f.blob.hash);
            if f.status == 200 && !f.body.is_empty() {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&f.body) {
                    match &json {
                        serde_json::Value::Array(items) if !items.is_empty() => {
                            for item in items.iter().take(50) {
                                record
                                    .candidates
                                    .push(ScrapeCandidate::new(item.clone(), 0.7));
                            }
                            record.status = ScrapeStatus::Ok;
                            record.notes =
                                format!("via json_endpoint ({} record(s))", record.candidates.len());
                            return Ok(record);
                        }
                        serde_json::Value::Array(_) => {
                            record.status = ScrapeStatus::NotFound;
                            record.notes = "json_endpoint returned empty array (no licence)".to_string();
                            return Ok(record);
                        }
                        serde_json::Value::Object(_) => {
                            record.candidates.push(ScrapeCandidate::new(json.clone(), 0.6));
                            record.status = ScrapeStatus::Ok;
                            record.notes = "via json_endpoint (object)".to_string();
                            return Ok(record);
                        }
                        _ => {}
                    }
                }
            } else if f.status == 429 {
                record.status = ScrapeStatus::RateLimited;
                return Ok(record);
            }
        }

        // Escalate to the browser lane on the SPA page — honest Blocked if absent.
        let spa_url = "https://www.bcfsa.ca/en/licensee-lookup";
        match browser::render_via_ctx(ctx, spa_url, 5000).await {
            Ok(page) => {
                let sels: &[(&str, &str)] = &[
                    ("title", "meta[property=\"og:title\"]"),
                    ("description", "meta[property=\"og:description\"]"),
                ];
                record.candidates = html::extract(&page.html, sels, 0.6);
                record.status = if record.candidates.is_empty() {
                    ScrapeStatus::NotFound
                } else {
                    ScrapeStatus::Ok
                };
                record.notes = "via browser (SPA render)".to_string();
            }
            Err(e) => {
                record.status = ScrapeStatus::Blocked(e.to_string());
            }
        }
        Ok(record)
    }
}
