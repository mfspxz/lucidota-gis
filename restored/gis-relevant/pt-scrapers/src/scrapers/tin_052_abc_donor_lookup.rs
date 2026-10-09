//! TIN_052 — ABC Vancouver Donor Lookup (honest stub, not yet wired).
//!
//! Rust port of `TIN_052_abc_donor_lookup.py`. The original intends to cross-
//! reference known developer entities against Elections BC campaign-finance
//! disclosures (POST-ELECTION REPORT: BC local election 2022) for ABC Vancouver,
//! via `https://efs.elections.bc.ca/`. That network path is NOT yet wired here,
//! so this stub returns `ScrapeStatus::Blocked` and never fabricates a result.

use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct AbcDonorLookup;

#[async_trait]
impl Scraper for AbcDonorLookup {
    fn id(&self) -> &'static str {
        "TIN_052"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_052".to_string(),
            source_label: "Elections BC — ABC Vancouver Donor Lookup".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Cross-references developer entities with Elections BC campaign-finance disclosures for ABC Vancouver — not yet wired to the live endpoint.".to_string(),
        }
    }

    fn limits(&self) -> crate::controls::ScraperLimits {
        crate::controls::ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(2000),
            max_results: None,
            max_pages: None,
            notes: "Elections BC EFS lookup not yet wired; deliberately blocked until implemented.".to_string(),
        }
    }

    async fn run(&self, _ctx: &ScrapeCtx, params: ScrapeParams) -> anyhow::Result<ScrapeRecord> {
        let meta = self.meta();
        let mut record = ScrapeRecord::start(&meta, &params);
        record.status = ScrapeStatus::Blocked("not yet wired to efs.elections.bc.ca donor endpoint".to_string());
        record.notes = "Stub — Elections BC donor lookup not yet implemented; no donor data fabricated.".to_string();
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn is_honest_non_harvest_stub() {
        let rec = AbcDonorLookup
            .run(&ScrapeCtx::new_default().unwrap(), ScrapeParams::from_query("South Seas Development"))
            .await
            .unwrap();
        assert!(matches!(rec.status, ScrapeStatus::Blocked(_)));
        assert!(rec.candidates.is_empty());
    }
}
