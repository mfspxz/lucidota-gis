//! TIN_027 (id `TIN_027_ppr`) — BC Personal Property Registry search.
//!
//! Rust port of `TIN_027_bc_ppr.py` (static-html archetype). Searches BC PPR for
//! liens/security interests by entity name. The Python selected `table.results tr`
//! and read each row's `<td>` cells. NOTE: BC PPR has no real public API and often
//! requires an authenticated session — the source itself warned HTTP failures are
//! expected; those degrade to Blocked, not Error.
//!
//! Collision-safe id `TIN_027_ppr` (the sibling ports TIN_027_bc_courts).

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

const PPR_URL: &str = "https://www.bcregistryservices.gov.bc.ca/bcreg/ppr";

pub struct BcPpr;

#[async_trait]
impl Scraper for BcPpr {
    fn id(&self) -> &'static str {
        "TIN_027_ppr"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_027_ppr".to_string(),
            source_label: "BC Personal Property Registry".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Search BC PPR for liens/security interests by entity name (best-effort)."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.bcregistryservices.gov.bc.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(1000),
            max_results: None,
            max_pages: None,
            notes: "BC PPR — no public API; often requires an authenticated session. HTTP \
                    failures are expected and degrade to Blocked."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut selectors = std::collections::BTreeMap::new();
        selectors.insert("result_row".to_string(), "table.results tr".to_string());
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://www.bcregistryservices.gov.bc.ca/bcreg/ppr?searchType=individual&name={{query}}".to_string(),
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
                record.status = ScrapeStatus::Error("missing 'query' param (entity name)".to_string());
                return Ok(record);
            }
        };

        let url = format!(
            "{}?searchType=individual&name={}",
            PPR_URL,
            query.replace(' ', "+")
        );
        let fetch = html::get_html(ctx, &url).await?;
        record.raw_capture = Some(fetch.blob.hash);

        if fetch.status != 200 {
            record.status = match fetch.status {
                429 => ScrapeStatus::RateLimited,
                403 | 401 => ScrapeStatus::Blocked(
                    "BC PPR requires authenticated session (manual verification recommended)"
                        .to_string(),
                ),
                s => ScrapeStatus::Blocked(format!(
                    "BC PPR HTTP {s} — likely requires browser session"
                )),
            };
            return Ok(record);
        }

        let cfg_selectors: &[(&str, &str)] = &[("result_row", "table.results tr")];
        record.candidates = html::extract(&fetch.body, cfg_selectors, 0.8);
        record.notes = format!("{} PPR registration row(s)", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
