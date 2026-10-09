//! TIN_033_zillow — Zillow / Zumper Listing Search.
//!
//! Rust port of `TIN_033_zillow.py` (browser archetype). Both platforms block
//! scraping. The Python fetches Zumper (`/apartments-for-rent/{slug}`) and
//! selects `[class*="ListingCard"]`, and fetches Zillow (`/homes/{query}`) and
//! counts `"zpid"` markers (403 = bot detection). This port tries Zumper card
//! extraction and Zillow zpid counting on the cheap lane, then escalates Zumper
//! to the browser lane — honest Blocked on 403 / when the browser is unavailable.

use crate::controls::{BrowserConfig, LaneConfig, ScraperLimits};
use crate::lanes::{browser, html};
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct Zillow;

#[async_trait]
impl Scraper for Zillow {
    fn id(&self) -> &'static str {
        "TIN_033_zillow"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_033_zillow".to_string(),
            source_label: "Zillow / Zumper (rental listings)".to_string(),
            domain: Domain::Social,
            archetype: Archetype::Browser,
            auth_required: false,
            auth_hint: None,
            description: "Search Zillow + Zumper for rental/sale listings by address/area (both block scraping; JS-rendered).".to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.zumper.com")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(5000),
            max_results: Some(20),
            max_pages: None,
            notes: "Zillow (403 bot detection) and Zumper both block scraping. Zillow only exposes zpid markers in static HTML.".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        LaneConfig::Browser(BrowserConfig {
            url_template: "https://www.zumper.com/apartments-for-rent/{{query}}".to_string(),
            wait_ms: Some(6000),
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

        let zumper_slug = query.to_lowercase().replace(", ", "-").replace(',', "").replace(' ', "-");
        let zumper_url = format!("https://www.zumper.com/apartments-for-rent/{}", zumper_slug);
        let zillow_url = format!("https://www.zillow.com/homes/{}", query.replace(' ', "%20"));

        let zumper_sels: &[(&str, &str)] = &[
            ("title", "[class*=\"ListingCard\"] h3, [class*=\"listing-card\"] h3, [class*=\"title\"]"),
            ("price", "[class*=\"price\"]"),
        ];

        let mut ok = false;

        // --- Zumper (cheap lane) ---
        if let Ok(f) = html::get_html(ctx, &zumper_url).await {
            record.raw_capture = Some(f.blob.hash);
            if f.status == 200 && !f.body.is_empty() {
                let cands = html::extract(&f.body, zumper_sels, 0.6);
                if !cands.is_empty() {
                    record.candidates.extend(cands);
                    ok = true;
                }
            }
        }

        // --- Zillow (cheap lane): count zpid markers ---
        if let Ok(f) = html::get_html(ctx, &zillow_url).await {
            if f.status == 200 && !f.body.is_empty() {
                let n = f.body.matches("\"zpid\"").count();
                if n > 0 {
                    record.candidates.push(ScrapeCandidate::new(
                        serde_json::json!({ "source": "zillow", "zpid_markers": n, "query": query }),
                        0.5,
                    ));
                    ok = true;
                }
            } else if f.status == 403 {
                record.notes = "Zillow: HTTP 403 (bot detection)".to_string();
            }
        }

        if ok {
            record.status = ScrapeStatus::Ok;
            if record.notes.is_empty() {
                record.notes = format!("via html: {} candidate(s)", record.candidates.len());
            }
            return Ok(record);
        }

        // Escalate Zumper to the browser lane — honest Blocked if unavailable.
        match browser::render_via_ctx(ctx, &zumper_url, 6000).await {
            Ok(page) => {
                record.candidates = html::extract(&page.html, zumper_sels, 0.65);
                record.status = if record.candidates.is_empty() {
                    ScrapeStatus::NotFound
                } else {
                    ScrapeStatus::Ok
                };
                record.notes = "via browser (Zumper)".to_string();
            }
            Err(e) => {
                record.status = ScrapeStatus::Blocked(e.to_string());
            }
        }
        Ok(record)
    }
}
