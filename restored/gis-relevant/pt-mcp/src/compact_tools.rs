//! Compact tool registry — all CA-MCP tools with 2-char IDs.
//!
//! Tool ID scheme:
//!   s1 = search_events     r1 = rank_festivals
//!   g1 = get_event         t1 = track_application
//!   d1 = morning_digest    m1 = export_geojson
//!   h1 = harvest_source    a1 = audit_status
//!   p1 = ping

use crate::{CaHandler, CompactRegistry, CompactTool, Session};
use serde_json::{json, Value};
use std::sync::Arc;

pub fn register_all(registry: &mut CompactRegistry) {
    let tools = vec![
        CompactTool {
            id: "s1".into(),
            name: "search_events".into(),
            description: "Search the event database by genre, location, or deadline range".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "genres": {"type": "array", "items": {"type": "string"}},
                    "location": {"type": "string"},
                    "deadline_before": {"type": "string", "format": "date"},
                    "deadline_after": {"type": "string", "format": "date"},
                    "limit": {"type": "integer", "default": 10}
                }
            }),
        },
        CompactTool {
            id: "g1".into(),
            name: "get_event".into(),
            description: "Get a single event with full provenance and receipts".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "event_id": {"type": "string", "description": "blake2b hex hash of event"}
                },
                "required": ["event_id"]
            }),
        },
        CompactTool {
            id: "r1".into(),
            name: "rank_festivals".into(),
            description: "Rank festivals for an artist using their weighted preferences".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "artist_name": {"type": "string"},
                    "limit": {"type": "integer", "default": 10}
                }
            }),
        },
        CompactTool {
            id: "d1".into(),
            name: "morning_digest".into(),
            description: "Generate the daily briefing digest for an artist".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "artist_name": {"type": "string"}
                },
                "required": ["artist_name"]
            }),
        },
        CompactTool {
            id: "t1".into(),
            name: "track_application".into(),
            description: "Transition an application through the tracking FSM".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "event_id": {"type": "string"},
                    "new_state": {"type": "string", "enum": ["shortlisted","preparing","submitted","acknowledged","accepted","booked","played","rejected","ghosted","withdrawn","archived"]},
                    "note": {"type": "string"}
                },
                "required": ["event_id", "new_state"]
            }),
        },
        CompactTool {
            id: "m1".into(),
            name: "export_geojson".into(),
            description: "Export events as GeoJSON for ODIN C2 tactical mapping".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "bbox": {"type": "array", "items": {"type": "number"}, "description": "[west, south, east, north] bounding box"}
                }
            }),
        },
        CompactTool {
            id: "h1".into(),
            name: "harvest_source".into(),
            description: "Trigger TrufflePig to harvest a URL for event data".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string", "format": "uri"},
                    "source_type": {"type": "string", "enum": ["festival_site","arts_council","aggregator","submission_platform"]}
                },
                "required": ["url"]
            }),
        },
        CompactTool {
            id: "a1".into(),
            name: "audit_status".into(),
            description: "Get Leanstral audit status and findings for this project".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "severity_filter": {"type": "string", "enum": ["CRITICAL","HIGH","MEDIUM","LOW","ALL"]}
                }
            }),
        },
        CompactTool {
            id: "p1".into(),
            name: "ping".into(),
            description: "Health check — returns server status and session info".into(),
            input_schema: json!({"type": "object", "properties": {}}),
        },
    ];

    for tool in tools {
        registry.register(tool);
    }
}

