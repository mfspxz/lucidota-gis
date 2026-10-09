//! TIN_028 (id `TIN_028_411`) — Canada411 reverse phone lookup.
//!
//! Rust port of `TIN_028_canada411.py` (static-html archetype). Reverse-lookup a
//! phone number on canada411.ca. The Python normalized to digits, required >=10,
//! then selected `.c411Listing, .listing__item` and pulled name/address. Canada411
//! actively blocks scraping — HTTP 403 degrades to Blocked, per the source's note.
//!
//! Collision-safe id `TIN_028_411` (TIN_028_canada_post_fsa already ported).

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct Canada411;

#[async_trait]
impl Scraper for Canada411 {
    fn id(&self) -> &'static str {
        "TIN_028_411"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_028_411".to_string(),
            source_label: "Canada411 (reverse phone lookup)".to_string(),
            domain: Domain::General,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Reverse-lookup a phone number on Canada411 (name/address; often blocked)."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.canada411.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(2000),
            max_results: None,
            max_pages: None,
            notes: "Canada411 — actively blocks scraping; expect 403 (degrades to Blocked)."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        selectors.insert(
            "listing".to_string(),
            ".c411Listing, .listing__item".to_string(),
        );
        selectors.insert(
            "name".to_string(),
            ".listing__name, .c411ListedName".to_string(),
        );
        selectors.insert(
            "address".to_string(),
            ".listing__address, .c411Address".to_string(),
        );
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://www.canada411.ca/search/reverse.html?phoneNumber={{query}}"
                .to_string(),
            selectors,
            regex_fallback: Default::default(),
            encoding: None,
            auth: Default::default(),
        })
    }

    async fn run(&self, ctx: &ScrapeCtx, params: ScrapeParams) -> anyhow::Result<ScrapeRecord> {
        let meta = self.meta();
        let mut record = ScrapeRecord::start(&meta, &params);

        let query = match params.query() {
            Some(q) if !q.is_empty() => q.to_string(),
            _ => {
                record.status = ScrapeStatus::Error("missing 'query' param (phone number)".to_string());
                return Ok(record);
            }
        };

        // Mirror the Python: keep digits only, require >= 10.
        let phone: String = query.chars().filter(|c| c.is_ascii_digit()).collect();
        if phone.len() < 10 {
            record.status =
                ScrapeStatus::Error("phone number must be at least 10 digits".to_string());
            return Ok(record);
        }

        let url = format!(
            "https://www.canada411.ca/search/reverse.html?phoneNumber={}",
            phone
        );
        let fetch = html::get_html(ctx, &url).await?;
        record.raw_capture = Some(fetch.blob.hash);

        if fetch.status != 200 {
            record.status = match fetch.status {
                429 => ScrapeStatus::RateLimited,
                403 => ScrapeStatus::Blocked(
                    "Canada411 blocking automated access (403)".to_string(),
                ),
                s => ScrapeStatus::Error(format!("HTTP {s}")),
            };
            return Ok(record);
        }

        let cfg_selectors: &[(&str, &str)] = &[
            ("listing", ".c411Listing, .listing__item"),
            ("name", ".listing__name, .c411ListedName"),
            ("address", ".listing__address, .c411Address"),
        ];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.75);
        record.notes = format!("{} extracted fields (phone={phone})", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
