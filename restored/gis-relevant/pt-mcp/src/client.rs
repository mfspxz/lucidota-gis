//! Stdio MCP client — spawns a local MCP server (Serena, or any other
//! stdio-speaking tool server) as a subprocess and drives it over
//! newline-delimited JSON-RPC on its stdin/stdout.
//!
//! This is the piece this ecosystem was missing: `pt-mcp`/`pt-percyphon-mcp`
//! already speak stdio JSON-RPC as *servers* (see transport.rs); nothing
//! spoke it as a *client*. `ironclaw_mcp`'s extension host only knows how to
//! reach remote HTTPS-hosted MCP endpoints (Notion-style), which is a
//! structurally different transport and can't drive a local subprocess tool
//! like Serena at all. This closes that gap directly in-process, no
//! llama-server browser tab, no HTTP hop — spawn, JSON-RPC, done.
//!
//! Deliberately simple for a first working version: one in-flight request
//! at a time (write, then read lines until the matching `id` arrives).
//! Concurrent/pipelined requests and server-initiated notifications beyond
//! startup are not yet handled — real limitations, not hidden ones.

use crate::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};

/// One MCP tool as advertised by `tools/list`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct McpTool {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "inputSchema", default)]
    pub input_schema: Option<Value>,
}

/// A live connection to a subprocess-hosted MCP server.
pub struct McpStdioClient {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    next_id: i64,
    server_name: String,
}

impl McpStdioClient {
    /// Spawn `command args...` and take over its stdio. `label` is just for
    /// logging (e.g. "serena").
    pub fn spawn(label: &str, command: &str, args: &[&str]) -> Result<Self> {
        let mut child = tokio::process::Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // let the server's own logs surface directly
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("failed to spawn MCP server '{label}' ({command})"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("no stdin handle for '{label}'"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("no stdout handle for '{label}'"))?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout).lines(),
            next_id: 1,
            server_name: label.to_string(),
        })
    }

    /// MCP handshake: `initialize` request, then the `notifications/initialized`
    /// notification the spec requires before any other call. Returns the
    /// server's `initialize` result (protocol version, capabilities, serverInfo).
    pub async fn initialize(&mut self, client_name: &str, client_version: &str) -> Result<Value> {
        let result = self
            .request(
                "initialize",
                Some(json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": { "name": client_name, "version": client_version },
                })),
            )
            .await?;

        self.notify("notifications/initialized", None).await?;
        Ok(result)
    }

    /// List tools the server advertises.
    pub async fn list_tools(&mut self) -> Result<Vec<McpTool>> {
        let result = self.request("tools/list", None).await?;
        let tools = result
            .get("tools")
            .cloned()
            .ok_or_else(|| anyhow!("'{}': tools/list result had no 'tools' field", self.server_name))?;
        Ok(serde_json::from_value(tools)?)
    }

    /// Call a tool by name with arguments, return its raw `result` value.
    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
        self.request("tools/call", Some(json!({ "name": name, "arguments": arguments })))
            .await
    }

    /// Send a JSON-RPC request, block until the response with the matching
    /// id arrives, return `result` or bail on `error`.
    async fn request(&mut self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;

        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(id)),
            method: method.to_string(),
            params,
        };
        self.write_line(&serde_json::to_string(&req)?).await?;

        loop {
            let line = self
                .stdout
                .next_line()
                .await?
                .ok_or_else(|| anyhow!("'{}': MCP server closed stdout mid-request", self.server_name))?;
            if line.trim().is_empty() {
                continue;
            }

            let resp: JsonRpcResponse = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(_) => continue, // not a JSON-RPC message we understand (e.g. a stray log line); skip
            };

            let matches_id = resp.id.as_ref().and_then(Value::as_i64) == Some(id);
            if !matches_id {
                continue; // response to a notification/earlier stray message; keep reading
            }

            if let Some(JsonRpcError { code, message, .. }) = resp.error {
                bail!("'{}': MCP error {code}: {message}", self.server_name);
            }
            return resp
                .result
                .ok_or_else(|| anyhow!("'{}': response had neither result nor error", self.server_name));
        }
    }

    /// Send a JSON-RPC notification (no id, no response expected).
    async fn notify(&mut self, method: &str, params: Option<Value>) -> Result<()> {
        let payload = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.write_line(&payload.to_string()).await
    }

    async fn write_line(&mut self, line: &str) -> Result<()> {
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Terminate the subprocess.
    pub async fn shutdown(mut self) -> Result<()> {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        Ok(())
    }
}
