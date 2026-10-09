//! Deterministic validator — confirms oracle candidates against raw captures.
//!
//! The gate between AI pointing and truth promotion.
//! Validation methods: byte-span verification, regex confirmation,
//! cross-source corroboration, and re-fetch comparison.
//!
//! From FRAMEWORK §6: "Agents point, receipts promote."

use crate::candidate::{CandidateStore, StagedCandidate, ValidationRuling};
use crate::{ByteSpan, OracleOutput};
use pt_core::ContentHash;

/// The validator — confirms or rejects oracle candidates.
pub struct Validator {
    /// CAS store for re-reading raw captures.
    // In production: reference to pt-db CAS/TruthStore
    _cas: (),
}

impl Validator {
    pub fn new() -> Self {
        Self { _cas: () }
    }

    /// Validate a single candidate against its raw capture.
    /// Returns a ruling with evidence.
    pub fn validate_candidate(
        &self,
        candidate: &StagedCandidate,
        raw_capture: &[u8],
    ) -> ValidationRuling {
        // M3: bind the ruling to the exact capture the candidate was extracted
        // from. Without this, a caller could validate candidate A's claim against
        // a *different* capture B's bytes (which might happen to contain the value).
        if pt_core::hash_bytes(raw_capture) != candidate.capture_hash {
            return ValidationRuling {
                passed: false,
                method: "capture_binding".into(),
                evidence: "raw capture hash does not match candidate.capture_hash".into(),
                validator_version: env!("CARGO_PKG_VERSION").into(),
            };
        }
        match &candidate.output {
            OracleOutput::FieldCandidate { field, value, span } => {
                self.validate_field(field, value, span, raw_capture)
            }
            OracleOutput::UrlNomination { url, why_span } => {
                self.validate_url(url, why_span, raw_capture)
            }
            OracleOutput::GenreSuggestion { tags, span } => {
                self.validate_genres(tags, span, raw_capture)
            }
            OracleOutput::Nothing => ValidationRuling {
                passed: false,
                method: "none".into(),
                evidence: "No claim to validate".into(),
                validator_version: env!("CARGO_PKG_VERSION").into(),
            },
        }
    }

    /// Validate a field extraction: check the byte span contains the claimed value.
    fn validate_field(
        &self,
        field: &str,
        value: &str,
        span: &ByteSpan,
        raw: &[u8],
    ) -> ValidationRuling {
        let method = "byte_span_verify".to_string();

        match span.extract(raw) {
            Some(extracted) => {
                // M1: the span must be EXACTLY the value (modulo surrounding
                // whitespace + ASCII case), not merely contain it. `contains`
                // let a span far wider than — and semantically different from —
                // the claim pass (e.g. "Deadline: March 15, 2026 — no fee"
                // validating value "March 15, 2026"). The receipt must pin the
                // claim to its exact bytes.
                let passed = extracted.trim().eq_ignore_ascii_case(value.trim());

                if passed {
                    ValidationRuling {
                        passed: true,
                        method,
                        evidence: format!(
                            "field '{field}': byte span [{}-{}] contains '{extracted}' matching claimed '{value}'",
                            span.start, span.end
                        ),
                        validator_version: env!("CARGO_PKG_VERSION").into(),
                    }
                } else {
                    ValidationRuling {
                        passed: false,
                        method,
                        evidence: format!(
                            "field '{field}': byte span contains '{extracted}' but claimed '{value}' — mismatch"
                        ),
                        validator_version: env!("CARGO_PKG_VERSION").into(),
                    }
                }
            }
            None => ValidationRuling {
                passed: false,
                method,
                evidence: format!(
                    "field '{field}': byte span [{}-{}] is out of bounds or invalid UTF-8",
                    span.start, span.end
                ),
                validator_version: env!("CARGO_PKG_VERSION").into(),
            },
        }
    }

