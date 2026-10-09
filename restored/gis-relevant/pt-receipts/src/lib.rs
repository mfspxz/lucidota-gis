//! pt-receipts — typed agent completion reports and validation receipts.
//!
//! Phase 0 of a larger orchestration-kernel proposal (task DAG, bounded
//! agent trees, soft interrupts, provenance-aware memory — see
//! `05_OUTPUTS/JCODE_KERNEL_PROPOSAL.md` at the LUCIDOTA root for the full
//! roadmap). Only this piece is built: everything else in that proposal is
//! deferred, not implemented, and should not be assumed to exist. Building
//! the rest speculatively — a task-DAG crate, an agent-kernel crate, a
//! memory-graph crate — with no real caller yet would be exactly the
//! orphan-abstraction problem this repo's own ponytail discipline exists to
//! prevent.
//!
//! What this crate actually does: gives every agent (this one included) a
//! structured way to say what it did, with what evidence, and what's still
//! open — instead of a prose summary that could be fabricated. Ecosystem-
//! wide (PALANTOUR, not SHAMBHALA-local) because a completion report's shape
//! shouldn't differ per project, same reasoning as GO-50 and the organelle
//! pattern (see `GO50_ENGINEERING_DOCTRINE.md`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// How strongly a claim is backed by evidence, weakest to strongest. Derives
/// `Ord` deliberately — callers can assert a minimum bar (e.g. "reject any
/// report claiming done at less than `Tested`") instead of string-matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLevel {
    Discussed,
    Planned,
    Implemented,
    Compiled,
    Tested,
    IntegrationTested,
    OperationallyObserved,
    IndependentlyVerified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionStatus {
    Done,
    PartiallyDone,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationResult {
    Pass,
    Fail,
    Skipped,
}

/// A pointer to a real artifact (file, log, commit) — never a paraphrase of
/// one. Mirrors this repo's standing rule to point at a log/diff/commit
/// rather than narrate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub path: String,
    /// Commit hash, line range, or other locator within `path`. Optional —
    /// not every artifact is inside a git repo (e.g. a live curl response).
    #[serde(default)]
    pub locator: Option<String>,
}

impl ArtifactRef {
    pub fn new(path: impl Into<String>) -> Self {
        Self { path: path.into(), locator: None }
    }

    pub fn at(path: impl Into<String>, locator: impl Into<String>) -> Self {
        Self { path: path.into(), locator: Some(locator.into()) }
    }
}

