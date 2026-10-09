//! Oracle edge case tests — adversarial validation scenarios.

#[cfg(test)]
mod adversarial_tests {
    use crate::{ByteSpan, OracleOutput, Posture};
    use crate::candidate::{CandidateStore, StagedCandidate};
    use crate::validator::Validator;
    use pt_core::{hash_bytes, hash_str};
    use std::collections::HashSet;

    #[test]
    fn test_empty_span_rejected() {
        let raw = b"text";
        let span = ByteSpan { start: 0, end: 0 }; // empty span
        assert!(span.extract(raw).is_none());
    }

    #[test]
    fn test_span_exactly_at_bounds() {
        let raw = b"hello";
        let span = ByteSpan { start: 0, end: 5 };
        assert_eq!(span.extract(raw).unwrap(), "hello");
    }

    #[test]
    fn test_partial_utf8_span_rejected() {
        // é is 2 bytes in UTF-8: 0xC3 0xA9
        let raw: &[u8] = &[0x63, 0x61, 0x66, 0xC3, 0xA9]; // "café"
        // Span that cuts the é in half
        let span = ByteSpan { start: 2, end: 4 }; // "f" + half of é
        assert!(span.extract(raw).is_none());
    }

    #[test]
    fn test_case_insensitive_field_match() {
        let raw = b"Deadline: MARCH 15, 2026";
        let span = ByteSpan { start: 10, end: 24 };
        let candidate = StagedCandidate {
            capture_hash: hash_bytes(raw),
            output: OracleOutput::FieldCandidate {
                field: "app_window.closes".into(),
                value: "march 15, 2026".into(),
                span,
            },
            validation_count: 0,
            validating_captures: HashSet::new(),
            promoted: false,
            ruling: None,
        };
        let validator = Validator::new();
        let ruling = validator.validate_candidate(&candidate, raw);
        assert!(ruling.passed, "case-insensitive match should pass: {}", ruling.evidence);
    }

    #[test]
    fn test_attacker_injects_span_outside_range() {
        // Attacker claims the deadline is at span [0, 5] but raw text is different
        let raw = b"Deadline: April 1, 2026. Apply by March 15 for early bird.";
        let span = ByteSpan { start: 0, end: 8 }; // "Deadline"
        let candidate = StagedCandidate {
            capture_hash: hash_bytes(raw),
            output: OracleOutput::FieldCandidate {
                field: "app_window.closes".into(),
                value: "March 15, 2026".into(), // claimed value NOT at this span
                span,
            },
            validation_count: 0,
            validating_captures: HashSet::new(),
            promoted: false,
            ruling: None,
        };
        let validator = Validator::new();
        let ruling = validator.validate_candidate(&candidate, raw);
        assert!(!ruling.passed, "span with wrong value must be rejected");
    }

    #[test]
    fn test_ballistic_posture_skips_validation() {
        assert!(!Posture::Ballistic.should_validate());
        assert!(!Posture::Ballistic.should_enrich());
        assert!(!Posture::Ballistic.should_be_polite());
    }

    #[test]
    fn test_candidate_store_promotable_threshold() {
        let mut store = CandidateStore::new();
        store.stage(&hash_str("cap1"), OracleOutput::Nothing);

        // 2 validations from TWO DISTINCT captures → promotable (H2: replaying
        // the same capture would NOT reach the threshold).
        assert!(store.increment_validation(0, &hash_str("cap1")));
        assert!(!store.increment_validation(0, &hash_str("cap1")), "replay does not count");
        assert!(store.increment_validation(0, &hash_str("cap2")));
        store.validate(0, crate::candidate::ValidationRuling {
            passed: true,
            method: "test".into(),
            evidence: "".into(),
            validator_version: "1".into(),
        });

        let promotable = store.promotable();
        assert_eq!(promotable.len(), 1, "2 distinct-capture validations + passed → promotable");
    }

    #[test]
    fn test_validator_rejects_nothing_output() {
        let raw = b"anything";
        let candidate = StagedCandidate {
            capture_hash: hash_bytes(raw),
            output: OracleOutput::Nothing,
            validation_count: 0,
            validating_captures: HashSet::new(),
            promoted: false,
            ruling: None,
        };
        let validator = Validator::new();
        let ruling = validator.validate_candidate(&candidate, raw);
        assert!(!ruling.passed, "Nothing output should never pass validation");
    }
}
