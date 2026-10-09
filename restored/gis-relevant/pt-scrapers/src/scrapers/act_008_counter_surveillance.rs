//! ACT_008 — Counter-Surveillance Toolkit (honest non-harvest stub).
//!
//! Rust port of `ACT_008_counter_surveillance.py`. The original is an in-memory
//! toolkit for vehicle-plate logging, a surveillance-sighting database, repeat-
//! sighting correlation, and IMSI-catcher detection indicators. It makes NO HTTP
//! calls — it is a management/analysis module, NOT a network harvester. This stub
//! declares that truth via `ScrapeStatus::Blocked` and never fabricates data.

use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct CounterSurveillance;

#[async_trait]
impl Scraper for CounterSurveillance {
    fn id(&self) -> &'static str {
        "ACT_008"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "ACT_008".to_string(),
            source_label: "Counter-Surveillance Toolkit (analysis module)".to_string(),
            domain: Domain::Activist,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "In-memory sighting database with plate logging, repeat-sighting correlation, and IMSI-catcher indicators — NOT a network harvester.".to_string(),
        }
    }

    fn limits(&self) -> crate::controls::ScraperLimits {
        crate::controls::ScraperLimits {
            max_concurrency: None,
            min_delay_ms: None,
            max_results: None,
            max_pages: None,
            notes: "Non-harvest analysis module; no network.".to_string(),
        }
    }

    async fn run(&self, _ctx: &ScrapeCtx, params: ScrapeParams) -> anyhow::Result<ScrapeRecord> {
        let meta = self.meta();
        let mut record = ScrapeRecord::start(&meta, &params);
        record.status = ScrapeStatus::Blocked("management/analysis module, not a network harvester — belongs in the pt-* management layer, not the harvest fleet".to_string());
        record.notes = "Non-harvest stub: original is an in-memory sighting database. No data fabricated.".to_string();
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn is_honest_non_harvest_stub() {
        let rec = CounterSurveillance
            .run(&ScrapeCtx::new_default().unwrap(), ScrapeParams::from_query("x"))
            .await
            .unwrap();
        assert!(matches!(rec.status, ScrapeStatus::Blocked(_)));
        assert!(rec.candidates.is_empty());
    }
}
