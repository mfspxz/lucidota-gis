//! Scraper MCP tools — expose the pt-scrapers fleet over CA-MCP so agents can
//! drive scraping "agentically via MCP" (operator directive). Three tools:
//!   sl = scrape_list      list the compiled fleet + enablement
//!   sd = scrape_describe  a scraper's declared limits + resolved controls
//!   sc = scrape_run       run one scraper by id with a query
//!
//! The CA-MCP handler signature is SYNC (`Fn(Value, &mut Session) -> Result<Value,
//! String>`), but scraper dispatch is async. We bridge by running the future on a
//! dedicated current-thread tokio runtime per call. This is correct for a
//! request/response tool call and avoids leaking a runtime handle requirement into
//! the MCP core.

use crate::{CaHandler, CompactRegistry, CompactTool, Session};
use serde_json::{json, Value};
use std::sync::Arc;

/// Register the scraper tool descriptors.
pub fn register_scraper_tools(registry: &mut CompactRegistry) {
    let tools = vec![
        CompactTool {
            id: "sl".into(),
            name: "scrape_list".into(),
            description: "List the compiled scraper fleet (id, domain, archetype, source)".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "domain": {"type": "string", "description": "optional domain filter e.g. BcOsint, Social, Finance"}
                }
            }),
        },
        CompactTool {
            id: "sd".into(),
            name: "scrape_describe".into(),
            description: "Describe a scraper's declared limits + default lane config (what a run will do)".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "scraper id e.g. TIN_025"}
                },
                "required": ["id"]
            }),
        },
        CompactTool {
            id: "sc".into(),
            name: "scrape_run".into(),
            description: "Run a scraper by id with a query. Returns real ScrapeStatus + candidates; honest Blocked/Error on failure (never fabricated).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "scraper id e.g. TIN_002"},
                    "query": {"type": "string", "description": "the search query / target"},
                    "max_candidates": {"type": "integer", "default": 10}
                },
                "required": ["id", "query"]
            }),
        },
    ];
    for t in tools {
        registry.register(t);
    }
}

/// Run an async scrape future to completion.
///
/// The CA-MCP `handle_call` path is SYNC but is itself invoked from inside the
/// server's tokio runtime (`serve_stdio`). Calling `Runtime::block_on` directly
/// here would panic ("cannot start a runtime from within a runtime"). We therefore
/// run the future on a SEPARATE OS thread that owns its own current-thread runtime
/// — that thread has no ambient runtime, so block_on is valid. This is honest and
/// correct for a request/response tool call (the caller is already blocking on the
/// sync handler). `F` + `Output` must be `Send` to cross the thread boundary.
fn block_on<F>(fut: F) -> F::Output
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    std::thread::scope(|s| {
        s.spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build current-thread runtime for scrape")
                .block_on(fut)
        })
        .join()
        .expect("scrape thread panicked")
    })
}

