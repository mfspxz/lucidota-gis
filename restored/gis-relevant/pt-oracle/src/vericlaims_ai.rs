// IMPLEMENTS: DEVSTRAL_PULL_AUDIT.md Pull #1 (VeriClaims AI)
//! VeriClaims AI — Policy-aware claim verification through the oracle gate.
//!
//! Insurance claims verification is the canonical "accountability over
//! access control" use case. Each claim is an assertion that needs
//! verification against evidence. Integrates with pt-oracle's byte-span
//! enforcement and epistemic gating.

use crate::{ByteSpan, Posture};
use pt_core::ContentHash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// A claim assertion — the core unit of verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimAssertion {
    pub claim_id: ContentHash,
    pub claimant: String,
    pub predicate: ClaimPredicate,
    pub object: String,
    pub evidence_chain: Vec<EvidenceLink>,
    pub timestamp: i64,
    pub policy_hash: ContentHash,
}

/// A link in the evidence chain connecting a claim to proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceLink {
    pub evidence_hash: ContentHash,
    pub evidence_type: EvidenceType,
    pub source: String,
    pub byte_span: Option<ByteSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceType {
    Document,
    Photo,
    SensorData,
    WitnessStatement,
    ExpertReport,
    OnChainProof,
}

/// Policy-aware verification result with accountability trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationResult {
    Verified {
        by: String,
        evidence_used: Vec<ContentHash>,
        confidence: f64,
    },
    Rejected {
        reason: String,
        missing_evidence: Vec<ClaimPredicate>,
    },
    Deferred {
        to: String,
        timeout: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ClaimPredicate {
    Damage,
    Loss,
    Injury,
    Theft,
    Delay,
    BreachOfContract,
    Custom(String),
}

/// Core verifier trait — each verification method is a composable strategy.
pub trait ClaimVerifier: Send + Sync {
    fn verify(&self, claim: &ClaimAssertion) -> VerificationResult;
    fn method_name(&self) -> &'static str;
    fn policy_compatible(&self, policy_hash: &ContentHash) -> bool;
}

/// Chain verifiers into a pipeline.
pub struct VerificationPipeline {
    stages: Vec<Box<dyn ClaimVerifier>>,
    policy_hash: ContentHash,
}

impl VerificationPipeline {
    pub fn new(policy_hash: ContentHash) -> Self {
        Self {
            stages: Vec::new(),
            policy_hash,
        }
    }

    pub fn add_stage(&mut self, verifier: Box<dyn ClaimVerifier>) {
        self.stages.push(verifier);
    }

    /// Run full verification pipeline — returns accountable audit trail.
    pub fn execute(
        &self,
        claim: &ClaimAssertion,
    ) -> Result<Vec<VerificationResult>, VeriClaimsError> {
        if claim.policy_hash != self.policy_hash {
            return Err(VeriClaimsError::PolicyMismatch);
        }
        let results: Vec<_> = self
            .stages
            .iter()
            .filter(|v| v.policy_compatible(&self.policy_hash))
            .map(|v| v.verify(claim))
            .collect();
        Ok(results)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VeriClaimsError {
    #[error("claim policy hash does not match pipeline policy")]
    PolicyMismatch,
    #[error("missing required evidence: {0:?}")]
    MissingEvidence(ClaimPredicate),
}

impl ClaimAssertion {
    /// Create a new claim from claimant, predicate, and object.
    pub fn new(
        claimant: &str,
        predicate: ClaimPredicate,
        object: &str,
        policy_hash: ContentHash,
    ) -> Self {
        let claim_id = pt_core::hash_str(&format!(
            "vericlaim:{}:{:?}:{}",
            claimant, predicate, object
        ));
        Self {
            claim_id,
            claimant: claimant.to_string(),
            predicate,
            object: object.to_string(),
            evidence_chain: Vec::new(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            policy_hash,
        }
    }

    /// Add evidence to the claim's chain.
    pub fn add_evidence(&mut self, link: EvidenceLink) {
        self.evidence_chain.push(link);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pt_core::hash_str;

    struct AlwaysPassVerifier;
    impl ClaimVerifier for AlwaysPassVerifier {
        fn verify(&self, _claim: &ClaimAssertion) -> VerificationResult {
            VerificationResult::Verified {
                by: "always_pass".into(),
                evidence_used: vec![],
                confidence: 0.5,
            }
        }
        fn method_name(&self) -> &'static str {
            "always_pass"
        }
        fn policy_compatible(&self, _policy_hash: &ContentHash) -> bool {
            true
        }
    }

    #[test]
    fn claim_is_reproducible() {
        let policy = hash_str("policy_v1");
        let c1 = ClaimAssertion::new("alice", ClaimPredicate::Damage, "car_window", policy);
        let c2 = ClaimAssertion::new("alice", ClaimPredicate::Damage, "car_window", policy);
        assert_eq!(c1.claim_id, c2.claim_id);
    }

    #[test]
    fn pipeline_runs_all_stages() {
        let policy = hash_str("policy_v1");
        let mut pipeline = VerificationPipeline::new(policy);
        pipeline.add_stage(Box::new(AlwaysPassVerifier));
        pipeline.add_stage(Box::new(AlwaysPassVerifier));

        let claim = ClaimAssertion::new("bob", ClaimPredicate::Theft, "laptop", policy);
        let results = pipeline.execute(&claim).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn policy_mismatch_is_rejected() {
        let policy_a = hash_str("policy_a");
        let policy_b = hash_str("policy_b");
        let pipeline = VerificationPipeline::new(policy_a);
        let claim = ClaimAssertion::new("carol", ClaimPredicate::Loss, "wallet", policy_b);
        assert!(pipeline.execute(&claim).is_err());
    }
}
