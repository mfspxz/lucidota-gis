//! SCI_007 — Tortured Phrase Detector (offline analyzer, NON-HARVEST).
//!
//! Rust port of `SCI_007_tortured_phrases.py`. Detects "tortured phrases" —
//! mangled synonyms produced by paper mills using automated paraphrasing to evade
//! plagiarism detection (Cabanac/Labbé/Magazinov 2021 methodology).
//! e.g. "deep learning" -> "profound learning", "big data" -> "colossal data".
//!
//! This is NOT a scraper — it makes no HTTP calls. It implements `Scraper` so the
//! fleet can dispatch it uniformly, but it operates on text passed in `params`
//! (key "text", falling back to "query"). Archetype is tagged ApiJson only because
//! the trait needs a value; there is no network lane.

use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

/// (original technical term, tortured variant) pairs — the detection DB.
const TORTURED_PHRASE_DB: &[(&str, &str)] = &[
    ("deep learning", "profound learning"),
    ("deep learning", "deep adapting"),
    ("artificial intelligence", "counterfeit consciousness"),
    ("artificial intelligence", "manufactured intelligence"),
    ("machine learning", "apparatus learning"),
    ("machine learning", "device learning"),
    ("breast cancer", "bosom malignancy"),
    ("breast cancer", "chest disease"),
    ("random forest", "irregular woodland"),
    ("random forest", "arbitrary forest"),
    ("convolutional neural network", "convolution neural system"),
    ("recurrent neural network", "intermittent neural system"),
    ("support vector machine", "bolster vector machine"),
    ("support vector machine", "backing vector machine"),
    ("principal component analysis", "primary component examination"),
    ("big data", "enormous information"),
    ("big data", "colossal data"),
    ("cloud computing", "distributed cloud figuring"),
    ("Internet of Things", "Web of Things"),
    ("Internet of Things", "system of things"),
    ("feature extraction", "highlight extraction"),
    ("feature extraction", "element extraction"),
    ("transfer learning", "exchange learning"),
    ("natural language processing", "characteristic language handling"),
    ("gradient descent", "slope descent"),
    ("gradient descent", "inclination descent"),
    ("backpropagation", "rear spread"),
    ("neural network", "neural system"),
    ("decision tree", "choice tree"),
    ("logistic regression", "strategic relapse"),
    ("logistic regression", "calculated relapse"),
    ("time series", "time arrangement"),
    ("signal processing", "flag handling"),
    ("reinforcement learning", "fortification learning"),
    ("reinforcement learning", "support adapting"),
    ("genetic algorithm", "hereditary calculation"),
    ("fuzzy logic", "fluffy rationale"),
    ("blockchain", "square chain"),
    ("cryptocurrency", "digital money"),
    ("edge computing", "fringe figuring"),
    ("hidden layer", "shrouded layer"),
];

/// A single tortured-phrase match.
struct TorturedMatch {
    original: String,
    tortured: String,
    position: usize,
    context: String,
}

/// Case-insensitive whole-phrase scan (mirrors the Python `\b...\b` + IGNORECASE).
/// We approximate word-boundary by requiring the surrounding chars to be
/// non-alphanumeric (or string edge), which matches the regex `\b` for these
/// alphabetic phrases.
fn scan_text(text: &str) -> Vec<TorturedMatch> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let lower = text.to_lowercase();
    let bytes = lower.as_bytes();
    let mut matches = Vec::new();

    for (original, tortured) in TORTURED_PHRASE_DB {
        let needle = tortured.to_lowercase();
        let nlen = needle.len();
        let mut from = 0;
        while let Some(rel) = lower[from..].find(&needle) {
            let start = from + rel;
            let end = start + nlen;
            // Word-boundary check: char before/after must be non-alphanumeric.
            let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
            let after_ok = end >= bytes.len() || !bytes[end].is_ascii_alphanumeric();
            if before_ok && after_ok {
                let ctx_start = start.saturating_sub(40);
                let ctx_end = (end + 40).min(text.len());
                // Slice on the ORIGINAL text for readable context; guard char bounds.
                let context = text
                    .get(ctx_start..ctx_end)
                    .unwrap_or("")
                    .replace('\n', " ")
                    .trim()
                    .to_string();
                matches.push(TorturedMatch {
                    original: original.to_string(),
                    tortured: text.get(start..end).unwrap_or(tortured).to_string(),
                    position: start,
                    context,
                });
            }
            from = start + nlen;
            if from >= lower.len() {
                break;
            }
        }
    }
    matches.sort_by_key(|m| m.position);
    matches
}

