//! TIN_040 (id `TIN_040`) — Metro Vancouver / CoV planning portal.
//!
//! Rust port of `TIN_040_metro_van.py`. Despite the "static-html" wave grouping,
//! the Python source hits the City of Vancouver **opendata JSON API** (it calls
//! `resp.json()["results"]`), so the faithful archetype is api-json — porting it
//! with CSS selectors would always yield empty results (fabrication by silence).
//!
//! Parity target: `query_cov_dev_permits(address)` →
//! `development-permit-applications/records` with a `where lower(address) like
//! '%<addr>%'` ODSQL filter, `order_by date_received desc`, `limit 10`, reading
//! the `results` array. The Python also queried a zoning dataset + Cambie-corridor
//! sweep; those are additional endpoints left for follow-up.

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

const COV_PERMITS_URL: &str = "https://opendata.vancouver.ca/api/explore/v2.1/catalog/datasets/development-permit-applications/records";

pub struct MetroVan;

#[async_trait]
impl Scraper for MetroVan {
    fn id(&self) -> &'static str {
        "TIN_040"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_040".to_string(),
            source_label: "Metro Vancouver / CoV planning (dev permits)".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Query CoV development-permit-applications by address (Cambie corridor context)."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("opendata.vancouver.ca")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(2),
            min_delay_ms: Some(2000),
            max_results: Some(10),
            max_pages: None,
            notes: "CoV Explore v2.1 open-data API — free, no auth; source used 2s spacing."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut field_mappings = std::collections::BTreeMap::new();
        field_mappings.insert("permit_number".to_string(), "permitnumber".to_string());
        field_mappings.insert("address".to_string(), "address".to_string());
        field_mappings.insert("date_received".to_string(), "date_received".to_string());
        field_mappings.insert("status".to_string(), "status".to_string());
        let mut query_params = std::collections::BTreeMap::new();
        query_params.insert(
            "where".to_string(),
            "lower(address) like \"%{{query}}%\"".to_string(),
        );
        query_params.insert("order_by".to_string(), "date_received desc".to_string());
        query_params.insert("limit".to_string(), "10".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template: COV_PERMITS_URL.to_string(),
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
                record.status = ScrapeStatus::Error("missing 'query' param (address)".to_string());
                return Ok(record);
            }
        };

        // Mirror the Python: lowercase, truncate to first 20 chars for the LIKE.
        let addr_frag: String = query.to_lowercase().chars().take(20).collect();
        let where_clause = format!("lower(address) like \"%{addr_frag}%\"");
        let fetch = api::get_json(
            ctx,
            COV_PERMITS_URL,
            &[
                ("where", where_clause.as_str()),
                ("order_by", "date_received desc"),
                ("limit", "10"),
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

        for rec in results {
            let data = serde_json::json!({
                "type": "dev_permit",
                "query": query,
                "permit_number": rec.get("permitnumber").or_else(|| rec.get("dp_number")),
                "address": rec.get("address"),
                "date_received": rec.get("date_received").or_else(|| rec.get("datereceived")),
                "status": rec.get("status"),
                "description": rec.get("description"),
                "applicant": rec.get("applicantname").or_else(|| rec.get("applicant")),
            });
            record.candidates.push(ScrapeCandidate::new(data, 0.85));
        }

        record.notes = format!("{} development permit application(s)", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
