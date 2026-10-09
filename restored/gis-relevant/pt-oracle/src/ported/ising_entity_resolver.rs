// ising_entity_resolver.rs — Physics-as-Computation Entity Resolution
//
// PORTED-FROM: src/ising_entity_resolver.py (659 lines, Python)
//
// Maps entity resolution to an Ising spin glass problem:
//   - Each entity pair has coupling J_{ij} derived from multi-dimensional evidence
//   - Simulated annealing finds the global minimum energy configuration
//   - Clusters emerge from aligned spin states
//   - Confidence is the energy gap between found and random configurations
//
// KEY INSIGHT (arXiv:2607.02112 — Acoustic Ising Machine):
//   The physical Ising machine achieves 4 orders of magnitude better stability
//   than optical CIMs by using acoustic waves instead of light. Pipeline analogy:
//   greedy clustering = optically unstable (perturbation-sensitive)
//   Ising global optimization = acoustically stable (reproducible clusters)
//
// CRITICAL CAVEAT: sentiment vectors come from payload.text, entity names also
// come from payload.text — both clustering axes from same attacker-controlled
// source. Apply DIMENSIONAL_INDEPENDENCE_WEIGHT penalty when sources overlap.
//
// Module fits into pt-oracle as the entity resolution gate:
//   workflow_events → IsingEntityResolver::resolve() → OracleOutput::EntityCluster
//   Each cluster is a candidate for promotion; confidence gates truth-layer routing.
//
// TERNARY EPISTEMOLOGY (pt_core integration):
//   +1 PROVEN  = cluster confirmed by ≥2 independent evidence dimensions
//    0 UNKNOWN = cluster exists but only text evidence
//   -1 DISPROVEN = anti-correlated entities forced apart by physics evidence

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────
// TERNARY EPISTEMOLOGY — matches pt_core::EpistemicGate.valence
// ─────────────────────────────────────────────────────────────────────

/// Ternary truth state for entity resolution.
/// PORTED-FROM: src/ising_entity_resolver.py §TERNARY EPISTEMOLOGY
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i8)]
pub enum TruthValence {
    /// +1 — proven by multi-dimensional evidence
    Proven = 1,
    /// 0  — candidate, insufficient evidence
    Unknown = 0,
    /// -1 — disproven / anti-correlated
    Disproven = -1,
}

// ─────────────────────────────────────────────────────────────────────
// ENTITY — PORTED-FROM: src/ising_entity_resolver.py Entity dataclass
// ─────────────────────────────────────────────────────────────────────

/// An extracted entity from a workflow event.
/// PORTED-FROM: src/ising_entity_resolver.py:46-92 Entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    /// Entity name (extracted from payload.text)
    pub name: String,
    /// Owning workflow event ID
    pub event_id: String,
    /// Transport origin: Gateway, VEX, Indy, etc.
    pub source: String,
    /// Provenance chain (ordered list of agents that touched this event)
    pub provenance_chain: Vec<String>,

    // ── Multi-dimensional evidence ──
    /// GPS coordinates (lat, lon) — metadata-derived
    pub gps: Option<(f64, f64)>,
    /// ISO 8601 timestamp
    pub timestamp: Option<String>,
    /// 8-dim sentiment vector [pos, neg, neut, comp, anger, fear, sad, joy]
    pub sentiment_vector: Option<Vec<f64>>,
    /// ENF grid frequency region — physics-derived
    pub enf_fingerprint: Option<String>,
    /// Provenance-based confidence [0.0, 1.0]
    pub confidence: f64,

    // ── Source tracking for dimensional independence check ──
    /// Where entity NAME came from
    pub text_source: String,
    /// Where sentiment vector came from
    pub sentiment_source: String,
    /// Where GPS came from
    pub geo_source: String,
    /// Where ENF came from
    pub enf_source: String,
}

impl Entity {
    /// How many INDEPENDENT evidence dimensions does this entity have?
    /// Returns score in [0.0, 1.0] — max 4 independent source classes.
    /// PORTED-FROM: src/ising_entity_resolver.py:67-92 dimensional_independence_score
    pub fn dimensional_independence_score(&self) -> f64 {
        let mut sources: BTreeSet<&str> = BTreeSet::new();

        if !self.name.is_empty() {
            sources.insert(self.text_source.as_str());
        }
        if self.sentiment_vector.is_some() {
            sources.insert(self.sentiment_source.as_str());
        }
        if self.gps.is_some() {
            sources.insert(self.geo_source.as_str());
        }
        if self.enf_fingerprint.is_some() {
            sources.insert(self.enf_source.as_str());
        }

        // Count unique source CLASSES
        let mut classes: BTreeSet<&str> = BTreeSet::new();
        for s in &sources {
            if s.contains("audio") || s.contains("enf") {
                classes.insert("physics");
            } else if s.contains("geo") || s.contains("exif") {
                classes.insert("metadata");
            } else if s.contains("text") || s.contains("payload") {
                classes.insert("text");
            } else {
                classes.insert("external");
            }
        }
        classes.len() as f64 / 4.0 // max 4 classes
    }
}

// ─────────────────────────────────────────────────────────────────────
// SPIN CONFIG — PORTED-FROM: src/ising_entity_resolver.py:96-102
// ─────────────────────────────────────────────────────────────────────

/// A single spin configuration — one possible entity clustering.
/// PORTED-FROM: src/ising_entity_resolver.py:96-102 SpinConfig
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpinConfig {
    /// entity_name → cluster_id
    pub spins: HashMap<String, usize>,
    /// Ising energy of this configuration (lower = better)
    pub energy: f64,
    /// cluster_id → list of entity names in that cluster
    pub cluster_assignments: HashMap<usize, Vec<String>>,
}

impl SpinConfig {
    pub fn new() -> Self {
        Self {
            spins: HashMap::new(),
            energy: f64::INFINITY,
            cluster_assignments: HashMap::new(),
        }
    }
}

impl Default for SpinConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────
// COUPLING MATRIX — PORTED-FROM: src/ising_entity_resolver.py:104-109
// ─────────────────────────────────────────────────────────────────────

/// Evidence dimension contribution to a coupling edge.
/// PORTED-FROM: src/ising_entity_resolver.py:108 dimensional_weights
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionWeight {
    /// Evidence dimension weight applied
    pub weight: f64,
    /// Raw similarity score for this dimension
    pub similarity: f64,
}

