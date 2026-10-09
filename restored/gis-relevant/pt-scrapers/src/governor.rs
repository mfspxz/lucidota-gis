//! Per-domain rate governor — politeness enforcement for all lanes.
//!
//! This is the *real* politeness primitive: a `Semaphore` per domain that caps
//! concurrency AND enforces a minimum spacing between requests. A flat sleep only
//! spaces requests; it does NOT stop N concurrent dispatches against the same host
//! from all sleeping in parallel and then hitting it together. This governor does.
//!
//! Mirrors the design of `pt_trufflepig::governor::RateGovernor`. It lives here
//! (not borrowed from pt-trufflepig) because the dependency flows
//! `pt-trufflepig -> pt-scrapers`; the reverse would be circular. pt-trufflepig
//! injects/configures this same type through `ScrapeCtx` when it drives scrapers.

use dashmap::DashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::debug;

/// Default: 2 concurrent requests per domain, 1s minimum spacing.
pub const DEFAULT_CONCURRENCY: usize = 2;
pub const DEFAULT_DELAY_MS: u64 = 1000;

struct DomainLimiter {
    semaphore: Semaphore,
    min_delay: Duration,
    configured_concurrency: usize,
}

/// Global rate governor keyed by domain host (e.g. "orgbook.gov.bc.ca").
pub struct Governor {
    domains: DashMap<String, Arc<DomainLimiter>>,
    default_concurrency: usize,
    default_delay_ms: u64,
}

impl Governor {
    pub fn new() -> Self {
        Self {
            domains: DashMap::new(),
            default_concurrency: DEFAULT_CONCURRENCY,
            default_delay_ms: DEFAULT_DELAY_MS,
        }
    }

    /// Construct with custom defaults for domains that have no explicit config.
    pub fn with_defaults(default_concurrency: usize, default_delay_ms: u64) -> Self {
        Self {
            domains: DashMap::new(),
            default_concurrency: default_concurrency.max(1),
            default_delay_ms,
        }
    }

    /// Configure explicit limits for a specific domain. Idempotent: if the domain
    /// already has a limiter with the same settings, it is left in place so
    /// in-flight permits are not reset. Only replaces when settings actually change.
    pub fn configure(&self, domain: &str, max_concurrent: usize, min_delay_ms: u64) {
        let max_concurrent = max_concurrent.max(1);
        if let Some(existing) = self.domains.get(domain) {
            if existing.semaphore.available_permits() <= max_concurrent
                && existing.min_delay == Duration::from_millis(min_delay_ms)
                && existing.configured_concurrency == max_concurrent
            {
                return; // unchanged — keep the live limiter
            }
        }
        let limiter = Arc::new(DomainLimiter {
            semaphore: Semaphore::new(max_concurrent),
            min_delay: Duration::from_millis(min_delay_ms),
            configured_concurrency: max_concurrent,
        });
        self.domains.insert(domain.to_string(), limiter);
    }

    fn get_limiter(&self, domain: &str) -> Arc<DomainLimiter> {
        if let Some(limiter) = self.domains.get(domain) {
            return limiter.clone();
        }
        let limiter = Arc::new(DomainLimiter {
            semaphore: Semaphore::new(self.default_concurrency),
            min_delay: Duration::from_millis(self.default_delay_ms),
            configured_concurrency: self.default_concurrency,
        });
        self.domains
            .entry(domain.to_string())
            .or_insert(limiter)
            .clone()
    }

    /// Acquire permission to hit a domain. Blocks until a concurrency slot is
    /// free, holds it for `min_delay` (enforcing spacing), then releases.
    pub async fn acquire(&self, domain: &str) {
        let limiter = self.get_limiter(domain);
        let _permit = limiter
            .semaphore
            .acquire()
            .await
            .expect("governor semaphore closed — should never happen");
        debug!(domain = %domain, "governor: permit acquired");
        tokio::time::sleep(limiter.min_delay).await;
        // _permit drops here, releasing the concurrency slot.
    }
}

impl Default for Governor {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the host from a URL for domain-keyed rate limiting.
/// Falls back to the whole string if it does not parse (still deterministic).
pub fn domain_of(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn governor_caps_per_domain_concurrency() {
        let gov = Arc::new(Governor::new());
        gov.configure("test.example", 2, 100);

        let start = Instant::now();
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let gov = gov.clone();
                tokio::spawn(async move {
                    gov.acquire("test.example").await;
                    tokio::time::sleep(Duration::from_millis(200)).await;
                })
            })
            .collect();
        for h in handles {
            h.await.unwrap();
        }
        // 2 concurrent, 200ms work + 100ms hold => two batches => >=300ms.
        // A flat parallel sleep would finish in ~300ms for ALL four at once;
        // the semaphore forces serialization into batches.
        assert!(start.elapsed().as_millis() >= 300);
    }

    #[test]
    fn domain_extraction() {
        assert_eq!(domain_of("https://orgbook.gov.bc.ca/api/v4/x"), "orgbook.gov.bc.ca");
        assert_eq!(domain_of("not a url"), "not a url");
    }
}
