//! Control system — full manual control over every scraper. NOTHING IS AUTOMATIC.
//!
//! Operator law (verbatim intent): "FULL CONTROLS ON SETTINGS NOTHING IS
//! AUTOMATIC; IF THERES A KNOWN CONCURRENT CONNECTIONS OR RATE LIMIT OR
//! SPECIFICITY THAT IS THE MAX, THAT IS NOTED; BUT I WANT TO BE ABLE TO CONTROL
//! ALL SETTINGS; ALL SCRAPERS SHOULD BE PROGRAMMABLE SIMPLY BY THEIR CONTROLS."
//!
//! Model:
//!   ScraperLimits   — the NOTED known maximum (rate/concurrency/source ceiling).
//!                     None = unknown (never a fabricated number).
//!   ScraperControls — every operational knob, resolved for a run.
//!   LaneConfig      — archetype-specific declarative config (one shape per type).
//!   ControlConfig   — layered TOML: [global] < [domain.<host>] < [scraper.<id>],
//!                     then CLAMPED to ScraperLimits (clamp is the final gate).
//!
//! Resolution order: scraper > domain > global, THEN clamp to declared limits.
//! An operator can always dial DOWN; they can never silently exceed a noted max.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ─────────────────────────────────────────────────────────────────────────────
// Declared known limits — the NOTED maximum.
// ─────────────────────────────────────────────────────────────────────────────

/// A scraper's declared known source limits. `None` = unknown (not fabricated).
/// The effective controls are clamped to these; a clamp is recorded in the record.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScraperLimits {
    /// Max safe concurrent connections to this source, if known.
    pub max_concurrency: Option<u32>,
    /// Minimum request spacing in ms (rate floor), if the source publishes one.
    pub min_delay_ms: Option<u64>,
    /// Max results the source will return per query, if capped.
    pub max_results: Option<u32>,
    /// Max pagination depth the source supports, if capped.
    pub max_pages: Option<u32>,
    /// Free-text note on the source's limits/specificity (why these values).
    pub notes: String,
}

impl ScraperLimits {
    /// A scraper with no known published limits — everything unknown, noted so.
    pub fn unknown(note: impl Into<String>) -> Self {
        Self {
            notes: note.into(),
            ..Default::default()
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Lane-specific declarative config — one standardized shape per archetype.
// ─────────────────────────────────────────────────────────────────────────────

/// Pagination strategy — NOTHING automatic; the operator declares it explicitly.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Pagination {
    /// Query param that carries the page/offset (e.g. "page", "offset").
    pub page_param: Option<String>,
    /// JSON path to a next-page token in the response, if token-based.
    pub next_token_path: Option<String>,
    /// Hard cap on pages to fetch (further clamped by ScraperLimits.max_pages).
    pub max_pages: Option<u32>,
}

/// Where an auth secret comes from — ENV VAR NAME ONLY, never an inline secret.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Env var holding the key/token (e.g. "CANLII_API_KEY").
    pub env_var: Option<String>,
    /// How to present it: "header:X-API-Key", "bearer", "query:apikey".
    pub scheme: Option<String>,
}

/// api-json lane config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApiConfig {
    /// URL with a `{{query}}` placeholder (and optionally `{{page}}`).
    pub url_template: String,
    /// "GET" or "POST".
    pub method: String,
    /// Static or templated query params.
    pub query_params: BTreeMap<String, String>,
    /// JSON path to the results array (e.g. "results", "data.user").
    pub json_path: Option<String>,
    /// Map of candidate-field-name → JSON path within each result item.
    pub field_mappings: BTreeMap<String, String>,
    /// Pagination strategy (declared, not inferred).
    pub pagination: Pagination,
    /// Auth source (env var only).
    pub auth: AuthConfig,
}

/// static-html lane config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HtmlConfig {
    pub url_template: String,
    /// Map of candidate-field-name → CSS selector.
    pub selectors: BTreeMap<String, String>,
    /// Optional map of field → regex (fallback when selectors miss).
    pub regex_fallback: BTreeMap<String, String>,
    /// Override text encoding if the source is not UTF-8.
    pub encoding: Option<String>,
    pub auth: AuthConfig,
}

/// feed lane config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeedConfig {
    pub url_template: String,
    /// "rss", "atom", "sitemap".
    pub feed_type: Option<String>,
    /// Map of candidate-field-name → feed entry field (title/link/summary/...).
    pub entry_fields: BTreeMap<String, String>,
    /// Optional substring filters an entry must match to be kept.
    pub entry_filters: Vec<String>,
}

