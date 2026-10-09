//! Static-HTML lane — fetch HTML and extract candidates via CSS selectors.

use crate::types::{ScrapeCandidate, ScrapeCtx};
use pt_core::cas::CasBlob;
use scraper::{Html, Selector};

/// Result of an HTML fetch: the CAS blob of the raw bytes + the body string.
pub struct HtmlFetch {
    pub blob: CasBlob,
    pub body: String,
    pub status: u16,
}

/// GET an HTML page, respecting the ctx politeness floor.
pub async fn get_html(ctx: &ScrapeCtx, url: &str) -> anyhow::Result<HtmlFetch> {
    ctx.net_guard(url)?;
    ctx.acquire_for(url).await;

    let resp = ctx.apply_stealth(ctx.http.get(url)).send().await?;
    let status = resp.status().as_u16();
    let bytes = resp.bytes().await?.to_vec();
    let body = String::from_utf8_lossy(&bytes).to_string();
    let blob = CasBlob::new(bytes, "text/html");
    Ok(HtmlFetch { blob, body, status })
}

/// Extract candidates from HTML using a field→CSS-selector map.
/// Each matched element's text becomes a candidate at the given confidence.
pub fn extract(body: &str, selectors: &[(&str, &str)], confidence: f32) -> Vec<ScrapeCandidate> {
    let document = Html::parse_document(body);
    let mut out = Vec::new();

    for (field, selector_str) in selectors {
        let sel = match Selector::parse(selector_str) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(selector = %selector_str, error = ?e, "invalid CSS selector");
                continue;
            }
        };
        for element in document.select(&sel) {
            let text = element.text().collect::<Vec<_>>().join(" ").trim().to_string();
            if !text.is_empty() {
                out.push(ScrapeCandidate::new(
                    serde_json::json!({
                        "field": field,
                        "value": text,
                        "selector": selector_str,
                    }),
                    confidence,
                ));
            }
        }
    }
    out
}
