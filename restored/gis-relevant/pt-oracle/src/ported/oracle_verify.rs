// PORTED-FROM: /home/mfspx/LUCIDOTA/scripts/oracle_verify.py
//
// oracle_verify.rs — THE ORACLE TOUCHES ALL, applied to research claims.
//
// Parses research .md files for (quote, URL) pairs, FETCHES each URL, and
// re-derives the quote as a span in the fetched source. FIVE-STATE verdict:
//
//   PROVEN_VERBATIM  (+1) : quote is a RAW byte substring of the fetched source
//   PROVEN_NORMALIZED(+1) : quote matches only after norm() (lowercase/strip-tags/
//                           collapse-ws/unify-quotes) — honest: this is normalized-
//                           span re-derivation, NOT byte-verbatim
//   DISPROVEN        (-1) : source fetched OK, quote genuinely ABSENT (real refutation)
//   UNVERIFIED        (0) : could NOT fetch/read the source (network fail, non-2xx,
//                           bot-wall, JS challenge) — CANNOT re-derive, so Unknown
//
// DOCTRINE: DISPROVEN is ABSORBING (a Disproven can't be demoted, later proof can't
// resurrect it). Therefore a fetch failure / wall MUST NEVER mint DISPROVEN — it is
// UNVERIFIED(0) so the claim stays re-sourceable. Only a successful fetch with the
// quote truly absent earns -1.
//
// The normalization split exists because norm() widens matching beyond raw bytes;
// letting that silently ride under a "verbatim" label would inflate what PROVEN means.
//
// Integrates with pt-oracle::ByteSpan for span tracking and OracleOutput for candidate
// promotion gating.

use regex::Regex;
use serde::{Deserialize, Serialize};

// ── Dependencies (declare; not build-time) ──────────────────────────
// - regex: claim pattern parsing
// - serde / serde_json: verdict report serialization
// - ureq or reqwest: HTTP fetching (ureq preferred — no async runtime needed)
//   use ureq::Agent;  Agent::new().get(url).set("User-Agent", UA).call()
// - html-escape or htmlescape: html::unescape equivalent

// ── Five-State Verdict ───────────────────────────────────────────────

/// The five-state epistemic verdict for a research claim.
/// Trit values match the Python oracle: +1 proven, 0 unknown, -1 disproven.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// Quote is a raw byte substring of the fetched source. Trit = +1.
    ProvenVerbatim,
    /// Quote matches only after normalization. Trit = +1 (weaker tier, honestly named).
    ProvenNormalized,
    /// Source fetched OK, quote genuinely absent — real refutation. Trit = -1.
    /// ABSORBING: once disproven, cannot be resurrected by later proof.
    Disproven,
    /// Could not fetch/read the source. Trit = 0. NOT disproven — stays re-sourceable.
    Unverified,
}

impl Verdict {
    /// Epistemic trit value: +1 for proven, 0 for unknown, -1 for disproven.
    pub fn trit(&self) -> i8 {
        match self {
            Self::ProvenVerbatim => 1,
            Self::ProvenNormalized => 1,
            Self::Disproven => -1,
            Self::Unverified => 0,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ProvenVerbatim => "PROVEN_VERBATIM",
            Self::ProvenNormalized => "PROVEN_NORMALIZED",
            Self::Disproven => "DISPROVEN",
            Self::Unverified => "UNVERIFIED",
        }
    }
}

// ── Byte Span (matches pt-oracle::ByteSpan; re-declared for standalone draft) ──

/// Byte range into the raw capture proving the claim exists.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

// ── Claim ────────────────────────────────────────────────────────────

/// A parsed research claim: a quote and the URL it claims to source from.
#[derive(Debug, Clone)]
pub struct Claim {
    pub quote: String,
    pub url: String,
    /// Source file the claim was parsed from.
    pub source_file: String,
}

// ── Verdict Entry ────────────────────────────────────────────────────

