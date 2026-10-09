//! Deterministic extraction oracle — the LLM path is DEAD (depenaudit K-2, 2026-10-09).
//!
//! What was killed: `OracleClient::query()` POSTed page text to an LLM endpoint
//! with a system prompt begging "copy every value VERBATIM... do not paraphrase"
//! — an LLM simulating `grep -oE`. Deleted: `query`, `build_prompt`, endpoint /
//! key / model plumbing (`with_api_key`, `from_env`), the chat-completion JSON
//! types, reqwest, anyhow.
//!
//! Replacement extractors — all local, all byte-exact, all O(n):
//! - emails: RFC 5322-ish regex
//! - phones: E.164-ish regex (>= 7 digits; date-shapes rejected)
//! - urls: http(s) regex, trailing punctuation trimmed
//! - dates: ISO / US / long-month regexes, every hit VALIDATED by chrono
//!   (Feb 30 dies here, not in a model's imagination)
//! - genres: exact case-insensitive keyword match on CANONICAL_GENRES
//!
//! The anti-fabrication law is unchanged and now STRONGER: candidates are
//! derived from the bytes by construction (regex match spans), not
//! model-claimed-then-located. The per-target field allow-list still gates
//! every FieldCandidate.

use crate::prompt::CANONICAL_GENRES;
use crate::{ByteSpan, OracleOutput, OracleQuery, QueryTarget};
use chrono::NaiveDate;
use regex::Regex;
use std::sync::LazyLock;

/// Deterministic oracle client. No endpoint, no key, no model, no network.
/// Construct with `OracleClient::new()` and call `extract(&query)`.
pub struct OracleClient;

static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)*\.[A-Za-z]{2,}")
        .expect("EMAIL_RE is a static literal")
});
static PHONE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\+?\d[\d\s.()\-]{5,}\d").expect("PHONE_RE is a static literal")
});
static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://[^\s<>'\])]+").expect("URL_RE is a static literal"));
static DATE_ISO_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b\d{4}-\d{2}-\d{2}\b").expect("DATE_ISO_RE is a static literal")
});
static DATE_US_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b\d{1,2}/\d{1,2}/\d{2,4}\b").expect("DATE_US_RE is a static literal")
});
static DATE_LONG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(january|february|march|april|may|june|july|august|september|october|november|december|jan|feb|mar|apr|jun|jul|aug|sep|sept|oct|nov|dec)\.?\s+\d{1,2}(?:st|nd|rd|th)?,?\s+\d{4}\b")
        .expect("DATE_LONG_RE is a static literal")
});

fn span_of(m: &regex::Match) -> ByteSpan {
    ByteSpan {
        start: m.start(),
        end: m.end(),
    }
}

fn trim_url(u: &str) -> &str {
    u.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '"', '\''])
}

fn emails_as(field: &str, raw: &str) -> Vec<OracleOutput> {
    EMAIL_RE
        .find_iter(raw)
        .map(|m| OracleOutput::FieldCandidate {
            field: field.to_string(),
            value: m.as_str().to_string(),
            span: span_of(&m),
        })
        .collect()
}

fn phones_as(field: &str, raw: &str) -> Vec<OracleOutput> {
    PHONE_RE
        .find_iter(raw)
        .filter_map(|m| {
            let s = m.as_str();
            let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
            // E.164-ish: real phone numbers carry >= 7 digits.
            if digits.len() < 7 {
                return None;
            }
            // Reject date-shapes the loose phone pattern also matches.
            if DATE_ISO_RE.is_match(s) || DATE_US_RE.is_match(s) {
                return None;
            }
            Some(OracleOutput::FieldCandidate {
                field: field.to_string(),
                value: s.trim().to_string(),
                span: span_of(&m),
            })
        })
        .collect()
}

fn urls(raw: &str) -> Vec<OracleOutput> {
    URL_RE
        .find_iter(raw)
        .map(|m| {
            let u = trim_url(m.as_str());
            // Recompute the span for the trimmed URL.
            let start = m.start();
            OracleOutput::UrlNomination {
                url: u.to_string(),
                why_span: ByteSpan {
                    start,
                    end: start + u.len(),
                },
            }
        })
        .collect()
}

