//! Oracle client — talks to an OpenAI-compatible LLM endpoint (llama-server,
//! Mistral/Devstral, Leanstral — all speak /v1/chat/completions).
//!
//! THE LOAD-BEARING CONTRACT: an LLM cannot reliably count bytes. So we do NOT
//! trust it to produce byte spans. The model returns field→value pairs (plain
//! JSON); THIS CLIENT re-derives every span LOCALLY by locating the value inside
//! the raw capture. If a claimed value cannot be found in the raw bytes, it is
//! DROPPED — no span, no candidate, no promotion. That is the anti-fabrication law
//! in code: the model points at *what*, the raw bytes decide *where*, and a claim
//! the bytes don't support never enters the pipeline. A hallucinated value simply
//! fails to locate and vanishes.

use crate::{ByteSpan, OracleOutput, OracleQuery, QueryTarget};
use serde::Deserialize;

/// The oracle client. POSTs to an OpenAI-compatible chat endpoint and parses the
/// reply into byte-spanned OracleOutputs (spans re-derived locally, never trusted).
pub struct OracleClient {
    endpoint: String,
    api_key: Option<String>,
    model: String,
    http: reqwest::Client,
}

/// Minimal shape of an OpenAI-compatible chat completion response.
#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}
#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}
#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

/// What we ask the model to return: a flat list of {field,value} + optional
/// url/genre lists. No byte offsets requested — we compute those ourselves.
#[derive(Deserialize, Default)]
struct ModelExtraction {
    #[serde(default)]
    fields: Vec<ModelField>,
    #[serde(default)]
    urls: Vec<String>,
    #[serde(default)]
    genres: Vec<String>,
}
#[derive(Deserialize)]
struct ModelField {
    field: String,
    value: String,
}

impl OracleClient {
    /// New client. `endpoint` is the base (e.g. "http://localhost:8080" or
    /// "https://api.mistral.ai"). `api_key` optional (local servers need none).
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            api_key: None,
            model: "default".to_string(),
            // M6: bare `Client::new()` has NO request timeout (a hung endpoint
            // blocks query() forever) and follows up to 10 redirects. Bound both.
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::limited(3))
                .build()
                .expect("reqwest client builder is infallible with these options"),
        }
    }

    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Convenience: build from env (MISTRAL_API_KEY / HERMES_LLM_* / local default).
    /// Returns None if no usable endpoint+key can be found (honest — no fake client).
    pub fn from_env() -> Option<Self> {
        // Prefer an explicit Hermes LLM config, then Mistral, then local llama.
        if let (Ok(base), Ok(key)) = (
            std::env::var("HERMES_LLM_BASE_URL"),
            std::env::var("HERMES_LLM_API_KEY"),
        ) {
            let model = std::env::var("HERMES_LLM_MODEL").unwrap_or_else(|_| "default".into());
            return Some(Self::new(base).with_api_key(key).with_model(model));
        }
        if let Ok(key) = std::env::var("MISTRAL_API_KEY") {
            return Some(
                Self::new("https://api.mistral.ai")
                    .with_api_key(key)
                    .with_model("devstral-2512"),
            );
        }
        None
    }

    /// Query the oracle. Returns byte-spanned candidates; spans re-derived locally.
    pub async fn query(&self, query: &OracleQuery) -> anyhow::Result<Vec<OracleOutput>> {
        let prompt = self.build_prompt(query);
        let url = format!("{}/v1/chat/completions", self.endpoint);

        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content":
                    "You are an extraction oracle. Return ONLY compact JSON: \
                     {\"fields\":[{\"field\":\"name\",\"value\":\"...\"}],\"urls\":[\"...\"],\"genres\":[\"...\"]}. \
                     Copy values VERBATIM from the page text (exact substrings). \
                     Do not paraphrase, do not invent. Omit anything not present."},
                {"role": "user", "content": prompt}
            ],
            "temperature": 0.0,
            "response_format": {"type": "json_object"}
        });

        let mut req = self.http.post(&url).json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("oracle endpoint {status}: {}", text.chars().take(200).collect::<String>());
        }
        let parsed: ChatResponse = resp.json().await?;
        let content = parsed
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        // M4: drop any FieldCandidate whose field name is not on this query
        // target's allow-list — a model (or a prompt-injected page) can emit
        // {"field":"admin_privilege",...}; without a whitelist it would stage and
        // pollute the truth layer. Custom targets stay permissive (operator's own).
        let mut outs = self.parse_and_span(&content, &query.raw_text);
        outs.retain(|o| match o {
            OracleOutput::FieldCandidate { field, .. } => field_allowed(&query.target, field),
            _ => true,
        });
        if outs.is_empty() {
            outs.push(OracleOutput::Nothing);
        }
        Ok(outs)
    }

    /// Parse the model's JSON and re-derive byte spans against the RAW text.
    /// Any value not found verbatim in raw is DROPPED (no span → no candidate).
    /// This is the anti-fabrication gate: the model can't smuggle a claim past the
    /// bytes. Public for offline testing without a live endpoint.
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

    /// Build a prompt from the query target.
    pub fn build_prompt(&self, query: &OracleQuery) -> String {
        match &query.target {
            QueryTarget::ExtractEvent => format!(
                "Extract festival/event information from this page. \
                 Return fields: name, dates, application_opens, application_closes, \
                 location, contact_email, website. Copy values verbatim.\n\n{}",
                query.raw_text
            ),
            QueryTarget::FindDeadlines => format!(
                "Find application deadlines. Return field 'deadline' with the date \
                 copied verbatim.\n\n{}",
                query.raw_text
            ),
            QueryTarget::FindContacts => format!(
                "Extract contact info. Return fields: contact_email, phone, form_url. \
                 Verbatim.\n\n{}",
                query.raw_text
            ),
            QueryTarget::SuggestGenres => format!(
                "Suggest genre tags from canonical set (folk, bluegrass, blues, rock, \
                 country, americana, roots, singer-songwriter, punk, electronic, edm, \
                 jazz, hip-hop, rnb, classical, world). Return 'genres' array.\n\n{}",
                query.raw_text
            ),
            QueryTarget::FindUrls => format!(
                "Find URLs of related festivals/application pages. Return 'urls' \
                 array with full http(s) URLs copied verbatim.\n\n{}",
                query.raw_text
            ),
            QueryTarget::Custom(prompt) => prompt.clone(),
        }
    }
}

/// The allow-list of field names each query target may legitimately produce.
/// `None` = no whitelist (Custom targets carry the operator's own prompt).
fn allowed_fields(target: &QueryTarget) -> Option<&'static [&'static str]> {
    match target {
        QueryTarget::ExtractEvent => Some(&[
            "name", "dates", "application_opens", "application_closes",
            "location", "contact_email", "website",
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
        let client = OracleClient::new("http://unused");
        let raw = "Robson Valley Music Festival. Applications close March 15, 2026.";
        // Model returns one REAL value and one HALLUCINATED value.
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
        use crate::validator::Validator;
        use crate::candidate::CandidateStore;
        let client = OracleClient::new("http://unused");
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
}
