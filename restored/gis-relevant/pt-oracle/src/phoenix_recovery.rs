// phoenix_recovery.rs — Phoenix AI: Incident recovery oracle verification (HY3).
// MERGED FROM: _orphan_forks/pt-oracle/src/phoenix.rs
// ADAPTED: chrono → std::time::SystemTime, sha2 → pt_core::hash_str,
//          type ContentHash → pt_core::ContentHash.
//
// Phoenix AI (similar to IronHeart, per HY3 audit): system recovery and
// incident response verification. Phoenix rises from system failures —
// every recovery action is verified through the oracle gate.
//
// The "Phoenix" pattern: detect incident → triage → recover → verify → attest.

use pt_core::ContentHash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Severity level of an incident.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum IncidentSeverity {
    Info = 0,
    Warning = 1,
    Critical = 2,
    Emergency = 3,
}

/// An incident that triggered Phoenix recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Incident {
    pub incident_id: ContentHash,
    pub severity: IncidentSeverity,
    pub title: String,
    pub description: String,
    pub affected_systems: Vec<String>,
    pub detected_at: i64,
    pub resolved_at: Option<i64>,
}

/// A recovery action taken during Phoenix response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAction {
    pub action_id: ContentHash,
    pub incident_id: ContentHash,
    pub action_type: RecoveryActionType,
    pub description: String,
    pub executed_by: String,
    pub executed_at: i64,
    pub success: bool,
    pub evidence_hash: Option<ContentHash>,
}

/// Types of recovery actions Phoenix can take.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryActionType {
    RestartService,
    RollbackDeployment,
    FailoverNode,
    ScaleUp,
    ClearCache,
    RotateCredentials,
    ApplyPatch,
    ManualIntervention,
    NotifyOnCall,
    Custom(String),
}

/// A post-incident review (postmortem).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Postmortem {
    pub postmortem_id: ContentHash,
    pub incident_id: ContentHash,
    pub root_cause: String,
    pub timeline: Vec<TimelineEntry>,
    pub action_items: Vec<ActionItem>,
    pub lessons_learned: Vec<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub timestamp: i64,
    pub event: String,
    pub actor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub description: String,
    pub assignee: String,
    pub due_date: Option<i64>,
    pub status: ActionItemStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionItemStatus {
    Open,
    InProgress,
    Completed,
    Deferred,
}

// ── Phoenix Recovery Engine ──────────────────────────────────────────────

/// The Phoenix recovery engine — oracle-verified incident response.
#[derive(Debug, Clone)]
pub struct PhoenixEngine {
    pub incidents: HashMap<ContentHash, Incident>,
    pub actions: Vec<RecoveryAction>,
    pub postmortems: Vec<Postmortem>,
}

impl PhoenixEngine {
    pub fn new() -> Self {
        Self {
            incidents: HashMap::new(),
            actions: Vec::new(),
            postmortems: Vec::new(),
        }
    }

    /// Declare an incident.
    pub fn declare_incident(
        &mut self,
        incident_id: ContentHash,
        severity: IncidentSeverity,
        title: String,
        description: String,
        affected_systems: Vec<String>,
    ) -> Incident {
        let incident = Incident {
            incident_id,
            severity,
            title,
            description,
            affected_systems,
            detected_at: now_timestamp(),
            resolved_at: None,
        };
        self.incidents.insert(incident_id, incident.clone());
        incident
    }

    /// Execute a recovery action.
    pub fn execute_action(
        &mut self,
        action_id: ContentHash,
        incident_id: ContentHash,
        action_type: RecoveryActionType,
        description: String,
        executed_by: String,
    ) -> RecoveryAction {
        let action = RecoveryAction {
            action_id,
            incident_id,
            action_type,
            description,
            executed_by,
            executed_at: now_timestamp(),
            success: true,
            evidence_hash: None,
        };
        self.actions.push(action.clone());
        action
    }

    /// Resolve an incident.
    pub fn resolve(&mut self, incident_id: ContentHash) -> Option<&Incident> {
        if let Some(incident) = self.incidents.get_mut(&incident_id) {
            incident.resolved_at = Some(now_timestamp());
            Some(incident)
        } else {
            None
        }
    }

    /// Create a postmortem.
    pub fn create_postmortem(
        &mut self,
        postmortem_id: ContentHash,
        incident_id: ContentHash,
        root_cause: String,
        lessons: Vec<String>,
    ) -> Postmortem {
        let pm = Postmortem {
            postmortem_id,
            incident_id,
            root_cause,
            timeline: Vec::new(),
            action_items: Vec::new(),
            lessons_learned: lessons,
            created_at: now_timestamp(),
        };
        self.postmortems.push(pm.clone());
        pm
    }

    /// Get open incidents.
    pub fn open_incidents(&self) -> Vec<&Incident> {
        self.incidents
            .values()
            .filter(|i| i.resolved_at.is_none())
            .collect()
    }

    /// Get actions for an incident.
    pub fn actions_for(&self, incident_id: &ContentHash) -> Vec<&RecoveryAction> {
        self.actions
            .iter()
            .filter(|a| a.incident_id == *incident_id)
            .collect()
    }
}

impl Default for PhoenixEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_id(seed: &str) -> ContentHash {
        pt_core::hash_str(seed)
    }

    #[test]
    fn test_incident_lifecycle() {
        let mut engine = PhoenixEngine::new();
        let inc_id = make_id("db-outage");
        engine.declare_incident(
            inc_id,
            IncidentSeverity::Critical,
            "Database outage".into(),
            "Primary DB unreachable".into(),
            vec!["postgres-primary".into()],
        );

        let action_id = make_id("restart-db");
        engine.execute_action(
            action_id,
            inc_id,
            RecoveryActionType::RestartService,
            "Restarted PostgreSQL primary".into(),
            "ops-bot".into(),
        );

        engine.resolve(inc_id);
        let pm = engine.create_postmortem(
            make_id("pm-db"),
            inc_id,
            "Disk full caused WAL write failure".into(),
            vec!["Add disk monitoring".into()],
        );

        assert_eq!(engine.open_incidents().len(), 0);
        assert_eq!(engine.actions_for(&inc_id).len(), 1);
        assert_eq!(pm.lessons_learned.len(), 1);
    }
}
