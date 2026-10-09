//! ACT_002 — NLG Legal Observer Tracker (honest non-harvest stub).
//!
//! Rust port of `ACT_002_legal_observer.py`. The original maintains an in-memory
//! directory of National Lawyers Guild (NLG) chapter contacts, formats legal-
//! observer request messages, tracks per-event deployment status, and generates
//! readiness reports. It makes NO HTTP calls — it is a structured data-management
//! layer. This stub declares that truth via `ScrapeStatus::Blocked`.

use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct LegalObserver;

#[async_trait]
impl Scraper for LegalObserver {
    fn id(&self) -> &'static str {
        "ACT_002"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "ACT_002".to_string(),
            source_label: "NLG Legal Observer Tracker (management module)".to_string(),
            domain: Domain::Activist,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "In-memory legal-observer chapter directory and per-event request tracker — NOT a network harvester.".to_string(),
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
        let rec = LegalObserver
            .run(&ScrapeCtx::new_default().unwrap(), ScrapeParams::from_query("x"))
            .await
            .unwrap();
        assert!(matches!(rec.status, ScrapeStatus::Blocked(_)));
        assert!(rec.candidates.is_empty());
    }
}