/// A single claim's verification result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerdictEntry {
    pub file: String,
    pub verdict: String,
    pub trit: i8,
    pub url: String,
    pub quote: String,
    pub span: Option<ByteSpan>,
}

// ── Fetch Status ─────────────────────────────────────────────────────

/// Status returned by fetch_url.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchStatus {
    /// 2xx, real content, safe to search.
    Ok,
    /// Network error / timeout / non-2xx / bot-wall / JS challenge.
    FetchFail,
    /// JS-rendered SPA shell — server HTML may or may not contain the quote.
    SpaShell,
}

// ── Fetch Result ─────────────────────────────────────────────────────

pub struct FetchResult {
    pub body: String,
    pub status: FetchStatus,
}

// ── Constants ────────────────────────────────────────────────────────

const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) LUCIDOTA-oracle/1.0";
const FETCH_TIMEOUT_SECS: u64 = 25;

/// SPA hosts whose server HTML rarely contains content verbatim.
/// Reddit, Twitter/X, Instagram, Facebook, Threads.
const SPA_HOSTS: &[&str] = &[
    "reddit.com", "www.reddit.com", "old.reddit.com",
    "twitter.com", "x.com",
    "instagram.com", "facebook.com", "threads.net",
];

// ── Text Normalization ───────────────────────────────────────────────
// PORTED-FROM: /home/mfspx/LUCIDOTA/scripts/oracle_verify.py:norm()

/// Normalize text for byte-span re-derivation: strip HTML tags, unify
/// smart quotes/dashes/nbsp to ASCII, collapse whitespace, lowercase.
pub fn normalize_text(s: &str) -> String {
    // HTML unescape (approx — full entity decode needs a proper lib)
    let s = html_unescape_fast(s);

    // Unify smart quotes / dashes / nbsp to ASCII
    let s = s
        .replace('\u{201c}', "\"") // left double quote
        .replace('\u{201d}', "\"") // right double quote
        .replace('\u{2018}', "'")  // left single quote
        .replace('\u{2019}', "'")  // right single quote
        .replace('\u{2013}', "-")  // en dash
        .replace('\u{2014}', "-")  // em dash
        .replace('\u{00a0}', " "); // non-breaking space

    // Strip HTML tags: <...> → space
    let re_tags = Regex::new(r"<[^>]+>").unwrap();
    let s = re_tags.replace_all(&s, " ");

    // Collapse whitespace
    let re_ws = Regex::new(r"\s+").unwrap();
    let s = re_ws.replace_all(&s, " ");

    s.trim().to_lowercase()
}

/// Fast HTML unescape covering the most common entities.
fn html_unescape_fast(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&#x27;", "'")
        .replace("&#x2F;", "/")
        .replace("&#x60;", "`")
        .replace("&#x3D;", "=")
        .replace("&nbsp;", " ")
}

// ── URL Fetching ─────────────────────────────────────────────────────
// PORTED-FROM: /home/mfspx/LUCIDOTA/scripts/oracle_verify.py:fetch()

