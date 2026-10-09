//! Candidate management — staging area for oracle outputs.
//!
//! Oracle candidates land here at epistemic 0. Only the deterministic
//! validator can promote them to the truth layer.
//!
//! Rule promotion: when a candidate is validated N times across different
//! captures, its extraction pattern gets compiled into a fast-lane AdapterRule.

use crate::{ByteSpan, OracleOutput};
use pt_core::ContentHash;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A staged candidate awaiting validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedCandidate {
    /// Which capture this was extracted from.
    pub capture_hash: ContentHash,
    /// The oracle output.
    pub output: OracleOutput,
    /// How many times this pattern has been validated (== `validating_captures.len()`).
    pub validation_count: u32,
    /// The set of DISTINCT captures that have validated this candidate. Rule
    /// promotion requires ≥2 *independent* captures; counting raw validations
    /// let a replayed single capture forge a second "independent" validation
    /// (see SECURITY_AUDIT_VERIFIED_2026-07-22 H2). Dedup is by capture hash.
    #[serde(default)]
    pub validating_captures: HashSet<ContentHash>,
    /// Whether this candidate has been promoted to truth.
    pub promoted: bool,
    /// The validator's ruling.
    pub ruling: Option<ValidationRuling>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRuling {
    pub passed: bool,
    pub method: String,
    pub evidence: String,
    pub validator_version: String,
}

/// Candidate store — manages the staging area.
pub struct CandidateStore {
    candidates: Vec<StagedCandidate>,
}

impl CandidateStore {
    pub fn new() -> Self {
        Self { candidates: Vec::new() }
    }

    /// Stage a new candidate for validation.
    pub fn stage(&mut self, capture_hash: &ContentHash, output: OracleOutput) {
        self.candidates.push(StagedCandidate {
            capture_hash: *capture_hash,
            output,
            validation_count: 0,
            validating_captures: HashSet::new(),
            promoted: false,
            ruling: None,
        });
    }

    /// Get all unvalidated candidates.
    pub fn pending(&self) -> Vec<&StagedCandidate> {
        self.candidates.iter().filter(|c| !c.promoted).collect()
    }

    /// Real (full-vec) indices of unvalidated candidates. `validate_all` MUST
    /// use these — enumerating `pending()` yields *filtered* indices that no
    /// longer map to the full vec once anything is promoted, which wrote rulings
    /// to the wrong candidate (SECURITY_AUDIT_VERIFIED_2026-07-22 M2).
    pub fn pending_indices(&self) -> Vec<usize> {
        self.candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.promoted)
            .map(|(i, _)| i)
            .collect()
    }

    /// Borrow a candidate by its real index.
    pub fn candidate(&self, index: usize) -> Option<&StagedCandidate> {
        self.candidates.get(index)
    }

    /// Mark a candidate as validated (ruled on).
    pub fn validate(&mut self, index: usize, ruling: ValidationRuling) {
        if let Some(c) = self.candidates.get_mut(index) {
            c.ruling = Some(ruling);
            c.promoted = true;
        }
    }

    /// Record a validation by a specific capture. Counts only DISTINCT captures:
    /// returns true if this capture is new to the candidate (and bumps the
    /// count), false if it was already counted (a replay). This is the fix that
    /// makes "≥2 independent validations" mean two *different* captures.
    pub fn increment_validation(&mut self, index: usize, capture_hash: &ContentHash) -> bool {
        if let Some(c) = self.candidates.get_mut(index) {
            let is_new = c.validating_captures.insert(*capture_hash);
            if is_new {
                c.validation_count = c.validating_captures.len() as u32;
            }
            is_new
        } else {
            false
        }
    }

    /// Candidates validated by ≥2 DISTINCT captures — eligible to compile into a
    /// fast-lane rule. Distinctness (not raw count) is the security invariant.
    pub fn promotable(&self) -> Vec<&StagedCandidate> {
        self.candidates
            .iter()
            .filter(|c| c.validating_captures.len() >= 2 && c.promoted)
            .collect()
    }
}

impl Default for CandidateStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ruling() -> ValidationRuling {
        ValidationRuling {
            passed: true,
            method: "test".into(),
            evidence: "e".into(),
            validator_version: "0".into(),
        }
    }

    // CONTRACT (H2): promotion to a fast-lane rule requires ≥2 DISTINCT captures.
    // Replaying the SAME capture must not forge a second independent validation.
    #[test]
    fn promotion_requires_two_distinct_captures_not_replays() {
        let cap_a = pt_core::hash_str("capture-A");
        let cap_b = pt_core::hash_str("capture-B");
        let mut store = CandidateStore::new();
        store.stage(&cap_a, OracleOutput::Nothing);

        assert!(store.increment_validation(0, &cap_a), "first capture counts");
        assert!(!store.increment_validation(0, &cap_a), "replaying the same capture does NOT count");
        store.validate(0, ruling());
        assert!(store.promotable().is_empty(), "one distinct capture is not promotable");

        assert!(store.increment_validation(0, &cap_b), "a second, distinct capture counts");
        assert_eq!(store.promotable().len(), 1, "two distinct captures + ruled -> promotable");
        assert_eq!(store.candidate(0).unwrap().validation_count, 2, "count tracks distinct captures");
    }

    // CONTRACT (M2): pending_indices returns REAL vec indices, stable as
    // candidates get promoted — so rulings never land on the wrong candidate.
    #[test]
    fn pending_indices_are_real_and_survive_promotion() {
        let cap = pt_core::hash_str("c");
        let mut store = CandidateStore::new();
        store.stage(&cap, OracleOutput::Nothing); // idx 0
        store.stage(&cap, OracleOutput::Nothing); // idx 1
        store.stage(&cap, OracleOutput::Nothing); // idx 2
        store.validate(0, ruling()); // promote idx 0

        let idxs = store.pending_indices();
        assert_eq!(idxs, vec![1, 2], "real indices of the still-pending candidates, not 0..n");
    }
}
