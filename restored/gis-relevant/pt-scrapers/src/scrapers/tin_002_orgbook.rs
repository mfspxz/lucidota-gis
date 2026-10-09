//! TIN_002 — BC OrgBook / Corporate Registry.
//!
//! Rust port of `TIN_002_orgbook.py` (api-json archetype). Searches the BC
//! OrgBook v4 API for a company and pulls credential attributes (addresses,
//! directors, status). Free API, no auth. Public endpoint.
//!
//! Parity target: the Python scraper's `search_orgbook()` result shape.

use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

const ORGBOOK_API: &str = "https://orgbook.gov.bc.ca/api/v4";

pub struct OrgBook;

#[async_trait]
impl Scraper for OrgBook {
    fn id(&self) -> &'static str {
        "TIN_002"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_002".to_string(),
            source_label: "BC OrgBook / Corporate Registry".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Search BC OrgBook for company registration, directors, addresses, status."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("orgbook.gov.bc.ca")
    }

    fn limits(&self) -> crate::controls::ScraperLimits {
        // OrgBook is a free public BC Gov API with no published hard rate limit or
        // concurrency cap. Unknown = not fabricated; polite defaults apply and the
        // operator can raise concurrency via config up to whatever they deem safe.
        crate::controls::ScraperLimits {
            max_concurrency: None,
            min_delay_ms: None,
            max_results: None,
            max_pages: None,
            notes: "BC Gov OrgBook v4 — free public API, no published rate/concurrency cap; \
                    polite defaults applied, operator-tunable."
                .to_string(),
        }
    }

    fn default_config(&self) -> crate::controls::LaneConfig {
        use crate::controls::{ApiConfig, LaneConfig};
        let mut field_mappings = std::collections::BTreeMap::new();
        field_mappings.insert("name".to_string(), "names.0.text".to_string());
        field_mappings.insert("source_id".to_string(), "source_id".to_string());
        field_mappings.insert("topic_id".to_string(), "id".to_string());
        field_mappings.insert("inactive".to_string(), "inactive".to_string());
        let mut query_params = std::collections::BTreeMap::new();
        query_params.insert("q".to_string(), "{{query}}".to_string());
        query_params.insert("format".to_string(), "json".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template: "https://orgbook.gov.bc.ca/api/v4/search/topic?q={{query}}&format=json"
                .to_string(),
            method: "GET".to_string(),
            query_params,
            json_path: Some("results".to_string()),
            field_mappings,
            pagination: Default::default(),
            auth: Default::default(),
        })
    }

    async fn run(&self, ctx: &ScrapeCtx, params: ScrapeParams) -> anyhow::Result<ScrapeRecord> {
        let meta = self.meta();
        let mut record = ScrapeRecord::start(&meta, &params);

        let query = match params.query() {
            Some(q) if !q.is_empty() => q.to_string(),
            _ => {
                record.status = ScrapeStatus::Error("missing 'query' param".to_string());
                return Ok(record);
            }
        };

        // 1) Topic search.
        let search_url = format!("{ORGBOOK_API}/search/topic");
        let fetch = api::get_json(ctx, &search_url, &[("q", query.as_str()), ("format", "json")]).await?;
        record.raw_capture = Some(fetch.hash());

        if fetch.status != 200 {
            record.status = if fetch.status == 429 {
                ScrapeStatus::RateLimited
            } else {
                ScrapeStatus::Error(format!("HTTP {}", fetch.status))
            };
            record.notes = format!("search returned HTTP {}", fetch.status);
            return Ok(record);
        }

        // Malformed 200 body: surface it as an error, not a silent "not found".
        if let Some(note) = &fetch.parse_note {
            record.status = ScrapeStatus::Error(format!("malformed JSON from OrgBook: {note}"));
            record.notes = note.clone();
            return Ok(record);
        }

        let total = fetch.json.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        if total == 0 {
            record.status = ScrapeStatus::NotFound;
            record.notes = "no matching entities in BC Corporate Registry".to_string();
            return Ok(record);
        }

        let empty = Vec::new();
        let results = fetch
            .json
            .get("results")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        for item in results.iter().take(5) {
            let name = item
                .get("names")
                .and_then(|n| n.as_array())
                .and_then(|a| a.first())
                .and_then(|f| f.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            let source_id = item.get("source_id").and_then(|v| v.as_str()).unwrap_or("");
            let topic_id = item.get("id").and_then(|v| v.as_i64());
            let inactive = item.get("inactive").and_then(|v| v.as_bool()).unwrap_or(false);

            let mut entity = serde_json::json!({
                "type": "entity",
                "name": name,
                "source_id": source_id,
                "topic_id": topic_id,
                "inactive": inactive,
            });

            // 2) Credential enrichment per entity.
            if let Some(tid) = topic_id {
                let cred_url = format!("{ORGBOOK_API}/topic/{tid}/credential");
                if let Ok(cred) = api::get_json(ctx, &cred_url, &[]).await {
                    if let Some(creds) = cred.json.get("results").and_then(|v| v.as_array()) {
                        let mut addresses = Vec::new();
                        let mut directors = Vec::new();
                        let mut status = serde_json::Value::Null;
                        for c in creds {
                            let ctype = c
                                .get("credential_type")
                                .and_then(|t| t.get("description"))
                                .and_then(|d| d.as_str())
                                .unwrap_or("")
                                .to_lowercase();
                            let attrs: serde_json::Value = c
                                .get("attributes")
                                .and_then(|a| a.as_array())
                                .map(|arr| {
                                    let mut m = serde_json::Map::new();
                                    for a in arr {
                                        if let (Some(k), Some(v)) = (
                                            a.get("type").and_then(|x| x.as_str()),
                                            a.get("value"),
                                        ) {
                                            m.insert(k.to_string(), v.clone());
                                        }
                                    }
                                    serde_json::Value::Object(m)
                                })
                                .unwrap_or(serde_json::Value::Null);

                            if ctype.contains("address") {
                                addresses.push(attrs.clone());
                            }
                            if ctype.contains("director") || ctype.contains("officer") {
                                directors.push(attrs.clone());
                            }
                            if ctype.contains("status") {
                                status = attrs;
                            }
                        }
                        entity["addresses"] = serde_json::Value::Array(addresses);
                        entity["directors"] = serde_json::Value::Array(directors);
                        entity["status"] = status;
                    }
                }
            }

            // Confidence: exact-ish name match higher; inactive entities flagged.
            let confidence = if inactive { 0.7 } else { 0.9 };
            record.candidates.push(ScrapeCandidate::new(entity, confidence));
        }

        record.status = ScrapeStatus::Ok;
        record.notes = format!("found {total} matching entities");
        Ok(record)
    }
}
