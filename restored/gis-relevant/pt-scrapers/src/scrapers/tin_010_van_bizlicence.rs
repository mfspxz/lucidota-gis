//! TIN_010 — Vancouver Business Licence deep search.
//!
//! Rust port of `TIN_010_van_bizlicence.py` (api-json archetype). Targeted
//! business-licence lookups against the Vancouver Open Data Explore v2.1 API,
//! using ODSQL full-text `search()` on the `businessname` field. Free, no auth.
//!
//! Parity target: `search_licences(name=...)` → dataset `records` endpoint,
//! `results` array.

use crate::controls::{ApiConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

const BASE: &str = "https://opendata.vancouver.ca/api/explore/v2.1/catalog/datasets";
const DATASET: &str = "business-licences";
const SELECT: &str = "licencenumber,businessname,businesstradename,status,businesstype,businesssubtype,house,street,unit,city,postalcode,issueddate,expireddate,folderyear";

pub struct VanBizLicence;

#[async_trait]
impl Scraper for VanBizLicence {
    fn id(&self) -> &'static str {
        "TIN_010"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_010".to_string(),
            source_label: "Vancouver Business Licence Search".to_string(),
            domain: Domain::BcOsint,
            archetype: Archetype::ApiJson,
            auth_required: false,
            auth_hint: None,
            description: "Full-text search of City of Vancouver business licences by business name."
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
        field_mappings.insert("businessname".to_string(), "businessname".to_string());
        field_mappings.insert("status".to_string(), "status".to_string());
        field_mappings.insert("businesstype".to_string(), "businesstype".to_string());
        let mut query_params = std::collections::BTreeMap::new();
        query_params.insert("where".to_string(), "search(businessname, '{{query}}')".to_string());
        query_params.insert("select".to_string(), SELECT.to_string());
        query_params.insert("order_by".to_string(), "issueddate DESC".to_string());
        query_params.insert("limit".to_string(), "50".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template: format!("{BASE}/{DATASET}/records"),
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

        let url = format!("{BASE}/{DATASET}/records");
        // ODSQL search() full-text; escape single quotes like the Python port.
        let safe = query.replace('\'', "''");
        let where_clause = format!("search(businessname, '{safe}')");
        let fetch = api::get_json(
            ctx,
            &url,
            &[
                ("where", where_clause.as_str()),
                ("select", SELECT),
                ("order_by", "issueddate DESC"),
                ("limit", "50"),
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

        // Property-management-related business types flagged in the Python.
        const PM_TYPES: &[&str] = &[
            "property manager",
            "property management",
            "real estate",
            "real estate services",
            "rental agency",
            "strata management",
            "property rental",
            "rental",
            "strata",
        ];

        for item in results {
            let btype = item
                .get("businesstype")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            let bsub = item
                .get("businesssubtype")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            let pm_related = PM_TYPES.iter().any(|t| btype.contains(t) || bsub.contains(t));

            let data = serde_json::json!({
                "type": "business_licence",
                "pm_related": pm_related,
                "record": item,
            });
            // PM-related hits are the target signal → slightly higher confidence.
            let confidence = if pm_related { 0.85 } else { 0.8 };
            record.candidates.push(ScrapeCandidate::new(data, confidence));
        }

        record.notes = format!("{} licence record(s)", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
