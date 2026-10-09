//! Extraction contract constants for the deterministic oracle.
//!
//! Depenaudit K-2 (2026-10-09): the LLM system instruction is DELETED — there
//! is no model to instruct anymore. What remains are the data contracts the
//! deterministic extractors honor: the canonical event field names and the
//! canonical genre taxonomy (exact keyword match in client.rs).

/// Canonical event field names the oracle should target for QueryTarget::ExtractEvent.
pub const EVENT_FIELDS: &[&str] = &[
    "name",
    "dates",
    "application_opens",
    "application_closes",
    "location",
    "contact_email",
    "website",
];

/// Canonical genre taxonomy tags (music-festival domain).
pub const CANONICAL_GENRES: &[&str] = &[
    "folk", "bluegrass", "blues", "rock", "country", "americana", "roots",
    "singer-songwriter", "punk", "electronic", "edm", "jazz", "hip-hop", "rnb",
    "classical", "world",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_constants_are_present() {
        // Depenaudit K-2: SYSTEM_INSTRUCTION is deleted (no model to instruct).
        // The data contracts remain.
        assert!(EVENT_FIELDS.contains(&"application_closes"));
        assert!(CANONICAL_GENRES.contains(&"bluegrass"));
    }
}
