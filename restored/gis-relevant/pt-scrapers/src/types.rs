//! Core scraper types — the standardized contract every organelle implements.
//!
//! One trait (`Scraper`), one output (`ScrapeRecord`), one injected context
//! (`ScrapeCtx`). Archetype lanes do the fetching; scrapers compose lanes and
//! never build their own HTTP client or rate limiter.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use pt_core::epistemic::TruthValence;
use pt_core::ContentHash;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// The four harvest archetypes. Each maps to a lane helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Archetype {
    /// Structured JSON/REST API — reqwest + serde_json path extraction.
    ApiJson,
    /// Known HTML layout — reqwest + CSS selectors / regex.
    StaticHtml,
    /// RSS / Atom / sitemap feed.
    Feed,
    /// Anti-bot / JS-rendered — needs the headless browser lane.
    Browser,
}

/// Deployment domain grouping — mirrors the cargo feature groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Domain {
    Festival,
    BcOsint,
    Finance,
    AcademicIntegrity,
    Activist,
    Social,
    General,
}

/// Static metadata describing a scraper. Drives registry listing, CLI `--list`,
/// REST `/scrapers`, and MCP `describe_scraper`/tool schemas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScraperMeta {
    /// Unique scraper id, e.g. "TIN_002".
    pub id: String,
    /// Human-readable target, e.g. "BC OrgBook / Corporate Registry".
    pub source_label: String,
    /// Deployment domain.
    pub domain: Domain,
    /// Harvest archetype.
    pub archetype: Archetype,
    /// Whether the scraper needs an auth key/credential to function.
    pub auth_required: bool,
    /// Name of the env var / key it reads, if any.
    pub auth_hint: Option<String>,
    /// One-line description of what it returns.
    pub description: String,
}

/// Parameters passed to a scraper run. Free-form JSON so each scraper defines
/// its own shape (a query string, a username, a company name, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScrapeParams(pub serde_json::Value);

impl ScrapeParams {
    /// Build params from a single positional query string (the common CLI case).
    pub fn from_query(q: impl Into<String>) -> Self {
        ScrapeParams(serde_json::json!({ "query": q.into() }))
    }

    /// Best-effort extraction of the primary "query" field.
    pub fn query(&self) -> Option<&str> {
        self.0.get("query").and_then(|v| v.as_str())
    }
}

/// The terminal status of a scrape. Honest degradation is a first-class citizen:
/// blocked/rate-limited are NOT errors and NOT fabricated success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "reason")]
pub enum ScrapeStatus {
    /// Completed and produced (possibly zero) candidates.
    Ok,
    /// Ran cleanly but the target was not found.
    NotFound,
    /// Blocked (anti-bot, missing browser lane, disabled) — reason carried.
    Blocked(String),
    /// Rate-limited by the source (e.g. HTTP 429).
    RateLimited,
    /// Something failed — reason carried.
    Error(String),
}

/// A single extracted candidate. NEVER promoted to Proven by the scraper —
/// mutation class `candidate_writer`. Promotion is pt-oracle's job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeCandidate {
    /// JSON-serialized extracted entity/field snapshot.
    pub data: serde_json::Value,
    /// Byte span into the raw capture this was extracted from, when addressable.
    pub byte_span: Option<(usize, usize)>,
    /// Extractor confidence, 0.0–1.0.
    pub confidence: f32,
    /// Ternary posture. New candidates default to Unknown (0).
    pub valence: TruthValence,
}

impl ScrapeCandidate {
    /// A fresh candidate: Unknown valence, no truth promotion.
    pub fn new(data: serde_json::Value, confidence: f32) -> Self {
        Self {
            data,
            byte_span: None,
            confidence,
            valence: TruthValence::Unknown,
        }
    }

    pub fn with_span(mut self, span: (usize, usize)) -> Self {
        self.byte_span = Some(span);
        self
    }
}

/// The uniform output of every scraper across every surface (CLI/REST/MCP).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeRecord {
    /// Which scraper produced this.
    pub scraper_id: String,
    /// Human-readable source label.
    pub source_label: String,
    /// The params echoed back.
    pub query: serde_json::Value,
    /// When the harvest happened.
    pub captured_at: DateTime<Utc>,
    /// blake2b CAS handle of the raw captured bytes (custody), if any.
    pub raw_capture: Option<ContentHash>,
    /// Extracted candidates — each a CANDIDATE, never Proven.
    pub candidates: Vec<ScrapeCandidate>,
    /// Terminal status.
    pub status: ScrapeStatus,
    /// Free-form notes (parity/debug/why-blocked).
    pub notes: String,
    /// Wall-clock time spent.
    pub elapsed_ms: u64,
}

impl ScrapeRecord {
    /// Start an empty record for a scraper, to be filled by its run.
    pub fn start(meta: &ScraperMeta, params: &ScrapeParams) -> Self {
        Self {
            scraper_id: meta.id.clone(),
            source_label: meta.source_label.clone(),
            query: params.0.clone(),
            captured_at: Utc::now(),
            raw_capture: None,
            candidates: Vec::new(),
            status: ScrapeStatus::Ok,
            notes: String::new(),
            elapsed_ms: 0,
        }
    }

