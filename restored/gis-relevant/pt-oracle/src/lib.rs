//! pt-oracle — AI slow lane: bounded pointing with byte-span enforcement.
//!
//! The contract is architectural, not honor-system:
//! every candidate MUST carry a byte span into the stored raw capture.
//! No span → structurally impossible to promote. The Hermes lesson made load-bearing.
//!
//! Architecture:
//!   1. Raw capture (HTML/JSON) stored in CAS
//!   2. Oracle points at candidates with byte spans
//!   3. Deterministic validator confirms (re-fetch, regex/DOM, cross-source)
//!   4. Validated patterns → promoted back to fast lane as AdapterRules
//!
//! From PALANTOUR_FRAMEWORK.md §6 and DOCTRINE Part B (P0→P3 posture).

pub mod adversarial_tests;
pub mod candidate;
pub mod client;
pub mod phoenix_ai;
pub mod phoenix_recovery;
pub mod prompt;
pub mod validator;
pub mod vericlaims_ai;

use pt_core::ContentHash;
use serde::{Deserialize, Serialize};

// ── Oracle Output Types ────────────────────────────────────────────

/// Every oracle output MUST carry a byte span. Non-negotiable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OracleOutput {
    /// A field value extracted from the raw capture.
    FieldCandidate {
        field: String,
        value: String,
        span: ByteSpan,
    },
    /// A URL found in prose that should be added to the frontier.
    UrlNomination {
        url: String,
        why_span: ByteSpan,
    },
    /// Genre taxonomy suggestions.
    GenreSuggestion {
        tags: Vec<String>,
        span: ByteSpan,
    },
    /// Nothing useful found in this capture.
    Nothing,
}

/// Byte range into the raw capture proving the claim exists.
/// The validator checks the span actually contains supporting text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

impl ByteSpan {
    /// Extract the referenced text from raw bytes.
    pub fn extract<'a>(&self, raw: &'a [u8]) -> Option<&'a str> {
        if self.start < raw.len() && self.end <= raw.len() && self.start < self.end {
            std::str::from_utf8(&raw[self.start..self.end]).ok()
        } else {
            None
        }
    }

    /// Verify the span contains (or fuzzy-matches) the expected text.
    pub fn verify(&self, raw: &[u8], expected: &str) -> bool {
        match self.extract(raw) {
            Some(text) => {
                // Exact match or contains
                text.trim() == expected.trim() || text.contains(expected.trim())
            }
            None => false,
        }
    }
}

/// An oracle query — what we ask the slow lane to look at.
#[derive(Debug, Clone)]
pub struct OracleQuery {
    /// Hash of the raw capture in CAS.
    pub capture_hash: ContentHash,
    /// The raw text content from the capture.
    pub raw_text: String,
    /// What we're asking the oracle to find.
    pub target: QueryTarget,
    /// Source URL for context.
    pub source_url: String,
}

#[derive(Debug, Clone)]
pub enum QueryTarget {
    /// Extract all fields for a festival/event.
    ExtractEvent,
    /// Find application deadlines specifically.
    FindDeadlines,
    /// Extract contact information.
    FindContacts,
    /// Suggest genre taxonomy tags.
    SuggestGenres,
    /// Find URLs of related events/sources.
    FindUrls,
    /// Custom prompt.
    Custom(String),
}

// ── Posture Dial (DOCTRINE Part B) ──────────────────────────────────

/// The P0→P3 acquisition posture. Every acquisition subsystem exposes this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Posture {
    /// P0 — Precision: full pipeline, validate, normalize, truth. Default.
    Precision,
    /// P1 — Fast-accurate: skip enrichment, keep validation.
    FastAccurate,
    /// P2 — Capture-and-defer: store raw to CAS, process later.
    CaptureAndDefer,
    /// P3 — Ballistic/Salvage: saturate I/O, grab raw, skip everything.
    Ballistic,
}

impl Default for Posture {
    fn default() -> Self {
        Self::Precision
    }
}

impl Posture {
    /// Should we run the validator at this posture?
    pub fn should_validate(&self) -> bool {
        matches!(self, Self::Precision | Self::FastAccurate)
    }

    /// Should we run enrichment (genre taxonomy, geo, etc.)?
    pub fn should_enrich(&self) -> bool {
        matches!(self, Self::Precision)
    }

    /// Should we be polite (respect robots.txt, rate limits)?
    pub fn should_be_polite(&self) -> bool {
        !matches!(self, Self::Ballistic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byte_span_extract() {
        let raw = b"Applications close March 15, 2026. Late submissions not accepted.";
        let span = ByteSpan { start: 19, end: 33 };  // "March 15, 2026"
        let text = span.extract(raw).unwrap();
        assert_eq!(text, "March 15, 2026");
    }

    #[test]
    fn test_byte_span_verify() {
        let raw = b"Deadline: 2026-03-01. Apply at https://fest.example.com";
        let span = ByteSpan { start: 10, end: 20 };
        assert!(span.verify(raw, "2026-03-01"));

        // Wrong span — shouldn't match
        let bad_span = ByteSpan { start: 0, end: 5 };
        assert!(!bad_span.verify(raw, "2026-03-01"));
    }

    #[test]
    fn test_byte_span_out_of_bounds() {
        let raw = b"short";
        let span = ByteSpan { start: 0, end: 100 };
        assert!(span.extract(raw).is_none());
    }

    #[test]
    fn test_posture_defaults() {
        assert!(Posture::Precision.should_validate());
        assert!(Posture::Precision.should_enrich());
        assert!(Posture::Precision.should_be_polite());

        assert!(!Posture::Ballistic.should_validate());
        assert!(!Posture::Ballistic.should_enrich());
        assert!(!Posture::Ballistic.should_be_polite());
    }
}

// ── PORTED-FROM Python (bare clay → Rust permanent) ────────────────
// Drafts under ports/ — compile-verified, not yet canon-promoted.
pub mod ported {
    pub mod oracle_verify;
    pub mod ising_entity_resolver;
    pub mod pipeline_taint_tracker;
}
