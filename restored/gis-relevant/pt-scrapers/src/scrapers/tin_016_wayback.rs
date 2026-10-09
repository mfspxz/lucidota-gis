//! TIN_016 — Wayback Machine (Internet Archive CDX).
//!
//! Rust port of `TIN_016_wayback.py` (api-json archetype). Queries the Internet
//! Archive CDX API for historical snapshots of a domain. Free, no auth.
//!
//! Parity target: `search_cdx()` → CDX API with `output=json`. The response is a
//! JSON array-of-arrays: row 0 is the header row (field names), rows 1..N are the
//! snapshot rows. Each row is zipped with the header to build a record.

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

const CDX_API: &str = "https://web.archive.org/cdx/search/cdx";
const FL: &str = "timestamp,original,mimetype,statuscode,length";

pub struct Wayback;

#[async_trait]
impl Scraper for Wayback {
    fn id(&self) -> &'static str {
        "TIN_016"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_016".to_string(),
            source_label: "Wayback Machine (Internet Archive CDX)".to_string(),
            domain: Domain::General,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Query the Internet Archive CDX API for historical snapshots of a domain."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("web.archive.org")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: None,
            min_delay_ms: None,
            max_results: None,
            max_pages: None,
            notes: "Internet Archive CDX — free, public, no auth; no published hard \
                    rate limit, but be polite (the Python port spaces batch requests)."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut query_params = std::collections::BTreeMap::new();
        query_params.insert("url".to_string(), "{{query}}/*".to_string());
        query_params.insert("output".to_string(), "json".to_string());
        query_params.insert("limit".to_string(), "500".to_string());
        query_params.insert("fl".to_string(), FL.to_string());
        query_params.insert("collapse".to_string(), "urlkey".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template: CDX_API.to_string(),
            method: "GET".to_string(),
            query_params,
            // Array-of-arrays; row 0 is the header. No single json_path applies.
            json_path: None,
            field_mappings: Default::default(),
            pagination: Default::default(),
            auth: Default::default(),
        })
    }

    async fn run(&self, ctx: &ScrapeCtx, params: ScrapeParams) -> anyhow::Result<ScrapeRecord> {
        let meta = self.meta();
        let mut record = ScrapeRecord::start(&meta, &params);

        let domain = match params.query() {
            Some(q) if !q.is_empty() => q.to_string(),
            _ => {
                record.status = ScrapeStatus::Error("missing 'query' param (domain)".to_string());
                return Ok(record);
            }
        };

        let url_param = format!("{domain}/*");
        let fetch = api::get_json(
            ctx,
            CDX_API,
            &[
                ("url", url_param.as_str()),
                ("output", "json"),
                ("limit", "500"),
                ("fl", FL),
                ("collapse", "urlkey"),
            ],
        )
        .await?;
        record.raw_capture = Some(fetch.hash());

        if fetch.status != 200 {
            record.status = if fetch.status == 429 {
                ScrapeStatus::RateLimited
            } else {
                ScrapeStatus::Error(format!("HTTP {}", fetch.status))
            };
            return Ok(record);
        }
        if let Some(note) = &fetch.parse_note {
            record.status = ScrapeStatus::Error(format!("malformed JSON: {note}"));
            record.notes = note.clone();
            return Ok(record);
        }

        // Response is a JSON array of arrays. Row 0 = header field names.
        let rows = match fetch.json.as_array() {
            Some(a) if a.len() >= 2 => a,
            _ => {
                record.status = ScrapeStatus::NotFound;
                record.notes = "no snapshots for domain".to_string();
                return Ok(record);
            }
        };

        let empty = Vec::new();
        let headers: Vec<String> = rows[0]
            .as_array()
            .unwrap_or(&empty)
            .iter()
            .map(|h| h.as_str().unwrap_or("").to_string())
            .collect();

        for row in &rows[1..] {
            let cells = match row.as_array() {
                Some(c) => c,
                None => continue,
            };
            let mut obj = serde_json::Map::new();
            for (i, h) in headers.iter().enumerate() {
                if let Some(cell) = cells.get(i) {
                    obj.insert(h.clone(), cell.clone());
                }
            }
            // Reconstruct the wayback_url like the Python port.
            let timestamp = obj.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
            let original = obj.get("original").and_then(|v| v.as_str()).unwrap_or("");
            obj.insert(
                "wayback_url".to_string(),
                serde_json::Value::String(format!(
                    "https://web.archive.org/web/{timestamp}/{original}"
                )),
            );
            let data = serde_json::json!({
                "type": "wayback_snapshot",
                "snapshot": serde_json::Value::Object(obj),
            });
            record.candidates.push(ScrapeCandidate::new(data, 0.9));
        }

        record.notes = format!("{} snapshot(s)", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
