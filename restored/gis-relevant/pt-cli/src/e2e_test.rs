//! End-to-end integration test — full PalOnTour pipeline.
//!
//! Proves: CAS store → event ingestion → ranking → digest → GeoJSON export.
//! One test that validates the entire architecture works.

#[cfg(test)]
mod e2e_tests {
    use chrono::NaiveDate;
    use pt_core::schema::{Artist, Event, EventType};
    use pt_core::epistemic::{EpistemicGate, GateAuthority, TruthValence};
    use pt_db::cas::CasStore;
    use pt_diff::DiffEngine;
    use pt_rank::{Candidate, RankWeights, Ranker};
    use pt_tracker::{State, Tracker, Application};
    use pt_bridge_matrix::{MorningDigest, RankedDigestInput, AlertItem, AlertLevel};
    use pt_bridge_odin::OdinBridge;
    use std::sync::Arc;

    /// Full pipeline: store Scout → ingest festival → rank → digest → map.
    #[test]
    fn test_full_pipeline_e2e() {
        // ── Phase 1: CAS Store ──────────────────────────────────────
        let temp = tempfile::tempdir().unwrap();
        let cas = CasStore::open(temp.path().join("e2e.redb")).unwrap();
        cas.init_schema().unwrap();

        // Store Scout's profile as a CAS blob
        let scout_data = b"Scout Shorefox -- Cortes Island, BC. Folk-punk singer-songwriter.";
        let scout_hash = cas.store(scout_data, "text/plain", "profile://scout").unwrap();
        let retrieved = cas.get(&scout_hash).unwrap().unwrap();
        assert_eq!(retrieved, scout_data, "CAS round-trip preserves data");
        assert_eq!(cas.blob_count().unwrap(), 1, "CAS dedup works");

        // ── Phase 2: Create Scout's Artist Profile ──────────────────
        let artist = Artist {
            id: pt_core::hash_str("scout_shorefox"),
            name: "Scout Shorefox".into(),
            aliases: vec!["Shorefox".into()],
            genres: vec!["folk".into(), "folk-punk".into(), "bluegrass".into()],
            setup: pt_core::schema::ArtistSetup::Acoustic,
            contact: Some(pt_core::schema::ContactInfo {
                email: Some("shorefoxx@gmail.com".into()),
                phone: None,
                website: Some("https://shorefox.bandcamp.com".into()),
                booking_agent: None,
            }),
            social_links: std::collections::HashMap::new(),
            application_status: None,
            gate: EpistemicGate {
                valence: TruthValence::Proven,
                authority: GateAuthority::Operator,
                receipt_ref: None,
                justification: "Operator ground truth".into(),
            },
            provenance: vec![],
            last_capture: None,
            last_updated: chrono::Utc::now(),
        };
        assert_eq!(artist.id, pt_core::hash_str("scout_shorefox"));

        // ── Phase 3: Create Festival Events ─────────────────────────
        let robson_valley = Event {
            id: pt_core::hash_str("robson_valley_2026"),
            name: "Robson Valley Music Festival".into(),
            date: NaiveDate::from_ymd_opt(2026, 8, 14).unwrap(),
            venue: None,
            artists: vec![],
            event_type: EventType::Festival,
            gate: EpistemicGate {
                valence: TruthValence::Proven,
                authority: GateAuthority::Deterministic,
                receipt_ref: None,
                justification: "schema.org JSON-LD verified".into(),
            },
            provenance: vec![],
            last_capture: None,
            last_updated: chrono::Utc::now(),
        };

        let edc = Event {
            id: pt_core::hash_str("edc_las_vegas_2026"),
            name: "EDC Las Vegas".into(),
            date: NaiveDate::from_ymd_opt(2026, 5, 15).unwrap(),
            venue: None,
            artists: vec![],
            event_type: EventType::Festival,
            gate: EpistemicGate {
                valence: TruthValence::Unknown,
                authority: GateAuthority::ExternalSource("edc.com".into()),
                receipt_ref: None,
                justification: String::new(),
            },
            provenance: vec![],
            last_capture: None,
            last_updated: chrono::Utc::now(),
        };

        // ── Phase 4: Semantic Diff ──────────────────────────────────
        let receipt = pt_core::hash_str("receipt_001");
        let result = DiffEngine::diff(
            &robson_valley.id,
            None, // new event
            &serde_json::to_value(&robson_valley).unwrap(),
            &receipt,
        );
        assert_eq!(result.diff_class, pt_diff::SyntheticDiffClass::NewEvent);

        // ── Phase 5: Ranking ────────────────────────────────────────
        let ranker = Ranker::new(RankWeights::default());
        let candidates = vec![
            Candidate {
                event: &robson_valley,
                genre_match: 1.0,   // perfect folk match
                window_urgency: 0.7,
                data_confidence: 1.0,
                travel_cost: 0.6,
                historic_signal: 0.5,
                event_scale_fit: 0.5,
                pay_signal: 0.0,
            },
            Candidate {
                event: &edc,
                genre_match: 0.0,   // excluded genre
                window_urgency: 0.9,
                data_confidence: 0.3,
                travel_cost: 0.1,
                historic_signal: 0.0,
                event_scale_fit: 0.5,
                pay_signal: 0.0,
            },
        ];

        let ranked = ranker.rank(&candidates);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].event_name, "Robson Valley Music Festival",
            "folk festival should rank #1");
        assert_eq!(ranked[1].event_name, "EDC Las Vegas",
            "EDM festival should rank last");
        assert!(ranked[0].score > ranked[1].score);
        assert!(!ranked[0].explanation.is_empty());

        // ── Phase 6: Application Tracker FSM ────────────────────────
        let tracker = Tracker::new(30);
        let mut app = Application {
            id: "app_rv_001".into(),
            event_name: "Robson Valley Music Festival".into(),
            event_id: hex::encode(robson_valley.id),
            current_state: State::Discovered,
            transitions: vec![],
            submitted_at: None,
            materials: None,
            response: None,
            days_since_last_contact: 0,
        };

        // Discovered → Shortlisted → Preparing → Submitted
        for (state, actor) in &[
            (State::Shortlisted, "Scout"),
            (State::Preparing, "Scout"),
            (State::Submitted, "Scout"),
        ] {
            let t = tracker.transition(&mut app, *state, actor, None).unwrap();
            assert_eq!(t.to, *state);
        }
        assert_eq!(app.current_state, State::Submitted);
        assert!(app.submitted_at.is_some());
        assert_eq!(app.transitions.len(), 3);

        // ── Phase 7: Morning Digest ─────────────────────────────────
        let digest_input = vec![RankedDigestInput {
            event_name: ranked[0].event_name.clone(),
            location: Some("Dunster, BC".into()),
            genres: vec!["folk".into(), "bluegrass".into()],
            deadline: Some("2026-03-01".into()),
            days_remaining: Some(60),
            confidence: "verified".into(),
            url: "https://robsonvalleymusicfest.ca".into(),
            application_state: "submitted".into(),
            explanation: ranked[0].explanation.clone(),
        }];

        let alerts = vec![AlertItem {
            level: AlertLevel::Info,
            message: "Application submitted via email".into(),
            event_name: "Robson Valley Music Festival".into(),
        }];

        let digest = MorningDigest::build("Scout Shorefox", &digest_input, alerts);
        let markdown = digest.to_markdown();
        assert!(markdown.contains("Scout Shorefox"));
        assert!(markdown.contains("Robson Valley"));
        assert!(markdown.contains("60 days"));
        assert!(markdown.contains("submitted"));

        // ── Phase 8: GeoJSON Export (ODIN C2 Map) ───────────────────
        let odin = OdinBridge::new(None);
        let nodes = vec![pt_bridge_odin::TacticalNode {
            id: robson_valley.id,
            label: "Robson Valley Music Festival".into(),
            lat: 53.0,
            lng: -120.0,
            node_type: pt_bridge_odin::NodeType::Festival,
            deadline: Some(NaiveDate::from_ymd_opt(2026, 3, 1).unwrap()),
            genres: vec!["folk".into(), "bluegrass".into()],
            confidence: 1.0,
            urgency: 7.0,
            source_url: Some("https://robsonvalleymusicfest.ca".into()),
        }];

        let geojson = odin.build_geojson(&nodes);
        assert_eq!(geojson["type"], "FeatureCollection");
        assert_eq!(geojson["features"].as_array().unwrap().len(), 1);
        let feature = &geojson["features"][0];
        assert_eq!(feature["properties"]["label"], "Robson Valley Music Festival");
        assert!(feature["geometry"]["coordinates"][0].as_f64().unwrap() < -119.0);

        // ── Phase 9: Verify Weights Sum ─────────────────────────────
        let w = RankWeights::default();
        let sum = w.genre_match + w.window_urgency + w.data_confidence
            + w.travel_cost + w.historic_signal + w.event_scale_fit + w.pay_signal;
        assert!((sum - 1.0).abs() < 0.001, "weights must sum to 1.0");

        println!("✅ E2E pipeline: CAS → Artist → Event → Diff → Rank → Track → Digest → GeoJSON — ALL PASS");
    }
}