/// Fetch a URL and return (body, status).
///
/// A bot-wall or 403 returns FetchFail → UNVERIFIED (trit 0), NEVER DISPROVEN.
/// Disproven is absorbing: a claim we simply couldn't read must stay Unknown
/// so it's re-sourceable later.
///
/// NOTE: This is a DRAFT — real implementation uses `ureq` or `reqwest::blocking`.
/// The `http_get_body` stub must be replaced with an actual HTTP client.
pub fn fetch_url(url: &str) -> FetchResult {
    // ── STUB: replace with real HTTP fetch ──
    // In production:
    //   let resp = ureq::Agent::new()
    //       .get(url)
    //       .set("User-Agent", UA)
    //       .timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS))
    //       .call();
    //   match resp {
    //       Ok(r) if r.status() >= 200 && r.status() < 300 => { ... }
    //       Ok(_) => return FetchResult { body: String::new(), status: FetchStatus::FetchFail },
    //       Err(_) => return FetchResult { body: String::new(), status: FetchStatus::FetchFail },
    //   }
    let (body, http_status) = http_get_body(url);

    if http_status == 0 || http_status < 200 || http_status >= 300 {
        return FetchResult {
            body: String::new(),
            status: FetchStatus::FetchFail,
        };
    }

    // Bot-wall / challenge heuristics — a wall is a fetch failure, not a source.
    // H3: slice on the largest CHAR BOUNDARY <= 4000, never a raw byte index —
    // an attacker page with a multibyte char straddling byte 4000 would panic a
    // `body[..4000]` slice and crash the verifier.
    let cap = body.len().min(4000);
    let end = (0..=cap).rev().find(|&i| body.is_char_boundary(i)).unwrap_or(0);
    let low = body[..end].to_lowercase();
    let walls = &[
        "just a moment",
        "enable javascript and cookies",
        "cf-challenge",
        "attention required",
        "verifying you are human",
        "captcha",
        "access denied",
        "request blocked",
        "are you a robot",
        "whoa there",
        "network security",
        "blocked by network security",
    ];

    if body.len() < 1500 || walls.iter().any(|w| low.contains(w)) {
        return FetchResult {
            body: String::new(),
            status: FetchStatus::FetchFail,
        };
    }

    // JS-rendered SPA shells — return body but mark as spa_shell
    let host = extract_host(url).to_lowercase();
    if SPA_HOSTS.iter().any(|h| host == *h || host.ends_with(&format!(".{}", h))) {
        return FetchResult {
            body,
            status: FetchStatus::SpaShell,
        };
    }

    FetchResult {
        body,
        status: FetchStatus::Ok,
    }
}

/// Extract the host portion from a URL (between scheme and first '/').
fn extract_host(url: &str) -> &str {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("")
}

/// STUB: replace with actual HTTP client (ureq or reqwest::blocking).
/// Returns (body, http_status). http_status=0 means network error.
fn http_get_body(_url: &str) -> (String, u16) {
    // TODO: real HTTP fetch
    // ureq::get(url).set("User-Agent", UA).timeout(Duration::from_secs(25)).call()
    //   - 2xx: decode body as UTF-8 (fallback latin-1)
    //   - non-2xx / error: return (String::new(), 0)
    (String::new(), 0)
}

// ── Claim Parsing ────────────────────────────────────────────────────
// PORTED-FROM: /home/mfspx/LUCIDOTA/scripts/oracle_verify.py:CLAIM_RE + CLAIM_RE2

/// Claim pattern 1: **Source quote (verbatim...):** "..."\n**URL:** <url>
const CLAIM_PATTERN_1: &str = concat!(
    r"Source quote \(verbatim[^)]*\):\*\*\s*[",
    "\"\u{201c}",
    r"](.+?)[",
    "\"\u{201d}",
    r"]\s*\n\*\*URL:\*\*\s*(\S+)",
);

/// Claim pattern 2: **Source quote:** "..." — https://url (em-dash separator)
/// or URL on next line.
const CLAIM_PATTERN_2: &str = concat!(
    r"Source quote:?\*\*\s*[",
    "\"\u{201c}",
    r"](.+?)[",
    "\"\u{201d}",
    r"]\s*(?:[",
    "\u{2014}",
    r"\-]{1,2}|\n[^\n]*?)\s*(https?://\S+)",
);

/// Parse research .md file for (quote, URL) claim pairs.
/// Returns deduplicated claims (by (first 40 chars of quote, URL)).
pub fn parse_claims(file_path: &str, source_file: &str) -> Vec<Claim> {
    let Ok(text) = std::fs::read_to_string(file_path) else {
        return Vec::new();
    };

    let mut claims = Vec::new();
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();

    let re1 = Regex::new(CLAIM_PATTERN_1).expect("CLAIM_PATTERN_1 invalid regex");
    let re2 = Regex::new(CLAIM_PATTERN_2).expect("CLAIM_PATTERN_2 invalid regex");

    for re in [&re1, &re2] {
        for cap in re.captures_iter(&text) {
            let quote = cap.get(1).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
            let url = cap
                .get(2)
                .map(|m| m.as_str().trim().trim_end_matches(|c: char| c == ')' || c == '.' || c == ','))
                .unwrap_or("")
                .to_string();

            if quote.is_empty() || url.is_empty() {
                continue;
            }

            // Deduplicate by (first 40 chars of quote, URL) — matches Python behavior
            let key = (quote.chars().take(40).collect::<String>(), url.clone());
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key);

            claims.push(Claim {
                quote,
                url,
                source_file: source_file.to_string(),
            });
        }
    }

    claims
}

