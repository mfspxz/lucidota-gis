//! TIN_025 — BC Address Geocoder.
//!
//! Rust port of `TIN_025_bc_geocoder.py` (api-json archetype). Official BC Gov
//! geocoding API — address → coordinates. Free, no auth. Returns GeoJSON features.

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

pub struct BcGeocoder;

#[async_trait]
impl Scraper for BcGeocoder {
    fn id(&self) -> &'static str {
        "TIN_025"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_025".to_string(),
            source_label: "BC Address Geocoder".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Geocode an address to coordinates via the official BC Gov geocoder."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("geocoder.api.gov.bc.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: None,
            min_delay_ms: None,
            max_results: None,
            max_pages: None,
            notes: "BC Gov geocoder — free, no auth, no published rate limit.".to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut field_mappings = std::collections::BTreeMap::new();
        field_mappings.insert("fullAddress".to_string(), "properties.fullAddress".to_string());
        field_mappings.insert("score".to_string(), "properties.score".to_string());
        field_mappings.insert("locality".to_string(), "properties.localityName".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template:
                "https://geocoder.api.gov.bc.ca/addresses.json?addressString={{query}}&maxResults=5"
                    .to_string(),
            method: "GET".to_string(),
            query_params: Default::default(),
            json_path: Some("features".to_string()),
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

        let url = "https://geocoder.api.gov.bc.ca/addresses.json";
        let fetch = api::get_json(ctx, url, &[("addressString", query.as_str()), ("maxResults", "5")]).await?;
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

        let empty = Vec::new();
        let features = fetch
            .json
            .get("features")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        for f in features {
            let props = f.get("properties").cloned().unwrap_or(serde_json::Value::Null);
            let coords = f
                .get("geometry")
                .and_then(|g| g.get("coordinates"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let score = props.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0);
            let data = serde_json::json!({
                "type": "geocode_match",
                "fullAddress": props.get("fullAddress"),
                "localityName": props.get("localityName"),
                "score": score,
                "coordinates": coords,
            });
            // Confidence from the geocoder's own score (0-100 → 0.0-1.0).
            let confidence = (score / 100.0).clamp(0.0, 1.0) as f32;
            record.candidates.push(ScrapeCandidate::new(data, confidence));
        }

        record.notes = format!("{} matches", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