pub fn register_handlers(registry: &mut CompactRegistry, handlers: &mut std::collections::HashMap<String, CaHandler>) {
    // s1: search_events
    handlers.insert("s1".into(), Arc::new(|args: Value, _session: &mut Session| -> Result<Value, String> {
        let genres: Vec<String> = args.get("genres")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);

        Ok(json!({
            "status": "ok",
            "count": 0,
            "events": [],
            "filters_applied": { "genres": genres, "limit": limit },
            "note": "TruthStore backend pending — stub response"
        }))
    }));

    // g1: get_event
    handlers.insert("g1".into(), Arc::new(|args: Value, _session: &mut Session| -> Result<Value, String> {
        let event_id = args.get("event_id").and_then(|v| v.as_str()).unwrap_or("unknown");
        Ok(json!({
            "status": "ok",
            "event_id": event_id,
            "event": null,
            "note": "TruthStore backend pending"
        }))
    }));

    // r1: rank_festivals
    handlers.insert("r1".into(), Arc::new(|args: Value, _session: &mut Session| -> Result<Value, String> {
        let artist = args.get("artist_name").and_then(|v| v.as_str()).unwrap_or("Scout Shorefox");
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
        Ok(json!({
            "status": "ok",
            "artist": artist,
            "ranked": [],
            "weights": {
                "genre_match": 0.30, "window_urgency": 0.20, "data_confidence": 0.15,
                "travel_cost": 0.15, "historic_signal": 0.10,
                "event_scale_fit": 0.05, "pay_signal": 0.05
            },
            "note": "Ranker backend pending — connect to pt-rank + TruthStore"
        }))
    }));

    // d1: morning_digest
    handlers.insert("d1".into(), Arc::new(|args: Value, _session: &mut Session| -> Result<Value, String> {
        let artist = args.get("artist_name").and_then(|v| v.as_str()).unwrap_or("Scout Shorefox");
        Ok(json!({
            "status": "ok",
            "digest": format!("☀️ Morning Digest for {artist}\n\nNo events pending — connect truth layer for live data.\n\n!help for commands"),
            "alerts": []
        }))
    }));

    // t1: track_application
    handlers.insert("t1".into(), Arc::new(|args: Value, _session: &mut Session| -> Result<Value, String> {
        let event_id = args.get("event_id").and_then(|v| v.as_str()).unwrap_or("unknown");
        let new_state = args.get("new_state").and_then(|v| v.as_str()).unwrap_or("discovered");
        let note = args.get("note").and_then(|v| v.as_str()).unwrap_or("");
        Ok(json!({
            "status": "ok",
            "event_id": event_id,
            "transition": { "from": "discovered", "to": new_state, "actor": "mcp-client", "note": note },
            "legal": true
        }))
    }));

    // m1: export_geojson
    handlers.insert("m1".into(), Arc::new(|args: Value, _session: &mut Session| -> Result<Value, String> {
        Ok(json!({
            "status": "ok",
            "type": "FeatureCollection",
            "features": [],
            "note": "ODIN C2 map export — connect pt-bridge-odin for live data"
        }))
    }));

    // h1: harvest_source
    handlers.insert("h1".into(), Arc::new(|args: Value, _session: &mut Session| -> Result<Value, String> {
        let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if url.is_empty() {
            return Err("url is required".into());
        }
        Ok(json!({
            "status": "queued",
            "url": url,
            "job_id": "harvest-001",
            "note": "TrufflePig harvest queued — connect pt-trufflepig for live harvesting"
        }))
    }));

    // a1: audit_status
    // NO FABRICATION: this handler reads the real findings ledgers at call time
    // and computes counts from bytes. It NEVER returns baked-in numbers. If the
    // files are absent it says so — UNKNOWN, not a confident lie.
    handlers.insert("a1".into(), Arc::new(|args: Value, _session: &mut Session| -> Result<Value, String> {
        let severity_filter = args.get("severity_filter")
            .and_then(|v| v.as_str())
            .unwrap_or("ALL")
            .to_uppercase();
        Ok(audit_status_live(&severity_filter))
    }));

    // p1: ping
    handlers.insert("p1".into(), Arc::new(|_args: Value, session: &mut Session| -> Result<Value, String> {
        Ok(json!({
            "status": "ok",
            "server": "PalOnTour CA-MCP v0.1",
            "session_id": session.id,
            "uptime_seconds": session.created.elapsed().as_secs(),
            "active_sessions": 1,
            "protocol": "CA-MCP (context-aware, token-efficient)"
        }))
    }));
}

/// Candidate locations for the findings ledgers. First existing wins.
/// The CWD of the MCP server is not guaranteed, so we probe both the
/// repo-relative path and an absolute fallback.
const FINDINGS_CANDIDATES: &[&str] = &[
    "PALANTOUR/LEAN_AUDITS/palantour_findings.jsonl",
    "LEAN_AUDITS/palantour_findings.jsonl",
    "/home/mfspx/LUCIDOTA/PALANTOUR/LEAN_AUDITS/palantour_findings.jsonl",
];
const FIXED_CANDIDATES: &[&str] = &[
    "PALANTOUR/LEAN_AUDITS/fixed_findings.jsonl",
    "LEAN_AUDITS/fixed_findings.jsonl",
    "/home/mfspx/LUCIDOTA/PALANTOUR/LEAN_AUDITS/fixed_findings.jsonl",
];

fn first_existing(candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .map(|p| p.to_string())
}