// ── Verification Engine ──────────────────────────────────────────────
// PORTED-FROM: /home/mfspx/LUCIDOTA/scripts/oracle_verify.py:verify()

/// Verify a single claim against its fetched source.
///
/// Returns (verdict, span). Span is the byte range in the fetched body
/// where the quote was re-derived (or in the normalized body for
/// PROVEN_NORMALIZED).
pub fn verify_claim(claim: &Claim, fetch_result: &FetchResult) -> (Verdict, Option<ByteSpan>) {
    let (ref body, ref status) = (fetch_result.body.as_str(), &fetch_result.status);

    match status {
        FetchStatus::FetchFail => {
            // COULD NOT READ SOURCE → Unknown. NEVER Disproven (absorbing rule).
            (Verdict::Unverified, None)
        }
        FetchStatus::SpaShell => {
            // JS shell: quote MIGHT be present in server HTML; if so, proven.
            // If absent, can't fault the claim on a shell → UNVERIFIED, not -1.
            if let Some(idx) = body.find(&claim.quote) {
                (
                    Verdict::ProvenVerbatim,
                    Some(ByteSpan {
                        start: idx,
                        end: idx + claim.quote.len(),
                    }),
                )
            } else {
                let nq = normalize_text(&claim.quote);
                let nb = normalize_text(body);
                if let Some(idx) = nb.find(&nq) {
                    (
                        Verdict::ProvenNormalized,
                        Some(ByteSpan {
                            start: idx,
                            end: idx + nq.len(),
                        }),
                    )
                } else {
                    (Verdict::Unverified, None)
                }
            }
        }
        FetchStatus::Ok => {
            // Raw byte substring — true verbatim re-derivation
            if let Some(idx) = body.find(&claim.quote) {
                (
                    Verdict::ProvenVerbatim,
                    Some(ByteSpan {
                        start: idx,
                        end: idx + claim.quote.len(),
                    }),
                )
            } else {
                let nq = normalize_text(&claim.quote);
                let nb = normalize_text(body);
                if nb.contains(&nq) {
                    // Matches only after normalization — a weaker tier. M5: do NOT
                    // emit a byte span here. The match index is into the transient
                    // NORMALIZED string (HTML-stripped, whitespace-collapsed); those
                    // offsets do not map back to the raw capture, so a span would be
                    // false provenance. The verdict stands; the span is None.
                    (Verdict::ProvenNormalized, None)
                } else {
                    // Source read fine, quote genuinely absent → real refutation
                    (Verdict::Disproven, None)
                }
            }
        }
    }
}

/// Tally of verdict counts.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct VerdictTally {
    pub proven_verbatim: usize,
    pub proven_normalized: usize,
    pub disproven: usize,
    pub unverified: usize,
}

impl VerdictTally {
    pub fn total(&self) -> usize {
        self.proven_verbatim + self.proven_normalized + self.disproven + self.unverified
    }

    pub fn total_proven(&self) -> usize {
        self.proven_verbatim + self.proven_normalized
    }
}

// ── Batch Verification ───────────────────────────────────────────────
// PORTED-FROM: /home/mfspx/LUCIDOTA/scripts/oracle_verify.py:verify() main loop

