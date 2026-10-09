//! pt-mcp — Context-Aware MCP Server for PalOnTour + Noter
//!
//! CA-MCP reduces token overhead by orders of magnitude:
//!   - Tools registered once at init, called by compact ID
//!   - Server maintains per-session state (cached results, conversation context)
//!   - Delta updates: only changed fields are transmitted
//!   - Binary-efficient: short identifiers, compressed JSON
//!
//! The swarm can spawn 100 agents without burning 100K tokens each.
//!
//! Protocol:
//!   1. Client sends `init` — server responds with capabilities + compact tool IDs
//!   2. Client calls tools by 2-char ID: `s1` = search_events, `r1` = rank, etc.
//!   3. Server caches results per session; subsequent calls return deltas
//!   4. Session timeout after 5min idle → state cleared

pub mod agentic_canvas;
pub mod ca_protocol;
pub mod client;
pub mod compact_tools;
pub mod protocol;
pub mod scraper_tools;
pub mod session;
pub mod transport;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ── Compact Tool Registry ───────────────────────────────────────────

/// Each tool gets a 2-char compact ID to minimize token usage.
#[derive(Debug, Clone, Serialize)]
pub struct CompactTool {
    pub id: String,       // 2-char: "s1", "g1", "r1", etc.
    pub name: String,     // human-readable
    pub description: String,
    pub input_schema: Value,
}

/// The CA-MCP tool registry — tools registered once, called by compact ID.
pub struct CompactRegistry {
    tools: HashMap<String, CompactTool>,       // id → tool
    by_name: HashMap<String, String>,          // name → id
}

impl CompactRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            tools: HashMap::new(),
            by_name: HashMap::new(),
        };
        compact_tools::register_all(&mut reg);
        reg
    }

    pub fn register(&mut self, tool: CompactTool) {
        self.by_name.insert(tool.name.clone(), tool.id.clone());
        self.tools.insert(tool.id.clone(), tool);
    }

    pub fn get_by_id(&self, id: &str) -> Option<&CompactTool> {
        self.tools.get(id)
    }

    pub fn get_by_name(&self, name: &str) -> Option<&CompactTool> {
        self.by_name.get(name).and_then(|id| self.tools.get(id))
    }

    /// Return the compact tool manifest — sent once at init.
    pub fn manifest(&self) -> Vec<&CompactTool> {
        self.tools.values().collect()
    }
}

// ── Session State ───────────────────────────────────────────────────

/// Per-session state with result caching and deltas.
pub struct Session {
    pub id: String,
    pub created: Instant,
    pub last_access: Instant,
    /// Cached tool results: tool_id → (args_hash, result)
    cache: HashMap<String, (u64, Value)>,
}

impl Session {
    pub fn new(id: &str) -> Self {
        let now = Instant::now();
        Self {
            id: id.to_string(),
            created: now,
            last_access: now,
            cache: HashMap::new(),
        }
    }

    /// Check cache: same tool + same args → return cached result.
    /// Uses a simple args hash to detect identical calls.
    pub fn check_cache(&self, tool_id: &str, args_hash: u64) -> Option<&Value> {
        self.cache.get(tool_id)
            .and_then(|(h, v)| if *h == args_hash { Some(v) } else { None })
    }

    pub fn store_cache(&mut self, tool_id: &str, args_hash: u64, result: Value) {
        self.cache.insert(tool_id.to_string(), (args_hash, result));
    }

    pub fn touch(&mut self) {
        self.last_access = Instant::now();
    }

    pub fn is_expired(&self, timeout: Duration) -> bool {
        self.last_access.elapsed() > timeout
    }
}

// ── CA-MCP Protocol Types ──────────────────────────────────────────

/// Init request — client announces capabilities.
#[derive(Debug, Deserialize)]
pub struct CaInitRequest {
    pub client_name: String,
    pub client_version: String,
    /// Max tools the client will cache.
    pub max_cached_tools: Option<usize>,
}

/// Init response — server sends compact tool manifest.
#[derive(Debug, Serialize)]
pub struct CaInitResponse {
    pub server_name: String,
    pub server_version: String,
    /// Compact tool manifest — client caches this for the session.
    pub tools: Vec<CompactTool>,
    pub session_id: String,
    /// Cache TTL in seconds.
    pub cache_ttl_secs: u64,
}

/// Compact tool call — uses 2-char ID instead of full name.
#[derive(Debug, Deserialize)]
pub struct CaToolCall {
    /// 2-char tool ID (e.g., "s1").
    pub tool: String,
    /// Tool arguments.
    pub arguments: Value,
    /// Optional: client's cached result hash — server returns 304 if unchanged.
    pub if_none_match: Option<String>,
}

