//! TIN_007 — LTSA Title Search (STUB, honest-blocked).
//!
//! Rust port of `TIN_007_ltsa.py`. The BC Land Title & Survey Authority requires
//! a PAID myLTSA account (~$12-15 per title search). The Python original raises
//! NotImplementedError on purpose. This port faithfully returns
//! `ScrapeStatus::Blocked` with the reason — honest degradation, never a fake
//! result. Wire real credentials only when a funded LTSA account is confirmed.

use crate::types::{
    Archetype, Domain, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus, Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct LtsaTitle;

#[async_trait]
impl Scraper for LtsaTitle {
    fn id(&self) -> &'static str {
        "TIN_007"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_007".to_string(),
            source_label: "BC LTSA Title Search (paid account required)".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::ApiJson,
            auth_required: true,
            auth_hint: Some("LTSA_MYLTSA_ACCOUNT (paid, ~$12-15/search)".to_string()),
            description: "Land Title search by PID — BLOCKED until a funded myLTSA account is wired."
                .to_string(),
        }
    }

    fn limits(&self) -> crate::controls::ScraperLimits {
        crate::controls::ScraperLimits {
            max_concurrency: Some(1),
            min_delay_ms: Some(2000),
            max_results: None,
            max_pages: None,
            notes: "Paid per-search source. Deliberately blocked until account funded (see CLAUDE.md)."
                .to_string(),
        }
    }

    async fn run(&self, _ctx: &ScrapeCtx, params: ScrapeParams) -> anyhow::Result<ScrapeRecord> {
        let meta = self.meta();
        let mut record = ScrapeRecord::start(&meta, &params);
        // Honest degradation: the source requires a paid, funded account we do not
        // have. Never fabricate a title result. Mirrors the Python NotImplementedError.
        record.status = ScrapeStatus::Blocked(
            "LTSA requires a paid, funded myLTSA account — not wired (see CLAUDE.md blockers)"
                .to_string(),
        );
        record.notes = "STUB — port intentionally blocked; no title data fabricated.".to_string();
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ltsa_is_honestly_blocked() {
        let s = LtsaTitle;
        let ctx = ScrapeCtx::new_default().unwrap();
        let rec = s
            .run(&ctx, ScrapeParams::from_query("012-345-678"))
            .await
            .unwrap();
        assert!(matches!(rec.status, ScrapeStatus::Blocked(_)));
        assert!(rec.candidates.is_empty(), "must NOT fabricate title data");
    }
}