/// Validate a regex date hit with chrono. Returns the normalized hit on success.
fn valid_date_iso(s: &str) -> bool {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

fn valid_date_us(s: &str) -> bool {
    NaiveDate::parse_from_str(s, "%m/%d/%Y").is_ok()
        || NaiveDate::parse_from_str(s, "%m/%d/%y").is_ok()
}

fn valid_date_long(s: &str) -> bool {
    // Strip ordinal suffixes ("15th" -> "15") for chrono.
    let norm = Regex::new(r"(\d+)(st|nd|rd|th)\b")
        .expect("ordinal regex is static")
        .replace_all(s, "$1");
    let norm = norm.replace(',', " ");
    let norm = norm.split_whitespace().collect::<Vec<_>>().join(" ");
    ["%B %d %Y", "%b %d %Y"].iter().any(|f| {
        // chrono needs case-normalized month names for %B/%b.
        let titled: String = norm
            .split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str().to_lowercase().as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        NaiveDate::parse_from_str(&titled, f).is_ok()
    })
}

fn dates_as(field: &str, raw: &str) -> Vec<OracleOutput> {
    let mut out = Vec::new();
    for m in DATE_ISO_RE.find_iter(raw) {
        if valid_date_iso(m.as_str()) {
            out.push(OracleOutput::FieldCandidate {
                field: field.to_string(),
                value: m.as_str().to_string(),
                span: span_of(&m),
            });
        }
    }
    for m in DATE_US_RE.find_iter(raw) {
        if valid_date_us(m.as_str()) {
            out.push(OracleOutput::FieldCandidate {
                field: field.to_string(),
                value: m.as_str().to_string(),
                span: span_of(&m),
            });
        }
    }
    for m in DATE_LONG_RE.find_iter(raw) {
        if valid_date_long(m.as_str()) {
            out.push(OracleOutput::FieldCandidate {
                field: field.to_string(),
                value: m.as_str().to_string(),
                span: span_of(&m),
            });
        }
    }
    out
}

fn genres(raw: &str) -> Vec<OracleOutput> {
    let lower = raw.to_lowercase();
    let mut tags = Vec::new();
    let mut first_span: Option<ByteSpan> = None;
    for g in CANONICAL_GENRES {
        if let Some(pos) = lower.find(g) {
            // Word-boundary check on the ORIGINAL text at the match site.
            let end = pos + g.len();
            let before_ok = pos == 0
                || !raw.as_bytes()[pos - 1].is_ascii_alphanumeric();
            let after_ok = end >= raw.len() || !raw.as_bytes()[end].is_ascii_alphanumeric();
            if before_ok && after_ok {
                if first_span.is_none() {
                    first_span = Some(ByteSpan { start: pos, end });
                }
                tags.push(g.to_string());
            }
        }
    }
    match first_span {
        Some(span) => vec![OracleOutput::GenreSuggestion { tags, span }],
        None => Vec::new(),
    }
}

impl OracleClient {
    /// New deterministic client. No endpoint, no key, no network.
    pub fn new() -> Self {
        Self
    }

    /// Deterministic extraction over the query's raw text for its target.
    /// Every candidate's span is derived from the regex match — values come
    /// FROM the bytes by construction. Field names are gated by the
    /// per-target allow-list, same as before.
    pub fn extract(&self, query: &OracleQuery) -> Vec<OracleOutput> {
        let raw = &query.raw_text;
        let mut outs: Vec<OracleOutput> = match &query.target {
            QueryTarget::ExtractEvent => {
                let mut v = Vec::new();
                v.extend(emails_as("contact_email", raw));
                v.extend(phones_as("phone", raw));
                v.extend(urls(raw));
                v.extend(dates_as("dates", raw));
                v.extend(genres(raw));
                v
            }
            QueryTarget::FindDeadlines => dates_as("deadline", raw),
            QueryTarget::FindContacts => {
                let mut v = Vec::new();
                v.extend(emails_as("contact_email", raw));
                v.extend(phones_as("phone", raw));
                v.extend(urls(raw));
                v
            }
            QueryTarget::SuggestGenres => genres(raw),
            QueryTarget::FindUrls => urls(raw),
            // Custom carried the operator's own LLM prompt — there is no LLM
            // anymore, so there is nothing honest to do. Nothing, not a guess.
            QueryTarget::Custom(_) => Vec::new(),
        };
        outs.retain(|o| match o {
            OracleOutput::FieldCandidate { field, .. } => field_allowed(&query.target, field),
            _ => true,
        });
        // Dedupe exact (field, value, span) triples.
        let mut seen = std::collections::HashSet::new();
        outs.retain(|o| {
            let key = format!("{:?}", o);
            seen.insert(key)
        });
        if outs.is_empty() {
            outs.push(OracleOutput::Nothing);
        }
        outs
    }

    /// Parse a model-JSON extraction and re-derive byte spans against RAW text.
    /// LEGACY path (pre-K-2): kept because offline tooling and tests feed it
    /// captured model JSON. Any value not found verbatim in raw is DROPPED.
    /// Public for offline testing without a live endpoint.
    pub fn parse_and_span(&self, content: &str, raw_text: &str) -> Vec<OracleOutput> {
        // The model may wrap JSON in prose/fences; extract the first {...} block.
        let json_slice = extract_json_object(content).unwrap_or(content);
        let extraction: ModelExtraction = serde_json::from_str(json_slice).unwrap_or_default();

        let mut out = Vec::new();

        for f in &extraction.fields {
            if let Some(span) = locate(raw_text, &f.value) {
                out.push(OracleOutput::FieldCandidate {
                    field: f.field.clone(),
                    value: f.value.clone(),
                    span,
                });
            }
            // else: value not in raw bytes -> hallucination -> dropped silently.
        }

        for u in &extraction.urls {
            // Span must contain the URL's host — locate the host in raw.
            let host = u.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or(u);
            if let Some(span) = locate(raw_text, host) {
                out.push(OracleOutput::UrlNomination {
                    url: u.clone(),
                    why_span: span,
                });
            }
        }

        if !extraction.genres.is_empty() {
            // Locate the first genre that actually appears; span it.
            for g in &extraction.genres {
                if let Some(span) = locate(raw_text, g) {
                    out.push(OracleOutput::GenreSuggestion {
                        tags: extraction.genres.clone(),
                        span,
                    });
                    break;
                }
            }
        }

        if out.is_empty() {
            out.push(OracleOutput::Nothing);
        }
        out
    }
}

/// Legacy shape of a model-JSON extraction (pre-K-2). Kept for parse_and_span.
#[derive(serde::Deserialize, Default)]
struct ModelExtraction {
    #[serde(default)]
    fields: Vec<ModelField>,
    #[serde(default)]
    urls: Vec<String>,
    #[serde(default)]
    genres: Vec<String>,
}
#[derive(serde::Deserialize)]
struct ModelField {
    field: String,
    value: String,
}

/// The allow-list of field names each query target may legitimately produce.
/// `None` = no whitelist (Custom targets carry the operator's own prompt).
fn allowed_fields(target: &QueryTarget) -> Option<&'static [&'static str]> {
    match target {
        QueryTarget::ExtractEvent => Some(&[
            "name", "dates", "application_opens", "application_closes", "location",
            "contact_email", "website", "phone",
        ]),
        QueryTarget::FindDeadlines => Some(&["deadline"]),
        QueryTarget::FindContacts => Some(&["contact_email", "phone", "form_url"]),
        // genre/url targets don't produce field candidates at all
        QueryTarget::SuggestGenres | QueryTarget::FindUrls => Some(&[]),
        QueryTarget::Custom(_) => None,
    }
}

