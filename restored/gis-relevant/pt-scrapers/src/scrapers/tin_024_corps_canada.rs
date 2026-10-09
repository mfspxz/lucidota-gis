//! TIN_024 — Corporations Canada (federal, ISED).
//!
//! Rust port of `TIN_024_corps_canada.py` (api-json archetype). Looks up a
//! federal (CBCA) corporation by corporation ID or 9-digit business number via
//! the ISED Corporations Canada JSON API. Free, no auth. NOTE: only covers
//! federal corps — BC-incorporated companies are not here (use TIN_002 OrgBook).
//!
//! Parity target: `_fetch_corp()` → `{CORPS_API}/{id}.json?lang=eng`. The API
//! returns a list `[eng_result, fra_result]`; the English entry (index 0) is
//! taken. A string at index 0 is an error message ("could not find corporation").

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

const CORPS_API: &str =
    "https://www.ic.gc.ca/app/scr/cc/CorporationsCanada/api/corporations";

pub struct CorpsCanada;

#[async_trait]
impl Scraper for CorpsCanada {
    fn id(&self) -> &'static str {
        "TIN_024"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_024".to_string(),
            source_label: "Corporations Canada (ISED, federal)".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Look up a federal (CBCA) corporation by corp ID or 9-digit business number."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.ic.gc.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: None,
            min_delay_ms: None,
            max_results: None,
            max_pages: None,
            notes: "ISED Corporations Canada JSON API — free, no auth, \
                    no published rate/concurrency cap; polite defaults applied."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut field_mappings = std::collections::BTreeMap::new();
        field_mappings.insert("corporationName".to_string(), "corporationName".to_string());
        field_mappings.insert("status".to_string(), "status".to_string());
        field_mappings.insert("businessNumber".to_string(), "businessNumber".to_string());
        let mut query_params = std::collections::BTreeMap::new();
        query_params.insert("lang".to_string(), "eng".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template: format!("{CORPS_API}/{{{{query}}}}.json"),
            method: "GET".to_string(),
            query_params,
            // Response is a list [eng, fra]; the English corp object is index 0.
            json_path: Some("0".to_string()),
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
                record.status =
                    ScrapeStatus::Error("missing 'query' param (corp ID or BN9)".to_string());
                return Ok(record);
            }
        };

        let url = format!("{CORPS_API}/{query}.json");
        let fetch = api::get_json(ctx, &url, &[("lang", "eng")]).await?;
        record.raw_capture = Some(fetch.hash());

        if fetch.status != 200 {
            record.status = if fetch.status == 429 {
                ScrapeStatus::RateLimited
            } else {
                ScrapeStatus::Error(format!("HTTP {}", fetch.status))
            };
            record.notes = format!("lookup returned HTTP {}", fetch.status);
            return Ok(record);
        }
        if let Some(note) = &fetch.parse_note {
            record.status = ScrapeStatus::Error(format!("malformed JSON: {note}"));
            record.notes = note.clone();
            return Ok(record);
        }

        // API returns [eng_result, fra_result]; take English (index 0).
        let corp = match fetch.json.as_array().and_then(|a| a.first()) {
            Some(v) if v.is_object() => v.clone(),
            Some(v) if v.is_string() => {
                // Error message like "could not find corporation X".
                record.status = ScrapeStatus::NotFound;
                record.notes = v.as_str().unwrap_or("not found").to_string();
                return Ok(record);
            }
            _ => {
                // Some deployments return a bare object rather than a list.
                if fetch.json.is_object() {
                    fetch.json.clone()
                } else {
                    record.status = ScrapeStatus::NotFound;
                    record.notes = "unexpected response format".to_string();
                    return Ok(record);
                }
            }
        };

        let name = corp
            .get("corporationName")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let status = corp.get("status").and_then(|v| v.as_str()).unwrap_or("?");
        let directors = corp
            .get("directors")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let data = serde_json::json!({
            "type": "federal_corporation",
            "corporation": corp,
            "directors": directors,
        });
        record.candidates.push(ScrapeCandidate::new(data, 0.9));
        record.notes = format!("found: {name} ({status})");
        record.status = ScrapeStatus::Ok;
        Ok(record)
    }
}
