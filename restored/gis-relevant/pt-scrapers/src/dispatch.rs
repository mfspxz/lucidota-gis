//! Dispatch core — the single entry point all surfaces (CLI/REST/MCP) route through.

use crate::registry::ScraperRegistry;
use crate::types::{ScrapeCtx, ScrapeParams, ScrapeRecord};

/// Runtime enablement policy: which compiled scrapers are actually live.
#[derive(Debug, Clone)]
pub enum Enablement {
    /// Every compiled scraper is live.
    All,
    /// Only the listed ids are live.
    Only(Vec<String>),
}

impl Enablement {
    pub fn is_enabled(&self, id: &str) -> bool {
        match self {
            Enablement::All => true,
            Enablement::Only(ids) => ids.iter().any(|x| x == id),
        }
    }
}

impl Default for Enablement {
    fn default() -> Self {
        Enablement::All
    }
}

/// The single dispatch function. Resolves `id` in the registry, checks runtime
/// enablement, and runs the scraper against `params` using the shared `ctx`.
///
/// Never panics. Unknown id or disabled scraper → a `ScrapeRecord` with an
/// Error/Blocked status, never a fabricated result.
pub async fn dispatch(
    registry: &ScraperRegistry,
    enablement: &Enablement,
    ctx: &ScrapeCtx,
    id: &str,
    params: ScrapeParams,
) -> ScrapeRecord {
    let scraper = match registry.get(id) {
        Some(s) => s,
        None => {
            return ScrapeRecord::errored(id, format!("no such scraper: {id} (not compiled in this build)"));
        }
    };

    if !enablement.is_enabled(id) {
        let meta = scraper.meta();
        return ScrapeRecord::blocked(&meta, &params, "scraper disabled by runtime enablement config");
    }

    // Also honor the config-file enablement layer ([enablement] in scrapers.toml).
    if !ctx.config.is_enabled(id) {
        let meta = scraper.meta();
        return ScrapeRecord::blocked(&meta, &params, "scraper disabled by scrapers.toml [enablement]");
    }

    // Resolve controls (layered + clamped to declared limits) and configure the
    // governor for this scraper's domain FROM those controls — before the run.
    // This is where "nothing is automatic" is enforced: the governor's concurrency
    // and spacing come from the resolved config, not a fixed default.
    let limits = scraper.limits();
    let domain = scraper
        .primary_domain()
        .map(|d| d.to_string())
        .unwrap_or_default();
    if !domain.is_empty() {
        let _controls = ctx.prepare(id, &domain, &limits);
    }

    let start = std::time::Instant::now();
    match scraper.run(ctx, params).await {
        Ok(mut record) => {
            record.elapsed_ms = start.elapsed().as_millis() as u64;
            record
        }
        Err(e) => {
            let mut record = ScrapeRecord::errored(id, e.to_string());
            record.elapsed_ms = start.elapsed().as_millis() as u64;
            record
        }
    }
}