    /// Convenience: a blocked record with a reason.
    pub fn blocked(meta: &ScraperMeta, params: &ScrapeParams, reason: impl Into<String>) -> Self {
        let mut r = Self::start(meta, params);
        r.status = ScrapeStatus::Blocked(reason.into());
        r
    }

    /// Convenience: an errored record with a reason.
    pub fn errored(scraper_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            scraper_id: scraper_id.into(),
            source_label: String::new(),
            query: serde_json::Value::Null,
            captured_at: Utc::now(),
            raw_capture: None,
            candidates: Vec::new(),
            status: ScrapeStatus::Error(reason.into()),
            notes: String::new(),
            elapsed_ms: 0,
        }
    }
}

/// A per-request stealth identity applied to HTTP requests (RFC §5.2: "inject a
/// derived persona as the request fingerprint"). This is a PLAIN struct so the
/// scraper crate stays decoupled from pt-percyphon — the operator maps a Percyphon
/// persona -> Fingerprint -> StealthProfile at the edge and injects it via
/// `ScrapeCtx::with_stealth`. When present, lanes override UA + add these headers.
#[derive(Debug, Clone, Default)]
pub struct StealthProfile {
    /// User-Agent to present (overrides the ctx default for this run).
    pub user_agent: Option<String>,
    /// Accept-Language header.
    pub accept_language: Option<String>,
    /// Extra headers (e.g. Sec-CH-UA client hints) — name → value.
    pub headers: std::collections::BTreeMap<String, String>,
    /// Provenance: which persona vuuid this profile came from (for receipts/audit).
    pub source_vuuid: Option<String>,
}

/// Policy governing whether a Percyphon fingerprint is required before touching the
/// outside net. Operator directive: fingerprint FIRST, ALWAYS — with a toggle
/// (nothing automatic / everything controllable).
///
/// - `Always` (fail-closed): a request WITHOUT a stealth profile is REFUSED. The
///   lane returns an error rather than leaking the real rustls/native fingerprint.
///   This is the "always wear a mask on the net" posture.
/// - `Preferred`: use the fingerprint if one is injected; allow a bare request if
///   not. Pragmatic default so tooling/tests that don't inject a persona still run.
/// - `Off`: never apply a fingerprint even if present (debugging / trusted APIs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StealthPolicy {
    /// Fingerprint REQUIRED — bare net access is refused (fail-closed).
    Always,
    /// Use fingerprint if present; allow bare otherwise.
    Preferred,
    /// Never apply a fingerprint.
    Off,
}

impl Default for StealthPolicy {
    /// Preferred by default: honors an injected persona, doesn't break bare tooling.
    /// The operator flips this to `Always` for true fail-closed "mask first" posture.
    fn default() -> Self {
        StealthPolicy::Preferred
    }
}

/// Shared machinery injected into every scraper run. Scrapers borrow the HTTP
/// client, the per-domain rate governor, the control config, and (optionally) the
/// browser pool from here — they never construct their own. This is how politeness
/// (per-domain concurrency caps + spacing), CAS custody, and "nothing automatic"
/// (every knob read from config) are enforced uniformly.
#[derive(Clone)]
pub struct ScrapeCtx {
    /// Shared HTTP client (rustls, cookies, gzip/brotli — from workspace config).
    pub http: reqwest::Client,
    /// Per-domain rate governor: caps concurrency AND spaces requests per host.
    /// Configured FROM resolved controls — NOT a fixed default.
    pub governor: Arc<crate::governor::Governor>,
    /// The layered control config (global < domain < scraper), clamped to limits
    /// at resolve time. Lanes read every knob from here — nothing hardcoded.
    pub config: Arc<crate::controls::ControlConfig>,
    /// Optional headless browser pool. `None` in API-only builds/deployments.
    pub browser: Option<Arc<crate::lanes::browser::BrowserHandle>>,
    /// Optional per-request stealth identity (Percyphon persona fingerprint).
    /// `None` = no fingerprint injected. Set via `with_stealth`.
    pub stealth: Option<StealthProfile>,
    /// Policy for whether a fingerprint is REQUIRED before net access. Default
    /// `Preferred`; set `Always` for fail-closed "mask first, always" posture.
    pub stealth_policy: StealthPolicy,
}

impl ScrapeCtx {
    /// A default context: standard UA, default governor, empty (all-defaults)
    /// control config, no browser.
    pub fn new_default() -> anyhow::Result<Self> {
        Self::with_config(Arc::new(crate::controls::ControlConfig::default()))
    }

