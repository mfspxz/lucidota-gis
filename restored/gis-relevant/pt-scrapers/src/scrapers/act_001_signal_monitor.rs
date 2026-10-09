//! ACT_001 — Signal Group Monitor (honest non-harvest stub).
//!
//! Rust port of `ACT_001_signal_monitor.py`. The original parses a LOCAL Signal
//! group-chat export (JSON/text) and scans for infiltrator / OPSEC-violation /
//! event-coordination keywords. It makes NO HTTP calls — it is a structured
//! data-extraction layer, not a network harvester. This stub declares that truth
//! via `ScrapeStatus::Blocked` and never fabricates data.

use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct SignalMonitor;

#[async_trait]
impl Scraper for SignalMonitor {
    fn id(&self) -> &'static str {
        "ACT_001"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "ACT_001".to_string(),
            source_label: "Signal Group Monitor (local-export analysis module)".to_string(),
            domain: Domain::Activist,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Parses a local Signal group-chat export for infiltrator/OPSEC keywords — NOT a network harvester.".to_string(),
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
        record.notes = "Non-harvest stub: original parses a local Signal export in-memory. No data fabricated.".to_string();
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn is_honest_non_harvest_stub() {
        let rec = SignalMonitor
            .run(&ScrapeCtx::new_default().unwrap(), ScrapeParams::from_query("x"))
            .await
            .unwrap();
        assert!(matches!(rec.status, ScrapeStatus::Blocked(_)));
        assert!(rec.candidates.is_empty());
    }
}
