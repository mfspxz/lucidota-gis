//! pt-scrapers — the standardized scraper fleet crate.
//!
//! Every scraper is an "organelle": one module + one cargo feature, all behind a
//! single [`Scraper`] trait returning a uniform [`ScrapeRecord`]. Ship only what
//! a deployment needs via feature groups (`festival`, `bc-osint`, `finance`,
//! `academic-integrity`, `activist`, `social`, `full`).
//!
//! Three operation surfaces (CLI, REST, MCP) all route through one
//! [`dispatch::dispatch`] core. Nothing here promotes candidates to Proven —
//! that stays in `pt-oracle`. Scrapers are `custody_writer` (raw bytes → CAS)
//! and `candidate_writer` (extracted fields stay Unknown until gated).
//!
//! ```text
//!   Scraper trait  ──┐
//!   ScrapeRecord     ├── dispatch(registry, enablement, ctx, id, params)
//!   ScrapeCtx      ──┘        │
//!   lanes: api/html/feed/browser
//! ```

pub mod adversarial;
pub mod controls;
pub mod dispatch;
pub mod governor;
pub mod lanes;
pub mod registry;
pub mod scrapers;
pub mod types;

// Re-exports for ergonomic consumers (pt-cli, pt-api, pt-mcp, pt-trufflepig).
pub use controls::{
    ApiConfig, BrowserConfig, ControlConfig, FeedConfig, HtmlConfig, LaneConfig,
    ScraperControls, ScraperLimits,
};
pub use dispatch::{dispatch, Enablement};
pub use governor::Governor;
pub use registry::{build_registry, ScraperRegistry};
pub use types::{
    Archetype, Domain, Scraper, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord,
    ScrapeStatus, ScraperMeta, StealthPolicy, StealthProfile,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_build_has_expected_registry() {
        // The registry only contains scrapers whose feature is compiled in.
        let reg = build_registry();
        // With bc-osint enabled, TIN_002 is present; without it, absent.
        #[cfg(feature = "bc-osint")]
        assert!(reg.get("TIN_002").is_some(), "TIN_002 should be registered with bc-osint");
        #[cfg(not(feature = "bc-osint"))]
        assert!(reg.get("TIN_002").is_none(), "TIN_002 must be absent without bc-osint");
        let _ = reg.len();
    }

    #[tokio::test]
    async fn dispatch_unknown_id_is_error_not_panic() {
        let reg = build_registry();
        let ctx = ScrapeCtx::new_default().unwrap();
        let rec = dispatch(&reg, &Enablement::All, &ctx, "TIN_999", ScrapeParams::default()).await;
        assert!(matches!(rec.status, ScrapeStatus::Error(_)));
    }

    #[tokio::test]
    async fn browser_scraper_without_browser_lane_blocks() {
        // A ctx with no browser handle must yield a clear error path, never panic.
        let ctx = ScrapeCtx::new_default().unwrap();
        assert!(ctx.browser.is_none());
        let res = lanes::browser::render_via_ctx(&ctx, "https://example.com", 100).await;
        assert!(res.is_err(), "render without a browser handle must error, not succeed");
    }

    #[tokio::test]
    async fn governor_caps_concurrency_through_ctx() {
        // The politeness law: N concurrent acquires against the SAME host must
        // serialize into batches, not all pass at once. A flat per-call sleep
        // would let all 4 through in parallel (~150ms); the governor forces >=2
        // batches. This proves ScrapeCtx wires the real semaphore governor.
        use std::sync::Arc;
        use std::time::Instant;

        let mut ctx = ScrapeCtx::new_default().unwrap();
        let gov = Arc::new(governor::Governor::with_defaults(2, 50));
        gov.configure("host.example", 2, 50);
        ctx.governor = gov;
        let ctx = Arc::new(ctx);

        let start = Instant::now();
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let ctx = ctx.clone();
                tokio::spawn(async move {
                    ctx.acquire_for("https://host.example/x").await;
                })
            })
            .collect();
        for h in handles {
            h.await.unwrap();
        }
        // 4 acquires, cap 2, 50ms hold each => 2 batches => >=100ms.
        assert!(
            start.elapsed().as_millis() >= 100,
            "governor did not serialize per-domain concurrency"
        );
    }

    #[tokio::test]
    async fn config_drives_governor_via_prepare() {
        // Prove the operator's control config actually configures the governor.
        // scrapers.toml sets concurrency=1 for host.example; prepare() must apply it.
        use crate::controls::{ControlConfig, ControlOverride, ScraperLimits};
        use std::sync::Arc;
        use std::time::Instant;

        let mut cfg = ControlConfig::default();
        cfg.scraper.insert(
            "TIN_CFG".into(),
            ControlOverride {
                concurrency: Some(1),
                min_delay_ms: Some(50),
                ..Default::default()
            },
        );
        let ctx = Arc::new(ScrapeCtx::with_config(Arc::new(cfg)).unwrap());

        // prepare() resolves controls and configures the governor for the domain.
        let _c = ctx.prepare("TIN_CFG", "host.example", &ScraperLimits::default());

        let start = Instant::now();
        let handles: Vec<_> = (0..3)
            .map(|_| {
                let ctx = ctx.clone();
                tokio::spawn(async move {
                    ctx.acquire_for("https://host.example/x").await;
                })
            })
            .collect();
        for h in handles {
            h.await.unwrap();
        }
        // concurrency=1, 50ms hold, 3 acquires => 3 serial batches => >=150ms.
        assert!(
            start.elapsed().as_millis() >= 150,
            "config concurrency=1 did not serialize acquires through the governor"
        );
    }
}
