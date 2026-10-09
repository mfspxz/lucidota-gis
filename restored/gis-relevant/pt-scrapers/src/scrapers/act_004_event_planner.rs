//! ACT_004 — Event / Production Planner (honest non-harvest stub).
//!
//! Rust port of `ACT_004_event_planner.py`. The original coordinates event
//! logistics in-memory: weather checks, venue availability, route planning,
//! equipment inventory, and crew roster management, emitting readiness signals.
//! It is a planning/management module, NOT a network harvester. This stub
//! declares that truth via `ScrapeStatus::Blocked` and never fabricates data.

use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct EventPlanner;

#[async_trait]
impl Scraper for EventPlanner {
    fn id(&self) -> &'static str {
        "ACT_004"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "ACT_004".to_string(),
            source_label: "Event / Production Planner (management module)".to_string(),
            domain: Domain::Activist,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "In-memory event-logistics planner (weather, venue, route, equipment, crew) — NOT a network harvester.".to_string(),
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
        record.notes = "Non-harvest stub: original is an in-memory planner. No data fabricated.".to_string();
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn is_honest_non_harvest_stub() {
        let rec = EventPlanner
            .run(&ScrapeCtx::new_default().unwrap(), ScrapeParams::from_query("x"))
            .await
            .unwrap();
        assert!(matches!(rec.status, ScrapeStatus::Blocked(_)));
        assert!(rec.candidates.is_empty());
    }
}
