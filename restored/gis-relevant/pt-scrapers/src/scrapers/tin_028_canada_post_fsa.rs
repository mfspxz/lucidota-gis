//! TIN_028 — Canada Post FSA / geocoder.ca lookup.
//!
//! Rust port of `TIN_028_canada_post_fsa.py` (api-json archetype). Resolves a
//! Forward Sortation Area (FSA) / postal code to geographic data via geocoder.ca.
//! Free, no auth (basic queries; geocoder.ca throttles heavy use).
//!
//! Parity target: `lookup_fsa()` live branch → `https://geocoder.ca/?locate=<fsa>
//! &geoit=XML&json=1`. `json=1` makes geocoder.ca return a JSON object.

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

const GEOCODER_URL: &str = "https://geocoder.ca/";

pub struct CanadaPostFsa;

#[async_trait]
impl Scraper for CanadaPostFsa {
    fn id(&self) -> &'static str {
        "TIN_028"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_028".to_string(),
            source_label: "Canada Post FSA (geocoder.ca)".to_string(),
            domain: Domain::General,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Resolve a postal code / FSA to geographic data via geocoder.ca."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("geocoder.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: None,
            min_delay_ms: None,
            max_results: None,
            max_pages: None,
            notes: "geocoder.ca — free for basic queries, no auth; it throttles heavy \
                    use but publishes no exact number here, so left unset (not fabricated)."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut query_params = std::collections::BTreeMap::new();
        query_params.insert("locate".to_string(), "{{query}}".to_string());
        query_params.insert("geoit".to_string(), "XML".to_string());
        query_params.insert("json".to_string(), "1".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template: GEOCODER_URL.to_string(),
            method: "GET".to_string(),
            query_params,
            // Single JSON object, not an array — extracted directly in run().
            json_path: None,
            field_mappings: Default::default(),
            pagination: Default::default(),
            auth: Default::default(),
        })
    }

    async fn run(&self, ctx: &ScrapeCtx, params: ScrapeParams) -> anyhow::Result<ScrapeRecord> {
        let meta = self.meta();
        let mut record = ScrapeRecord::start(&meta, &params);

        let fsa = match params.query() {
            Some(q) if !q.is_empty() => q.to_string(),
            _ => {
                record.status =
                    ScrapeStatus::Error("missing 'query' param (postal code / FSA)".to_string());
                return Ok(record);
            }
        };

        let fetch = api::get_json(
            ctx,
            GEOCODER_URL,
            &[("locate", fsa.as_str()), ("geoit", "XML"), ("json", "1")],
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

        // geocoder.ca signals a failed lookup with an "error" object in the JSON.
        if fetch.json.get("error").is_some() {
            record.status = ScrapeStatus::NotFound;
            record.notes = "geocoder.ca returned an error (no match)".to_string();
            return Ok(record);
        }

        let data = serde_json::json!({
            "type": "fsa_geocode",
            "fsa": fsa,
            "geocode": fetch.json,
        });
        record.candidates.push(ScrapeCandidate::new(data, 0.85));
        record.notes = "1 geocode result".to_string();
        record.status = ScrapeStatus::Ok;
        Ok(record)
    }
}
