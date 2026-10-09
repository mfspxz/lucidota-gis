//! CA-MCP stdio transport — tokio async, event-driven, non-blocking.
//!
//! Reads newline-delimited JSON-RPC from tokio::io::stdin,
//! writes responses to tokio::io::stdout. Stderr reserved for tracing.
//! Spawns one task per request for parallelism.

use crate::protocol::JsonRpcRequest;
use crate::{CaInitRequest, CaMcpServer, CaToolCall};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Run CA-MCP on async stdio. Spawns each request as a tokio task.
pub async fn ca_stdio_loop(server: Arc<CaMcpServer>) -> anyhow::Result<()> {
    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();
    let stdout = tokio::io::stdout();

    // Wrap stdout in a buffered writer shared across tasks
    let writer = std::sync::Arc::new(tokio::sync::Mutex::new(stdout));

    tracing::info!("CA-MCP async stdio transport online");

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let err = serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": {"code": -32700, "message": format!("Parse error: {e}")}
                });
                let mut out = writer.lock().await;
                let _ = out.write_all(format!("{}\n", err).as_bytes()).await;
                let _ = out.flush().await;
                continue;
            }
        };

        // Clone request data for the spawned task
        let id = request.id.clone();
        let method = request.method.clone();
        let params = request.params.clone();

        // Clone Arc for the spawned task
        let server_clone = server.clone();
        let writer = writer.clone();
        tokio::spawn(async move {
            let response = handle_ca_request(&server_clone, &method, &params, &id);
            let mut out = writer.lock().await;
            let _ = out.write_all(format!("{}\n", response).as_bytes()).await;
            let _ = out.flush().await;
        });
    }

    tracing::info!("CA-MCP stdio transport closed");
    Ok(())
}

/// Handle a single CA-MCP request. Sync — no I/O, just dispatch.
fn handle_ca_request(
    server: &CaMcpServer,
    method: &str,
    params: &Option<serde_json::Value>,
    id: &Option<serde_json::Value>,
) -> serde_json::Value {
    match method {
        "ca/init" => {
            let init_req: CaInitRequest = serde_json::from_value(
                params.clone().unwrap_or_default()
            ).unwrap_or(CaInitRequest {
                client_name: "unknown".into(),
                client_version: "0".into(),
                max_cached_tools: None,
            });
            let init_resp = server.handle_init(&init_req);
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": init_resp
            })
        }
        "ca/call" => {
            let params = params.clone().unwrap_or_default();
            let session_id = params.get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if session_id.is_empty() {
                return serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32001, "message": "Missing session_id — call ca/init first"}
                });
            }

            let call: CaToolCall = match serde_json::from_value(
                params.get("call").cloned().unwrap_or_default()
            ) {
                Ok(c) => c,
                Err(e) => {
                    return serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": -32602, "message": format!("Invalid params: {e}")}
                    });
                }
            };

            match server.handle_call(session_id, &call) {
                Ok(result) => serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": result
                }),
                Err(e) => serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32000, "message": e}
                }),
            }
        }
        "ca/ping" => {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {"status": "ok", "sessions": server.session_count()}
            })
        }
        _ => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32601, "message": format!("Method not found: {method}")}
        }),
    }
}