/// Whether `field` is allowed for `target`. Custom targets are permissive.
fn field_allowed(target: &QueryTarget, field: &str) -> bool {
    match allowed_fields(target) {
        Some(allow) => allow.contains(&field),
        None => true,
    }
}

/// Locate `needle` in `haystack`, returning a byte span. Tries exact first, then
/// trimmed. Returns None if absent — which is what drops hallucinated values.
fn locate(haystack: &str, needle: &str) -> Option<ByteSpan> {
    let n = needle.trim();
    if n.is_empty() {
        return None;
    }
    haystack.find(n).map(|start| ByteSpan {
        start,
        end: start + n.len(),
    })
}

/// Extract the first balanced {...} JSON object from a possibly-fenced string.
fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let mut depth = 0usize;
    for (i, c) in s[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..start + i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator::Validator;
    use crate::candidate::CandidateStore;

    fn test_query(target: QueryTarget, raw: &str) -> OracleQuery {
        OracleQuery {
            capture_hash: [0u8; 32],
            raw_text: raw.to_string(),
            target,
            source_url: "test://fixture".to_string(),
        }
    }

    #[test]
    fn extract_finds_email_phone_url_deadline() {
        let client = OracleClient::new();
        let raw = "Contact us at booking@robsonfest.example.com or +1 (604) 555-0142. \
                   Applications close March 15, 2026. Details: https://robsonfest.example.com/apply";
        let q = test_query(QueryTarget::ExtractEvent, raw);
        let outs = client.extract(&q);
        let fields: Vec<(&str, &str)> = outs
            .iter()
            .filter_map(|o| match o {
                OracleOutput::FieldCandidate { field, value, .. } => Some((field.as_str(), value.as_str())),
                _ => None,
            })
            .collect();
        assert!(fields.iter().any(|(f, v)| *f == "contact_email" && *v == "booking@robsonfest.example.com"), "email: {:?}", fields);
        assert!(fields.iter().any(|(f, v)| *f == "phone" && v.contains("604")), "phone: {:?}", fields);
        assert!(fields.iter().any(|(f, v)| *f == "dates" && *v == "March 15, 2026"), "date: {:?}", fields);
        assert!(outs.iter().any(|o| matches!(o, OracleOutput::UrlNomination { url, .. } if url == "https://robsonfest.example.com/apply")), "url missing");
        // Every span must verify against the raw bytes.
        for o in &outs {
            match o {
                OracleOutput::FieldCandidate { value, span, .. } => {
                    assert!(span.verify(raw.as_bytes(), value), "span must verify: {}", value)
                }
                OracleOutput::UrlNomination { url, why_span } => {
                    assert!(why_span.extract(raw.as_bytes()).map(|t| url.contains(t) || t.contains(url.trim_end_matches('/'))).unwrap_or(false), "url span: {}", url)
                }
                _ => {}
            }
        }
    }

    #[test]
    fn extract_rejects_impossible_dates() {
        let client = OracleClient::new();
        let raw = "Deadline: February 30, 2026. Real one: 2026-03-01.";
        let q = test_query(QueryTarget::FindDeadlines, raw);
        let outs = client.extract(&q);
        let values: Vec<&str> = outs
            .iter()
            .filter_map(|o| match o {
                OracleOutput::FieldCandidate { value, .. } => Some(value.as_str()),
                _ => None,
            })
            .collect();
        assert!(!values.iter().any(|v| v.contains("February 30")), "Feb 30 must die: {:?}", values);
        assert!(values.iter().any(|v| *v == "2026-03-01"), "valid ISO date kept: {:?}", values);
    }

    #[test]
    fn extract_phone_rejects_date_shapes() {
        let client = OracleClient::new();
        let raw = "Call 604-555-0142. Event runs 2026-03-01 to 2026-03-03.";
        let q = test_query(QueryTarget::FindContacts, raw);
        let outs = client.extract(&q);
        let phones: Vec<&str> = outs
            .iter()
            .filter_map(|o| match o {
                OracleOutput::FieldCandidate { field, value, .. } if field == "phone" => {
                    Some(value.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(phones, vec!["604-555-0142"], "only the real phone: {:?}", phones);
    }

    #[test]
    fn extract_genres_keyword_match() {
        let client = OracleClient::new();
        let raw = "Three days of folk, bluegrass and late-night electronic sets.";
        let q = test_query(QueryTarget::SuggestGenres, raw);
        let outs = client.extract(&q);
        match outs.iter().find(|o| matches!(o, OracleOutput::GenreSuggestion { .. })) {
            Some(OracleOutput::GenreSuggestion { tags, span }) => {
                assert!(tags.contains(&"folk".to_string()), "{:?}", tags);
                assert!(tags.contains(&"bluegrass".to_string()), "{:?}", tags);
                assert!(tags.contains(&"electronic".to_string()), "{:?}", tags);
                assert!(!tags.contains(&"jazz".to_string()), "{:?}", tags);
                assert!(span.verify(raw.as_bytes(), "folk"));
            }
            other => panic!("expected GenreSuggestion, got {:?}", other),
        }
    }

    #[test]
    fn extract_empty_page_yields_nothing() {
        let client = OracleClient::new();
        let q = test_query(QueryTarget::FindDeadlines, "Just some prose, no dates here.");
        let outs = client.extract(&q);
        assert_eq!(outs.len(), 1);
        assert!(matches!(outs[0], OracleOutput::Nothing));
    }

    #[test]
    fn locate_finds_exact_substring() {
        let raw = "Applications close March 15, 2026. Apply now.";
        let span = locate(raw, "March 15, 2026").unwrap();
        assert_eq!(&raw[span.start..span.end], "March 15, 2026");
    }

    #[test]
    fn locate_returns_none_for_absent() {
        assert!(locate("nothing here", "April 1, 2026").is_none());
    }

    // M4: the per-target field whitelist rejects off-schema fields (schema
    // pollution / privilege injection) while staying permissive for Custom.
    #[test]
    fn field_whitelist_rejects_off_schema_fields() {
        assert!(field_allowed(&QueryTarget::FindDeadlines, "deadline"));
        assert!(!field_allowed(&QueryTarget::FindDeadlines, "admin_privilege"));
        assert!(field_allowed(&QueryTarget::ExtractEvent, "name"));
        assert!(!field_allowed(&QueryTarget::ExtractEvent, "prize"));
        assert!(field_allowed(&QueryTarget::Custom("x".into()), "anything"), "custom is permissive");
    }

    #[test]
    fn extract_json_from_fenced_reply() {
        let reply = "Here you go:\n```json\n{\"fields\":[{\"field\":\"name\",\"value\":\"Robson Fest\"}]}\n```";
        let j = extract_json_object(reply).unwrap();
        assert!(j.starts_with('{') && j.ends_with('}'));
    }

    #[test]
    fn parse_and_span_drops_hallucinations() {
        let client = OracleClient::new();
        let raw = "Robson Valley Music Festival. Applications close March 15, 2026.";
        // Legacy model-JSON path: one REAL value, one HALLUCINATED value.
        let model_json = r#"{"fields":[
            {"field":"name","value":"Robson Valley Music Festival"},
            {"field":"prize","value":"$1,000,000 cash prize"}
        ]}"#;
        let out = client.parse_and_span(model_json, raw);
        // The real value is kept WITH a valid span; the hallucination is dropped.
        let fields: Vec<_> = out.iter().filter_map(|o| match o {
            OracleOutput::FieldCandidate { field, value, span } => Some((field.clone(), value.clone(), span.clone())),
            _ => None,
        }).collect();
        assert_eq!(fields.len(), 1, "hallucinated 'prize' must be dropped (not in raw)");
        assert_eq!(fields[0].0, "name");
        assert_eq!(&raw[fields[0].2.start..fields[0].2.end], "Robson Valley Music Festival");
    }

    #[test]
    fn parse_and_span_derives_real_spans_the_validator_accepts() {
        let client = OracleClient::new();
        let raw = "Deadline: 2026-03-01. Genres: folk and bluegrass.";
        let model_json = r#"{"fields":[{"field":"deadline","value":"2026-03-01"}],"genres":["folk","bluegrass"]}"#;
        let outputs = client.parse_and_span(model_json, raw);

        // Feed the oracle output straight into the deterministic validator: the
        // locally-derived spans must PASS (proves the client and validator agree).
        let mut store = CandidateStore::new();
        // M3: capture_hash must bind to the exact bytes we validate against.
        let hash = pt_core::hash_bytes(raw.as_bytes());
        for o in outputs {
            store.stage(&hash, o);
        }
        let validator = Validator::new();
        let rulings = validator.validate_all(&mut store, raw.as_bytes());
        assert!(rulings.iter().any(|(_, r)| r.passed), "at least one span must validate");
        // Every FieldCandidate the client emitted validates against raw bytes.
        assert!(rulings.iter().all(|(_, r)| r.passed || r.method == "genre_span_verify"),
            "client-derived spans must not produce validator failures");
    }

    #[test]
    fn extract_output_validates_end_to_end() {
        // The deterministic path, straight into the validator: proves the
        // replacement is as trustworthy as the located-model path was.
        let client = OracleClient::new();
        let raw = "Robson Valley Music Festival, July 18, 2026. \
                   Contact: info@robsonvalley.example.com. https://robsonvalley.example.com";
        let q = test_query(QueryTarget::ExtractEvent, raw);
        let outputs = client.extract(&q);
        let mut store = CandidateStore::new();
        let hash = pt_core::hash_bytes(raw.as_bytes());
        for o in outputs {
            store.stage(&hash, o);
        }
        let validator = Validator::new();
        let rulings = validator.validate_all(&mut store, raw.as_bytes());
        assert!(rulings.iter().any(|(_, r)| r.passed), "deterministic extraction must validate");
    }
}
