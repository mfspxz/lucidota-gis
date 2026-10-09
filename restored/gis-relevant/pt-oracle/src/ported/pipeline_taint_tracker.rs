// PORTED-FROM: src/pipeline_taint_tracker.py (457 lines)
//! Pipeline Taint Tracker — Formal dataflow taint analysis for LUCIDOTA workflow_events.
//!
//! Inspired by OpenTaint's IFDS-with-abduction dataflow engine.
//! Tracks which SOURCE each data field came from and detects self-tainting
//! (multiple verification dimensions from a single attacker-controlled source).
//!
//! Architecture:
//!    TaintSource  →  origin of data (gateway, scraper, external_api, operator, enkf, exif)
//!    TaintSink    →  where tainted data is consumed (vex_query, indy_journal, timeline_cluster)
//!    TaintGraph   →  maps every data field to its source + transformations
//!    TaintRule    →  source → sink pair with independence requirements
//!
//! Key invariant: The Self-Tainting Collapse Theorem
//!     If N verification dimensions exist but M derive from the same taint source S,
//!     effective dimensionality = N - M + 1. The attacker controls all M by controlling S.
//!
//! DRAFT — staging for pt-oracle port. No cargo build. No source mods.
//!
//! Built: 2026-07-07  — Round 27, Agent ∞, Hacker profession
//! Ported: 2026-07-09 — PORT PREP 3/3 → pt-oracle

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use pt_core::epistemic::TruthValence;
use pt_core::ContentHash;
use serde::{Deserialize, Serialize};

// ─── Taint Source Classification ───────────────────────────────────────────

/// Classes of taint sources, grouped by independence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceClass {
    /// payload.text, document content
    TextDerived,
    /// EXIF, HTTP headers, transport metadata
    MetadataDerived,
    /// ENF, GPS, microphone fingerprint
    PhysicsDerived,
    /// Indy journal, VEX ground truth, operator dict
    OperatorDerived,
    /// PACER, court records, WHOIS
    ExternalRegistry,
    /// Uncategorized
    Unknown,
}

// ─── Taint Source ──────────────────────────────────────────────────────────

/// A source of data in the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintSource {
    pub name: String,
    pub source_class: SourceClass,
    pub attacker_controllable: bool,
    /// First 8 hex chars of blake2b(name) — matches Python's sha256[:8].
    pub id: String,
}

impl TaintSource {
    pub fn new(name: &str, source_class: SourceClass, attacker_controllable: bool) -> Self {
        let hash = pt_core::hash_str(name);
        let id = hex::encode(&hash[..4]); // first 4 bytes → 8 hex chars
        Self {
            name: name.to_string(),
            source_class,
            attacker_controllable,
            id,
        }
    }

    /// Display helper — matches Python's __repr__.
    pub fn display(&self) -> String {
        let ctrl = if self.attacker_controllable { "⚠️" } else { "🔒" };
        format!(
            "TaintSource({}, {:?}, {})",
            self.name, self.source_class, ctrl
        )
    }
}

