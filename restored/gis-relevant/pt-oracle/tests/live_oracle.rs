//! Live oracle test — hits a real LLM endpoint (from_env). Ignored by default.
//! Run: cargo test -p pt-oracle --test live_oracle -- --ignored --nocapture
use pt_oracle::client::OracleClient;
use pt_oracle::{OracleQuery, QueryTarget};

#[tokio::test]
#[ignore]
async fn live_extract_event() {
    let Some(client) = OracleClient::from_env() else {
        eprintln!("SKIP: no LLM endpoint in env (HERMES_LLM_* or MISTRAL_API_KEY)");
        return;
    };
    let raw = "Robson Valley Music Festival returns August 15-17, 2026 in Dunster, BC. \
               Artist applications close on March 1, 2026. Genres: folk, bluegrass, roots. \
               Submit at https://robsonvalleymusicfestival.com/apply or email booking@rvmf.ca";
    let query = OracleQuery {
        capture_hash: pt_core::hash_str(raw),
        raw_text: raw.to_string(),
        target: QueryTarget::ExtractEvent,
        source_url: "https://robsonvalleymusicfestival.com".to_string(),
    };
    match client.query(&query).await {
        Ok(outputs) => {
            eprintln!("oracle returned {} candidate(s):", outputs.len());
            for o in &outputs {
                eprintln!("  {o:?}");
            }
            // Every FieldCandidate's span must actually resolve in raw (contract).
            for o in &outputs {
                if let pt_oracle::OracleOutput::FieldCandidate { value, span, .. } = o {
                    let extracted = &raw[span.start..span.end];
                    assert!(
                        raw.contains(value.trim()),
                        "value must exist in raw (anti-fabrication)"
                    );
                    eprintln!("  span[{}..{}] = {:?}", span.start, span.end, extracted);
                }
            }
        }
        Err(e) => eprintln!("SKIP: oracle query failed (endpoint state): {e}"),
    }
}