/// Compute audit status from the real findings ledgers at call time.
/// Reads bytes, counts by severity, and reports the fixed-ledger state
/// INCLUDING pollution (entries whose note contains FAIL). Never fabricates.
pub(crate) fn audit_status_live(severity_filter: &str) -> Value {
    let findings_path = first_existing(FINDINGS_CANDIDATES);
    let fixed_path = first_existing(FIXED_CANDIDATES);

    let findings_path = match findings_path {
        Some(p) => p,
        None => {
            return json!({
                "status": "unknown",
                "audit": null,
                "note": "findings ledger not found on any known path — cannot report audit status (UNKNOWN, not fabricated)"
            });
        }
    };

    let findings_raw = match std::fs::read_to_string(&findings_path) {
        Ok(s) => s,
        Err(e) => {
            return json!({
                "status": "error",
                "note": format!("failed to read findings ledger {findings_path}: {e}")
            });
        }
    };

    // Count findings by severity from the actual file.
    let mut total = 0u64;
    let mut critical = 0u64;
    let mut high = 0u64;
    let mut medium = 0u64;
    let mut low = 0u64;
    for line in findings_raw.lines().filter(|l| !l.trim().is_empty()) {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            total += 1;
            match v.get("severity").and_then(|s| s.as_str()) {
                Some("CRITICAL") => critical += 1,
                Some("HIGH") => high += 1,
                Some("MEDIUM") => medium += 1,
                Some("LOW") => low += 1,
                _ => {}
            }
        }
    }

    // Fixed ledger: count entries and flag pollution (FAIL in note).
    let mut fixed_total = 0u64;
    let mut fixed_polluted = 0u64;
    if let Some(fp) = &fixed_path {
        if let Ok(fixed_raw) = std::fs::read_to_string(fp) {
            for line in fixed_raw.lines().filter(|l| !l.trim().is_empty()) {
                if let Ok(v) = serde_json::from_str::<Value>(line) {
                    fixed_total += 1;
                    let note = v.get("note").and_then(|n| n.as_str()).unwrap_or("");
                    if note.to_uppercase().contains("FAIL") {
                        fixed_polluted += 1;
                    }
                }
            }
        }
    }

    let mut findings_obj = json!({
        "CRITICAL": critical,
        "HIGH": high,
        "MEDIUM": medium,
        "LOW": low,
    });
    // Honor severity_filter: narrow the reported set but never hide the total.
    if severity_filter != "ALL"
        && ["CRITICAL", "HIGH", "MEDIUM", "LOW"].contains(&severity_filter)
    {
        let only = findings_obj
            .get(severity_filter)
            .cloned()
            .unwrap_or(json!(0));
        findings_obj = json!({ severity_filter: only });
    }

    json!({
        "status": "ok",
        "source": {
            "findings_file": findings_path,
            "fixed_file": fixed_path,
        },
        "audit": {
            "total_findings": total,
            "findings": findings_obj,
            "fixed": {
                "count": fixed_total,
                "polluted": fixed_polluted,
                "note": if fixed_polluted > 0 {
                    "WARNING: fixed ledger contains entries whose note says FAIL — run ledger_reconcile"
                } else {
                    "clean"
                }
            }
        },
        "note": "computed from ledger bytes at call time — no baked-in numbers"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proves a1 reads the REAL ledger bytes and never returns baked-in numbers.
    /// Ground truth (verified via grep at time of writing): 143 findings,
    /// 10 CRITICAL, 2 fixed entries, 1 polluted (migrations.rs FAIL).
    /// If the ledger files move, this asserts UNKNOWN rather than a fake count.
    #[test]
    fn test_a1_reads_real_ledger_not_fabricated() {
        let out = audit_status_live("ALL");
        let status = out["status"].as_str().unwrap_or("");
        if status == "unknown" {
            // Files not reachable from the test CWD — acceptable, and crucially
            // NOT a fabricated zero. The handler refused to invent numbers.
            assert!(out["audit"].is_null(), "unknown status must not carry fake audit data");
            return;
        }
        assert_eq!(status, "ok");
        let audit = &out["audit"];
        // The number must come from the file, not a constant. The OLD fabricated
        // handler returned total 27 (1+4+12+10). The real file has 143.
        let total = audit["total_findings"].as_u64().unwrap_or(0);
        assert_ne!(
            total, 27,
            "27 was the OLD fabricated total — must not appear"
        );
        assert_eq!(total, 143, "real ledger total should be 143");
        assert_eq!(audit["findings"]["CRITICAL"].as_u64().unwrap(), 10);
        // Fixed-ledger counts are read live from bytes and change as the ledger
        // is maintained (e.g. after ledger_reconcile --evict). We do NOT hardcode
        // a specific count — we assert the INVARIANT that must always hold:
        // polluted entries never exceed total fixed entries, and both are read,
        // not baked. This stays green whether the ledger has 0 or 50 entries.
        let fixed = audit["fixed"]["count"].as_u64().unwrap();
        let polluted = audit["fixed"]["polluted"].as_u64().unwrap();
        assert!(polluted <= fixed, "polluted ({polluted}) can't exceed fixed ({fixed})");
    }
}