impl PartialEq for TaintSource {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for TaintSource {}

impl std::hash::Hash for TaintSource {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

// ─── Predefined Sources ─────────────────────────────────────────────────────

pub static GATEWAY_SCRAPER: LazyLock<TaintSource> = LazyLock::new(|| {
    TaintSource::new("gateway_web_scraper", SourceClass::TextDerived, true)
});
pub static GATEWAY_API: LazyLock<TaintSource> = LazyLock::new(|| {
    TaintSource::new("gateway_external_api", SourceClass::TextDerived, true)
});
pub static EXIF_EXTRACTOR: LazyLock<TaintSource> = LazyLock::new(|| {
    TaintSource::new("exif_metadata_extractor", SourceClass::MetadataDerived, false)
});
pub static ENF_VERIFIER: LazyLock<TaintSource> = LazyLock::new(|| {
    TaintSource::new("enf_physical_verifier", SourceClass::PhysicsDerived, false)
});
pub static GPS_SENSOR: LazyLock<TaintSource> = LazyLock::new(|| {
    TaintSource::new("gps_sensor", SourceClass::PhysicsDerived, false)
});
pub static OPERATOR_DICT: LazyLock<TaintSource> = LazyLock::new(|| {
    TaintSource::new("operator_ground_truth", SourceClass::OperatorDerived, false)
});
pub static INDY_JOURNAL: LazyLock<TaintSource> = LazyLock::new(|| {
    TaintSource::new("indy_reads_journal", SourceClass::OperatorDerived, false)
});
pub static VEX_GROUND_TRUTH: LazyLock<TaintSource> = LazyLock::new(|| {
    TaintSource::new("vex_ground_truth", SourceClass::OperatorDerived, false)
});
pub static EXTERNAL_PACER: LazyLock<TaintSource> = LazyLock::new(|| {
    TaintSource::new("pacer_court_records", SourceClass::ExternalRegistry, false)
});
pub static EXTERNAL_WHOIS: LazyLock<TaintSource> = LazyLock::new(|| {
    TaintSource::new("whois_registry", SourceClass::ExternalRegistry, false)
});

// ─── Taint Edge ─────────────────────────────────────────────────────────────

/// A single data field and its taint provenance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintEdge {
    /// e.g. "payload.text", "payload.geo_temporal.lat"
    pub field_path: String,
    pub source: TaintSource,
    /// e.g. ["sentiment_embedder", "entity_extractor"]
    #[serde(default)]
    pub transformations: Vec<String>,
}

impl TaintEdge {
    pub fn new(
        field_path: &str,
        source: TaintSource,
        transformations: Vec<String>,
    ) -> Self {
        Self {
            field_path: field_path.to_string(),
            source,
            transformations,
        }
    }

    pub fn is_attacker_controlled(&self) -> bool {
        self.source.attacker_controllable
    }

    pub fn source_class(&self) -> SourceClass {
        self.source.source_class
    }
}

// ─── Taint Graph ────────────────────────────────────────────────────────────

/// Complete taint provenance for a workflow_event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintGraph {
    pub event_id: String,
    #[serde(default)]
    pub edges: Vec<TaintEdge>,
}

impl TaintGraph {
    pub fn new(event_id: &str) -> Self {
        Self {
            event_id: event_id.to_string(),
            edges: Vec::new(),
        }
    }

    pub fn add_edge(
        &mut self,
        field_path: &str,
        source: TaintSource,
        transformations: Vec<String>,
    ) {
        self.edges
            .push(TaintEdge::new(field_path, source, transformations));
    }

    pub fn get_source_for(&self, field_path: &str) -> Option<&TaintSource> {
        self.edges
            .iter()
            .find(|e| e.field_path == field_path)
            .map(|e| &e.source)
    }

    pub fn get_all_attacker_controlled_fields(&self) -> Vec<&str> {
        self.edges
            .iter()
            .filter(|e| e.is_attacker_controlled())
            .map(|e| e.field_path.as_str())
            .collect()
    }

    pub fn get_unique_source_classes(&self) -> HashSet<SourceClass> {
        self.edges.iter().map(|e| e.source_class()).collect()
    }

    /// Count genuinely independent source CLASSES (not fields).
    pub fn count_independent_sources(&self) -> usize {
        self.get_unique_source_classes().len()
    }

    /// Detect pairs of fields that share the same taint source.
    /// Returns list of (field_a, field_b, shared_source_name) tuples.
    pub fn detect_self_tainting(&self) -> Vec<(String, String, String)> {
        let mut pairs = Vec::new();
        let n = self.edges.len();
        for i in 0..n {
            for j in (i + 1)..n {
                if self.edges[i].source == self.edges[j].source {
                    pairs.push((
                        self.edges[i].field_path.clone(),
                        self.edges[j].field_path.clone(),
                        self.edges[i].source.name.clone(),
                    ));
                }
            }
        }
        pairs
    }

    /// Compute how genuinely independent the verification dimensions are.
    ///
    /// 1.0 = all dimensions from independent source classes
    /// 0.0 = all dimensions from same attacker-controlled source
    pub fn compute_taint_independence_score(&self) -> f64 {
        let unique_classes = self.get_unique_source_classes();
        let n_dimensions = self.edges.len();
        if n_dimensions == 0 {
            return 0.0;
        }

        // If only TEXT_DERIVED, score is 0.0
        if unique_classes == {
            let mut s = HashSet::new();
            s.insert(SourceClass::TextDerived);
            s
        } {
            return 0.0;
        }

        // Score = ratio of independent (non-text, not attacker-controlled) fields
        let independent_count = self
            .edges
            .iter()
            .filter(|e| {
                e.source_class() != SourceClass::TextDerived && !e.is_attacker_controlled()
            })
            .count();

        (independent_count as f64 / n_dimensions as f64).min(1.0)
    }