/// Run the full oracle verification pipeline over a set of research files.
///
/// Files: list of (file_path, display_name) tuples.
/// Returns (entries, tally).
pub fn verify_files(
    research_dir: &str,
    file_names: &[&str],
) -> (Vec<VerdictEntry>, VerdictTally) {
    let mut cache: std::collections::HashMap<String, FetchResult> = std::collections::HashMap::new();
    let mut entries: Vec<VerdictEntry> = Vec::new();
    let mut tally = VerdictTally::default();

    // Rate-limit delay between fetches (matches Python's time.sleep(0.5))
    let delay = std::time::Duration::from_millis(500);

    for &fname in file_names {
        let file_path = format!("{}/{}", research_dir, fname);
        let meta = match std::fs::metadata(&file_path) {
            Ok(m) if m.is_file() => m,
            _ => continue,
        };
        // drop meta to avoid unused warning
        let _ = meta;

        let claims = parse_claims(&file_path, fname);

        for claim in &claims {
            // Cache fetch results by URL
            let fetch_result = if let Some(fr) = cache.get(&claim.url) {
                fr
            } else {
                // Simulate rate-limited fetch
                std::thread::sleep(delay);
                let fr = fetch_url(&claim.url);
                cache.insert(claim.url.clone(), fr);
                // Re-borrow from cache
                cache.get(&claim.url).unwrap()
            };

            let (verdict, span) = verify_claim(claim, fetch_result);

            // Update tally
            match verdict {
                Verdict::ProvenVerbatim => tally.proven_verbatim += 1,
                Verdict::ProvenNormalized => tally.proven_normalized += 1,
                Verdict::Disproven => tally.disproven += 1,
                Verdict::Unverified => tally.unverified += 1,
            }

            entries.push(VerdictEntry {
                file: claim.source_file.clone(),
                verdict: verdict.as_str().to_string(),
                trit: verdict.trit(),
                url: claim.url.clone(),
                quote: claim.quote.chars().take(80).collect(),
                span,
            });
        }
    }

    (entries, tally)
}

/// Generate a markdown report string from verification results.
pub fn render_markdown_report(entries: &[VerdictEntry], tally: &VerdictTally) -> String {
    let mut out = String::new();
    out.push_str("# ORACLE VERDICTS — research claim span re-derivation\n\n");

    // Timestamp
    if let Ok(ts) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        out.push_str(&format!(
            "Generated {}.\n\n",
            chrono_strftime(ts.as_secs())
        ));
    }

    out.push_str(
        "Five-state gate. **DISPROVEN is absorbing**, so a source we could not\n",
    );
    out.push_str(
        "fetch/read is **UNVERIFIED (0)**, never DISPROVEN — it stays re-sourceable.\n",
    );
    out.push_str(
        "PROVEN is split into VERBATIM (raw byte substring) vs NORMALIZED\n",
    );
    out.push_str(
        "(matches only after lowercase/strip-tags/collapse-whitespace) so the\n",
    );
    out.push_str("standard is exactly what it claims.\n\n");

    let total_proven = tally.total_proven();
    out.push_str(&format!(
        "**Tally:** +1 PROVEN_VERBATIM={} · +1 PROVEN_NORMALIZED={} · \
         -1 DISPROVEN(fetched, quote absent)={} · 0 UNVERIFIED(could-not-read)={}  \
         (total proven={})\n\n",
        tally.proven_verbatim,
        tally.proven_normalized,
        tally.disproven,
        tally.unverified,
        total_proven,
    ));

    // Table header
    out.push_str("| verdict | trit | file | quote | url | span |\n");
    out.push_str("|---|---:|---|---|---|---|\n");

    for e in entries {
        let span_str = match &e.span {
            Some(s) => format!("{}:{}", s.start, s.end),
            None => "-".to_string(),
        };
        out.push_str(&format!(
            "| {} | {:+#} | {} | {} | {} | {} |\n",
            e.verdict, e.trit, e.file, e.quote, e.url, span_str
        ));
    }

    out
}

