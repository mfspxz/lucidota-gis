//! ACT_003 — FOIA/ATIP Request Tracker (honest non-harvest stub).
//!
//! Rust port of `ACT_003_foia_tracker.py`. The original tracks filed information
//! requests (FOIA in the US, ATIP/FOI in Canada), expected response and follow-up
//! deadlines, and parses response documents for key findings. It is an in-memory
//! tracker, NOT a network harvester. This stub declares that truth via
//! `ScrapeStatus::Blocked` and never fabricates data.

use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct FoiaTracker;

#[async_trait]
impl Scraper for FoiaTracker {
    fn id(&self) -> &'static str {
        "ACT_003"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "ACT_003".to_string(),
            source_label: "FOIA/ATIP Request Tracker (management module)".to_string(),
            domain: Domain::Activist,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "In-memory FOIA/ATIP request tracker with statutory-deadline and response tracking — NOT a network harvester.".to_string(),
        }
    }

    fn limits(&self) -> crate::controls::ScraperLimits {
        crate::controls::ScraperLimits {
            max_concurrency: None,
            min_delay_ms: None,
            max_results: None,
            max_pages: None,
            notes: "Non-harvest management module; no network.".to_string(),
        }
    }

    async fn run(&self, _ctx: &ScrapeCtx, params: ScrapeParams) -> anyhow::Result<ScrapeRecord> {
        let meta = self.meta();
        let mut record = ScrapeRecord::start(&meta, &params);
        record.status = ScrapeStatus::Blocked("management/analysis module, not a network harvester — belongs in the pt-* management layer, not the harvest fleet".to_string());
        record.notes = "Non-harvest stub: original is an in-memory tracker. No data fabricated.".to_string();
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn is_honest_non_harvest_stub() {
        let rec = FoiaTracker
            .run(&ScrapeCtx::new_default().unwrap(), ScrapeParams::from_query("x"))
            .await
            .unwrap();
        assert!(matches!(rec.status, ScrapeStatus::Blocked(_)));
        assert!(rec.candidates.is_empty());
    }
}