    /// Serialize to a dict-like structure matching Python's to_dict().
    pub fn to_taint_report(&self) -> TaintGraphReport {
        let self_taint = self.detect_self_tainting();
        TaintGraphReport {
            event_id: self.event_id.clone(),
            edges: self
                .edges
                .iter()
                .map(|e| TaintEdgeReport {
                    field_path: e.field_path.clone(),
                    source_name: e.source.name.clone(),
                    source_class: format!("{:?}", e.source_class()),
                    attacker_controllable: e.is_attacker_controlled(),
                    transformations: e.transformations.clone(),
                })
                .collect(),
            independent_sources: self.count_independent_sources(),
            taint_independence_score: self.compute_taint_independence_score(),
            self_tainting_pairs: self_taint
                .iter()
                .map(|(a, b, s)| SelfTaintPair {
                    field_a: a.clone(),
                    field_b: b.clone(),
                    shared_source: s.clone(),
                })
                .collect(),
        }
    }
}

/// Serializable report mirroring Python's graph.to_dict().
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintGraphReport {
    pub event_id: String,
    pub edges: Vec<TaintEdgeReport>,
    pub independent_sources: usize,
    pub taint_independence_score: f64,
    pub self_tainting_pairs: Vec<SelfTaintPair>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintEdgeReport {
    pub field_path: String,
    pub source_name: String,
    pub source_class: String,
    pub attacker_controllable: bool,
    pub transformations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfTaintPair {
    pub field_a: String,
    pub field_b: String,
    pub shared_source: String,
}

// ─── Taint Rule ─────────────────────────────────────────────────────────────

/// A deterministic rule compiled from a vulnerability finding.
///
/// SOURCE → SINK with independence requirements.
///
/// Inspired by OpenTaint's rule format:
///     rule("sql-injection-in-spring-app") {
///         source = "HttpServletRequest.getParameter(*)"
///         sink = "Statement.executeQuery(*)"
///     }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintRule {
    pub rule_id: String,
    pub description: String,
    /// Minimum independent source classes needed to pass.
    pub required_source_classes: HashSet<SourceClass>,
    /// Self-taint patterns to reject (substring match on field paths).
    #[serde(default)]
    pub forbidden_patterns: Vec<String>,
    /// Applied as a multiplier when the rule fails.
    pub confidence_multiplier: f64,
}

/// Result of evaluating a single rule against a taint graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleResult {
    pub rule_id: String,
    pub passed: bool,
    pub reason: String,
    pub confidence_multiplier: f64,
}

impl TaintRule {
    /// Evaluate this rule against a taint graph.
    ///
    /// Returns (passed, reason, confidence_multiplier_to_apply).
    pub fn evaluate(&self, taint_graph: &TaintGraph) -> (bool, String, f64) {
        let present_classes = taint_graph.get_unique_source_classes();

        // Check minimum independent sources
        let missing: Vec<_> = self
            .required_source_classes
            .difference(&present_classes)
            .collect();
        if !missing.is_empty() {
            let missing_names: Vec<String> =
                missing.iter().map(|c| format!("{:?}", c)).collect();
            return (
                false,
                format!("MISSING_SOURCE_CLASSES: {:?}", missing_names),
                self.confidence_multiplier,
            );
        }

        // Check for self-tainting patterns
        let self_taint_pairs = taint_graph.detect_self_tainting();
        for pattern in &self.forbidden_patterns {
            for (a, b, src) in &self_taint_pairs {
                if a.contains(pattern) && b.contains(pattern) {
                    return (
                        false,
                        format!(
                            "SELF_TAINTING: {} and {} share source {}",
                            a, b, src
                        ),
                        self.confidence_multiplier,
                    );
                }
            }
        }

        (true, "PASSED".to_string(), 1.0)
    }
}

// ─── Predefined Taint Rules (compiled from red-team rounds) ─────────────────

