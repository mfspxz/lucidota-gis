//! Dogfood: this session's own completion report, generated through the
//! real types (not hand-typed JSON) and written to .lucidota/reports/ — the
//! first real consumer of pt-receipts, proving the schema against actual
//! work instead of only unit tests.

use chrono::Utc;
use pt_receipts::{
    ArtifactRef, Assumption, CompletionReport, CompletionStatus, ValidationReceipt,
    ValidationResult,
};
use std::io::Write;

fn main() {
    let report = CompletionReport {
        // PartiallyDone, not Done: the workspace test suite for the
        // model-registry change was interrupted before finishing (see the
        // Skipped validation below) — live behavior is verified, the full
        // automated suite is not yet, so this can't honestly claim Done.
        status: CompletionStatus::PartiallyDone,
        summary: "Dashboard real login flow (DID:KEY) closing the read-path RBAC gap on both \
                  the Next.js dashboard and the embedded ODIN map viewer; cleared swarm \
                  TASK-009/010/011 (dead mock constants, verified real ODIN geodata, real \
                  llama-server replacing the silent StubModel path); registered three real \
                  local models (fsm-230m, bonsai-4b, router) in shambhala's ModelRegistry with \
                  StubModel demoted to a loud last-resort only."
            .into(),
        changed_artifacts: vec![
            ArtifactRef::new("S_H_A_M_B_A_L_A/ui-dashboard/src/lib/auth.ts"),
            ArtifactRef::new("S_H_A_M_B_A_L_A/ui-dashboard/src/components/login-screen.tsx"),
            ArtifactRef::at("S_H_A_M_B_A_L_A/ui-dashboard/src/components/app-shell.tsx", "MAP tab iframe + login gate"),
            ArtifactRef::at("S_H_A_M_B_A_L_A/crates/pt-odin-viewer/src/main.rs", "authHeaders() JS addition"),
            ArtifactRef::at("S_H_A_M_B_A_L_A/crates/shambhala/src/main.rs", "step 4, model registry"),
            ArtifactRef::at("S_H_A_M_B_A_L_A/crates/shambhala-core/src/config.rs", "ModelConfig bonsai/router endpoints"),
            ArtifactRef::new("S_H_A_M_B_A_L_A/shambhala.toml"),
            ArtifactRef::at(".gitignore", "narrowed blanket auth.* rule"),
        ],
        validations: vec![
            ValidationReceipt {
                command: "playwright headless E2E — login + ODIN iframe /incidents call".into(),
                exit_code: Some(0),
                started_at: Utc::now(),
                finished_at: Utc::now(),
                result: ValidationResult::Pass,
                output_digest: "ODIN-origin /incidents: 200 (was 401 before the fix)".into(),
            },
            ValidationReceipt {
                command: "curl -s http://localhost:8088/odin.geojson".into(),
                exit_code: Some(0),
                started_at: Utc::now(),
                finished_at: Utc::now(),
                result: ValidationResult::Pass,
                output_digest: "18 real SHAMBHALA nodes (The Pagoda, Living Room, ...), zero tour/demo data".into(),
            },
            ValidationReceipt {
                command: "curl -s -X POST http://localhost:8090/chat".into(),
                exit_code: Some(0),
                started_at: Utc::now(),
                finished_at: Utc::now(),
                result: ValidationResult::Pass,
                output_digest: "model: http-http://127.0.0.1:8083/... — real FSM-230M reply, not StubModel canned text".into(),
            },
            ValidationReceipt {
                command: "cargo test --workspace -j2 (shambhala, post multi-model-registry change)".into(),
                exit_code: None,
                started_at: Utc::now(),
                finished_at: Utc::now(),
                result: ValidationResult::Skipped,
                output_digest: "run interrupted by a background-task tracking reset mid-session; \
                                 processes under test survived and were re-verified live via curl, \
                                 but the full workspace suite has not been re-run to completion \
                                 since this change — see follow_up_tasks".into(),
            },
        ],
        unresolved_findings: vec![],
        blockers: vec![],
        assumptions: vec![
            Assumption {
                summary: "no separate 1.58-bit ternary quant of Bonsai-4B tested against the 1-bit Q1_0 build".into(),
                reason: "only Q1_0 exists on disk for this model; no second variant found to compare, \
                         reported honestly rather than fabricating a second test".into(),
            },
        ],
        follow_up_tasks: vec![
            pt_receipts::TaskProposal {
                summary: "Re-run cargo test --workspace -j2 to completion for the shambhala/shambhala-core model-registry change".into(),
                reason: "the run in progress during this session got interrupted by an environment \
                         event before finishing; the code compiled and the live behavior was verified \
                         by hand, but the full automated suite hasn't confirmed it yet".into(),
            },
        ],
    };

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../.lucidota/reports/session_2026_07_29_shambhala.json");
    let json = serde_json::to_string_pretty(&report).expect("serialize report");
    let mut f = std::fs::File::create(&path).expect("create report file");
    f.write_all(json.as_bytes()).expect("write report file");
    println!("wrote {}", path.display());
}
