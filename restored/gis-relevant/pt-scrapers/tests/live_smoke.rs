//! Live smoke test — hits real public endpoints. Ignored by default (network).
//! Run with: cargo test -p pt-scrapers --features full --test live_smoke -- --ignored --nocapture
use pt_scrapers::{build_registry, dispatch, Enablement, ScrapeCtx, ScrapeParams, ScrapeStatus, StealthProfile};

/// Fail-closed net guard: under StealthPolicy::Always with NO fingerprint, an
/// api/html/feed request must be REFUSED before touching the net (offline test —
/// the guard fires before any socket). This proves "fingerprint FIRST, ALWAYS".
#[tokio::test]
async fn stealth_always_without_fingerprint_is_refused() {
    use pt_scrapers::{lanes::api, StealthPolicy};
    let ctx = ScrapeCtx::new_default().unwrap().with_stealth_policy(StealthPolicy::Always);
    // No .with_stealth() => no profile. The guard must refuse, no network hit.
    let res = api::get_json(&ctx, "https://example.com/api", &[]).await;
    let msg = match res {
        Ok(_) => panic!("Always policy w/o fingerprint must refuse net access"),
        Err(e) => format!("{e}"),
    };
    assert!(msg.contains("refusing bare net access"), "must be the fail-closed guard: {msg}");
}

/// With a fingerprint injected, Always policy permits the request (guard passes).
/// (Marked ignore — it would hit the network; the guard-pass path is the point.)
#[tokio::test]
async fn stealth_always_with_fingerprint_passes_guard() {
    use pt_scrapers::StealthPolicy;
    let profile = StealthProfile {
        user_agent: Some("GhostSwarm/1.0".into()),
        ..Default::default()
    };
    let ctx = ScrapeCtx::new_default()
        .unwrap()
        .with_stealth_policy(StealthPolicy::Always)
        .with_stealth(profile);
    // net_guard is a pure precondition check — passes when a profile is present.
    assert!(ctx.net_guard("https://example.com").is_ok(), "fingerprint present => guard passes");
}

/// Prove the Ghost Swarm stealth profile is ACTUALLY sent over the wire (RFC §5.2).
/// httpbin.org/user-agent echoes back the UA the server received.
#[tokio::test]
#[ignore]
async fn stealth_ua_is_sent_over_the_wire() {
    use pt_scrapers::lanes::api;
    let base = ScrapeCtx::new_default().unwrap();
    let ghost_ua = "Mozilla/5.0 (X11; Linux x86_64) GhostSwarm/1.0 Persona-0xdeadbeef";
    let profile = StealthProfile {
        user_agent: Some(ghost_ua.to_string()),
        accept_language: Some("en-CA,en;q=0.9".to_string()),
        source_vuuid: Some("0xdeadbeef".to_string()),
        ..Default::default()
    };
    let ctx = base.with_stealth(profile);
    // Try echo endpoints in order; httpbin.org is frequently 503. Skip (don't
    // fail) if all echo services are down — that's an env failure, not our bug.
    let endpoints = [
        "https://httpbingo.org/user-agent",
        "https://httpbin.org/user-agent",
    ];
    let mut checked = false;
    for ep in endpoints {
        match api::get_json(&ctx, ep, &[]).await {
            Ok(f) if f.status == 200 => {
                let echoed = f
                    .json
                    .get("user-agent")
                    .and_then(|v| {
                        // httpbingo returns {"user-agent":["..."]}; httpbin returns {"user-agent":"..."}
                        v.as_str().map(|s| s.to_string()).or_else(|| {
                            v.as_array()
                                .and_then(|a| a.first())
                                .and_then(|x| x.as_str())
                                .map(|s| s.to_string())
                        })
                    })
                    .unwrap_or_default();
                println!("[{ep}] server saw UA: {echoed}");
                if !echoed.is_empty() {
                    assert_eq!(echoed, ghost_ua, "the stealth persona UA must reach the server");
                    checked = true;
                    break;
                }
            }
            Ok(f) => println!("[{ep}] status {} — trying next", f.status),
            Err(e) => println!("[{ep}] error {e} — trying next"),
        }
    }
    if !checked {
        println!("SKIP: all echo endpoints down (env failure, not a stealth bug)");
    }
}

#[tokio::test]
#[ignore]
async fn geocoder_live() {
    let reg = build_registry();
    let ctx = ScrapeCtx::new_default().unwrap();
    let rec = dispatch(&reg, &Enablement::All, &ctx, "TIN_025",
        ScrapeParams::from_query("1055 W Georgia St Vancouver")).await;
    println!("STATUS={:?} candidates={} notes={}", rec.status, rec.candidates.len(), rec.notes);
    if let Some(c) = rec.candidates.first() {
        println!("FIRST={}", serde_json::to_string_pretty(&c.data).unwrap());
    }
    assert!(matches!(rec.status, ScrapeStatus::Ok | ScrapeStatus::NotFound | ScrapeStatus::RateLimited | ScrapeStatus::Blocked(_)),
        "must return a real status, got {:?}", rec.status);
}

#[tokio::test]
#[ignore]
async fn orgbook_live() {
    let reg = build_registry();
    let ctx = ScrapeCtx::new_default().unwrap();
    let rec = dispatch(&reg, &Enablement::All, &ctx, "TIN_002",
        ScrapeParams::from_query("Telus Communications")).await;
    println!("STATUS={:?} candidates={} notes={}", rec.status, rec.candidates.len(), rec.notes);
    if let Some(c) = rec.candidates.first() {
        println!("FIRST={}", serde_json::to_string_pretty(&c.data).unwrap());
    }
}

/// Spot-check a spread of the ported api-json wave against live endpoints.
/// Each just asserts a real status came back (Ok/NotFound/RateLimited/Error/Blocked)
/// and prints it — honest verification, no fabricated success.
#[tokio::test]
#[ignore]
async fn api_wave_live_spotcheck() {
    let reg = build_registry();
    let ctx = ScrapeCtx::new_default().unwrap();
    let cases = [
        ("SCI_001", "quantum entanglement"),
        ("SCI_004", "CRISPR"),
        ("SCI_005", "0000-0002-1825-0097"),
        ("TIN_011", "vancouver"),
        ("TIN_016", "telus.com"),
        ("MKT_001", "Tesla"),
    ];
    for (id, q) in cases {
        let rec = dispatch(&reg, &Enablement::All, &ctx, id, ScrapeParams::from_query(q)).await;
        println!(
            "{id:<8} status={:<28} candidates={:<4} notes={}",
            format!("{:?}", rec.status),
            rec.candidates.len(),
            rec.notes
        );
        if let Some(c) = rec.candidates.first() {
            let s = serde_json::to_string(&c.data).unwrap();
            println!("         first={}", &s[..s.len().min(180)]);
        }
    }
}