/// Format a unix timestamp as YYYY-MM-DDThh:mm:ssZ (no external crate needed for draft).
fn chrono_strftime(unix_secs: u64) -> String {
    // Approximate: seconds since epoch → date parts
    let secs_per_day: u64 = 86400;
    let days_since_epoch = unix_secs / secs_per_day;
    let rem_secs = unix_secs % secs_per_day;

    // Compute year/month/day from days since 1970-01-01
    let (year, month, day) = days_to_ymd(days_since_epoch);

    let hours = (rem_secs / 3600) % 24;
    let minutes = (rem_secs / 60) % 60;
    let seconds = rem_secs % 60;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Days in each month (non-leap)
    static MONTH_DAYS: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    let mut year: u64 = 1970;
    loop {
        let diy = if is_leap(year) { 366 } else { 365 };
        if days < diy {
            break;
        }
        days -= diy;
        year += 1;
    }

    let mut month: u64 = 1;
    for &md in MONTH_DAYS.iter() {
        let mut md = md;
        if month == 2 && is_leap(year) {
            md = 29;
        }
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }

    (year, month, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── normalize_text tests ───────────────────────────────────────

    #[test]
    fn test_normalize_collapses_whitespace() {
        let input = "Hello   World\n\tExtra";
        let out = normalize_text(input);
        assert_eq!(out, "hello world extra");
    }

    #[test]
    fn test_normalize_strips_html_tags() {
        let input = "<p>Hello <b>World</b></p>";
        let out = normalize_text(input);
        assert_eq!(out, "hello world");
    }

    #[test]
    fn test_normalize_smart_quotes() {
        let input = "\u{201c}hello\u{201d} \u{2018}world\u{2019}";
        let out = normalize_text(input);
        assert_eq!(out, "\"hello\" 'world'");
    }

    #[test]
    fn test_normalize_em_dash() {
        let input = "foo\u{2014}bar\u{2013}baz";
        let out = normalize_text(input);
        assert_eq!(out, "foo-bar-baz");
    }

    #[test]
    fn test_html_unescape_fast() {
        assert_eq!(html_unescape_fast("&amp;"), "&");
        assert_eq!(html_unescape_fast("&lt;tag&gt;"), "<tag>");
        assert_eq!(html_unescape_fast("&quot;hi&quot;"), "\"hi\"");
        assert_eq!(html_unescape_fast("&nbsp;"), " ");
    }

    // ── extract_host tests ─────────────────────────────────────────

    #[test]
    fn test_extract_host_simple() {
        assert_eq!(extract_host("https://example.com/path"), "example.com");
    }

    #[test]
    fn test_extract_host_no_path() {
        assert_eq!(extract_host("http://example.com"), "example.com");
    }

    #[test]
    fn test_extract_host_subdomain() {
        assert_eq!(extract_host("https://www.example.com/foo"), "www.example.com");
    }

    // ── verdict tests ──────────────────────────────────────────────

    #[test]
    fn test_verdict_trit_values() {
        assert_eq!(Verdict::ProvenVerbatim.trit(), 1);
        assert_eq!(Verdict::ProvenNormalized.trit(), 1);
        assert_eq!(Verdict::Disproven.trit(), -1);
        assert_eq!(Verdict::Unverified.trit(), 0);
    }

    #[test]
    fn test_verdict_as_str() {
        assert_eq!(Verdict::ProvenVerbatim.as_str(), "PROVEN_VERBATIM");
        assert_eq!(Verdict::ProvenNormalized.as_str(), "PROVEN_NORMALIZED");
        assert_eq!(Verdict::Disproven.as_str(), "DISPROVEN");
        assert_eq!(Verdict::Unverified.as_str(), "UNVERIFIED");
    }

    // ── verify_claim tests — FetchStatus::Ok path ──────────────────

    #[test]
    fn test_verify_claim_verbatim() {
        let claim = Claim {
            quote: "March 15, 2026".into(),
            url: "https://example.com".into(),
            source_file: "test.md".into(),
        };
        let fr = FetchResult {
            body: "Applications close March 15, 2026. Late submissions not accepted.".into(),
            status: FetchStatus::Ok,
        };
        let (verdict, span) = verify_claim(&claim, &fr);
        assert_eq!(verdict, Verdict::ProvenVerbatim);
        assert!(span.is_some());
        let s = span.unwrap();
        assert_eq!(&fr.body[s.start..s.end], "March 15, 2026");
    }

    #[test]
    fn test_verify_claim_normalized() {
        let claim = Claim {
            quote: "March 15, 2026".into(),
            url: "https://example.com".into(),
            source_file: "test.md".into(),
        };
        // Source has extra whitespace and uppercase — raw match fails, normalized should pass
        let fr = FetchResult {
            body: "Applications close  MARCH  15,  2026.  Late submissions not accepted.".into(),
            status: FetchStatus::Ok,
        };
        let (verdict, _span) = verify_claim(&claim, &fr);
        assert_eq!(verdict, Verdict::ProvenNormalized);
    }

    #[test]
    fn test_verify_claim_disproven() {
        let claim = Claim {
            quote: "nonexistent quote 12345".into(),
            url: "https://example.com".into(),
            source_file: "test.md".into(),
        };
        let fr = FetchResult {
            body: "This page contains real content but not that quote.".into(),
            status: FetchStatus::Ok,
        };
        let (verdict, span) = verify_claim(&claim, &fr);
        assert_eq!(verdict, Verdict::Disproven);
        assert!(span.is_none());
    }

    // ── verify_claim tests — FetchStatus::FetchFail absorbs to Unverified ──

    #[test]
    fn test_verify_claim_fetch_fail_is_unverified_not_disproven() {
        let claim = Claim {
            quote: "anything".into(),
            url: "https://example.com".into(),
            source_file: "test.md".into(),
        };
        let fr = FetchResult {
            body: String::new(),
            status: FetchStatus::FetchFail,
        };
        let (verdict, span) = verify_claim(&claim, &fr);
        assert_eq!(verdict, Verdict::Unverified, "fetch_fail must be UNVERIFIED, never DISPROVEN (absorbing rule)");
        assert!(span.is_none());
    }

    // ── verify_claim tests — FetchStatus::SpaShell ─────────────────

    #[test]
    fn test_verify_claim_spa_shell_verbatim() {
        let claim = Claim {
            quote: "real content in shell".into(),
            url: "https://reddit.com/r/test".into(),
            source_file: "test.md".into(),
        };
        let fr = FetchResult {
            body: "<script>app.boot()</script>some real content in shell here".into(),
            status: FetchStatus::SpaShell,
        };
        let (verdict, span) = verify_claim(&claim, &fr);
        assert_eq!(verdict, Verdict::ProvenVerbatim);
        assert!(span.is_some());
    }

    #[test]
    fn test_verify_claim_spa_shell_absent_stays_unverified() {
        let claim = Claim {
            quote: "missing from shell".into(),
            url: "https://reddit.com/r/test".into(),
            source_file: "test.md".into(),
        };
        let fr = FetchResult {
            body: "<script>app.boot()</script><div id='root'></div>".into(),
            status: FetchStatus::SpaShell,
        };
        let (verdict, span) = verify_claim(&claim, &fr);
        // Quote absent from SPA shell → UNVERIFIED, NOT Disproven (absorbing rule)
        assert_eq!(verdict, Verdict::Unverified);
        assert!(span.is_none());
    }

    // ── tally tests ────────────────────────────────────────────────

    #[test]
    fn test_verdict_tally_defaults() {
        let t = VerdictTally::default();
        assert_eq!(t.total(), 0);
        assert_eq!(t.total_proven(), 0);
    }

    #[test]
    fn test_verdict_tally_counts() {
        let t = VerdictTally {
            proven_verbatim: 5,
            proven_normalized: 3,
            disproven: 1,
            unverified: 4,
        };
        assert_eq!(t.total(), 13);
        assert_eq!(t.total_proven(), 8);
    }
}
