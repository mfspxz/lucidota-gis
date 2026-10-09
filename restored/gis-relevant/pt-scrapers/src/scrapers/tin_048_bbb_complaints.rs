//! TIN_048 — Better Business Bureau complaints.
//!
//! Rust port of `TIN_048_bbb_complaints.py` (static-html archetype). Searches BBB
//! for a business and extracts profile/complaint signals from the results HTML.
//! Selectors live in config; the html lane does the fetch + parse.

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct BbbComplaints;

#[async_trait]
impl Scraper for BbbComplaints {
    fn id(&self) -> &'static str {
        "TIN_048"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_048".to_string(),
            source_label: "Better Business Bureau (BC)".to_string(),
            domain: Domain::General,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Search BBB for a business profile, rating, and complaint signals."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.bbb.org")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(2),
            min_delay_ms: Some(2000),
            max_results: None,
            max_pages: None,
            notes: "BBB — public HTML; be polite (2s spacing). May serve interstitials."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        selectors.insert("business_name".to_string(), "span.bds-h4, h1".to_string());
        selectors.insert("rating".to_string(), "span.bbb-rating".to_string());
        selectors.insert("result".to_string(), "div.card.result-card".to_string());
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://www.bbb.org/search?find_country=CAN&find_text={{query}}"
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
                record.status = ScrapeStatus::Error("missing 'query' param".to_string());
                return Ok(record);
            }
        };

        let url = format!(
            "https://www.bbb.org/search?find_country=CAN&find_text={}",
            query.replace(' ', "+")
        );
        let fetch = html::get_html(ctx, &url).await?;
        record.raw_capture = Some(fetch.blob.hash);

        if fetch.status != 200 {
            record.status = match fetch.status {
                429 => ScrapeStatus::RateLimited,
                403 => ScrapeStatus::Blocked("BBB returned 403 (anti-bot interstitial)".to_string()),
                s => ScrapeStatus::Error(format!("HTTP {s}")),
            };
            return Ok(record);
        }

        let cfg_selectors: &[(&str, &str)] = &[
            ("business_name", "span.bds-h4, h1"),
            ("rating", "span.bbb-rating"),
            ("result", "div.card.result-card"),
        ];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.85);
        record.notes = format!("{} extracted fields", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