/// browser lane config.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BrowserConfig {
    pub url_template: String,
    /// Post-load wait in ms for JS to settle.
    pub wait_ms: Option<u64>,
    /// Stealth profile: custom UA + navigator patches on/off.
    pub stealth_user_agent: Option<String>,
    pub stealth_patches: bool,
    /// Ordered fallback chain: e.g. ["json_endpoint","html_meta","browser"].
    pub fallback_chain: Vec<String>,
    /// Selectors to extract from the rendered HTML (reuses html extraction).
    pub selectors: BTreeMap<String, String>,
    pub auth: AuthConfig,
}

/// The archetype-tagged lane config. A scraper declares exactly one.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "lane", rename_all = "kebab-case")]
pub enum LaneConfig {
    ApiJson(ApiConfig),
    StaticHtml(HtmlConfig),
    Feed(FeedConfig),
    Browser(BrowserConfig),
    /// Non-harvest module (whois/dns/offline analyzer) — free-form params.
    NonHarvest(serde_json::Value),
}

// ─────────────────────────────────────────────────────────────────────────────
// The full control surface — every knob, resolved for a run.
// ─────────────────────────────────────────────────────────────────────────────

/// Every operational knob. Resolved from the config layers, then clamped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScraperControls {
    pub concurrency: u32,
    pub min_delay_ms: u64,
    pub timeout_secs: u64,
    pub retries: u32,
    pub backoff_ms: u64,
    pub max_pages: u32,
    pub max_results: u32,
    pub user_agent: String,
    pub headers: BTreeMap<String, String>,
    pub proxy: Option<String>,
    /// Notes accumulated during resolution (e.g. "concurrency clamped 10->2").
    pub notes: Vec<String>,
}

impl Default for ScraperControls {
    /// Documented conservative defaults — used when no config is present.
    /// These are DEFAULTS, not automatic behavior: every value is overridable.
    fn default() -> Self {
        Self {
            concurrency: 2,
            min_delay_ms: 1000,
            timeout_secs: 30,
            retries: 0,        // NOTHING automatic: no retry unless operator sets it
            backoff_ms: 0,
            max_pages: 1,      // no auto-pagination unless operator raises it
            max_results: 50,
            user_agent: "TrufflePig/0.1 (PALANTOUR OSINT; +northernstrike.duckdns.org)"
                .to_string(),
            headers: BTreeMap::new(),
            proxy: None,
            notes: Vec::new(),
        }
    }
}

impl ScraperControls {
    /// Clamp these controls to a scraper's declared limits. Clamping only ever
    /// REDUCES a value to the noted max; each clamp is recorded in `notes`.
    pub fn clamp_to(&mut self, limits: &ScraperLimits) {
        if let Some(mc) = limits.max_concurrency {
            if self.concurrency > mc {
                self.notes
                    .push(format!("concurrency clamped {}->{} (source max)", self.concurrency, mc));
                self.concurrency = mc;
            }
        }
        if let Some(md) = limits.min_delay_ms {
            if self.min_delay_ms < md {
                self.notes
                    .push(format!("min_delay raised {}->{}ms (source rate limit)", self.min_delay_ms, md));
                self.min_delay_ms = md;
            }
        }
        if let Some(mr) = limits.max_results {
            if self.max_results > mr {
                self.notes
                    .push(format!("max_results clamped {}->{} (source cap)", self.max_results, mr));
                self.max_results = mr;
            }
        }
        if let Some(mp) = limits.max_pages {
            if self.max_pages > mp {
                self.notes
                    .push(format!("max_pages clamped {}->{} (source cap)", self.max_pages, mp));
                self.max_pages = mp;
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Layered TOML config: [global] < [domain.<host>] < [scraper.<id>].
// ─────────────────────────────────────────────────────────────────────────────

/// A partial control override — every field optional so layers merge cleanly.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ControlOverride {
    pub concurrency: Option<u32>,
    pub min_delay_ms: Option<u64>,
    pub timeout_secs: Option<u64>,
    pub retries: Option<u32>,
    pub backoff_ms: Option<u64>,
    pub max_pages: Option<u32>,
    pub max_results: Option<u32>,
    pub user_agent: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    pub proxy: Option<String>,
}

impl ControlOverride {
    /// Apply this override onto a base control set (set fields win).
    fn apply(&self, base: &mut ScraperControls) {
        if let Some(v) = self.concurrency { base.concurrency = v; }
        if let Some(v) = self.min_delay_ms { base.min_delay_ms = v; }
        if let Some(v) = self.timeout_secs { base.timeout_secs = v; }
        if let Some(v) = self.retries { base.retries = v; }
        if let Some(v) = self.backoff_ms { base.backoff_ms = v; }
        if let Some(v) = self.max_pages { base.max_pages = v; }
        if let Some(v) = self.max_results { base.max_results = v; }
        if let Some(ref v) = self.user_agent { base.user_agent = v.clone(); }
        for (k, v) in &self.headers { base.headers.insert(k.clone(), v.clone()); }
        if let Some(ref v) = self.proxy { base.proxy = Some(v.clone()); }
    }
}

/// Runtime enablement: which compiled scrapers are actually live.
/// Parsed from `[enablement] enabled = "all"` or `enabled = ["TIN_002", ...]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Enabled {
    /// The literal string "all".
    All(String),
    /// An explicit id list.
    List(Vec<String>),
}

impl Default for Enabled {
    fn default() -> Self {
        Enabled::All("all".to_string())
    }
}

/// The `[enablement]` table wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnablementConfig {
    #[serde(default)]
    pub enabled: Enabled,
}

impl Default for EnablementConfig {
    fn default() -> Self {
        Self {
            enabled: Enabled::default(),
        }
    }
}

impl EnablementConfig {
    pub fn is_enabled(&self, id: &str) -> bool {
        match &self.enabled {
            Enabled::All(s) => s.eq_ignore_ascii_case("all"),
            Enabled::List(ids) => ids.iter().any(|x| x == id),
        }
    }
}

/// The whole control config file (`scrapers.toml`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ControlConfig {
    #[serde(default)]
    pub global: ControlOverride,
    #[serde(default)]
    pub domain: BTreeMap<String, ControlOverride>,
    #[serde(default)]
    pub scraper: BTreeMap<String, ControlOverride>,
    #[serde(default)]
    pub enablement: EnablementConfig,
}