    /// Validate a URL nomination: the span MUST actually contain the URL (or its
    /// host). The old gate passed on any span merely containing "festival" or
    /// "apply" — that let a model (or attacker) nominate an arbitrary URL onto
    /// the crawl frontier just by pointing at unrelated text with those words,
    /// a self-directed-crawl amplification hole. Now the byte span has to carry
    /// Validate a URL nomination. Leanstral's own audit (pass5/pass11) caught
    /// that a naive `text.contains(url)` substring branch is exploitable: an
    /// attacker who controls span text can embed any URL as a substring, and a
    /// tiny `url.contains(text)` span can validate anything. The honest gate:
    /// require the URL's HOST to appear as a token in the span, AND require the
    /// URL to be a well-formed http(s) URL. No topical-vibe passes, no substring
    /// games. "Agents point, receipts promote" — the receipt is the host in the
    /// bytes.
    fn validate_url(&self, url: &str, span: &ByteSpan, raw: &[u8]) -> ValidationRuling {
        let method = "url_span_verify".to_string();

        // Reject non-http(s) schemes outright — file://, javascript:, data: etc.
        let scheme_ok = url.starts_with("http://") || url.starts_with("https://");

        // Extract the bare host (between scheme and first '/'). Empty => malformed.
        let host = url
            .split("://")
            .nth(1)
            .and_then(|rest| rest.split('/').next())
            .unwrap_or("");

        match span.extract(raw) {
            Some(text) => {
                // The span must literally contain the host. This is the receipt:
                // the domain we're about to crawl actually appears in the source
                // bytes. No substring-of-url branch (exploitable), no keyword vibe.
                let passed = scheme_ok && !host.is_empty() && text.contains(host);

                ValidationRuling {
                    passed,
                    method,
                    evidence: format!(
                        "URL '{url}' (host '{host}', scheme_ok={scheme_ok}): host_in_span={passed}"
                    ),
                    validator_version: env!("CARGO_PKG_VERSION").into(),
                }
            }
            None => ValidationRuling {
                passed: false,
                method,
                evidence: "span out of bounds".into(),
                validator_version: env!("CARGO_PKG_VERSION").into(),
            },
        }
    }

    /// Validate genre suggestions: the span should mention genre-related words.
    fn validate_genres(
        &self,
        tags: &[String],
        span: &ByteSpan,
        raw: &[u8],
    ) -> ValidationRuling {
        let method = "genre_span_verify".to_string();
        match span.extract(raw) {
            Some(text) => {
                let text_lower = text.to_lowercase();
                let matched: Vec<_> = tags
                    .iter()
                    .filter(|tag| text_lower.contains(&tag.to_lowercase()))
                    .collect();

                let passed = !matched.is_empty();
                ValidationRuling {
                    passed,
                    method,
                    evidence: format!(
                        "genres {:?}: matched {:?} in span text",
                        tags, matched
                    ),
                    validator_version: env!("CARGO_PKG_VERSION").into(),
                }
            }
            None => ValidationRuling {
                passed: false,
                method,
                evidence: "span out of bounds".into(),
                validator_version: env!("CARGO_PKG_VERSION").into(),
            },
        }
    }

