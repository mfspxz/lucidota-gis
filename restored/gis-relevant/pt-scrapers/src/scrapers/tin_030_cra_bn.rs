//! TIN_030 — CRA Business Number / GST registry lookup.
//!
//! Rust port of `TIN_030_cra_bn.py` (static-html archetype). Posts a business
//! number/name to the CRA GST/HST registry confirm endpoint and parses the
//! response. Python selects `.info, .result-details, td` and text-matches
//! "is a valid"/"registered". Uses collision-safe id "TIN_030_cra".
//!
//! NOTE: the Python uses an HTTP POST; the html lane here issues a GET with the
//! business number/name as a query param. The registry response body is still
//! captured for downstream text matching.

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct CraBn;

#[async_trait]
impl Scraper for CraBn {
    fn id(&self) -> &'static str {
        "TIN_030_cra"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_030_cra".to_string(),
            source_label: "CRA Business Number / GST registry".to_string(),
            domain: Domain::General,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Check CRA GST/HST registry for a business number or name registration status."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.businessregistration-inscriptionentreprise.gc.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(2000),
            max_results: None,
            max_pages: None,
            notes: "GST registry is POST-based in Python; GET fallback here. Selectors .info/.result-details/td (UNKNOWN confirm)."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        // From Python: soup.select(".info, .result-details, td"). Confirmation UNKNOWN.
        selectors.insert(
            "info".to_string(),
            ".info, .result-details, td".to_string(),
        );
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://www.businessregistration-inscriptionentreprise.gc.ca/ebci/brom/registry/registryConfirmSubmit.action?businessName={{query}}"
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
                record.status = ScrapeStatus::Error("missing 'query' param (BN or name)".to_string());
                return Ok(record);
            }
        };

        // Python distinguishes BN (>=9 digits) vs name; use businessNumber param when numeric.
        let digits: String = query.chars().filter(|c| c.is_ascii_digit()).collect();
        let base = "https://www.businessregistration-inscriptionentreprise.gc.ca/ebci/brom/registry/registryConfirmSubmit.action";
        let url = if digits.len() >= 9 {
            format!("{}?businessRegistrationNumber={}", base, &digits[..9])
        } else {
            format!("{}?businessName={}", base, query.replace(' ', "+"))
        };
        let fetch = html::get_html(ctx, &url).await?;
        record.raw_capture = Some(fetch.blob.hash);

        if fetch.status != 200 {
            record.status = match fetch.status {
                429 => ScrapeStatus::RateLimited,
                403 => ScrapeStatus::Blocked("403".to_string()),
                s => ScrapeStatus::Error(format!("HTTP {s}")),
            };
            return Ok(record);
        }

        let cfg_selectors: &[(&str, &str)] = &[("info", ".info, .result-details, td")];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.85);
        record.notes = format!("{} info field(s) extracted", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