/// J_{ij} coupling terms between every pair of entities.
/// PORTED-FROM: src/ising_entity_resolver.py:104-109 CouplingMatrix
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouplingMatrix {
    /// (entity_a, entity_b) → coupling strength J_{ij}
    pub couplings: HashMap<(String, String), f64>,
    /// Per-edge breakdown: (entity_a, entity_b) → dimension → weight info
    pub dimensional_weights: HashMap<(String, String), BTreeMap<String, DimensionWeight>>,
    /// Ordered list of entity names
    pub entity_list: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────
// CONFIDENCE OUTPUT — PORTED-FROM: src/ising_entity_resolver.py:476-546
// ─────────────────────────────────────────────────────────────────────

/// Per-cluster confidence data.
/// PORTED-FROM: src/ising_entity_resolver.py:531-536
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfidence {
    pub members: Vec<String>,
    pub size: usize,
    pub independent_dimensions: Vec<String>,
    pub confidence: f64,
}

/// Multi-dimensional confidence for a cluster configuration.
/// PORTED-FROM: src/ising_entity_resolver.py:538-546 compute_confidence return
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionConfidence {
    /// Overall confidence [0.0, 1.0]
    pub overall_confidence: f64,
    /// How much better than random baseline
    pub energy_gap: f64,
    /// Ratio of independent dimensions used
    pub dimensional_score: f64,
    /// Minimum confidence based on modality class (text-only → 0.5 ceiling)
    pub modality_floor: f64,
    /// Per-cluster confidence breakdown
    pub per_cluster: BTreeMap<String, ClusterConfidence>,
    /// Method identifier
    pub method: String,
    /// Reference paper
    pub paper: String,
}

// ─────────────────────────────────────────────────────────────────────
// TIMELINE OUTPUT — PORTED-FROM: src/ising_entity_resolver.py:548-568
// ─────────────────────────────────────────────────────────────────────

/// Timeline-compatible JSON output.
/// PORTED-FROM: src/ising_entity_resolver.py:548-568 format_timeline_json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineOutput {
    pub method: String,
    pub version: String,
    pub timestamp: String,
    pub num_clusters: usize,
    pub clusters: Vec<TimelineCluster>,
    pub confidence: ResolutionConfidence,
    pub energy: f64,
    pub reference_paper: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineCluster {
    pub cluster_id: String,
    pub entities: Vec<String>,
    pub confidence: f64,
}

// ─────────────────────────────────────────────────────────────────────
// WORKFLOW EVENT — pt-oracle input format
// ─────────────────────────────────────────────────────────────────────

/// A workflow event as received by the pipeline.
/// Matches pt-oracle input conventions: payload-based, provenance-tracked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEvent {
    pub event_id: Option<String>,
    pub id: Option<String>,
    pub source: Option<String>,
    pub transport_origin: Option<String>,
    pub provenance_chain: Option<Vec<String>>,
    pub timestamp: Option<String>,
    pub payload: Option<WorkflowPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPayload {
    pub text_content: Option<String>,
    pub text: Option<String>,
    /// Pre-extracted entity names from NER
    pub entities: Option<Vec<String>>,
    /// 8-dim sentiment vector
    pub sentiment_vector: Option<Vec<f64>>,
    /// Legacy field name
    pub sentiment: Option<Vec<f64>>,
    /// Geo-temporal data
    pub geo_temporal: Option<GeoData>,
    /// ENF physical-layer data
    pub enf: Option<EnfData>,
    pub physical_layer: Option<PhysicalLayerData>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoData {
    pub latitude: Option<f64>,
    pub lat: Option<f64>,
    pub longitude: Option<f64>,
    pub lon: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnfData {
    pub region: Option<String>,
    pub grid_frequency: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicalLayerData {
    pub enf: Option<EnfData>,
}

// ─────────────────────────────────────────────────────────────────────
// ERROR TYPE
// ─────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ResolverError {
    #[error("insufficient entities for resolution: got {0}, need ≥2")]
    InsufficientEntities(usize),
    #[error("temporal parse error: {0}")]
    TemporalParse(String),
    #[error("invalid sentiment vector: expected 8 dimensions, got {0}")]
    InvalidSentimentVector(usize),
}

// ─────────────────────────────────────────────────────────────────────
// ISING ENTITY RESOLVER
// PORTED-FROM: src/ising_entity_resolver.py:112-568 IsingEntityResolver
// ─────────────────────────────────────────────────────────────────────

/// Ising-model entity resolution using simulated annealing.
///
/// Maps entity resolution to energy minimization:
///   E = -Σ_{i<j} J_{ij} σ_i σ_j - Σ_i h_i σ_i
///
/// where:
///   J_{ij} = weighted sum of evidence dimensions (text, geo, time, sentiment, ENF)
///   h_i    = prior probability entity is real (provenance chain strength)
///   σ_i    = entity i's cluster membership encoding
///
/// The global minimum energy configuration is the optimal clustering.
///
/// PORTED-FROM: src/ising_entity_resolver.py:112-124 IsingEntityResolver
#[derive(Debug, Clone)]
pub struct IsingEntityResolver {
    // ── Evidence dimension weights (tunable) ──
    /// Text Jaccard similarity weight
    pub text_weight: f64,
    /// GPS proximity weight (independent evidence → higher)
    pub geo_weight: f64,
    /// Temporal proximity weight
    pub time_weight: f64,
    /// Sentiment similarity weight (penalized: same source as text)
    pub sentiment_weight: f64,
    /// ENF coherence weight (physics-derived → highest)
    pub enf_weight: f64,
    /// Provenance chain similarity weight
    pub provenance_weight: f64,

    // ── Annealing parameters ──
    /// Initial temperature for simulated annealing
    pub t_initial: f64,
    /// Final temperature (convergence threshold)
    pub t_final: f64,
    /// Cooling rate per temperature step
    pub cooling_rate: f64,
    /// Monte Carlo steps per temperature level
    pub steps_per_temp: usize,

    // ── Dimensional independence ──
    /// Penalty multiplier when sentiment_source == text_source
    pub single_source_penalty: f64,
    /// Hard confidence ceiling for text-only events
    pub modality_confidence_floor: f64,
}

impl Default for IsingEntityResolver {
    /// Default weights match Python defaults.
    /// PORTED-FROM: src/ising_entity_resolver.py:126-143 __init__
    fn default() -> Self {
        Self {
            text_weight: 1.0,
            geo_weight: 2.0,       // GPS is independent evidence → weight higher
            time_weight: 1.5,
            sentiment_weight: 0.5,  // Penalized: same source as text
            enf_weight: 3.0,        // Physics-derived → highest weight
            provenance_weight: 1.0,

            t_initial: 10.0,
            t_final: 0.01,
            cooling_rate: 0.95,
            steps_per_temp: 100,

            single_source_penalty: 0.5,
            modality_confidence_floor: 0.5,
        }
    }
}

impl IsingEntityResolver {
    // ─────────────────────────────────────────────────────────────
    // MAIN ENTRY POINT
    // PORTED-FROM: src/ising_entity_resolver.py:160-187 resolve()
    // ─────────────────────────────────────────────────────────────

    /// Resolve entities from workflow events using Ising model clustering.
    ///
    /// Returns the best `SpinConfig` per independent annealing run,
    /// sorted by energy (best first).
    ///
    /// PORTED-FROM: src/ising_entity_resolver.py:160-187 resolve()
    pub fn resolve(
        &self,
        events: &[WorkflowEvent],
        cooling_schedule: CoolingSchedule,
    ) -> Result<Vec<SpinConfig>, ResolverError> {
        let entities = self.extract_entities(events);
        if entities.len() < 2 {
            return Err(ResolverError::InsufficientEntities(entities.len()));
        }

        let coupling = self.build_coupling_matrix(&entities);
        let initial_config = self.random_initial(&coupling.entity_list);

        // Run 3 independent anneals from different random starts
        let mut best_configs: Vec<SpinConfig> = Vec::with_capacity(3);
        for _ in 0..3 {
            let config = self.simulated_annealing(&coupling, &initial_config, cooling_schedule);
            best_configs.push(config);
        }

        best_configs.sort_by(|a, b| {
            a.energy
                .partial_cmp(&b.energy)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(best_configs)
    }

    // ─────────────────────────────────────────────────────────────
    // ENTITY EXTRACTION
    // PORTED-FROM: src/ising_entity_resolver.py:189-242 _extract_entities()
    // ─────────────────────────────────────────────────────────────

    /// Extract entities from workflow events.
    /// PORTED-FROM: src/ising_entity_resolver.py:189-242 _extract_entities()
    pub fn extract_entities(&self, events: &[WorkflowEvent]) -> Vec<Entity> {
        let mut entities: Vec<Entity> = Vec::new();

        for event in events {
            let payload = match &event.payload {
                Some(p) => p,
                None => continue,
            };

            let text = payload
                .text_content
                .as_deref()
                .or(payload.text.as_deref())
                .unwrap_or("");

            let source = event
                .source
                .as_deref()
                .or(event.transport_origin.as_deref())
                .unwrap_or("unknown");

            let provenance = event
                .provenance_chain
                .clone()
                .unwrap_or_default();

            let event_id = event
                .event_id
                .as_deref()
                .or(event.id.as_deref())
                .unwrap_or("unknown")
                .to_string();

            // Extract entity names from text
            let entity_names: Vec<String> = match &payload.entities {
                Some(names) if !names.is_empty() => names.clone(),
                _ => {
                    if !text.is_empty() {
                        Self::crude_entity_extract(text)
                    } else {
                        vec![]
                    }
                }
            };

            for name in &entity_names {
                let mut entity = Entity {
                    name: name.clone(),
                    event_id: event_id.clone(),
                    source: source.to_string(),
                    provenance_chain: provenance.clone(),
                    gps: None,
                    timestamp: None,
                    sentiment_vector: None,
                    enf_fingerprint: None,
                    confidence: 1.0,
                    text_source: format!("{}.payload.text", source),
                    sentiment_source: format!("{}.payload.text", source),
                    geo_source: format!("{}.payload.geo_temporal", source),
                    enf_source: format!("{}.payload.audio", source),
                };

                // ── Geo-temporal evidence ──
                if let Some(geo) = &payload.geo_temporal {
                    let lat = geo.latitude.or(geo.lat);
                    let lon = geo.longitude.or(geo.lon);
                    if let (Some(lat), Some(lon)) = (lat, lon) {
                        entity.gps = Some((lat, lon));
                        entity.geo_source = format!("{}.payload.geo_temporal", source);
                    }
                }

                // ── Timestamp ──
                entity.timestamp = payload
                    .timestamp
                    .clone()
                    .or_else(|| event.timestamp.clone());

                // ── Sentiment vector (must be exactly 8-dim) ──
                let sentiment = payload
                    .sentiment_vector
                    .clone()
                    .or_else(|| payload.sentiment.clone());
                if let Some(sent) = sentiment {
                    if sent.len() == 8 {
                        entity.sentiment_vector = Some(sent);
                        entity.sentiment_source = format!("{}.payload.text", source);
                    }
                }

                // ── ENF fingerprint (physics-derived) ──
                let enf_data = payload
                    .enf
                    .as_ref()
                    .or_else(|| payload.physical_layer.as_ref().and_then(|pl| pl.enf.as_ref()));
                if let Some(enf) = enf_data {
                    let fp = enf
                        .region
                        .clone()
                        .or_else(|| enf.grid_frequency.clone());
                    if let Some(fp) = fp {
                        entity.enf_fingerprint = Some(fp);
                        entity.enf_source = format!("{}.payload.audio", source);
                    }
                }

                // ── Provenance confidence ──
                if !provenance.is_empty() {
                    entity.confidence = f64::min(1.0, provenance.len() as f64 * 0.25);
                }

                entities.push(entity);
            }
        }

        entities
    }

    // ─────────────────────────────────────────────────────────────
    // CRUDE ENTITY EXTRACTION (fallback NER)
    // PORTED-FROM: src/ising_entity_resolver.py:244-254 _crude_entity_extract()
    // ─────────────────────────────────────────────────────────────

    /// Crude entity extraction fallback when proper NER is not available.
    /// Heuristic: capitalized multi-word phrases, filtered against stop words.
    /// PORTED-FROM: src/ising_entity_resolver.py:244-254 _crude_entity_extract()
    fn crude_entity_extract(text: &str) -> Vec<String> {
        if text.is_empty() {
            return vec![];
        }

        // Simple heuristic: capitalized multi-word phrases
        // Regex: \b([A-Z][a-z]+(?:\s+[A-Z][a-z]+)*)\b
        let re = regex::Regex::new(r"\b([A-Z][a-z]+(?:\s+[A-Z][a-z]+)*)\b")
            .expect("static regex should compile");

        let stop_words: BTreeSet<&str> = [
            "The", "A", "An", "This", "That", "These", "Those", "It", "He", "She",
            "They", "We", "You", "I", "Is", "Are", "Was", "Were", "Be", "Been",
        ]
        .iter()
        .copied()
        .collect();

        let mut results: Vec<String> = Vec::new();
        for caps in re.captures_iter(text) {
            if let Some(m) = caps.get(1) {
                let candidate = m.as_str();
                let first_word = candidate.split_whitespace().next().unwrap_or("");
                if !stop_words.contains(first_word) {
                    results.push(candidate.to_string());
                    if results.len() >= 20 {
                        break; // max 20 entities
                    }
                }
            }
        }
        results
    }

    // ─────────────────────────────────────────────────────────────
    // COUPLING MATRIX CONSTRUCTION
    // PORTED-FROM: src/ising_entity_resolver.py:256-318 _build_coupling_matrix()
    // ─────────────────────────────────────────────────────────────

    /// Build the J_{ij} coupling matrix from all evidence dimensions.
    /// PORTED-FROM: src/ising_entity_resolver.py:256-318 _build_coupling_matrix()
    pub fn build_coupling_matrix(&self, entities: &[Entity]) -> CouplingMatrix {
        let names: Vec<String> = entities.iter().map(|e| e.name.clone()).collect();
        let mut cm = CouplingMatrix {
            couplings: HashMap::new(),
            dimensional_weights: HashMap::new(),
            entity_list: names.clone(),
        };

        for (i, ei) in entities.iter().enumerate() {
            for (j, ej) in entities.iter().enumerate() {
                if i >= j {
                    continue;
                }

                let key = (names[i].clone(), names[j].clone());
                let mut coupling = 0.0f64;
                let mut dim_weights: BTreeMap<String, DimensionWeight> = BTreeMap::new();

                // 1. TEXT OVERLAP — Jaccard on entity names
                let text_sim = Self::jaccard_similarity(&ei.name, &ej.name);
                if text_sim > 0.0 {
                    let contrib = self.text_weight * text_sim;
                    coupling += contrib;
                    dim_weights.insert(
                        "text".into(),
                        DimensionWeight {
                            weight: self.text_weight,
                            similarity: text_sim,
                        },
                    );
                }

                // 2. GPS PROXIMITY — independent dimension
                if let (Some(gps_i), Some(gps_j)) = (ei.gps, ej.gps) {
                    let geo_sim = Self::geo_similarity(gps_i, gps_j);
                    coupling += self.geo_weight * geo_sim;
                    dim_weights.insert(
                        "geo".into(),
                        DimensionWeight {
                            weight: self.geo_weight,
                            similarity: geo_sim,
                        },
                    );
                }

                // 3. TEMPORAL PROXIMITY
                if let (Some(ts_i), Some(ts_j)) = (&ei.timestamp, &ej.timestamp) {
                    let time_sim = Self::temporal_similarity(ts_i, ts_j);
                    coupling += self.time_weight * time_sim;
                    dim_weights.insert(
                        "time".into(),
                        DimensionWeight {
                            weight: self.time_weight,
                            similarity: time_sim,
                        },
                    );
                }

                // 4. SENTIMENT SIMILARITY — with dual-dimension collusion penalty
                if let (Some(sv_i), Some(sv_j)) = (&ei.sentiment_vector, &ej.sentiment_vector) {
                    let mut sent_sim = Self::cosine_similarity(sv_i, sv_j);
                    // CRITICAL: check dimensional independence
                    if ei.sentiment_source == ei.text_source
                        && ej.sentiment_source == ej.text_source
                    {
                        // Both from payload.text → collusion penalty
                        sent_sim *= self.single_source_penalty;
                    }
                    coupling += self.sentiment_weight * sent_sim;
                    dim_weights.insert(
                        "sentiment".into(),
                        DimensionWeight {
                            weight: self.sentiment_weight,
                            similarity: sent_sim,
                        },
                    );
                }

                // 5. ENF COHERENCE — physics-derived, highest trust
                if let (Some(enf_i), Some(enf_j)) = (&ei.enf_fingerprint, &ej.enf_fingerprint) {
                    let enf_sim = if enf_i == enf_j { 1.0 } else { 0.0 };
                    coupling += self.enf_weight * enf_sim;
                    dim_weights.insert(
                        "enf".into(),
                        DimensionWeight {
                            weight: self.enf_weight,
                            similarity: enf_sim,
                        },
                    );
                }

                // 6. PROVENANCE CHAIN similarity
                if !ei.provenance_chain.is_empty() && !ej.provenance_chain.is_empty() {
                    let prov_sim = Self::provenance_similarity(
                        &ei.provenance_chain,
                        &ej.provenance_chain,
                    );
                    coupling += self.provenance_weight * prov_sim;
                    dim_weights.insert(
                        "provenance".into(),
                        DimensionWeight {
                            weight: self.provenance_weight,
                            similarity: prov_sim,
                        },
                    );
                }

                cm.couplings.insert(key.clone(), coupling);
                cm.dimensional_weights.insert(key, dim_weights);
            }
        }

        cm
    }

    // ─────────────────────────────────────────────────────────────
    // SIMILARITY FUNCTIONS
    // ─────────────────────────────────────────────────────────────

    /// Jaccard similarity on word sets. Range [0.0, 1.0].
    /// PORTED-FROM: src/ising_entity_resolver.py:322-328 _jaccard_similarity()
    fn jaccard_similarity(a: &str, b: &str) -> f64 {
        let a_lower = a.to_lowercase();
        let b_lower = b.to_lowercase();
        let set_a: BTreeSet<&str> = a_lower.split_whitespace().collect();
        let set_b: BTreeSet<&str> = b_lower.split_whitespace().collect();

        if set_a.is_empty() || set_b.is_empty() {
            return 0.0;
        }

        let intersection = set_a.intersection(&set_b).count();
        let union = set_a.union(&set_b).count();

        intersection as f64 / union as f64
    }

    /// Haversine-based GPS similarity.
    /// 1.0 = same point, ~0.6 at 10km, ~0.01 at 100km.
    /// PORTED-FROM: src/ising_entity_resolver.py:330-342 _geo_similarity()
    fn geo_similarity(a: (f64, f64), b: (f64, f64)) -> f64 {
        let (lat1, lon1) = (a.0.to_radians(), a.1.to_radians());
        let (lat2, lon2) = (b.0.to_radians(), b.1.to_radians());

        let dlat = lat2 - lat1;
        let dlon = lon2 - lon1;

        let a_hav = (dlat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
        let dist_km = 2.0 * 6371.0 * a_hav.sqrt().asin();

        // Gaussian decay: 1.0 at 0km, ~0.6 at 10km, ~0.01 at 100km
        (-dist_km / 5000.0).exp()
    }

    /// Temporal proximity.
    /// 1.0 = same hour, decays over days (24h half-life).
    /// PORTED-FROM: src/ising_entity_resolver.py:344-353 _temporal_similarity()
    fn temporal_similarity(a: &str, b: &str) -> f64 {
        // Best-effort ISO 8601 parse with Z→+00:00 substitution
        let parse_iso = |s: &str| -> Option<f64> {
            let normalized = s.trim().replace('Z', "+00:00");
            // Try parsing as RFC 3339 / ISO 8601
            // We do a simple manual parse for robustness
            let date_str = if let Some(dot_pos) = normalized.find('.') {
                // Truncate sub-second precision
                if let Some(plus_pos) = normalized.find('+') {
                    format!("{}{}", &normalized[..dot_pos], &normalized[plus_pos..])
                } else if let Some(minus_pos) = normalized.rfind('-') {
                    // Be careful not to eat date hyphens: look for T separator
                    if let Some(t_pos) = normalized.find('T') {
                        if minus_pos > t_pos + 8 {
                            format!("{}{}", &normalized[..dot_pos], &normalized[minus_pos..])
                        } else {
                            normalized.clone()
                        }
                    } else {
                        normalized.clone()
                    }
                } else {
                    normalized.clone()
                }
            } else {
                normalized.clone()
            };

            // Use chrono for robust parsing (available in pt-oracle dependency tree)
            // Fallback: manual timestamp extraction
            // Format: YYYY-MM-DDTHH:MM:SS+HH:MM or YYYY-MM-DDTHH:MM:SSZ
            let parts: Vec<&str> = date_str.split(|c| c == 'T' || c == 't' || c == ' ').collect();
            if parts.len() < 2 {
                return None;
            }

            let date_parts: Vec<&str> = parts[0].split('-').collect();
            if date_parts.len() != 3 {
                return None;
            }
            let year: i32 = date_parts[0].parse().ok()?;
            let month: u32 = date_parts[1].parse().ok()?;
            let day: u32 = date_parts[2].parse().ok()?;

            let time_str = parts[1];
            let time_clean = if let Some(tz_pos) = time_str.find(|c| c == '+' || (c == '-' && time_str.rfind('-').unwrap_or(0) > 6))
            {
                &time_str[..tz_pos]
            } else {
                time_str
            };

            let time_parts: Vec<&str> = time_clean.split(':').collect();
            let hour: u32 = time_parts.first()?.parse().ok()?;
            let min: u32 = time_parts.get(1).unwrap_or(&"0").parse().ok()?;
            let sec_str = time_parts.get(2).unwrap_or(&"0");
            let sec: f64 = if let Some(dot_pos) = sec_str.find('.') {
                sec_str.parse().ok()?
            } else {
                sec_str.parse::<u32>().ok()? as f64
            };

            // Approximate: days since epoch (ignoring tz offset for similarity purposes)
            let days_before_month: [i32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
            let leap = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) { 1 } else { 0 };
            let month_idx = (month as usize).saturating_sub(1);
            let day_of_year = days_before_month[month_idx.min(11)] + (day as i32)
                + if month > 2 { leap } else { 0 };

            let total_days = (year as i64 - 1970) * 365
                + ((year as i64 - 1969) / 4)
                - ((year as i64 - 1901) / 100)
                + ((year as i64 - 1601) / 400)
                + day_of_year as i64;

            let total_seconds = total_days as f64 * 86400.0
                + hour as f64 * 3600.0
                + min as f64 * 60.0
                + sec;

            Some(total_seconds)
        };

        let ta = parse_iso(a);
        let tb = parse_iso(b);

        match (ta, tb) {
            (Some(ta), Some(tb)) => {
                let delta_hours = (ta - tb).abs() / 3600.0;
                (-delta_hours / 24.0).exp()
            }
            _ => 0.0,
        }
    }

    /// Cosine similarity between vectors. Range [0.0, 1.0].
    /// PORTED-FROM: src/ising_entity_resolver.py:355-364 _cosine_similarity()
    fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
        let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let mag_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
        let mag_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();

        if mag_a == 0.0 || mag_b == 0.0 {
            return 0.0;
        }

        let sim = dot / (mag_a * mag_b);
        // Cosine is [-1, 1]. Map to [0, 1]: 1.0 = identical, 0.0 = opposite
        (sim + 1.0) / 2.0
    }

    /// How similar are provenance chains? Shared agents = higher coupling.
    /// PORTED-FROM: src/ising_entity_resolver.py:366-372 _provenance_similarity()
    fn provenance_similarity(a: &[String], b: &[String]) -> f64 {
        let set_a: BTreeSet<&str> = a.iter().map(|s| s.as_str()).collect();
        let set_b: BTreeSet<&str> = b.iter().map(|s| s.as_str()).collect();

        if set_a.is_empty() || set_b.is_empty() {
            return 0.0;
        }

        let intersection = set_a.intersection(&set_b).count();
        let union = set_a.union(&set_b).count();

        intersection as f64 / union as f64
    }

    // ─────────────────────────────────────────────────────────────
    // SIMULATED ANNEALING
    // ─────────────────────────────────────────────────────────────

    /// Assign random initial cluster IDs (each entity in its own cluster).
    /// PORTED-FROM: src/ising_entity_resolver.py:376-382 _random_initial()
    pub fn random_initial(&self, entities: &[String]) -> SpinConfig {
        let mut config = SpinConfig::new();
        config.spins = entities
            .iter()
            .enumerate()
            .map(|(i, e)| (e.clone(), i))
            .collect();
        config
    }

    /// Compute Ising energy of a configuration.
    ///
    /// E = -Σ_{i<j} J_{ij} · δ(σ_i, σ_j)
    /// where δ = 1 if same cluster, 0 otherwise
    ///
    /// Lower energy = better clustering (more couplings satisfied).
    ///
    /// PORTED-FROM: src/ising_entity_resolver.py:384-405 _energy()
    pub fn energy(&self, config: &SpinConfig, coupling: &CouplingMatrix) -> f64 {
        let mut energy = 0.0f64;

        for ((ei, ej), &coupling_val) in &coupling.couplings {
            // M7: skip a coupling term if either entity has no spin assignment.
            // The old sentinels (usize::MAX vs MAX-1) made two absent entities
            // compare UNEQUAL, silently applying the separation penalty to pairs
            // that aren't even in the configuration.
            let (spin_i, spin_j) = match (config.spins.get(ei), config.spins.get(ej)) {
                (Some(&a), Some(&b)) => (a, b),
                _ => continue,
            };

            if spin_i == spin_j {
                // Same cluster → coupling contributes negatively (favorable)
                energy -= coupling_val;
            } else if coupling_val > 0.0 {
                // Different clusters + positive coupling → penalty for separating linked entities
                energy += coupling_val * 0.5;
            }
        }

        energy
    }

    /// Simulated annealing to find global minimum energy configuration.
    ///
    /// PORTED-FROM: src/ising_entity_resolver.py:407-472 _simulated_annealing()
    pub fn simulated_annealing(
        &self,
        coupling: &CouplingMatrix,
        initial: &SpinConfig,
        schedule: CoolingSchedule,
    ) -> SpinConfig {
        let mut current = SpinConfig {
            spins: initial.spins.clone(),
            energy: 0.0,
            cluster_assignments: HashMap::new(),
        };
        current.energy = self.energy(&current, coupling);

        let mut best = SpinConfig {
            spins: current.spins.clone(),
            energy: current.energy,
            cluster_assignments: HashMap::new(),
        };

        // M7: seed the RNG deterministically from the problem itself (the entity
        // list) instead of `thread_rng()`. Unseeded OS entropy made annealing
        // non-reproducible, directly contradicting the module's "reproducible
        // clusters" claim. Same entities -> same anneal, every run, every machine.
        use rand::SeedableRng;
        let seed = coupling.entity_list.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, e| {
            e.bytes().fold(h, |h, b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3))
        });
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut t = self.t_initial;
        let entities = &coupling.entity_list;
        let n = entities.len();

        while t > self.t_final {
            for _ in 0..self.steps_per_temp {
                // Propose: move one entity to a random existing or new cluster
                let entity = entities.choose(&mut rng).expect("entities non-empty");
                let current_cluster = *current.spins.get(entity).unwrap_or(&0);

                // Get all cluster IDs
                let clusters: BTreeSet<usize> = current.spins.values().copied().collect();
                let max_cluster = clusters.iter().max().copied().unwrap_or(0);
                let new_cluster_id = max_cluster + 1;

                // 80% chance: join existing cluster, 20% chance: new cluster
                let new_cluster = if !clusters.is_empty() && rng.r#gen::<f64>() < 0.8 {
                    let valid: Vec<usize> =
                        clusters.iter().copied().filter(|&c| c != current_cluster).collect();
                    if valid.is_empty() {
                        new_cluster_id
                    } else {
                        *valid.choose(&mut rng).unwrap_or(&&new_cluster_id)
                    }
                } else {
                    new_cluster_id
                };

                // Compute energy delta
                let old_energy = current.energy;
                let old_spin = *current.spins.get(entity).unwrap_or(&0);
                current.spins.insert(entity.clone(), new_cluster);
                let new_energy = self.energy(&current, coupling);

                let delta = new_energy - old_energy;

                if delta < 0.0 || rng.r#gen::<f64>() < (-delta / t).exp() {
                    // Accept move
                    current.energy = new_energy;
                    if new_energy < best.energy {
                        best.spins = current.spins.clone();
                        best.energy = new_energy;
                    }
                } else {
                    // Reject move
                    current.spins.insert(entity.clone(), old_spin);
                }
            }

            // Cool
            match schedule {
                CoolingSchedule::Linear => {
                    t -= self.cooling_rate * (self.t_initial - self.t_final) / 100.0;
                }
                CoolingSchedule::Exponential => {
                    t *= self.cooling_rate;
                }
                CoolingSchedule::Adaptive => {
                    // Faster cooling early, slower near convergence
                    let convergence_factor = 1.0 - best.energy / (best.energy + 1e-10);
                    t *= f64::max(0.85, self.cooling_rate * convergence_factor);
                }
            }
        }

        // Build cluster assignments for best config
        let mut cluster_map: HashMap<usize, Vec<String>> = HashMap::new();
        for (entity, &cluster_id) in &best.spins {
            cluster_map.entry(cluster_id).or_default().push(entity.clone());
        }

        // Remove clusters of size 1 (noise)
        cluster_map.retain(|_, members| members.len() >= 2);

        best.cluster_assignments = cluster_map;
        best
    }

    // ─────────────────────────────────────────────────────────────
    // CONFIDENCE SCORING
    // PORTED-FROM: src/ising_entity_resolver.py:476-546 compute_confidence()
    // ─────────────────────────────────────────────────────────────

    /// Compute multi-dimensional confidence for a cluster configuration.
    ///
    /// Confidence = energy_gap × dimensional_independence × modality_floor
    ///
    /// PORTED-FROM: src/ising_entity_resolver.py:476-546 compute_confidence()
    pub fn compute_confidence(
        &self,
        config: &SpinConfig,
        coupling: &CouplingMatrix,
        entities: &[Entity],
    ) -> ResolutionConfidence {
        // 1. Energy gap: compute random baseline energy
        let random_config = self.random_initial(&coupling.entity_list);
        let random_energy = self.energy(&random_config, coupling);

        let energy_range = f64::max(
            f64::max(random_energy.abs(), config.energy.abs()),
            1e-10,
        );
        let energy_gap = ((random_energy - config.energy) / energy_range).clamp(0.0, 1.0);

        // 2. Dimensional independence score
        let independence_scores: Vec<f64> = entities
            .iter()
            .map(|e| e.dimensional_independence_score())
            .collect();
        let dim_score = if independence_scores.is_empty() {
            0.0
        } else {
            independence_scores.iter().sum::<f64>() / independence_scores.len() as f64
        };

        // 3. Modality confidence floor — from VDAA fix (Round 24)
        let mut independent_dims: BTreeSet<&str> = BTreeSet::new();
        for e in entities {
            if e.gps.is_some() {
                independent_dims.insert("geo");
            }
            if e.enf_fingerprint.is_some() {
                independent_dims.insert("enf");
            }
        }

        let modality_floor = match independent_dims.len() {
            n if n >= 2 => 1.0,
            1 => 0.7,
            _ => 0.5, // Text-only → hard ceiling
        };

        // 4. Overall: product of all factors
        let overall = energy_gap * dim_score * modality_floor;

        // 5. Per-cluster confidence
        let mut per_cluster: BTreeMap<String, ClusterConfidence> = BTreeMap::new();
        for (&cluster_id, members) in &config.cluster_assignments {
            let cluster_entities: Vec<&Entity> = entities
                .iter()
                .filter(|e| members.contains(&e.name))
                .collect();

            let mut cluster_indep: BTreeSet<&str> = BTreeSet::new();
            for e in &cluster_entities {
                if e.gps.is_some() {
                    cluster_indep.insert("geo");
                }
                if e.enf_fingerprint.is_some() {
                    cluster_indep.insert("enf");
                }
            }

            let cluster_confidence = cluster_indep.len() as f64 / 2.0; // 0, 0.5, or 1.0
            per_cluster.insert(
                format!("cluster_{}", cluster_id),
                ClusterConfidence {
                    members: members.clone(),
                    size: members.len(),
                    independent_dimensions: cluster_indep.into_iter().map(String::from).collect(),
                    confidence: cluster_confidence,
                },
            );
        }

        ResolutionConfidence {
            overall_confidence: round4(overall),
            energy_gap: round4(energy_gap),
            dimensional_score: round4(dim_score),
            modality_floor,
            per_cluster,
            method: "ising_simulated_annealing".into(),
            paper: "arXiv:2607.02112".into(),
        }
    }

    // ─────────────────────────────────────────────────────────────
    // OUTPUT FORMATTING
    // PORTED-FROM: src/ising_entity_resolver.py:548-568 format_timeline_json()
    // ─────────────────────────────────────────────────────────────

    /// Format output as timeline-compatible struct.
    /// PORTED-FROM: src/ising_entity_resolver.py:548-568 format_timeline_json()
    pub fn format_timeline(
        &self,
        configs: &[SpinConfig],
        confidence: &ResolutionConfidence,
    ) -> Option<TimelineOutput> {
        let best = configs.first()?;

        let clusters: Vec<TimelineCluster> = best
            .cluster_assignments
            .iter()
            .map(|(&k, members)| {
                let cluster_key = format!("cluster_{}", k);
                let cluster_conf = confidence
                    .per_cluster
                    .get(&cluster_key)
                    .map(|c| c.confidence)
                    .unwrap_or(0.0);

                TimelineCluster {
                    cluster_id: format!("ising_c{}", k),
                    entities: members.clone(),
                    confidence: cluster_conf,
                }
            })
            .collect();

        // Timestamp: use chrono if available, otherwise a placeholder
        let now_iso = {
            #[cfg(feature = "chrono")]
            {
                chrono::Utc::now().to_rfc3339()
            }
            #[cfg(not(feature = "chrono"))]
            {
                "unknown".to_string()
            }
        };

        Some(TimelineOutput {
            method: "ising_entity_resolution".into(),
            version: "1.0.0".into(),
            timestamp: now_iso,
            num_clusters: best.cluster_assignments.len(),
            clusters,
            confidence: confidence.clone(),
            energy: best.energy,
            reference_paper: "arXiv:2607.02112".into(),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────
// COOLING SCHEDULE
// ─────────────────────────────────────────────────────────────────────

/// Annealing cooling schedule.
/// PORTED-FROM: src/ising_entity_resolver.py:161,455-460
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoolingSchedule {
    /// Constant step size: T -= rate * (T_initial - T_final) / 100
    Linear,
    /// Geometric: T *= rate
    Exponential,
    /// Faster cooling early, slower near convergence
    Adaptive,
}

// ─────────────────────────────────────────────────────────────────────
// HELPERS
// ─────────────────────────────────────────────────────────────────────

/// Round to 4 decimal places.
fn round4(x: f64) -> f64 {
    (x * 10000.0).round() / 10000.0
}

// ─────────────────────────────────────────────────────────────────────
// pt-oracle INTEGRATION POINT: Cluster → Candidate promotion
// ─────────────────────────────────────────────────────────────────────
//
// Integration with pt-oracle::candidate::CandidateStore:
//
//   let resolver = IsingEntityResolver::default();
//   let configs = resolver.resolve(&events, CoolingSchedule::Adaptive)?;
//   let best = &configs[0];
//   let coupling = resolver.build_coupling_matrix(&entities);
//   let confidence = resolver.compute_confidence(best, &coupling, &entities);
//
//   // Each cluster becomes a StagedCandidate for oracle gate validation
//   for (cluster_id, members) in &best.cluster_assignments {
//       let valence = if confidence.modality_floor >= 0.7 {
//           TruthValence::Proven   // ≥2 independent dims or physics evidence
//       } else if confidence.modality_floor >= 0.5 {
//           TruthValence::Unknown  // text-only, needs cross-source validation
//       } else {
//           TruthValence::Disproven
//       };
//
//       candidate_store.stage(
//           StagedCandidate::EntityCluster {
//               cluster_id: format!("ising_c{}", cluster_id),
//               members: members.clone(),
//               valence,
//               confidence: confidence.overall_confidence,
//               dimensions: confidence.per_cluster
//                   .get(&format!("cluster_{}", cluster_id))
//                   .map(|c| c.independent_dimensions.clone())
//                   .unwrap_or_default(),
//           }
//       );
//   }
//
// After 2 independent validations across captures, promotable() clusters
// can compile into fast-lane AdapterRules for pt-extract.

// ─────────────────────────────────────────────────────────────────────
// TESTS
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the same test events as Python self-test.
    /// PORTED-FROM: src/ising_entity_resolver.py:575-655 _test_with_fabricated_events()
    fn fabricated_events() -> Vec<WorkflowEvent> {
        vec![
            WorkflowEvent {
                event_id: Some("evt_001".into()),
                id: None,
                source: Some("Gateway".into()),
                transport_origin: None,
                provenance_chain: Some(vec![
                    "Gateway".into(),
                    "Shield".into(),
                    "Bytewax".into(),
                ]),
                timestamp: None,
                payload: Some(WorkflowPayload {
                    text_content: Some(
                        "Shell CEO furious at protestors. Angry clashes between security and indigenous activists."
                            .into(),
                    ),
                    text: None,
                    entities: Some(vec![
                        "Shell".into(),
                        "CEO".into(),
                        "protest".into(),
                        "indigenous".into(),
                        "activists".into(),
                    ]),
                    sentiment_vector: Some(vec![0.0, 1.0, 0.0, -0.9, 0.67, 0.0, 0.5, 0.33]),
                    sentiment: None,
                    geo_temporal: None,
                    enf: None,
                    physical_layer: None,
                    timestamp: Some("2026-07-06T14:00:00Z".into()),
                }),
            },
            WorkflowEvent {
                event_id: Some("evt_002".into()),
                id: None,
                source: Some("Gateway".into()),
                transport_origin: None,
                provenance_chain: Some(vec![
                    "Gateway".into(),
                    "Shield".into(),
                    "Bytewax".into(),
                ]),
                timestamp: None,
                payload: Some(WorkflowPayload {
                    text_content: Some(
                        "Shell spokesperson denies protest violence. Company committed to dialogue with indigenous leaders."
                            .into(),
                    ),
                    text: None,
                    entities: Some(vec![
                        "Shell".into(),
                        "spokesperson".into(),
                        "protest".into(),
                        "indigenous".into(),
                        "leaders".into(),
                    ]),
                    sentiment_vector: Some(vec![0.2, 0.3, 0.5, 0.1, 0.1, 0.0, 0.0, 0.3]),
                    sentiment: None,
                    geo_temporal: None,
                    enf: None,
                    physical_layer: None,
                    timestamp: Some("2026-07-06T14:30:00Z".into()),
                }),
            },
            WorkflowEvent {
                event_id: Some("evt_003".into()),
                id: None,
                source: Some("VEX".into()),
                transport_origin: None,
                provenance_chain: Some(vec!["VEX".into(), "Operator".into()]),
                timestamp: None,
                payload: Some(WorkflowPayload {
                    text_content: Some(
                        "Amazon deforestation linked to Shell supplier chain. GPS coordinates: -3.4653, -62.2159."
                            .into(),
                    ),
                    text: None,
                    entities: Some(vec![
                        "Amazon".into(),
                        "Shell".into(),
                        "deforestation".into(),
                        "supplier".into(),
                    ]),
                    sentiment_vector: Some(vec![0.0, 0.7, 0.2, -0.5, 0.5, 0.0, 0.3, 0.1]),
                    sentiment: None,
                    geo_temporal: Some(GeoData {
                        latitude: Some(-3.4653),
                        lat: None,
                        longitude: Some(-62.2159),
                        lon: None,
                    }),
                    enf: None,
                    physical_layer: None,
                    timestamp: Some("2026-07-06T15:00:00Z".into()),
                }),
            },
            WorkflowEvent {
                event_id: Some("evt_004".into()),
                id: None,
                source: Some("Gateway".into()),
                transport_origin: None,
                provenance_chain: Some(vec!["Gateway".into(), "Shield".into()]),
                timestamp: None,
                payload: Some(WorkflowPayload {
                    text_content: Some(
                        "Environmental report: Shell operations in Nigeria linked to oil spills."
                            .into(),
                    ),
                    text: None,
                    entities: Some(vec![
                        "Shell".into(),
                        "Nigeria".into(),
                        "oil".into(),
                        "spills".into(),
                    ]),
                    sentiment_vector: None,
                    sentiment: None,
                    geo_temporal: Some(GeoData {
                        latitude: Some(4.8156),
                        lat: None,
                        longitude: Some(7.0498),
                        lon: None,
                    }),
                    enf: None,
                    physical_layer: None,
                    timestamp: Some("2026-07-06T16:00:00Z".into()),
                }),
            },
        ]
    }

    #[test]
    fn test_resolve_with_fabricated_events() {
        let events = fabricated_events();
        let resolver = IsingEntityResolver::default();
        let configs = resolver
            .resolve(&events, CoolingSchedule::Adaptive)
            .expect("resolve should succeed");

        assert!(!configs.is_empty(), "should produce at least one config");

        let best = &configs[0];
        eprintln!("Energy: {:.4}", best.energy);
        eprintln!("Clusters: {}", best.cluster_assignments.len());
        for (cid, members) in &best.cluster_assignments {
            eprintln!("  Cluster {}: {:?}", cid, members);
        }

        // Compute confidence
        let entities = resolver.extract_entities(&events);
        let coupling = resolver.build_coupling_matrix(&entities);
        let confidence = resolver.compute_confidence(best, &coupling, &entities);

        eprintln!("\nConfidence: {:.4}", confidence.overall_confidence);
        eprintln!("Energy gap: {:.4}", confidence.energy_gap);
        eprintln!("Dimensional score: {:.4}", confidence.dimensional_score);
        eprintln!("Modality floor: {}", confidence.modality_floor);

        // Key assertion: text-only events (evt_001, evt_002) should trigger modality floor
        assert!(
            confidence.modality_floor <= 0.7,
            "Text-only events should cap confidence. Got: {}",
            confidence.modality_floor
        );
    }

    #[test]
    fn test_jaccard_similarity() {
        let sim = IsingEntityResolver::jaccard_similarity("Shell CEO", "Shell spokesperson");
        assert!(sim > 0.0);
        assert!(sim < 1.0);

        let ident = IsingEntityResolver::jaccard_similarity("Shell", "Shell");
        assert!((ident - 1.0).abs() < 1e-10);

        let diff = IsingEntityResolver::jaccard_similarity("Shell", "Microsoft");
        assert!((diff - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = IsingEntityResolver::cosine_similarity(&a, &b);
        // Orthogonal vectors: raw cosine = 0, mapped to [0,1] = 0.5
        assert!((sim - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_provenance_similarity() {
        let a = vec!["Gateway".to_string(), "Shield".to_string()];
        let b = vec!["Gateway".to_string(), "Bytewax".to_string()];
        let sim = IsingEntityResolver::provenance_similarity(&a, &b);
        // Intersection = {"Gateway"}, Union = {"Gateway","Shield","Bytewax"} = 1/3
        assert!((sim - 1.0 / 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_dimensional_independence_score() {
        let e = Entity {
            name: "Shell".into(),
            event_id: "evt_001".into(),
            source: "Gateway".into(),
            provenance_chain: vec![],
            gps: Some((-3.4653, -62.2159)),
            timestamp: None,
            sentiment_vector: Some(vec![0.0; 8]),
            enf_fingerprint: Some("50Hz-Europe".into()),
            confidence: 1.0,
            text_source: "Gateway.payload.text".into(),
            sentiment_source: "Gateway.payload.text".into(), // same as text!
            geo_source: "Gateway.payload.geo_temporal".into(),
            enf_source: "Gateway.payload.audio".into(),
        };
        let score = e.dimensional_independence_score();
        // Classes: text (from text_source+sentiment_source), metadata (geo), physics (enf)
        // = 3 classes / 4 = 0.75
        assert!((score - 0.75).abs() < 1e-10);
    }

    #[test]
    fn test_energy_decreases_on_better_clustering() {
        let e1 = Entity {
            name: "Shell".into(),
            event_id: "e1".into(),
            source: "GW".into(),
            provenance_chain: vec![],
            gps: None,
            timestamp: None,
            sentiment_vector: None,
            enf_fingerprint: None,
            confidence: 1.0,
            text_source: "GW.payload.text".into(),
            sentiment_source: "GW.payload.text".into(),
            geo_source: "GW.payload.geo_temporal".into(),
            enf_source: "GW.payload.audio".into(),
        };
        let e2 = Entity {
            name: "Shell PLC".into(),
            event_id: "e2".into(),
            source: "GW".into(),
            provenance_chain: vec![],
            gps: None,
            timestamp: None,
            sentiment_vector: None,
            enf_fingerprint: None,
            confidence: 1.0,
            text_source: "GW.payload.text".into(),
            sentiment_source: "GW.payload.text".into(),
            geo_source: "GW.payload.geo_temporal".into(),
            enf_source: "GW.payload.audio".into(),
        };
        let e3 = Entity {
            name: "Microsoft".into(),
            event_id: "e3".into(),
            source: "GW".into(),
            provenance_chain: vec![],
            gps: None,
            timestamp: None,
            sentiment_vector: None,
            enf_fingerprint: None,
            confidence: 1.0,
            text_source: "GW.payload.text".into(),
            sentiment_source: "GW.payload.text".into(),
            geo_source: "GW.payload.geo_temporal".into(),
            enf_source: "GW.payload.audio".into(),
        };

        let entities = vec![e1, e2, e3];
        let resolver = IsingEntityResolver::default();
        let coupling = resolver.build_coupling_matrix(&entities);

        // Config A: Shell + Shell PLC in same cluster, Microsoft alone
        let mut config_good = SpinConfig::new();
        config_good.spins.insert("Shell".into(), 0);
        config_good.spins.insert("Shell PLC".into(), 0);
        config_good.spins.insert("Microsoft".into(), 1);

        // Config B: all separate
        let mut config_bad = SpinConfig::new();
        config_bad.spins.insert("Shell".into(), 0);
        config_bad.spins.insert("Shell PLC".into(), 1);
        config_bad.spins.insert("Microsoft".into(), 2);

        let energy_good = resolver.energy(&config_good, &coupling);
        let energy_bad = resolver.energy(&config_bad, &coupling);

        eprintln!("energy_good={}, energy_bad={}", energy_good, energy_bad);
        // Better clustering (Shell + Shell PLC together) should have LOWER energy
        assert!(
            energy_good < energy_bad,
            "better clustering should have lower energy"
        );
    }

    #[test]
    fn test_insufficient_entities_errors() {
        let resolver = IsingEntityResolver::default();
        let empty: Vec<WorkflowEvent> = vec![];
        let result = resolver.resolve(&empty, CoolingSchedule::Adaptive);
        assert!(result.is_err());

        let single = vec![WorkflowEvent {
            event_id: Some("e1".into()),
            id: None,
            source: Some("GW".into()),
            transport_origin: None,
            provenance_chain: None,
            timestamp: None,
            payload: Some(WorkflowPayload {
                text_content: Some("Shell CEO".into()),
                text: None,
                entities: Some(vec!["Shell".into()]),
                sentiment_vector: None,
                sentiment: None,
                geo_temporal: None,
                enf: None,
                physical_layer: None,
                timestamp: None,
            }),
        }];
        let result = resolver.resolve(&single, CoolingSchedule::Adaptive);
        assert!(result.is_err());
    }

    #[test]
    fn test_format_timeline() {
        let events = fabricated_events();
        let resolver = IsingEntityResolver::default();
        let configs = resolver
            .resolve(&events, CoolingSchedule::Adaptive)
            .expect("resolve should succeed");
        let entities = resolver.extract_entities(&events);
        let coupling = resolver.build_coupling_matrix(&entities);
        let confidence = resolver.compute_confidence(&configs[0], &coupling, &entities);

        let timeline = resolver
            .format_timeline(&configs, &confidence)
            .expect("timeline should be Some");

        assert_eq!(timeline.method, "ising_entity_resolution");
        assert_eq!(timeline.version, "1.0.0");
        assert_eq!(timeline.reference_paper, "arXiv:2607.02112");
        assert!(timeline.num_clusters > 0);
    }
}