    /// Validate all pending candidates in the store against a raw capture.
    pub fn validate_all(
        &self,
        store: &mut CandidateStore,
        raw_capture: &[u8],
    ) -> Vec<(usize, ValidationRuling)> {
        let capture_hash = pt_core::hash_bytes(raw_capture);
        let mut results = Vec::new();

        // M2: use REAL vec indices (pending_indices), not enumerate(pending())
        // — the latter yields filtered indices that misalign with the full vec
        // once any candidate is promoted, writing rulings to the wrong candidate.
        for idx in store.pending_indices() {
            if let Some(c) = store.candidate(idx) {
                let ruling = self.validate_candidate(c, raw_capture);
                results.push((idx, ruling));
            }
        }

        for (idx, ruling) in &results {
            store.validate(*idx, ruling.clone());
            if ruling.passed {
                // H2: count this capture only if it's distinct from prior ones.
                store.increment_validation(*idx, &capture_hash);
            }
        }

        results
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OracleOutput;

    // capture_hash is bound to the raw bytes the candidate will be validated
    // against (M3 requires them to match).
    fn make_candidate(data: OracleOutput, raw: &[u8]) -> StagedCandidate {
        StagedCandidate {
            capture_hash: pt_core::hash_bytes(raw),
            output: data,
            validation_count: 0,
            validating_captures: std::collections::HashSet::new(),
            promoted: false,
            ruling: None,
        }
    }

    #[test]
    fn test_validate_field_correct_span() {
        let raw = b"<div class='apply-by'>Applications close March 15, 2026</div>";
        // "March 15, 2026" starts at byte 41, ends at 55
        let span = ByteSpan { start: 41, end: 55 };
        let candidate = make_candidate(OracleOutput::FieldCandidate {
            field: "app_window.closes".into(),
            value: "March 15, 2026".into(),
            span,
        }, raw);

        let validator = Validator::new();
        let ruling = validator.validate_candidate(&candidate, raw);
        assert!(ruling.passed, "correct span should pass: {}", ruling.evidence);
    }

    #[test]
    fn test_validate_field_wrong_span() {
        let raw = b"Deadline: April 1, 2026";
        let span = ByteSpan { start: 0, end: 8 }; // "Deadline" — not the date
        let candidate = make_candidate(OracleOutput::FieldCandidate {
            field: "app_window.closes".into(),
            value: "March 15, 2026".into(),
            span,
        }, raw);

        let validator = Validator::new();
        let ruling = validator.validate_candidate(&candidate, raw);
        assert!(!ruling.passed, "wrong span should fail");
    }

    // M1: a span WIDER than the value (containing it plus extra) must be rejected
    // — the receipt has to pin exactly the claimed bytes, not merely include them.
    #[test]
    fn test_validate_field_overwide_span_rejected() {
        let raw = b"Deadline: March 15, 2026 - no fee";
        let span = ByteSpan { start: 0, end: raw.len() }; // whole line, wider than value
        let candidate = make_candidate(OracleOutput::FieldCandidate {
            field: "app_window.closes".into(),
            value: "March 15, 2026".into(),
            span,
        }, raw);
        let validator = Validator::new();
        let ruling = validator.validate_candidate(&candidate, raw);
        assert!(!ruling.passed, "an over-wide span must not validate (M1): {}", ruling.evidence);
    }

    // M3: validating a candidate against bytes that don't hash to its
    // capture_hash must be refused — no cross-capture validation.
    #[test]
    fn test_validate_rejects_capture_mismatch() {
        let raw = b"<div>Applications close March 15, 2026</div>";
        let span = ByteSpan { start: 24, end: 38 };
        let mut candidate = make_candidate(OracleOutput::FieldCandidate {
            field: "app_window.closes".into(),
            value: "March 15, 2026".into(),
            span,
        }, raw);
        // point the candidate at a DIFFERENT capture than the bytes we pass
        candidate.capture_hash = pt_core::hash_str("some other capture");
        let validator = Validator::new();
        let ruling = validator.validate_candidate(&candidate, raw);
        assert!(!ruling.passed, "capture mismatch must be refused (M3)");
        assert_eq!(ruling.method, "capture_binding");
    }

    #[test]
    fn test_validate_genres_match() {
        let raw = b"Robson Valley Music Festival -- folk, bluegrass, and roots music in the mountains";
        let span = ByteSpan { start: 31, end: 79 };
        let candidate = make_candidate(OracleOutput::GenreSuggestion {
            tags: vec!["folk".into(), "bluegrass".into()],
            span,
        }, raw);

        let validator = Validator::new();
        let ruling = validator.validate_candidate(&candidate, raw);
        assert!(ruling.passed, "genre match should pass");
    }

    #[test]
    fn test_nothing_output_fails() {
        let raw = b"anything";
        let candidate = make_candidate(OracleOutput::Nothing, raw);
        let validator = Validator::new();
        let ruling = validator.validate_candidate(&candidate, raw);
        assert!(!ruling.passed, "Nothing output should not pass validation");
    }

    #[test]
    fn test_url_nomination_requires_url_in_span() {
        // Span carries the actual URL host — legitimate nomination, should pass.
        let raw = b"Apply at https://robsonvalley.ca/apply before the deadline";
        // "https://robsonvalley.ca/apply" spans bytes 9..38
        let span = ByteSpan { start: 9, end: 38 };
        let candidate = make_candidate(OracleOutput::UrlNomination {
            url: "https://robsonvalley.ca/apply".into(),
            why_span: span,
        }, raw);
        let validator = Validator::new();
        let ruling = validator.validate_candidate(&candidate, raw);
        assert!(ruling.passed, "URL present in span should pass: {}", ruling.evidence);
    }

    #[test]
    fn test_url_nomination_amplification_attack_rejected() {
        // The old gate passed on ANY span containing "festival"/"apply" even
        // with no URL present. This is that attack: the span is topical prose
        // with the word "festival" but does NOT contain the nominated URL.
        // The tightened gate MUST reject it — no self-directed-crawl amplification.
        let raw = b"the best folk festival vibes in the valley, apply soon";
        let span = ByteSpan { start: 0, end: raw.len() };
        let candidate = make_candidate(OracleOutput::UrlNomination {
            url: "https://evil-injected-domain.example/pwn".into(),
            why_span: span,
        }, raw);
        let validator = Validator::new();
        let ruling = validator.validate_candidate(&candidate, raw);
        assert!(
            !ruling.passed,
            "URL absent from span must be rejected (amplification hole closed): {}",
            ruling.evidence
        );
    }
}