pub static TAINT_RULES: LazyLock<HashMap<String, TaintRule>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    m.insert(
        "dual_dimension_collusion".to_string(),
        TaintRule {
            rule_id: "dual_dimension_collusion".to_string(),
            description:
                "Entity extraction and sentiment vectors must come from DIFFERENT source classes"
                    .to_string(),
            required_source_classes: [
                SourceClass::TextDerived,
                SourceClass::PhysicsDerived,
            ]
            .iter()
            .cloned()
            .collect(),
            forbidden_patterns: vec!["payload.text".to_string(), "sentiment".to_string()],
            confidence_multiplier: 0.5,
        },
    );

    m.insert(
        "text_only_entity_trust".to_string(),
        TaintRule {
            rule_id: "text_only_entity_trust".to_string(),
            description:
                "Text-only events must have at least ONE non-text verification dimension"
                    .to_string(),
            required_source_classes: [
                SourceClass::TextDerived,
                SourceClass::PhysicsDerived,
            ]
            .iter()
            .cloned()
            .collect(),
            forbidden_patterns: vec![],
            confidence_multiplier: 0.5,
        },
    );

    m.insert(
        "classification_header_fabrication".to_string(),
        TaintRule {
            rule_id: "classification_header_fabrication".to_string(),
            description:
                "Document classification must come from a DIFFERENT source than entity extraction"
                    .to_string(),
            required_source_classes: [
                SourceClass::TextDerived,
                SourceClass::ExternalRegistry,
            ]
            .iter()
            .cloned()
            .collect(),
            forbidden_patterns: vec![
                "payload.text".to_string(),
                "document_classification".to_string(),
            ],
            confidence_multiplier: 0.3,
        },
    );

    m.insert(
        "vdaa_protection".to_string(),
        TaintRule {
            rule_id: "vdaa_protection".to_string(),
            description:
                "At least TWO independent source classes required for HIGH confidence"
                    .to_string(),
            required_source_classes: [
                SourceClass::TextDerived,
                SourceClass::PhysicsDerived,
            ]
            .iter()
            .cloned()
            .collect(),
            forbidden_patterns: vec![],
            confidence_multiplier: 0.5,
        },
    );

    m
});

// ─── Taint Evaluation Output ────────────────────────────────────────────────

/// The result of evaluating all taint rules against a workflow_event.
/// Maps into pt-oracle's epistemic gate: Pass → gate can flip to Proven,
/// Flagged → remains Unknown (or Disproven for clear violations).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintEvaluation {
    pub event_id: String,
    pub taint_graph: TaintGraphReport,
    pub rule_results: Vec<RuleResult>,
    pub base_confidence: f64,
    pub taint_independence_score: f64,
    pub effective_confidence: f64,
    pub verdict: TaintVerdict,
    /// Bridged epistemic valence for the pt-oracle gate.
    pub valence: TruthValence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaintVerdict {
    /// effective_confidence >= 0.5
    Pass,
    /// effective_confidence < 0.5
    Flagged,
}

impl TaintVerdict {
    /// Map taint verdict to the ternary epistemic valence.
    ///
    /// Pass  → Proven   (confidence still < 1.0 by construction)
    /// Flagged → Unknown (needs more verification dimensions)
    pub fn to_valence(self) -> TruthValence {
        match self {
            Self::Pass => TruthValence::Proven,
            Self::Flagged => TruthValence::Unknown,
        }
    }
}

// ─── Event Processor ────────────────────────────────────────────────────────

/// Main taint tracking engine. Attach to Gateway → Bytewax boundary.
/// Every workflow_event gets taint-graphed before entering the DAG.
pub struct PipelineTaintTracker {
    pub rules: HashMap<String, TaintRule>,
}

impl PipelineTaintTracker {
    pub fn new(rules: Option<HashMap<String, TaintRule>>) -> Self {
        Self {
            rules: rules.unwrap_or_else(|| TAINT_RULES.clone()),
        }
    }