/// A single command run as evidence for a claim, with its real exit code —
/// not "tests passed" as prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReceipt {
    pub command: String,
    pub exit_code: Option<i32>,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub result: ValidationResult,
    /// Free-text digest of the output relevant to the claim (e.g. "18/18
    /// tests passed" or the curl response body) — not the full log, which
    /// belongs in `evidence` as an `ArtifactRef` if it's large.
    pub output_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub summary: String,
    pub evidence_level: EvidenceLevel,
    #[serde(default)]
    pub evidence: Vec<ArtifactRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blocker {
    pub summary: String,
    /// What would need to be true for this to stop being a blocker.
    pub unblock_condition: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assumption {
    pub summary: String,
    /// Why this was assumed rather than verified (e.g. "no access to prod",
    /// "operator confirmed verbally, no receipt").
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskProposal {
    pub summary: String,
    pub reason: String,
}

/// What an agent (or a human) reports back after a unit of work. The
/// central object of this crate — every other type exists to make this one
/// honest instead of narrative.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionReport {
    pub status: CompletionStatus,
    pub summary: String,
    #[serde(default)]
    pub changed_artifacts: Vec<ArtifactRef>,
    #[serde(default)]
    pub validations: Vec<ValidationReceipt>,
    #[serde(default)]
    pub unresolved_findings: Vec<Finding>,
    #[serde(default)]
    pub blockers: Vec<Blocker>,
    #[serde(default)]
    pub assumptions: Vec<Assumption>,
    #[serde(default)]
    pub follow_up_tasks: Vec<TaskProposal>,
}

impl CompletionReport {
    /// The strongest evidence level actually attached to this report via
    /// its validation receipts. `None` if nothing was validated at all —
    /// which is itself useful signal (a `Done` report with no validations
    /// is a claim, not a receipt).
    pub fn strongest_evidence(&self) -> Option<EvidenceLevel> {
        if self.validations.iter().any(|v| v.result == ValidationResult::Pass) {
            Some(EvidenceLevel::Tested)
        } else {
            None
        }
    }

    /// True if every validation receipt attached passed. Vacuously true for
    /// zero receipts — callers that care about "no receipts at all" should
    /// check `self.validations.is_empty()` separately.
    pub fn all_validations_passed(&self) -> bool {
        self.validations.iter().all(|v| v.result == ValidationResult::Pass)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_level_orders_weakest_to_strongest() {
        assert!(EvidenceLevel::Discussed < EvidenceLevel::Planned);
        assert!(EvidenceLevel::Tested < EvidenceLevel::IntegrationTested);
        assert!(EvidenceLevel::IntegrationTested < EvidenceLevel::OperationallyObserved);
        assert!(EvidenceLevel::OperationallyObserved < EvidenceLevel::IndependentlyVerified);
    }

    #[test]
    fn completion_report_round_trips_through_json() {
        let report = CompletionReport {
            status: CompletionStatus::Done,
            summary: "wired three real llama-server backends into the model registry".into(),
            changed_artifacts: vec![ArtifactRef::at(
                "S_H_A_M_B_A_L_A/crates/shambhala/src/main.rs",
                "step 4",
            )],
            validations: vec![ValidationReceipt {
                command: "curl -s -X POST http://localhost:8090/chat ...".into(),
                exit_code: Some(0),
                started_at: Utc::now(),
                finished_at: Utc::now(),
                result: ValidationResult::Pass,
                output_digest: "model: http-http://127.0.0.1:8083/... — real FSM-230M reply".into(),
            }],
            unresolved_findings: vec![],
            blockers: vec![],
            assumptions: vec![Assumption {
                summary: "no separate 1.58-bit ternary quant tested for Bonsai-4B".into(),
                reason: "only Q1_0 (true 1-bit) exists on disk; no second variant to compare".into(),
            }],
            follow_up_tasks: vec![],
        };

        let json = serde_json::to_string(&report).expect("serialize");
        let restored: CompletionReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.status, CompletionStatus::Done);
        assert_eq!(restored.assumptions.len(), 1);
        assert!(restored.all_validations_passed());
        assert_eq!(restored.strongest_evidence(), Some(EvidenceLevel::Tested));
    }

    #[test]
    fn report_with_no_validations_has_no_strongest_evidence() {
        let report = CompletionReport {
            status: CompletionStatus::Done,
            summary: "claimed done, nothing run to check it".into(),
            changed_artifacts: vec![],
            validations: vec![],
            unresolved_findings: vec![],
            blockers: vec![],
            assumptions: vec![],
            follow_up_tasks: vec![],
        };
        assert_eq!(report.strongest_evidence(), None);
        assert!(report.all_validations_passed(), "vacuously true for zero receipts");
    }

    #[test]
    fn failed_validation_is_not_all_passed() {
        let report = CompletionReport {
            status: CompletionStatus::PartiallyDone,
            summary: "one check failed".into(),
            changed_artifacts: vec![],
            validations: vec![ValidationReceipt {
                command: "cargo test -p shambhala-core".into(),
                exit_code: Some(1),
                started_at: Utc::now(),
                finished_at: Utc::now(),
                result: ValidationResult::Fail,
                output_digest: "1 failed, 34 passed".into(),
            }],
            unresolved_findings: vec![],
            blockers: vec![],
            assumptions: vec![],
            follow_up_tasks: vec![],
        };
        assert!(!report.all_validations_passed());
    }
}
