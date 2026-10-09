// IMPLEMENTS: DEVSTRAL_PULL_AUDIT.md Pull #6 (agentic canvas os)
//! Agentic Canvas OS — Capability-based agent runtime for pt-mcp.
//!
//! The agentic canvas provides a spatial reasoning surface where agents
//! are first-class entities with capability-based security. Agents act
//! through capabilities they hold; the MCP bridge exposes tools as
//! capability-gated resources.

use pt_core::ContentHash;
use pt_organelle_core::MutationClass;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A capability — the fundamental security primitive of the agent OS.
/// Agents can only act through capabilities they hold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub cap_id: ContentHash,
    pub resource_uri: String,
    pub permissions: PermissionSet,
    pub issuer: ContentHash,
    pub holder: ContentHash,
    pub expiry: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionSet {
    pub read: bool,
    pub write: bool,
    pub append: bool,
    pub delegate: bool,
    pub execute: bool,
}

/// An agent running in the canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasAgent {
    pub id: ContentHash,
    pub name: String,
    pub capabilities: Vec<Capability>,
    /// Current spatial position on the canvas [x, y].
    pub position: [f32; 2],
    pub status: AgentStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Thinking,
    Acting,
    WaitingForApproval,
    Error(String),
}

/// An action an agent can take, gated by capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAction {
    pub action_id: ContentHash,
    pub agent_id: ContentHash,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    /// The capability authorizing this action.
    pub required_cap: Option<ContentHash>,
    /// Side-effect class (pt-organelle-core). Decides the no-capability default:
    /// ReadOnly may run uncapabilitied; Write/ExternalEffect must present a cap.
    pub mutation_class: MutationClass,
}

/// The agentic canvas runtime — manages agents and their capabilities.
pub struct AgenticCanvas {
    agents: HashMap<ContentHash, CanvasAgent>,
    capabilities: HashMap<ContentHash, Capability>,
}

impl AgenticCanvas {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            capabilities: HashMap::new(),
        }
    }

    /// Register an agent on the canvas.
    pub fn register_agent(&mut self, agent: CanvasAgent) -> ContentHash {
        let id = agent.id;
        self.agents.insert(id, agent);
        id
    }

    /// Grant a capability to an agent.
    pub fn grant_capability(
        &mut self,
        agent_id: &ContentHash,
        cap: Capability,
    ) -> Result<(), CanvasError> {
        let agent = self
            .agents
            .get_mut(agent_id)
            .ok_or(CanvasError::AgentNotFound)?;
        self.capabilities.insert(cap.cap_id, cap.clone());
        agent.capabilities.push(cap);
        Ok(())
    }

    /// Check if an agent holds the capability required for an action.
    pub fn authorize_action(
        &self,
        agent_id: &ContentHash,
        action: &AgentAction,
    ) -> Result<bool, CanvasError> {
        let agent = self
            .agents
            .get(agent_id)
            .ok_or(CanvasError::AgentNotFound)?;

        if let Some(req_cap) = &action.required_cap {
            let has_cap = agent.capabilities.iter().any(|c| {
                c.cap_id == *req_cap
                    && c.permissions.execute
                    && c.expiry.map_or(true, |exp| exp > chrono::Utc::now().timestamp())
            });
            Ok(has_cap)
        } else {
            // No capability declared. Open ONLY for ReadOnly work; deny
            // Write/ExternalEffect by default (was: Ok(true) for everything,
            // which let any agent run any uncapabilitied mutating/external
            // action — RFC-0042 §3.1 / bug 10.7).
            Ok(action.mutation_class == MutationClass::ReadOnly)
        }
    }

    /// Get all registered agents.
    pub fn agents(&self) -> impl Iterator<Item = &CanvasAgent> {
        self.agents.values()
    }
}

impl Default for AgenticCanvas {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanvasError {
    AgentNotFound,
    InsufficientCapabilities,
    CapabilityExpired,
}

impl std::fmt::Display for CanvasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AgentNotFound => write!(f, "agent not found"),
            Self::InsufficientCapabilities => write!(f, "insufficient capabilities"),
            Self::CapabilityExpired => write!(f, "capability expired"),
        }
    }
}

