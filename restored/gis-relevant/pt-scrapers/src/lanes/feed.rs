//! Feed lane — RSS / Atom / sitemap harvesting.

use crate::types::{ScrapeCandidate, ScrapeCtx};
use pt_core::cas::CasBlob;

/// Result of a feed fetch: CAS blob of raw bytes + parsed entries as candidates.
pub struct FeedFetch {
    pub blob: CasBlob,
    pub candidates: Vec<ScrapeCandidate>,
    pub status: u16,
}

/// Fetch and parse an RSS/Atom feed into candidates (one per entry).
pub async fn get_feed(ctx: &ScrapeCtx, url: &str) -> anyhow::Result<FeedFetch> {
    ctx.net_guard(url)?;
    ctx.acquire_for(url).await;

    let resp = ctx.apply_stealth(ctx.http.get(url)).send().await?;
    let status = resp.status().as_u16();
    let bytes = resp.bytes().await?.to_vec();
    let blob = CasBlob::new(bytes.clone(), "application/xml");

    let mut candidates = Vec::new();
    if let Ok(feed) = feed_rs::parser::parse(&bytes[..]) {
        for entry in feed.entries {
            let title = entry.title.map(|t| t.content).unwrap_or_default();
            let link = entry.links.first().map(|l| l.href.clone()).unwrap_or_default();
            let summary = entry.summary.map(|s| s.content).unwrap_or_default();
            let published = entry.published.map(|d| d.to_rfc3339());
            candidates.push(ScrapeCandidate::new(
                serde_json::json!({
                    "title": title,
                    "link": link,
                    "summary": summary,
                    "published": published,
                }),
                0.9,
            ));
        }
    }

    Ok(FeedFetch { blob, candidates, status })
}