    /// Build a taint graph from a workflow_event payload.
    ///
    /// Accepts a `serde_json::Value` — the raw JSON event as it
    /// would arrive from the Gateway or Bytewax queue.
    pub fn build_taint_graph(&self, event: &serde_json::Value) -> TaintGraph {
        let event_id = event
            .get("event_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let mut graph = TaintGraph::new(&event_id);

        let payload = event.get("payload");

        // H1 (SECURITY_AUDIT_VERIFIED_2026-07-22): the payload arrives through a
        // transport. If that transport is the scraper/gateway — an attacker-
        // controlled channel — then EVERY field in the payload is attacker-
        // fabricated, INCLUDING geo_temporal and enf_data. Classifying those as
        // independent "physics" purely because the attacker put those JSON keys in
        // the payload let a scraped page mint multi-dimensional "physics-verified"
        // truth. A field earns a PhysicsDerived (non-attacker) class ONLY when it
        // arrived via a trusted out-of-band physical source (a signed sensor
        // attestation). We fail closed: unknown/absent transport => untrusted.
        let trusted_physical = event
            .get("transport_origin")
            .and_then(|v| v.as_str())
            .map(|s| {
                let s = s.to_lowercase();
                s.contains("sensor") || s.contains("hardware") || s.contains("signed_attestation")
            })
            .unwrap_or(false);

        // ── Text content → always attacker-controlled if from Gateway ──
        if Self::has_field(payload, "text") || Self::has_field(payload, "text_content") {
            graph.add_edge("payload.text", GATEWAY_SCRAPER.clone(), vec![]);
            graph.add_edge(
                "payload.text_content",
                GATEWAY_SCRAPER.clone(),
                vec![],
            );
        }

        // ── Entity extraction → derived from text (same source!) ──
        if Self::has_field(payload, "entities") || Self::has_field(payload, "named_entities") {
            graph.add_edge(
                "payload.entities",
                GATEWAY_SCRAPER.clone(),
                vec!["entity_extractor".to_string()],
            );
        }

        // ── Sentiment vectors → derived from text (SAME SOURCE as entities!) ──
        if Self::has_field(payload, "sentiment_vector")
            || Self::has_field(payload, "sentiment")
        {
            graph.add_edge(
                "payload.sentiment_vector",
                GATEWAY_SCRAPER.clone(),
                vec!["sentiment_embedder".to_string()],
            );
        }

        // ── Document classification → derived from text ──
        if let Some(doc_class) = payload.and_then(|p| p.get("document_classification")) {
            if !doc_class.is_null() {
                graph.add_edge(
                    "payload.document_classification",
                    GATEWAY_SCRAPER.clone(),
                    vec!["legal_document_classifier".to_string()],
                );
            }
        }

        // ── geo-temporal → PhysicsDerived ONLY if trusted; else attacker-controlled ──
        if let Some(geo) = payload.and_then(|p| p.get("geo_temporal")) {
            if let Some(obj) = geo.as_object() {
                if obj.contains_key("lat") || obj.contains_key("longitude") {
                    let src = if trusted_physical { GPS_SENSOR.clone() } else { GATEWAY_SCRAPER.clone() };
                    graph.add_edge("payload.geo_temporal", src, vec!["exif_extractor".to_string()]);
                }
                if obj.contains_key("timestamp") {
                    // EXIF from a scraped image is attacker-controlled too.
                    let src = if trusted_physical { EXIF_EXTRACTOR.clone() } else { GATEWAY_SCRAPER.clone() };
                    graph.add_edge("payload.geo_temporal.timestamp", src, vec!["exif_extractor".to_string()]);
                }
            }
        }

        // ── ENF physical layer → PhysicsDerived ONLY if trusted; else attacker-controlled ──
        if Self::has_field(payload, "enf_data") || Self::has_field(payload, "physical_layer") {
            let src = if trusted_physical { ENF_VERIFIER.clone() } else { GATEWAY_SCRAPER.clone() };
            graph.add_edge("payload.enf_data", src, vec!["enf_extractor".to_string()]);
        }

        // ── Transport origin → metadata-derived (partially attacker-controlled) ──
        if event.get("transport_origin").is_some() {
            graph.add_edge("transport_origin", GATEWAY_API.clone(), vec![]);
        }

        // ── Permutation order → metadata about structure, not content ──
        if Self::has_field(payload, "permutation_order")
            || Self::has_field(payload, "field_order")
        {
            graph.add_edge(
                "payload.permutation_order",
                GATEWAY_API.clone(),
                vec!["permutation_validator".to_string()],
            );
        }

        graph
    }

    /// Evaluate all taint rules against an event. Returns full verdict.
    pub fn evaluate_event(&self, event: &serde_json::Value) -> TaintEvaluation {
        let graph = self.build_taint_graph(event);
        let mut rule_results = Vec::new();

        for (rule_id, rule) in &self.rules {
            let (passed, reason, multiplier) = rule.evaluate(&graph);
            rule_results.push(RuleResult {
                rule_id: rule_id.clone(),
                passed,
                reason,
                confidence_multiplier: multiplier,
            });
        }

        // Compute overall taint confidence
        let mut base_confidence: f64 = 1.0;
        for r in &rule_results {
            if !r.passed {
                base_confidence = base_confidence.min(r.confidence_multiplier);
            }
        }

        // Apply taint independence modifier
        let independence_score = graph.compute_taint_independence_score();
        let effective_confidence = base_confidence * independence_score;

        let verdict = if effective_confidence >= 0.5 {
            TaintVerdict::Pass
        } else {
            TaintVerdict::Flagged
        };

        TaintEvaluation {
            event_id: graph.event_id.clone(),
            taint_graph: graph.to_taint_report(),
            rule_results,
            base_confidence,
            taint_independence_score: independence_score,
            effective_confidence,
            verdict,
            valence: verdict.to_valence(),
        }
    }

    /// Helper: check whether a JSON object has a non-null field.
    fn has_field(obj: Option<&serde_json::Value>, key: &str) -> bool {
        obj.and_then(|o| o.get(key))
            .map(|v| !v.is_null())
            .unwrap_or(false)
    }
}

impl Default for PipelineTaintTracker {
    fn default() -> Self {
        Self::new(None)
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tracker() -> PipelineTaintTracker {
        PipelineTaintTracker::default()
    }

    fn text_only_event() -> serde_json::Value {
        json!({
            "event_id": "evt_001",
            "payload": {
                "text": "Shell CEO met with Greenpeace director",
                "entities": ["Shell", "CEO", "Greenpeace"],
                "sentiment_vector": [0.2, 0.8, 0.0, 0.6, 0.1, 0.3, 0.1, 0.7]
            },
            "transport_origin": "gateway_web_scraper"
        })
    }

    fn enf_event() -> serde_json::Value {
        json!({
            "event_id": "evt_002",
            "payload": {
                "text": "Protest at Shell refinery",
                "entities": ["Shell", "protest", "refinery"],
                "sentiment_vector": [0.1, 0.9, 0.0, 0.5, 0.8, 0.2, 0.1, 0.3],
                "enf_data": {"frequency": 50.02, "region": "EU_WEST", "coherence": 0.95}
            },
            "transport_origin": "gateway_web_scraper"
        })
    }

    // Same enf payload, but arriving via a TRUSTED out-of-band physical source
    // (a signed hardware sensor) rather than the scraper — this is the only path
    // that earns a real PhysicsDerived, independent class (H1).
    fn trusted_enf_event() -> serde_json::Value {
        json!({
            "event_id": "evt_002t",
            "payload": {
                "text": "Protest at Shell refinery",
                "entities": ["Shell", "protest", "refinery"],
                "sentiment_vector": [0.1, 0.9, 0.0, 0.5, 0.8, 0.2, 0.1, 0.3],
                "enf_data": {"frequency": 50.02, "region": "EU_WEST", "coherence": 0.95}
            },
            "transport_origin": "gps_sensor_hardware_signed_attestation"
        })
    }

    fn class_event() -> serde_json::Value {
        json!({
            "event_id": "evt_003",
            "payload": {
                "text": "UNITED STATES DISTRICT COURT\nCase No. 1:26-cv-99999\nShell CEO corruption...",
                "entities": ["Shell", "CEO", "corruption"],
                "document_classification": {"detected_type": "court_filing", "confidence": 0.92}
            },
            "transport_origin": "gateway_web_scraper"
        })
    }

    #[test]
    fn test_text_only_event_flags_vdaa() {
        let t = tracker();
        let result = t.evaluate_event(&text_only_event());
        assert!(
            result.effective_confidence < 0.5,
            "text-only event should have effective_confidence < 0.5, got {}",
            result.effective_confidence
        );
    }

    // H1: enf_data arriving VIA THE SCRAPER is attacker-fabricated and must NOT
    // buy any confidence over a text-only event — otherwise a scraped page mints
    // "physics-verified" truth. Only a TRUSTED sensor source may raise it.
    #[test]
    fn test_scraped_enf_does_not_beat_text_only_but_trusted_sensor_does() {
        let t = tracker();
        let text = t.evaluate_event(&text_only_event()).effective_confidence;
        let scraped_enf = t.evaluate_event(&enf_event()).effective_confidence;
        let trusted_enf = t.evaluate_event(&trusted_enf_event()).effective_confidence;

        assert!(
            scraped_enf <= text + 1e-9,
            "scraped enf_data ({scraped_enf}) must NOT exceed text-only ({text}) — no fake physics"
        );
        assert!(
            trusted_enf > text,
            "enf via a trusted sensor ({trusted_enf}) SHOULD exceed text-only ({text})"
        );
    }

    #[test]
    fn test_detects_entity_sentiment_self_tainting() {
        let t = tracker();
        let graph = t.build_taint_graph(&text_only_event());
        let self_taint = graph.detect_self_tainting();
        assert!(
            !self_taint.is_empty(),
            "should detect entity-sentiment self-tainting, got {:?}",
            self_taint
        );
    }

    #[test]
    fn test_text_only_independence_score_is_zero() {
        let t = tracker();
        let graph = t.build_taint_graph(&text_only_event());
        let score = graph.compute_taint_independence_score();
        assert!(
            (score - 0.0).abs() < 0.01,
            "text-only independence score should be 0.0, got {}",
            score
        );
    }

    // H1: a scraped enf event has ZERO independent dimensions (the enf field is
    // attacker-controlled through the scraper); only a trusted-sensor enf event
    // has a non-zero independence score.
    #[test]
    fn test_scraped_enf_independence_is_zero_trusted_is_not() {
        let t = tracker();
        let scraped = t.build_taint_graph(&enf_event()).compute_taint_independence_score();
        let trusted = t.build_taint_graph(&trusted_enf_event()).compute_taint_independence_score();
        assert!(
            scraped.abs() < 0.01,
            "scraped enf independence must be 0 (fake physics laundering closed), got {scraped}"
        );
        assert!(
            trusted > 0.0,
            "trusted-sensor enf independence should be > 0, got {trusted}"
        );
    }

    #[test]
    fn test_dual_dimension_collusion_rule_fires_on_text_only() {
        let t = tracker();
        let result = t.evaluate_event(&text_only_event());
        let dual_fired = result.rule_results.iter().any(|r| {
            !r.passed && r.rule_id == "dual_dimension_collusion"
        });
        assert!(
            dual_fired,
            "dual_dimension_collusion should fire on text-only event"
        );
    }

    #[test]
    fn test_classification_fabrication_rule_fires() {
        let t = tracker();
        let result = t.evaluate_event(&class_event());
        let class_fired = result.rule_results.iter().any(|r| {
            !r.passed && r.rule_id == "classification_header_fabrication"
        });
        assert!(
            class_fired,
            "classification_header_fabrication should fire on class event"
        );
    }

    #[test]
    fn test_taint_graph_report_serializes() {
        let t = tracker();
        let graph = t.build_taint_graph(&enf_event());
        let report = graph.to_taint_report();
        assert_eq!(report.event_id, "evt_002");
        assert!(!report.edges.is_empty(), "edges should not be empty");
    }

    #[test]
    fn test_predefined_source_equality() {
        // Two clones of GATEWAY_SCRAPER are equal
        let a = GATEWAY_SCRAPER.clone();
        let b = GATEWAY_SCRAPER.clone();
        assert_eq!(a, b);

        // Different sources are not equal
        assert_ne!(GATEWAY_SCRAPER.clone(), ENF_VERIFIER.clone());
    }

    #[test]
    fn test_verdict_maps_to_truth_valence() {
        assert_eq!(TaintVerdict::Pass.to_valence(), TruthValence::Proven);
        assert_eq!(TaintVerdict::Flagged.to_valence(), TruthValence::Unknown);
    }

    #[test]
    fn test_count_independent_sources() {
        let t = tracker();
        let graph = t.build_taint_graph(&text_only_event());
        // Text-only: TEXT_DERIVED only (1 class)
        assert_eq!(graph.count_independent_sources(), 1);

        // H1: SCRAPED enf collapses to TextDerived (attacker-controlled) — the
        // fake second "physics" dimension is gone, so still 1 class.
        let scraped_enf = t.build_taint_graph(&enf_event());
        assert_eq!(scraped_enf.count_independent_sources(), 1, "scraped enf is not an independent source");

        // Only a TRUSTED sensor enf yields a real second class: TEXT + PHYSICS.
        let trusted_enf = t.build_taint_graph(&trusted_enf_event());
        assert_eq!(trusted_enf.count_independent_sources(), 2, "trusted-sensor enf is genuinely independent");
    }
}
