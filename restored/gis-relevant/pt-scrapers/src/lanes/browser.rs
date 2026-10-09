//! Browser lane — headless CDP + stealth for anti-bot / JS-rendered sources.
//!
//! Feature-gated behind `browser`. When the feature is OFF, `BrowserHandle` is a
//! zero-cost placeholder so `ScrapeCtx` still type-checks in API-only builds, and
//! any attempt to render returns a Blocked-style error the scraper converts into
//! `ScrapeStatus::Blocked`. When the feature is ON, it drives chromiumoxide over
//! the Chrome DevTools Protocol with a stealth profile.

use crate::types::ScrapeCtx;

/// Rendered output from the browser lane.
pub struct RenderedPage {
    pub html: String,
    pub final_url: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature OFF: placeholder so the type exists and honest degradation is possible.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(not(feature = "browser"))]
pub struct BrowserHandle;

#[cfg(not(feature = "browser"))]
impl BrowserHandle {
    /// Never succeeds without the `browser` feature — callers must map this to
    /// `ScrapeStatus::Blocked`, never panic.
    pub async fn render(
        &self,
        _url: &str,
        _wait_ms: u64,
        _ua_override: Option<&str>,
    ) -> anyhow::Result<RenderedPage> {
        anyhow::bail!("browser lane not enabled (compile with feature = \"browser\")")
    }
}

/// Probe whether the browser lane is available in this build. Used by
/// `pt-cli scrape --check-browser`.
#[cfg(not(feature = "browser"))]
pub fn feature_compiled() -> bool {
    false
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature ON: real chromiumoxide-backed pool.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(feature = "browser")]
pub struct BrowserHandle {
    browser: chromiumoxide::Browser,
    // Keep the handler task alive for the lifetime of the browser.
    _handler: tokio::task::JoinHandle<()>,
}

#[cfg(feature = "browser")]
impl BrowserHandle {
    /// Launch a headless Chromium with a stealth-ish config. Returns Blocked-style
    /// error (not panic) if no Chromium binary is reachable.
    pub async fn launch() -> anyhow::Result<Self> {
        use chromiumoxide::browser::{Browser, BrowserConfig};
        use futures::StreamExt;

        let config = BrowserConfig::builder()
            .no_sandbox()
            .build()
            .map_err(|e| anyhow::anyhow!("browser config error: {e}"))?;

        let (browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| anyhow::anyhow!("chromium launch failed (is a Chromium binary installed?): {e}"))?;

        let _handler = tokio::spawn(async move {
            while let Some(ev) = handler.next().await {
                if ev.is_err() {
                    break;
                }
            }
        });

        Ok(Self { browser, _handler })
    }

    /// Render a URL headlessly and return its HTML after a wait for JS.
    /// `ua_override`: when Some, sets the User-Agent so the browser wears the
    /// injected Percyphon persona instead of the default headless-Chrome UA.
    pub async fn render(
        &self,
        url: &str,
        wait_ms: u64,
        ua_override: Option<&str>,
    ) -> anyhow::Result<RenderedPage> {
        let page = self.browser.new_page(url).await?;
        // Stealth: patch navigator.webdriver before content settles.
        let _ = page
            .evaluate_on_new_document(
                "Object.defineProperty(navigator,'webdriver',{get:()=>undefined});",
            )
            .await;
        // Wear the persona UA at the network layer if one was injected.
        if let Some(ua) = ua_override {
            let _ = page.set_user_agent(ua).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
        let html = page.content().await?;
        let final_url = page.url().await?.unwrap_or_else(|| url.to_string());
        let _ = page.close().await;
        Ok(RenderedPage { html, final_url })
    }
}

#[cfg(feature = "browser")]
pub fn feature_compiled() -> bool {
    true
}

/// Convenience: render via the ctx's browser handle, or a clear error if absent.
///
/// FAIL-CLOSED: honors `StealthPolicy` exactly like the HTTP lanes — calls
/// `ctx.net_guard(url)?` BEFORE any egress, so under `StealthPolicy::Always` with
/// no persona injected the browser refuses to open the page (it will NOT leak the
/// real headless-Chrome fingerprint). When a stealth profile IS present, its
/// User-Agent is worn by the page at the network layer.
pub async fn render_via_ctx(ctx: &ScrapeCtx, url: &str, wait_ms: u64) -> anyhow::Result<RenderedPage> {
    // Same fail-closed guard every HTTP lane uses. The browser is the highest-
    // fingerprint surface, so this gate matters MOST here.
    ctx.net_guard(url)?;
    let ua_override = ctx
        .stealth
        .as_ref()
        .and_then(|s| s.user_agent.as_deref());
    match &ctx.browser {
        Some(handle) => handle.render(url, wait_ms, ua_override).await,
        None => anyhow::bail!("browser lane unavailable in this deployment"),
    }
}
