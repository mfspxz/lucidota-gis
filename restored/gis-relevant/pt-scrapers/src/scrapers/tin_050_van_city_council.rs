//! TIN_050 — Vancouver City Council voting records.
//!
//! Rust port of `TIN_050_van_city_council.py` (api-json archetype). Searches the
//! City of Vancouver `council-voting-records` open dataset by keyword against the
//! `agenda_description` field via ODSQL full-text `search()`. Free, no auth.
//!
//! Parity target: `search_council()` → dataset `records` endpoint, `results` array.

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

const COUNCIL_API: &str = "https://opendata.vancouver.ca/api/explore/v2.1/catalog/datasets/council-voting-records/records";

pub struct VanCityCouncil;

#[async_trait]
impl Scraper for VanCityCouncil {
    fn id(&self) -> &'static str {
        "TIN_050"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_050".to_string(),
            source_label: "Vancouver City Council (voting records)".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Search Vancouver council voting records by keyword (agenda descriptions)."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("opendata.vancouver.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: None,
            min_delay_ms: None,
            max_results: None,
            max_pages: None,
            notes: "City of Vancouver Explore v2.1 open-data API — free, no auth, \
                    no published rate/concurrency cap; polite defaults applied."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut field_mappings = std::collections::BTreeMap::new();
        field_mappings.insert("meeting_date".to_string(), "meeting_date".to_string());
        field_mappings.insert("agenda_description".to_string(), "agenda_description".to_string());
        field_mappings.insert("vote_result".to_string(), "vote_result".to_string());
        let mut query_params = std::collections::BTreeMap::new();
        query_params.insert("where".to_string(), "search(agenda_description, \"{{query}}\")".to_string());
        query_params.insert("order_by".to_string(), "meeting_date desc".to_string());
        query_params.insert("limit".to_string(), "20".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template: COUNCIL_API.to_string(),
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

        let where_clause = format!("search(agenda_description, \"{query}\")");
        let fetch = api::get_json(
            ctx,
            COUNCIL_API,
            &[
                ("where", where_clause.as_str()),
                ("order_by", "meeting_date desc"),
                ("limit", "20"),
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

        let empty = Vec::new();
        let results = fetch
            .json
            .get("results")
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        for record_item in results {
            let data = serde_json::json!({
                "type": "council_vote",
                "keyword": query,
                "meeting_date": record_item.get("meeting_date"),
                "agenda_item": record_item.get("agenda_description"),
                "vote_result": record_item.get("vote_result"),
                "councillor": record_item.get("councillor_name"),
                "vote": record_item.get("vote"),
            });
            record.candidates.push(ScrapeCandidate::new(data, 0.8));
        }

        record.notes = format!("{} council voting record(s)", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