/// Register the scraper tool handlers.
pub fn register_scraper_handlers(
    handlers: &mut std::collections::HashMap<String, CaHandler>,
) {
    // sl: scrape_list
    handlers.insert(
        "sl".into(),
        Arc::new(|args: Value, _session: &mut Session| -> Result<Value, String> {
            let domain_filter = args
                .get("domain")
                .and_then(|v| v.as_str())
                .map(|s| s.to_lowercase());
            let reg = pt_scrapers::build_registry();
            let mut list: Vec<Value> = Vec::new();
            for meta in reg.list() {
                let dom = format!("{:?}", meta.domain);
                if let Some(ref f) = domain_filter {
                    if !dom.to_lowercase().contains(f.as_str()) {
                        continue;
                    }
                }
                list.push(json!({
                    "id": meta.id,
                    "domain": dom,
                    "archetype": format!("{:?}", meta.archetype),
                    "source": meta.source_label,
                    "auth_required": meta.auth_required,
                }));
            }
            Ok(json!({ "status": "ok", "count": list.len(), "scrapers": list }))
        }),
    );

    // sd: scrape_describe
    handlers.insert(
        "sd".into(),
        Arc::new(|args: Value, _session: &mut Session| -> Result<Value, String> {
            let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if id.is_empty() {
                return Err("id is required".into());
            }
            let reg = pt_scrapers::build_registry();
            match reg.get(id) {
                Some(s) => {
                    let limits = s.limits();
                    Ok(json!({
                        "status": "ok",
                        "id": id,
                        "meta": {
                            "domain": format!("{:?}", s.meta().domain),
                            "archetype": format!("{:?}", s.meta().archetype),
                            "source": s.meta().source_label,
                            "auth_required": s.meta().auth_required,
                            "auth_hint": s.meta().auth_hint,
                        },
                        "limits": {
                            "max_concurrency": limits.max_concurrency,
                            "min_delay_ms": limits.min_delay_ms,
                            "max_results": limits.max_results,
                            "max_pages": limits.max_pages,
                            "notes": limits.notes,
                        },
                        "default_config": format!("{:?}", s.default_config()),
                    }))
                }
                None => Err(format!("no such scraper: {id}")),
            }
        }),
    );

    // sc: scrape_run
    handlers.insert(
        "sc".into(),
        Arc::new(|args: Value, _session: &mut Session| -> Result<Value, String> {
            let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let max_c = args.get("max_candidates").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
            if id.is_empty() || query.is_empty() {
                return Err("id and query are required".into());
            }

            let result = block_on({
                let id = id.clone();
                let query = query.clone();
                async move {
                    let reg = pt_scrapers::build_registry();
                    let ctx = pt_scrapers::ScrapeCtx::new_default()
                        .map_err(|e| format!("ctx init failed: {e}"))?;
                    let params = pt_scrapers::ScrapeParams::from_query(&query);
                    let rec = pt_scrapers::dispatch(
                        &reg,
                        &pt_scrapers::Enablement::All,
                        &ctx,
                        &id,
                        params,
                    )
                    .await;
                    Ok::<_, String>(rec)
                }
            })?;

            // Serialize the record honestly: status verbatim, candidates capped.
            let status = match &result.status {
                pt_scrapers::ScrapeStatus::Ok => "ok".to_string(),
                pt_scrapers::ScrapeStatus::NotFound => "not_found".to_string(),
                pt_scrapers::ScrapeStatus::Blocked(r) => format!("blocked: {r}"),
                pt_scrapers::ScrapeStatus::RateLimited => "rate_limited".to_string(),
                pt_scrapers::ScrapeStatus::Error(e) => format!("error: {e}"),
            };
            let candidates: Vec<Value> = result
                .candidates
                .iter()
                .take(max_c)
                .map(|c| c.data.clone())
                .collect();
            Ok(json!({
                "status": status,
                "id": id,
                "query": query,
                "candidate_count": result.candidates.len(),
                "candidates": candidates,
                "notes": result.notes,
            }))
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn scraper_tools_register() {
        let mut reg = CompactRegistry::new();
        register_scraper_tools(&mut reg);
        // sl, sd, sc must be present.
        assert!(reg.get_by_id("sl").is_some());
        assert!(reg.get_by_id("sd").is_some());
        assert!(reg.get_by_id("sc").is_some());
    }

    #[test]
    fn scrape_list_returns_the_fleet() {
        let mut handlers: HashMap<String, CaHandler> = HashMap::new();
        register_scraper_handlers(&mut handlers);
        let h = handlers.get("sl").unwrap();
        let mut session = Session::new("test");
        let out = h(json!({}), &mut session).unwrap();
        assert_eq!(out["status"], "ok");
        // The full fleet is compiled in — must be well over 50 scrapers.
        assert!(out["count"].as_u64().unwrap() > 50, "fleet should be >50 scrapers");
    }

    #[test]
    fn scrape_describe_unknown_is_error() {
        let mut handlers: HashMap<String, CaHandler> = HashMap::new();
        register_scraper_handlers(&mut handlers);
        let h = handlers.get("sd").unwrap();
        let mut session = Session::new("test");
        assert!(h(json!({"id": "NOPE_999"}), &mut session).is_err());
    }

    /// Critical: prove `sc` does NOT panic when invoked from INSIDE a tokio
    /// runtime (the real server path is async serve_stdio -> sync handle_call).
    /// The block_on-on-a-separate-thread fix must hold. Unknown id => honest
    /// dispatch (no network), so this is safe offline.
    #[test]
    fn scrape_run_survives_nested_runtime() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut handlers: HashMap<String, CaHandler> = HashMap::new();
            register_scraper_handlers(&mut handlers);
            let h = handlers.get("sc").unwrap().clone();
            // Run the sync handler from within the async runtime, on a blocking task.
            let out = tokio::task::spawn_blocking(move || {
                let mut session = Session::new("test");
                h(json!({"id": "NOPE_999", "query": "x"}), &mut session)
            })
            .await
            .unwrap()
            .unwrap();
            // Unknown scraper => dispatch returns an error-status record, serialized
            // as a "error:" status string. The point: NO runtime panic.
            let status = out["status"].as_str().unwrap_or("");
            assert!(status.starts_with("error"), "unknown id should be error status, got {status}");
        });
    }
}
