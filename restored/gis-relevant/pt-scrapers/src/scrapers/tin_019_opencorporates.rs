//! TIN_019 — OpenCorporates global corporate registry.
//!
//! Rust port of `TIN_019_opencorporates.py` (api-json archetype). Searches the
//! OpenCorporates v0.4 API for companies matching a query. Free tier works
//! without a key (rate-limited); an API token raises limits.
//!
//! Parity target: `search_companies()` → `{OC_BASE}/companies/search` with
//! `q`, `jurisdiction_code=ca_bc`, `per_page=30`. Results live at
//! `results.companies[].company`.

use crate::controls::{ApiConfig, AuthConfig, LaneConfig, ScraperLimits};
use crate::lanes::api;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

const OC_BASE: &str = "https://api.opencorporates.com/v0.4";
const OC_TOKEN_ENV: &str = "OPENCORPORATES_API_TOKEN";

pub struct OpenCorporates;

#[async_trait]
impl Scraper for OpenCorporates {
    fn id(&self) -> &'static str {
        "TIN_019"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_019".to_string(),
            source_label: "OpenCorporates (global registry)".to_string(),
            domain: Domain::General,
            archetype: Archetype::ApiJson,
            // Basic queries work without auth; a token only raises limits.
            auth_required: false,
            auth_hint: Some(OC_TOKEN_ENV.to_string()),
            description: "Search OpenCorporates for companies by name (jurisdiction ca_bc)."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("api.opencorporates.com")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: None,
            min_delay_ms: None,
            max_results: None,
            max_pages: None,
            notes: "OpenCorporates v0.4 — free tier without a key is heavily rate-limited \
                    (returns HTTP 403 when throttled). An optional API token via env \
                    OPENCORPORATES_API_TOKEN raises limits; no exact free-tier number \
                    is published here, so it is left unset (not fabricated)."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let mut field_mappings = std::collections::BTreeMap::new();
        field_mappings.insert("name".to_string(), "company.name".to_string());
        field_mappings.insert("company_number".to_string(), "company.company_number".to_string());
        field_mappings.insert("jurisdiction".to_string(), "company.jurisdiction_code".to_string());
        field_mappings.insert("status".to_string(), "company.current_status".to_string());
        let mut query_params = std::collections::BTreeMap::new();
        query_params.insert("q".to_string(), "{{query}}".to_string());
        query_params.insert("jurisdiction_code".to_string(), "ca_bc".to_string());
        query_params.insert("per_page".to_string(), "30".to_string());
        LaneConfig::ApiJson(ApiConfig {
            url_template: format!("{OC_BASE}/companies/search"),
            method: "GET".to_string(),
            query_params,
            json_path: Some("results.companies".to_string()),
            field_mappings,
            auth: AuthConfig {
                env_var: Some(OC_TOKEN_ENV.to_string()),
                scheme: Some("query:api_token".to_string()),
            },
            pagination: Default::default(),
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

        let url = format!("{OC_BASE}/companies/search");
        // Optional API token from env raises free-tier limits (query:api_token).
        let token = std::env::var(OC_TOKEN_ENV).ok();
        let mut q: Vec<(&str, &str)> = vec![
            ("q", query.as_str()),
            ("jurisdiction_code", "ca_bc"),
            ("per_page", "30"),
        ];
        if let Some(ref t) = token {
            q.push(("api_token", t.as_str()));
        }
        let fetch = api::get_json(ctx, &url, &q).await?;
        record.raw_capture = Some(fetch.hash());

        if fetch.status != 200 {
            record.status = match fetch.status {
                403 => ScrapeStatus::RateLimited, // OpenCorporates throttles with 403.
                429 => ScrapeStatus::RateLimited,
                s => ScrapeStatus::Error(format!("HTTP {s}")),
            };
            record.notes = format!("search returned HTTP {}", fetch.status);
            return Ok(record);
        }
        if let Some(note) = &fetch.parse_note {
            record.status = ScrapeStatus::Error(format!("malformed JSON: {note}"));
            record.notes = note.clone();
            return Ok(record);
        }

        let empty = Vec::new();
        let companies = fetch
            .json
            .get("results")
            .and_then(|r| r.get("companies"))
            .and_then(|v| v.as_array())
            .unwrap_or(&empty);

        for item in companies {
            let c = item.get("company").cloned().unwrap_or(serde_json::Value::Null);
            let data = serde_json::json!({
                "type": "company",
                "name": c.get("name"),
                "company_number": c.get("company_number"),
                "jurisdiction": c.get("jurisdiction_code"),
                "status": c.get("current_status"),
                "incorporation_date": c.get("incorporation_date"),
                "dissolution_date": c.get("dissolution_date"),
                "company_type": c.get("company_type"),
                "registered_address": c.get("registered_address_in_full"),
                "opencorporates_url": c.get("opencorporates_url"),
                "registry_url": c.get("registry_url"),
            });
            record.candidates.push(ScrapeCandidate::new(data, 0.85));
        }

        record.notes = format!("{} company result(s)", record.candidates.len());
        record.status = if record.candidates.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}