/// Paper-mill probability heuristic (faithful to the Python).
fn mill_probability(match_count: usize, text_length: usize) -> f32 {
    if match_count == 0 {
        return 0.0;
    }
    let text_length = text_length.max(1);
    let mut base: f32 = match match_count {
        1 => {
            if text_length < 2000 {
                0.25
            } else {
                0.15
            }
        }
        2 => 0.55,
        3 => 0.80,
        4 => 0.90,
        _ => 0.95,
    };
    let density = (match_count as f32 / text_length as f32) * 1000.0;
    if density > 5.0 {
        base = (base + 0.05).min(1.0);
    }
    base
}

pub struct TorturedPhrases;

#[async_trait]
impl Scraper for TorturedPhrases {
    fn id(&self) -> &'static str {
        "SCI_007"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "SCI_007".to_string(),
            source_label: "Tortured Phrase Detector (offline analyzer)".to_string(),
            domain: Domain::AcademicIntegrity,
            archetype: Archetype::ApiJson, // no network lane; trait needs a value
            auth_required: false,
            auth_hint: None,
            description: "Offline paper-mill detector: scans text for tortured-phrase paraphrases."
                .to_string(),
        }
    }

    fn limits(&self) -> crate::controls::ScraperLimits {
        crate::controls::ScraperLimits {
            max_concurrency: None,
            min_delay_ms: None,
            max_results: None,
            max_pages: None,
            notes: "OFFLINE analyzer — no network. Pass text via params 'text' or 'query'."
                .to_string(),
        }
    }

    async fn run(&self, _ctx: &ScrapeCtx, params: ScrapeParams) -> anyhow::Result<ScrapeRecord> {
        let meta = self.meta();
        let mut record = ScrapeRecord::start(&meta, &params);

        // Accept text under "text" or fall back to "query".
        let text = params
            .0
            .get("text")
            .and_then(|v| v.as_str())
            .or_else(|| params.query())
            .unwrap_or("")
            .to_string();

        if text.trim().is_empty() {
            record.status = ScrapeStatus::Error("missing 'text' (or 'query') to analyze".to_string());
            return Ok(record);
        }

        let matches = scan_text(&text);
        let prob = mill_probability(matches.len(), text.len());

        for m in &matches {
            record.candidates.push(ScrapeCandidate::new(
                serde_json::json!({
                    "type": "tortured_phrase",
                    "original": m.original,
                    "tortured": m.tortured,
                    "position": m.position,
                    "context": m.context,
                }),
                // Confidence of THIS match ~ overall mill probability.
                prob,
            ));
        }

        record.notes = format!(
            "{} tortured phrase(s); paper-mill probability {:.2}",
            matches.len(),
            prob
        );
        record.status = if matches.is_empty() {
            ScrapeStatus::NotFound
        } else {
            ScrapeStatus::Ok
        };
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_known_tortured_phrase() {
        let m = scan_text("The profound learning model achieved colossal data throughput.");
        // "profound learning" + "colossal data" => 2 matches
        assert_eq!(m.len(), 2);
        assert!(m.iter().any(|x| x.original == "deep learning"));
        assert!(m.iter().any(|x| x.original == "big data"));
    }

    #[test]
    fn clean_text_no_matches() {
        let m = scan_text("The deep learning model processed big data efficiently.");
        assert_eq!(m.len(), 0, "real terms must NOT match tortured variants");
    }

    #[test]
    fn probability_scales_with_matches() {
        assert_eq!(mill_probability(0, 100), 0.0);
        assert!(mill_probability(3, 500) >= 0.80);
        assert!(mill_probability(5, 500) >= 0.95);
    }
}
