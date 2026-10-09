//! TIN_035 (id `TIN_035`) — YouTube results search (ytInitialData blob).
//!
//! Rust port of `TIN_035_youtube.py` (static-html archetype). Fetches the public
//! YouTube results page. YouTube embeds all result data in a `ytInitialData` JSON
//! blob inside a `<script>` tag (`var ytInitialData = {...};</script>`). The Python
//! regex-extracted that blob and walked
//! `contents.twoColumnSearchResultsRenderer.primaryContents.sectionListRenderer
//! .contents[0].itemSectionRenderer.contents` for videoRenderer / channelRenderer.
//!
//! This port captures the raw page (custody) and best-effort slices out the
//! `ytInitialData` blob as a single candidate. Deep videoRenderer/channelRenderer
//! walking is NOT reimplemented here (would need a regex/serde walk); the blob is
//! surfaced verbatim and the candidate is flagged as needing further extraction.
//! Marked UNKNOWN extraction depth, per task guidance — still compiles + runs.

use crate::controls::{HtmlConfig, LaneConfig, ScraperLimits};
use crate::lanes::html;
use crate::types::{
    Archetype, Domain, ScrapeCandidate, ScrapeCtx, ScrapeParams, ScrapeRecord, ScrapeStatus,
    Scraper, ScraperMeta,
};
use async_trait::async_trait;

const YT_SEARCH: &str = "https://www.youtube.com/results";
const YT_MARKER: &str = "var ytInitialData";

pub struct YouTube;

#[async_trait]
impl Scraper for YouTube {
    fn id(&self) -> &'static str {
        "TIN_035"
    }

    fn meta(&self) -> ScraperMeta {
        ScraperMeta {
            id: "TIN_035".to_string(),
            source_label: "YouTube (results page)".to_string(),
            domain: Domain::General,
            archetype: Archetype::StaticHtml,
            auth_required: false,
            auth_hint: None,
            description: "Search YouTube results; captures the ytInitialData JSON blob (needs deeper extraction)."
                .to_string(),
        }
    }

    fn primary_domain(&self) -> Option<&'static str> {
        Some("www.youtube.com")
    }

    fn limits(&self) -> ScraperLimits {
        ScraperLimits {
            max_concurrency: Some(2),
            min_delay_ms: Some(1000),
            max_results: None,
            max_pages: None,
            notes: "YouTube — public results HTML; data lives in ytInitialData JSON. May 403 or \
                    require JS rendering / API key for full parity."
                .to_string(),
        }
    }

    fn default_config(&self) -> LaneConfig {
        let selectors = std::collections::BTreeMap::new();
        let mut regex_fallback = std::collections::BTreeMap::new();
        // Documented for the operator: the blob delimiter used by the Python source.
        regex_fallback.insert(
            "yt_initial_data".to_string(),
            r"var ytInitialData\s*=\s*(\{.*?\});\s*</script>".to_string(),
        );
        LaneConfig::StaticHtml(HtmlConfig {
            url_template: "https://www.youtube.com/results?search_query={{query}}".to_string(),
            selectors,
            regex_fallback,
            encoding: None,
            auth: Default::default(),
        })
    }

    async fn run(&self, ctx: &ScrapeCtx, params: ScrapeParams) -> anyhow::Result<ScrapeRecord> {
        let meta = self.meta();
        let mut record = ScrapeRecord::start(&meta, &params);

        let query = match params.query() {
            Some(q) if !q.is_empty() => q.to_string(),
            _ => {
                record.status = ScrapeStatus::Error("missing 'query' param".to_string());
                return Ok(record);
            }
        };

        let url = format!("{}?search_query={}", YT_SEARCH, query.replace(' ', "+"));
        let fetch = html::get_html(ctx, &url).await?;
        record.raw_capture = Some(fetch.blob.hash);

        if fetch.status != 200 {
            record.status = match fetch.status {
                429 => ScrapeStatus::RateLimited,
                403 => ScrapeStatus::Blocked("YouTube blocking automated access (403)".to_string()),
                s => ScrapeStatus::Error(format!("HTTP {s}")),
            };
            return Ok(record);
        }

        // Best-effort slice of the ytInitialData JSON blob (no regex dep).
        // Find `var ytInitialData`, then the first `{` after it, then the matching
        // `};</script>` terminator the Python source used.
        let blob = extract_yt_initial_data(&fetch.body);
        match blob {
            Some(json_str) => {
                let data = serde_json::json!({
                    "type": "yt_initial_data",
                    "query": query,
                    "extraction": "UNKNOWN — raw ytInitialData blob captured; videoRenderer/\
                                   channelRenderer walk not reimplemented (needs JSON walk)",
                    "blob_len": json_str.len(),
                    "blob": json_str,
                });
                record.candidates.push(ScrapeCandidate::new(data, 0.5));
                record.notes =
                    "captured ytInitialData blob (UNKNOWN extraction depth — needs JSON walk)"
                        .to_string();
                record.status = ScrapeStatus::Ok;
            }
            None => {
                record.notes =
                    "no ytInitialData blob found (YouTube may require JS rendering)".to_string();
                record.status = ScrapeStatus::NotFound;
            }
        }
        Ok(record)
    }
}

/// Best-effort extraction of the `ytInitialData` JSON object using plain string
/// scanning (no regex/serde dep). Returns the brace-balanced JSON substring, or
/// None if the marker/blob can't be located.
fn extract_yt_initial_data(body: &str) -> Option<String> {
    let marker_pos = body.find(YT_MARKER)?;
    // Find the first '{' after the marker.
    let rest = &body[marker_pos..];
    let brace_rel = rest.find('{')?;
    let start = marker_pos + brace_rel;
    let bytes = body.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    let mut i = start;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
        } else {
            match c {
                '"' => in_str = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(body[start..=i].to_string());
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}
