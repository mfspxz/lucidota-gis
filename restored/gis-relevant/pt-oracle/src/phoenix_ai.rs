// IMPLEMENTS: HY3_PULL_AUDIT.md Pull (Phoenix AI — oracle gate validator)
//! Phoenix AI — Oracle gate validator with adaptive evidence chains.
//!
//! Phoenix rides the oracle slow lane: every claim is trit 0 until
//! the oracle gate validates byte-span evidence. Phoenix adds
//! multi-source triangulation and adaptive confidence scoring.

use crate::{ByteSpan, OracleOutput, Posture, QueryTarget};
use pt_core::ContentHash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A Phoenix claim — a statement requiring oracle validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoenixClaim {
    pub claim_id: ContentHash,
    pub entity_id: ContentHash,
    pub field: String,
    pub proposed_value: String,
    /// Byte span evidence (optional — filled by oracle).
    pub evidence_spans: Vec<ByteSpanEvidence>,
    /// Multi-source confidence (0.0–1.0).
    pub confidence: f64,
    /// Number of independent sources that corroborate.
    pub corroboration_count: u32,
}

/// Evidence from a single source with a byte span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ByteSpanEvidence {
    pub source_url: String,
    pub capture_hash: ContentHash,
    pub span: ByteSpan,
    pub extracted_text: String,
}

/// Phoenix validator — multi-source oracle gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhoenixValidator {
    /// Minimum sources required to corroborate a claim.
    pub min_corroboration: u32,
    /// Confidence threshold for automatic promotion.
    pub auto_promote_threshold: f64,
    /// Evidence store (capture_hash → raw text).
    evidence_store: HashMap<ContentHash, String>,
}

impl PhoenixValidator {
    pub fn new(min_corroboration: u32, auto_promote_threshold: f64) -> Self {
        Self {
            min_corroboration,
            auto_promote_threshold,
            evidence_store: HashMap::new(),
        }
    }

    /// Register raw capture text for byte-span verification.
    pub fn register_capture(&mut self, capture_hash: ContentHash, raw_text: &str) {
        self.evidence_store
            .insert(capture_hash, raw_text.to_string());
    }

    /// Submit a claim for oracle validation.
    pub fn submit_claim(
        &self,
        entity_id: &ContentHash,
        field: &str,
        proposed_value: &str,
    ) -> PhoenixClaim {
        let claim_id = pt_core::hash_str(&format!(
            "phoenix:{}:{}:{}",
            hex::encode(entity_id),
            field,
            proposed_value
        ));
        PhoenixClaim {
            claim_id,
            entity_id: *entity_id,
            field: field.to_string(),
            proposed_value: proposed_value.to_string(),
            evidence_spans: Vec::new(),
            confidence: 0.0,
            corroboration_count: 0,
        }
    }

    /// Add evidence to a claim — verifies byte span against raw capture.
    pub fn add_evidence(
        &mut self,
        claim: &mut PhoenixClaim,
        source_url: &str,
        capture_hash: &ContentHash,
        span: ByteSpan,
    ) -> bool {
        let raw = match self.evidence_store.get(capture_hash) {
            Some(text) => text,
            None => return false,
        };
        let extracted = match span.extract(raw.as_bytes()) {
            Some(t) => t.to_string(),
            None => return false,
        };
        // Check extracted text contains the proposed value
        if !extracted.contains(&claim.proposed_value) {
            return false;
        }
        claim.evidence_spans.push(ByteSpanEvidence {
            source_url: source_url.to_string(),
            capture_hash: *capture_hash,
            span,
            extracted_text: extracted,
        });
        claim.corroboration_count = claim.evidence_spans.len() as u32;
        claim.confidence = (claim.corroboration_count as f64
            / self.min_corroboration as f64)
            .min(1.0);
        true
    }

    /// Check if a claim passes the auto-promotion threshold.
    pub fn can_auto_promote(&self, claim: &PhoenixClaim) -> bool {
        claim.corroboration_count >= self.min_corroboration
            && claim.confidence >= self.auto_promote_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pt_core::hash_str;

    #[test]
    fn claim_is_reproducible() {
        let entity = hash_str("festival_001");
        let c1 = PhoenixValidator::new(2, 0.8).submit_claim(&entity, "deadline", "2026-03-15");
        let c2 = PhoenixValidator::new(2, 0.8).submit_claim(&entity, "deadline", "2026-03-15");
        assert_eq!(c1.claim_id, c2.claim_id);
    }

    #[test]
    fn evidence_verification() {
        let entity = hash_str("event_001");
        let mut validator = PhoenixValidator::new(2, 0.7);
        let capture_hash = hash_str("raw_capture_001");
        validator.register_capture(capture_hash, "Deadline: March 15, 2026. Apply now.");

        let mut claim = validator.submit_claim(&entity, "deadline", "March 15, 2026");
        let span = ByteSpan { start: 10, end: 24 };
        let ok = validator.add_evidence(&mut claim, "https://source.example.com", &capture_hash, span);
        assert!(ok);
        assert_eq!(claim.corroboration_count, 1);
    }

    #[test]
    fn bad_span_fails_evidence() {
        let entity = hash_str("event_002");
        let mut validator = PhoenixValidator::new(2, 0.7);
        let capture_hash = hash_str("raw_capture_002");
        validator.register_capture(capture_hash, "Deadline: June 1, 2026");

        let mut claim = validator.submit_claim(&entity, "deadline", "March 15, 2026");
        let span = ByteSpan { start: 10, end: 22 };
        let ok = validator.add_evidence(&mut claim, "https://source.example.com", &capture_hash, span);
        assert!(!ok, "extracted text doesn't match claim value");
    }

    #[test]
    fn auto_promote_threshold() {
        let mut validator = PhoenixValidator::new(2, 0.7);
        let entity = hash_str("event_003");
        let cap1 = hash_str("capture_a");
        let cap2 = hash_str("capture_b");
        validator.register_capture(cap1, "Deadline: March 15, 2026 source A.");
        validator.register_capture(cap2, "Deadline: March 15, 2026 source B.");

        let mut claim = validator.submit_claim(&entity, "deadline", "March 15, 2026");
        let span = ByteSpan { start: 10, end: 24 };
        validator.add_evidence(&mut claim, "source_a", &cap1, span.clone());
        assert!(!validator.can_auto_promote(&claim), "need 2 sources");
        validator.add_evidence(&mut claim, "source_b", &cap2, span);
        assert!(validator.can_auto_promote(&claim), "2 sources → auto-promote");
    }
}
