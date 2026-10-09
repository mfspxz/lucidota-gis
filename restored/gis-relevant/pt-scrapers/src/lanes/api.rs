//! API-JSON lane — fetch a JSON/REST endpoint and extract candidates by path.

use crate::types::{ScrapeCandidate, ScrapeCtx};
use pt_core::cas::CasBlob;
use pt_core::ContentHash;

/// Result of a raw API fetch: the CAS blob of the bytes + parsed JSON.
pub struct ApiFetch {
    pub blob: CasBlob,
    pub json: serde_json::Value,
    pub status: u16,
    /// Set when the body was non-empty but failed to parse as JSON. Lets a
    /// scraper distinguish "broken 200" from "valid empty result".
    pub parse_note: Option<String>,
}

impl ApiFetch {
    pub fn hash(&self) -> ContentHash {
        self.blob.hash
    }
}

/// GET a JSON endpoint with optional query params, respecting the ctx politeness
/// floor. Returns the raw bytes (as a CAS blob) and the parsed JSON.
pub async fn get_json(
    ctx: &ScrapeCtx,
    url: &str,
    query: &[(&str, &str)],
) -> anyhow::Result<ApiFetch> {
    // Fail-closed: refuse bare net access under StealthPolicy::Always w/o fingerprint.
    ctx.net_guard(url)?;
    // Politeness: per-domain concurrency cap + spacing via the governor.
    ctx.acquire_for(url).await;

    let resp = ctx
        .apply_stealth(
            ctx.http
                .get(url)
                .query(query)
                .header("Accept", "application/json"),
        )
        .send()
        .await?;
    let status = resp.status().as_u16();
    let bytes = resp.bytes().await?.to_vec();
    // Distinguish "malformed body" from "empty result": a parse failure records a
    // note rather than silently reading as no-data. Status is preserved separately.
    let (json, parse_note) = match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(v) => (v, None),
        Err(e) => (serde_json::Value::Null, Some(format!("json parse failed: {e}"))),
    };
    let blob = CasBlob::new(bytes, "application/json");
    Ok(ApiFetch { blob, json, status, parse_note })
}

/// POST a JSON body to an endpoint and parse the JSON response.
pub async fn post_json(
    ctx: &ScrapeCtx,
    url: &str,
    body: &serde_json::Value,
) -> anyhow::Result<ApiFetch> {
    ctx.net_guard(url)?;
    ctx.acquire_for(url).await;

    let resp = ctx
        .apply_stealth(
            ctx.http
                .post(url)
                .header("Accept", "application/json")
                .json(body),
        )
        .send()
        .await?;
    let status = resp.status().as_u16();
    let bytes = resp.bytes().await?.to_vec();
    let (json, parse_note) = match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(v) => (v, None),
        Err(e) => (serde_json::Value::Null, Some(format!("json parse failed: {e}"))),
    };
    let blob = CasBlob::new(bytes, "application/json");
    Ok(ApiFetch { blob, json, status, parse_note })
}

/// Helper: turn an arbitrary JSON value into a single candidate at a confidence.
pub fn candidate(data: serde_json::Value, confidence: f32) -> ScrapeCandidate {
    ScrapeCandidate::new(data, confidence)
}
