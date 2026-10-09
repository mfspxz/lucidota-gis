//! serena_probe — end-to-end proof that this ecosystem can drive Serena
//! locally as a subprocess, no browser, no HTTP hop, no llama-server.
//!
//! Spawns Serena via `uvx` (stdio transport, its default), performs the MCP
//! handshake, lists its tools, and calls one read-only tool to prove actual
//! round-trip tool invocation works, not just the handshake.
//!
//! Usage: serena_probe <project-path>

use pt_mcp::client::McpStdioClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let project = std::env::args()
        .nth(1)
        .unwrap_or_else(|| ".".to_string());

    eprintln!("[serena_probe] spawning serena (uvx, stdio transport) for project: {project}");

    let mut client = McpStdioClient::spawn(
        "serena",
        "uvx",
        &[
            "--from",
            "git+https://github.com/oraios/serena",
            "serena",
            "start-mcp-server",
            "--transport",
            "stdio",
            "--context",
            "agent",
            "--project",
            &project,
            "--enable-web-dashboard",
            "False",
            "--open-web-dashboard",
            "False",
            "--log-level",
            "ERROR",
        ],
    )?;

    eprintln!("[serena_probe] handshaking (initialize)...");
    let init = client.initialize("pt-mcp-probe", "0.1.0").await?;
    eprintln!(
        "[serena_probe] initialized. server: {}",
        init.get("serverInfo")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "<no serverInfo>".to_string())
    );

    eprintln!("[serena_probe] tools/list...");
    let tools = client.list_tools().await?;
    eprintln!("[serena_probe] {} tools advertised:", tools.len());
    for t in &tools {
        eprintln!("  - {} : {}", t.name, t.description.as_deref().unwrap_or(""));
    }

    // Call a read-only, side-effect-free tool to prove round-trip
    // invocation, not just the handshake. list_dir is universally present
    // and safe (no writes, no shell exec).
    if tools.iter().any(|t| t.name == "list_dir") {
        eprintln!("[serena_probe] calling list_dir on '.' to prove round-trip tool invocation...");
        let result = client
            .call_tool("list_dir", serde_json::json!({ "relative_path": ".", "recursive": false }))
            .await?;
        eprintln!("[serena_probe] list_dir result:\n{}", serde_json::to_string_pretty(&result)?);
    } else {
        eprintln!("[serena_probe] no list_dir tool found — skipping round-trip call, handshake+list still proves the transport works");
    }

    client.shutdown().await?;
    eprintln!("[serena_probe] done — Serena driven end-to-end locally, no browser, no HTTP.");
    Ok(())
}
