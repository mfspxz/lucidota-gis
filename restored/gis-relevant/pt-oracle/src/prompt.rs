//! Prompt schema for the oracle — the JSON contract the model must return.
//!
//! The model returns VALUES ONLY (verbatim substrings of the page). It never
//! returns byte offsets: `OracleClient::parse_and_span` re-derives every span
//! locally against the raw capture, so a value the raw bytes don't contain is
//! dropped. This module documents the schema and provides the canonical system
//! instruction so prompt wording stays consistent across call sites.

/// The canonical system instruction. Kept here so every oracle call uses the same
/// extraction contract (verbatim values, compact JSON, omit-if-absent).
pub const SYSTEM_INSTRUCTION: &str = "You are an extraction oracle. Return ONLY \
compact JSON of the form \
{\"fields\":[{\"field\":\"name\",\"value\":\"...\"}],\"urls\":[\"...\"],\"genres\":[\"...\"]}. \
Copy every value VERBATIM from the page text (exact substrings). Do not paraphrase, \
do not summarize, do not invent. Omit any field not present in the text.";

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
        assert!(SYSTEM_INSTRUCTION.contains("VERBATIM"));
        assert!(EVENT_FIELDS.contains(&"application_closes"));
        assert!(CANONICAL_GENRES.contains(&"bluegrass"));
    }
}