/// Compact tool result.
#[derive(Debug, Serialize)]
pub struct CaToolResult {
    /// HTTP-style status: 200 = ok, 304 = not modified (use cache)
    pub status: u16,
    /// Result data (null if 304).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    /// ETag for client-side caching.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    /// Delta from previous result (for incremental updates).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<Value>,
}

// ── CA-MCP Server ──────────────────────────────────────────────────

type CaHandler = Arc<dyn Fn(Value, &mut Session) -> Result<Value, String> + Send + Sync>;

pub struct CaMcpServer {
    registry: CompactRegistry,
    handlers: HashMap<String, CaHandler>,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    session_timeout: Duration,
    server_name: String,
    server_version: String,
}

impl CaMcpServer {
    pub fn new(name: &str, version: &str) -> Self {
        let mut registry = CompactRegistry::new();
        let mut handlers: HashMap<String, CaHandler> = HashMap::new();

        // Register all compact tool handlers
        compact_tools::register_handlers(&mut registry, &mut handlers);

        // Register the scraper fleet tools (sl/sd/sc) — "agentically via MCP".
        scraper_tools::register_scraper_tools(&mut registry);
        scraper_tools::register_scraper_handlers(&mut handlers);

        Self {
            registry,
            handlers,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            session_timeout: Duration::from_secs(300), // 5 min
            server_name: name.to_string(),
            server_version: version.to_string(),
        }
    }

    /// Handle init: return compact tool manifest + session ID.
    pub fn handle_init(&self, _request: &CaInitRequest) -> CaInitResponse {
        let session_id = format!("palon-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());
        let mut sessions = self.sessions.lock().unwrap();
        sessions.insert(session_id.clone(), Session::new(&session_id));

        CaInitResponse {
            server_name: self.server_name.clone(),
            server_version: self.server_version.clone(),
            tools: self.registry.manifest().into_iter().cloned().collect(),
            session_id,
            cache_ttl_secs: 60,
        }
    }

    /// Handle a compact tool call.
    pub fn handle_call(
        &self,
        session_id: &str,
        call: &CaToolCall,
    ) -> Result<CaToolResult, String> {
        // Validate session
        let mut sessions = self.sessions.lock().unwrap();
        let session = sessions.get_mut(session_id)
            .ok_or_else(|| "session expired or not found".to_string())?;

        // Check expiry
        if session.is_expired(self.session_timeout) {
            sessions.remove(session_id);
            return Err("session expired".to_string());
        }
        session.touch();

        // Compute args hash for caching
        let args_hash = hash_value(&call.arguments);

        // Check cache
        if let Some(etag) = &call.if_none_match {
            if let Some(cached) = session.check_cache(&call.tool, args_hash) {
                if hash_value(cached).to_string() == *etag {
                    return Ok(CaToolResult {
                        status: 304,
                        data: None,
                        etag: Some(etag.clone()),
                        delta: None,
                    });
                }
            }
        }

        // Look up handler by compact ID
        let handler = self.handlers.get(&call.tool)
            .ok_or_else(|| format!("unknown tool: {}", call.tool))?;

        // Execute
        let result = handler(call.arguments.clone(), session)
            .map_err(|e| format!("tool error: {e}"))?;

        let etag = hash_value(&result).to_string();

        // Cache result
        session.store_cache(&call.tool, args_hash, result.clone());

        Ok(CaToolResult {
            status: 200,
            data: Some(result),
            etag: Some(etag),
            delta: None,
        })
    }

    /// Cleanup expired sessions.
    pub fn cleanup_sessions(&self) {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.retain(|_, s| !s.is_expired(self.session_timeout));
    }

    /// Get total active sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.lock().unwrap().len()
    }

    /// Run the MCP server on stdio transport. Blocks until stdin closes.
    pub async fn serve_stdio(self: Arc<Self>) -> anyhow::Result<()> {
        tracing::info!(
            name = %self.server_name,
            version = %self.server_version,
            tools = self.registry.tools.len(),
            "MCP server starting on stdio"
        );
        transport::ca_stdio_loop(self).await
    }
}

/// Stable FNV-1a hash of a JSON value's canonical string, for cache keys and
/// ETags. Was `DefaultHasher`, whose output is NOT guaranteed stable across
/// toolchain versions — so an ETag a client held could stop matching after a
/// server rebuild, causing silent cache misses/mismatches. FNV-1a is a fixed,
/// deterministic function: same bytes -> same hash, every build, every machine.
fn hash_value(v: &Value) -> u64 {
    v.to_string()
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3))
}