impl std::error::Error for CanvasError {}

#[cfg(test)]
mod tests {
    use super::*;
    use pt_core::hash_str;

    fn test_agent() -> CanvasAgent {
        CanvasAgent {
            id: hash_str("agent_alpha"),
            name: "Alpha".into(),
            capabilities: vec![],
            position: [0.0, 0.0],
            status: AgentStatus::Idle,
        }
    }

    #[test]
    fn agent_registration() {
        let mut canvas = AgenticCanvas::new();
        let agent = test_agent();
        let id = canvas.register_agent(agent);
        assert_eq!(id, hash_str("agent_alpha"));
    }

    #[test]
    fn capability_grant_and_check() {
        let mut canvas = AgenticCanvas::new();
        let agent = test_agent();
        let agent_id = canvas.register_agent(agent);

        let cap = Capability {
            cap_id: hash_str("cap_exec"),
            resource_uri: "tool://search".into(),
            permissions: PermissionSet {
                read: true,
                write: false,
                append: false,
                delegate: false,
                execute: true,
            },
            issuer: [0u8; 32],
            holder: agent_id,
            expiry: None,
        };
        canvas.grant_capability(&agent_id, cap.clone()).unwrap();

        let action = AgentAction {
            action_id: hash_str("action_1"),
            agent_id,
            tool_name: "search".into(),
            arguments: serde_json::json!({"q": "test"}),
            required_cap: Some(cap.cap_id),
            mutation_class: MutationClass::ReadOnly,
        };
        assert!(canvas.authorize_action(&agent_id, &action).unwrap());
    }

    #[test]
    fn unauthorized_action_fails() {
        let mut canvas = AgenticCanvas::new();
        let agent = test_agent();
        let agent_id = canvas.register_agent(agent);

        let action = AgentAction {
            action_id: hash_str("action_2"),
            agent_id,
            tool_name: "delete".into(),
            arguments: serde_json::json!({}),
            required_cap: Some(hash_str("nonexistent")),
            mutation_class: MutationClass::ExternalEffect,
        };
        assert!(!canvas.authorize_action(&agent_id, &action).unwrap());
    }

    #[test]
    fn agent_not_found() {
        let canvas = AgenticCanvas::new();
        let fake_id = hash_str("ghost_agent");
        assert!(canvas
            .authorize_action(&fake_id, &AgentAction {
                action_id: [0u8; 32],
                agent_id: fake_id,
                tool_name: "ping".into(),
                arguments: serde_json::json!({}),
                required_cap: None,
                mutation_class: MutationClass::ReadOnly,
            })
            .is_err());
    }

    // CONTRACT (bug 10.7 fix): an action with NO declared capability is allowed
    // only when ReadOnly; a Write/ExternalEffect action with no capability is
    // denied by default, not auto-approved.
    #[test]
    fn no_capability_readonly_allowed_but_external_denied() {
        let mut canvas = AgenticCanvas::new();
        let agent_id = canvas.register_agent(test_agent());

        let readonly = AgentAction {
            action_id: hash_str("ro"),
            agent_id,
            tool_name: "search".into(),
            arguments: serde_json::json!({}),
            required_cap: None,
            mutation_class: MutationClass::ReadOnly,
        };
        assert!(
            canvas.authorize_action(&agent_id, &readonly).unwrap(),
            "ReadOnly with no capability may run"
        );

        for mc in [MutationClass::Write, MutationClass::ExternalEffect] {
            let mutating = AgentAction {
                action_id: hash_str("mut"),
                agent_id,
                tool_name: "scrape".into(),
                arguments: serde_json::json!({}),
                required_cap: None,
                mutation_class: mc,
            };
            assert!(
                !canvas.authorize_action(&agent_id, &mutating).unwrap(),
                "{mc:?} with no capability must be DENIED, not auto-approved"
            );
        }
    }
}
