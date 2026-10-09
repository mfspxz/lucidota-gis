//! Deterministic oracle integration test (depenaudit K-2, 2026-10-09).
//!
//! The old live test hit a real LLM endpoint (from_env) — that path is dead.
//! This test exercises the deterministic extractor end to end: raw page text
//! in, byte-spanned candidates out, every span verified against the raw bytes.
use pt_oracle::client::OracleClient;
use pt_oracle::{OracleOutput, OracleQuery, QueryTarget};

fn query(target: QueryTarget, raw: &str) -> OracleQuery {
    OracleQuery {
        capture_hash: [7u8; 32],
        raw_text: raw.to_string(),
        target,
        source_url: "https://robsonvalleymusicfestival.com".to_string(),
    }
}

#[test]
fn deterministic_extract_event() {
    let client = OracleClient::new();
    let raw = "Robson Valley Music Festival returns August 15-17, 2026 in Dunster, BC. \
               Artist applications close on March 1, 2026. Genres: folk, bluegrass, roots. \
               Submit at https://robsonvalleymusicfestival.com/apply or email booking@rvmf.ca \
               or call +1 250-555-0184.";
    let outputs = client.extract(&query(QueryTarget::ExtractEvent, raw));
    eprintln!("deterministic oracle returned {} candidate(s)", outputs.len());

    // The anti-fabrication contract, now structural: every candidate's span
    // resolves in raw, and every value is a verbatim substring.
    let mut saw_email = false;
    let mut saw_phone = false;
    let mut saw_url = false;
    let mut saw_date = false;
    let mut saw_genres = false;
    for o in &outputs {
        match o {
            OracleOutput::FieldCandidate { field, value, span } => {
                let extracted = &raw[span.start..span.end];
                assert_eq!(extracted, value, "span must equal value exactly");
                assert!(raw.contains(value), "value must exist in raw");
                match field.as_str() {
                    "contact_email" if value == "booking@rvmf.ca" => saw_email = true,
                    "phone" if value.contains("250-555-0184") => saw_phone = true,
                    "dates" if value == "March 1, 2026" => saw_date = true,
                    _ => {}
                }
            }
            OracleOutput::UrlNomination { url, why_span } => {
                assert!(raw[why_span.start..why_span.end].contains("robsonvalleymusicfestival.com"));
                if url == "https://robsonvalleymusicfestival.com/apply" {
                    saw_url = true;
                }
            }
            OracleOutput::GenreSuggestion { tags, .. } => {
                assert!(tags.contains(&"folk".to_string()));
                assert!(tags.contains(&"bluegrass".to_string()));
                saw_genres = true;
            }
            OracleOutput::Nothing => {}
        }
    }
    assert!(saw_email, "email extracted");
    assert!(saw_phone, "phone extracted");
    assert!(saw_url, "url nominated");
    assert!(saw_date, "deadline date extracted");
    assert!(saw_genres, "genres suggested");
}

#[test]
fn deterministic_custom_target_yields_nothing_not_guesses() {
    let client = OracleClient::new();
    let outputs = client.extract(&query(
        QueryTarget::Custom("do something clever".into()),
        "Some page with an email a@b.com in it.",
    ));
    // Custom carried the operator's LLM prompt — no LLM, no guesses.
    assert_eq!(outputs.len(), 1);
    assert!(matches!(outputs[0], OracleOutput::Nothing));
}