    /// Build a context from a loaded control config. The governor starts empty and
    /// is configured per-domain lazily by `configure_governor_for` before each run.
    pub fn with_config(config: Arc<crate::controls::ControlConfig>) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent("TrufflePig/0.1 (PALANTOUR OSINT; +northernstrike.duckdns.org)")
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            governor: Arc::new(crate::governor::Governor::new()),
            config,
            browser: None,
            stealth: None,
            stealth_policy: StealthPolicy::default(),
        })
    }

    /// Return a clone of this ctx carrying a stealth identity. The Ghost Swarm
    /// injects a per-persona fingerprint here so each run wears a distinct,
    /// coherent identity (RFC §5.2). Cheap: reuses the same http client + governor.
    pub fn with_stealth(&self, profile: StealthProfile) -> Self {
        let mut c = self.clone();
        c.stealth = Some(profile);
        c
    }

    /// Return a clone of this ctx with a specific stealth POLICY. Use `Always` to
    /// enforce fail-closed "fingerprint first on any net touch".
    pub fn with_stealth_policy(&self, policy: StealthPolicy) -> Self {
        let mut c = self.clone();
        c.stealth_policy = policy;
        c
    }

    /// Fail-closed net guard. Lanes MUST call this before any outbound request.
    /// Under `StealthPolicy::Always` with no stealth profile injected, this returns
    /// an error so the lane REFUSES to touch the net bare (no leaking the real
    /// fingerprint). Under `Preferred`/`Off` it's a no-op. This is what makes
    /// "fingerprint FIRST, ALWAYS" a structural guarantee, not a hope.
    pub fn net_guard(&self, url: &str) -> anyhow::Result<()> {
        if self.stealth_policy == StealthPolicy::Always && self.stealth.is_none() {
            anyhow::bail!(
                "stealth policy is Always but no Percyphon fingerprint injected — \
                 refusing bare net access to {url} (fail-closed; inject a persona \
                 via ScrapeCtx::with_stealth or relax the policy)"
            );
        }
        Ok(())
    }

    /// Apply the stealth profile (if any) to a reqwest RequestBuilder: override
    /// UA + add stealth headers. Lanes call this on every request they build.
    /// Respects `StealthPolicy::Off` (skips application even if a profile exists).
    pub fn apply_stealth(&self, mut rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.stealth_policy == StealthPolicy::Off {
            return rb;
        }
        if let Some(ref s) = self.stealth {
            if let Some(ref ua) = s.user_agent {
                rb = rb.header(reqwest::header::USER_AGENT, ua);
            }
            if let Some(ref al) = s.accept_language {
                rb = rb.header(reqwest::header::ACCEPT_LANGUAGE, al);
            }
            for (k, v) in &s.headers {
                rb = rb.header(k.as_str(), v.as_str());
            }
        }
        rb
    }

    /// Resolve the effective controls for a scraper (layered + clamped to limits),
    /// and configure this ctx's governor for the scraper's domain FROM those
    /// controls. Call before running a scraper so manual settings drive politeness.
    pub fn prepare(
        &self,
        id: &str,
        domain: &str,
        limits: &crate::controls::ScraperLimits,
    ) -> crate::controls::ScraperControls {
        let controls = self.config.resolve(id, domain, limits);
        self.governor.configure(
            domain,
            controls.concurrency.max(1) as usize,
            controls.min_delay_ms,
        );
        controls
    }

    /// Acquire a per-domain politeness permit for the given URL before a request.
    /// Every lane calls this; it caps concurrency per host and enforces spacing
    /// per the controls-configured governor.
    pub async fn acquire_for(&self, url: &str) {
        let domain = crate::governor::domain_of(url);
        self.governor.acquire(&domain).await;
    }
}

/// The single trait every scraper organelle implements.
#[async_trait]
pub trait Scraper: Send + Sync {
    /// Unique, stable id (e.g. "TIN_002").
    fn id(&self) -> &'static str;

    /// Static metadata.
    fn meta(&self) -> ScraperMeta;

    /// Declared known source limits — the NOTED maximum the effective controls are
    /// clamped to. Default: everything unknown (never a fabricated number). A
    /// scraper with a known rate limit / concurrency cap overrides this.
    fn limits(&self) -> crate::controls::ScraperLimits {
        crate::controls::ScraperLimits::unknown("no known published limit")
    }

    /// The primary host for this scraper's source, used to key the rate governor.
    /// Default derives nothing; scrapers that hit one host should override or the
    /// lane derives it per-URL via `governor::domain_of`.
    fn primary_domain(&self) -> Option<&'static str> {
        None
    }

    /// The scraper's default lane config (the declarative body: url template,
    /// selectors/json paths, pagination, auth-env). Surfaces show this so the
    /// operator sees exactly what a run will do. Overridable via scrapers.toml.
    /// Default: NonHarvest with null params (for modules that don't use a lane).
    fn default_config(&self) -> crate::controls::LaneConfig {
        crate::controls::LaneConfig::NonHarvest(serde_json::Value::Null)
    }

    /// Harvest the source with the given params, using the shared context.
    async fn run(&self, ctx: &ScrapeCtx, params: ScrapeParams) -> anyhow::Result<ScrapeRecord>;
}