impl ControlConfig {
    /// Load from a TOML file. Missing file → documented defaults (not an error).
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)?;
        let cfg: ControlConfig = toml::from_str(&text)?;
        Ok(cfg)
    }

    /// Resolve the effective controls for a scraper: layer global < domain <
    /// scraper, THEN clamp to the declared limits. Clamp is always the final gate.
    pub fn resolve(&self, id: &str, domain: &str, limits: &ScraperLimits) -> ScraperControls {
        let mut controls = ScraperControls::default();
        // Layer 1: global.
        self.global.apply(&mut controls);
        // Layer 2: per-domain.
        if let Some(ov) = self.domain.get(domain) {
            ov.apply(&mut controls);
        }
        // Layer 3: per-scraper (highest precedence).
        if let Some(ov) = self.scraper.get(id) {
            ov.apply(&mut controls);
        }
        // Final gate: clamp to the NOTED source maximum.
        controls.clamp_to(limits);
        controls
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.enablement.is_enabled(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scraper_layer_beats_domain_and_global() {
        let mut cfg = ControlConfig::default();
        cfg.global.concurrency = Some(8);
        cfg.domain.insert("host.example".into(), ControlOverride { concurrency: Some(5), ..Default::default() });
        cfg.scraper.insert("TIN_X".into(), ControlOverride { concurrency: Some(3), ..Default::default() });
        let c = cfg.resolve("TIN_X", "host.example", &ScraperLimits::default());
        assert_eq!(c.concurrency, 3, "per-scraper override must win");
    }

    #[test]
    fn clamp_is_the_final_gate() {
        let mut cfg = ControlConfig::default();
        // Operator asks for 10 concurrent, but source max is 2.
        cfg.scraper.insert("TIN_X".into(), ControlOverride { concurrency: Some(10), ..Default::default() });
        let limits = ScraperLimits { max_concurrency: Some(2), ..Default::default() };
        let c = cfg.resolve("TIN_X", "host.example", &limits);
        assert_eq!(c.concurrency, 2, "must clamp to noted source max");
        assert!(c.notes.iter().any(|n| n.contains("clamped")), "clamp must be noted");
    }

    #[test]
    fn rate_floor_is_raised_not_lowered() {
        let mut cfg = ControlConfig::default();
        // Operator sets 100ms spacing, but source requires >= 500ms.
        cfg.scraper.insert("TIN_X".into(), ControlOverride { min_delay_ms: Some(100), ..Default::default() });
        let limits = ScraperLimits { min_delay_ms: Some(500), ..Default::default() };
        let c = cfg.resolve("TIN_X", "host.example", &limits);
        assert_eq!(c.min_delay_ms, 500, "must raise to the source rate floor");
    }

    #[test]
    fn missing_config_uses_defaults() {
        let cfg = ControlConfig::load(std::path::Path::new("/nonexistent/scrapers.toml")).unwrap();
        let c = cfg.resolve("TIN_X", "host.example", &ScraperLimits::default());
        assert_eq!(c.concurrency, 2);
        assert_eq!(c.retries, 0, "nothing automatic: no retry by default");
        assert!(cfg.is_enabled("anything"), "default enablement is all");
    }
}
