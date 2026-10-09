//! ACT_005 — Release Form Tracker (honest non-harvest stub).
//!
//! Rust port of `ACT_005_release_tracker.py`. The original tracks signed/unsigned
//! media release forms per event and participant, identifies coverage gaps, and
//! generates compliance reports. It is a self-contained in-memory tracking system
//! with NO external API calls. This stub declares that truth via
//! `ScrapeStatus::Blocked` and never fabricates data.

use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct ReleaseTracker;

#[async_trait]
impl Scraper for ReleaseTracker {
    fn id(&self) -> &'static str {
        "ACT_005"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "ACT_005".to_string(),
            source_label: "Release Form Tracker (management module)".to_string(),
            domain: Domain::Activist,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "In-memory media-release-form tracker with per-event compliance reporting — NOT a network harvester.".to_string(),
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
        let rec = ReleaseTracker
            .run(&ScrapeCtx::new_default().unwrap(), ScrapeParams::from_query("x"))
            .await
            .unwrap();
        assert!(matches!(rec.status, ScrapeStatus::Blocked(_)));
        assert!(rec.candidates.is_empty());
    }
}
