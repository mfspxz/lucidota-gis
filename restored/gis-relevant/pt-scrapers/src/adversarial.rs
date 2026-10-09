//! Adversarial detection — the ABSURD Gate's honest-failure signals.
//!
//! Implements Ghost Swarm RFC §5.3 phase 2 (canary analysis) + phase 4 (honest
//! failure), and the threat-adaptive `scraper-canary-defense` +
//! `scraper-detection-awareness` capabilities. Pure functions, no network.
//!
//! Doctrine: when we believe a page is a trap (honeypot / canary token) or a
//! challenge wall (proof-of-work / bot interstitial), we HALT and report — we
//! NEVER emit candidates from it and NEVER fabricate. This is the anti-fabrication
//! law applied to adversarial signals.

/// A recognized adversarial signal on a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectionSignal {
    /// The URL looks like a honeypot trap (pre-request).
    Honeypot(String),
    /// The response body carries a canary/tracking token.
    Canary(String),
    /// The response is a proof-of-work / bot challenge interstitial.
    ProofOfWork(String),
    /// A known WAF/anti-bot challenge page (Cloudflare/DataDome/etc).
    Challenge(String),
}

impl DetectionSignal {
    /// A human-readable reason for a `ScrapeStatus::Blocked`.
    pub fn reason(&self) -> String {
        match self {
            DetectionSignal::Honeypot(d) => format!("honeypot URL: {d}"),
            DetectionSignal::Canary(d) => format!("canary token detected: {d}"),
            DetectionSignal::ProofOfWork(d) => format!("proof-of-work challenge: {d}"),
            DetectionSignal::Challenge(d) => format!("anti-bot challenge: {d}"),
        }
    }
}

/// Honeypot URL heuristics — checked BEFORE a request (RFC §5.3, cheap gate).
/// Conservative: flags the well-known trap markers, not legitimate deep links.
pub fn is_honeypot(url: &str) -> Option<DetectionSignal> {
    let u = url.to_lowercase();
    const TRAP_MARKERS: &[&str] = &[
        "/honeypot",
        "/trap/",
        "/spider-trap",
        "/do-not-follow",
        "/bot-trap",
        "display:none", // links hidden via inline style (classic honeypot)
    ];
    for m in TRAP_MARKERS {
        if u.contains(m) {
            return Some(DetectionSignal::Honeypot(m.to_string()));
        }
    }
    None
}

/// Scan a response body for canary/tracking tokens. Operator supplies known
/// token patterns (e.g. from prior burns); we also flag generic canary markers.
pub fn scan_canary(body: &str, known_tokens: &[String]) -> Option<DetectionSignal> {
    for tok in known_tokens {
        if !tok.is_empty() && body.contains(tok.as_str()) {
            return Some(DetectionSignal::Canary(tok.clone()));
        }
    }
    // Generic canary markers seen in the wild (hidden tracking beacons).
    const CANARY_MARKERS: &[&str] = &["data-canary-token", "x-canary-id", "__canary__"];
    for m in CANARY_MARKERS {
        if body.contains(m) {
            return Some(DetectionSignal::Canary(m.to_string()));
        }
    }
    None
}

/// Recognize a proof-of-work / bot-challenge interstitial (RFC §5.3 vendor id).
/// Returns the recognized vendor/marker so the caller can Block honestly rather
/// than parse the challenge page as if it were content.
pub fn detect_challenge(body: &str, headers: &[(String, String)]) -> Option<DetectionSignal> {
    let lower = body.to_lowercase();

    // Proof-of-work challenge pages (e.g. Anubis-style).
    const POW_MARKERS: &[&str] = &[
        "proof-of-work",
        "proof of work",
        "making sure you're not a bot",
        "anubis",
        "this process is automatic",
    ];
    for m in POW_MARKERS {
        if lower.contains(m) {
            return Some(DetectionSignal::ProofOfWork(m.to_string()));
        }
    }

    // Known anti-bot vendor challenge signatures (body + headers).
    const CHALLENGE_MARKERS: &[&str] = &[
        "cf-challenge",
        "cf_chl_opt",
        "checking your browser",
        "attention required! | cloudflare",
        "datadome",
        "px-captcha",
        "please enable javascript and cookies",
        "_imperva_",
        "incapsula incident",
    ];
    for m in CHALLENGE_MARKERS {
        if lower.contains(m) {
            return Some(DetectionSignal::Challenge(m.to_string()));
        }
    }

    // Header signatures.
    for (k, v) in headers {
        let kl = k.to_lowercase();
        let vl = v.to_lowercase();
        if kl == "server" && (vl.contains("cloudflare") || vl.contains("datadome")) {
            // Server header alone isn't a block — only flag if paired with a 4xx/5xx
            // challenge body, which the body markers above catch. Skip here.
            continue;
        }
        if kl == "cf-mitigated" || kl == "x-datadome" {
            return Some(DetectionSignal::Challenge(format!("{k} header")));
        }
    }

    None
}

/// The full ABSURD-gate check: honeypot (pre) OR canary OR challenge (post).
/// Returns the FIRST signal found — caller maps it to `ScrapeStatus::Blocked`.
pub fn absurd_gate(
    url: &str,
    body: &str,
    headers: &[(String, String)],
    known_tokens: &[String],
) -> Option<DetectionSignal> {
    if let Some(s) = is_honeypot(url) {
        return Some(s);
    }
    if let Some(s) = scan_canary(body, known_tokens) {
        return Some(s);
    }
    detect_challenge(body, headers)
}

// ─────────────────────────────────────────────────────────────────────────────
// ABSURD Gate — four-phase adaptive stealth state machine (RFC §5.3).
//
// Orchestrates the probe WITHOUT owning the network or the persona store: it is a
// pure state machine that consumes observations (HTTP status + body + headers) and
// emits an ACTION for the caller to apply. This keeps it decoupled from both the
// lanes (network) and pt-percyphon (persona lifecycle) while encoding the doctrine:
// identify the vendor, then rotate persona, then fail honestly and burn.
// ─────────────────────────────────────────────────────────────────────────────

/// The four phases of the ABSURD adaptive-stealth probe (RFC §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatePhase {
    /// Phase 1: normal attempt with the current persona.
    Sacrificial,
    /// Phase 2: a block was seen; analyze the challenge/canary to ID the vendor.
    CanaryAnalysis,
    /// Phase 3: swap to a persona tuned (via Kronenberg) to bypass this vendor.
    ChameleonShift,
    /// Phase 4: still blocked after rotation — give up honestly, burn the persona.
    HonestFailure,
}

/// What the caller should DO next after feeding an observation to the gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateAction {
    /// The response is clean — proceed to parse it as content.
    Proceed,
    /// Identified an adversary; rotate to a fresh/tuned persona and retry.
    /// Carries the detection signal that triggered the shift (vendor evidence).
    RotatePersona(DetectionSignal),
    /// Terminal: block confirmed after rotation. The caller must mark the current
    /// persona's fingerprint slot deprecated (epigenome) and burn it (archive),
    /// then return `ScrapeStatus::Blocked`. NEVER fabricate content.
    BurnAndFail(DetectionSignal),
}

/// An observation of a single attempt's response.
pub struct Observation<'a> {
    pub url: &'a str,
    pub http_status: u16,
    pub body: &'a str,
    pub headers: &'a [(String, String)],
}

/// The gate's running state across an escalation sequence for one target.
#[derive(Debug, Clone)]
pub struct AbsurdGate {
    phase: GatePhase,
    /// How many persona rotations we've already spent (chameleon shifts).
    rotations: u8,
    /// Max rotations before honest failure (operator-controlled; RFC: bounded).
    max_rotations: u8,
    /// Known canary tokens to scan for (operator-supplied from prior burns).
    known_tokens: Vec<String>,
}

impl AbsurdGate {
    /// New gate. `max_rotations` bounds the chameleon phase (nothing automatic /
    /// infinite — the operator sets the ceiling).
    pub fn new(max_rotations: u8, known_tokens: Vec<String>) -> Self {
        Self {
            phase: GatePhase::Sacrificial,
            rotations: 0,
            max_rotations,
            known_tokens,
        }
    }

    pub fn phase(&self) -> GatePhase {
        self.phase
    }

    pub fn rotations(&self) -> u8 {
        self.rotations
    }

    /// Feed one attempt's observation; advance the state machine; return the action.
    ///
    /// Logic:
    /// - clean 2xx with no adversarial signal  => Proceed
    /// - a signal (honeypot/canary/challenge/PoW) OR a block status (403/429/503):
    ///     - if we still have rotations left     => RotatePersona (ChameleonShift)
    ///     - else                                => BurnAndFail (HonestFailure)
    pub fn observe(&mut self, obs: &Observation) -> GateAction {
        let signal = absurd_gate(obs.url, obs.body, obs.headers, &self.known_tokens);
        let is_block_status = matches!(obs.http_status, 401 | 403 | 407 | 429 | 503);

        match (signal, is_block_status) {
            (None, false) if (200..300).contains(&obs.http_status) => {
                self.phase = GatePhase::Sacrificial;
                GateAction::Proceed
            }
            (sig, blocked) => {
                // Something is wrong: an explicit adversarial signal OR a block code.
                let evidence = sig.unwrap_or_else(|| {
                    DetectionSignal::Challenge(format!("HTTP {}", obs.http_status))
                });
                let _ = blocked;
                if self.rotations < self.max_rotations {
                    self.rotations += 1;
                    self.phase = if self.rotations == 1 {
                        GatePhase::CanaryAnalysis
                    } else {
                        GatePhase::ChameleonShift
                    };
                    GateAction::RotatePersona(evidence)
                } else {
                    self.phase = GatePhase::HonestFailure;
                    GateAction::BurnAndFail(evidence)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn honeypot_url_flagged() {
        assert!(is_honeypot("https://x.com/bot-trap/page").is_some());
        assert!(is_honeypot("https://x.com/legit/article").is_none());
    }

    #[test]
    fn known_canary_token_detected() {
        let tokens = vec!["CANARY-abc123".to_string()];
        let sig = scan_canary("<html>CANARY-abc123</html>", &tokens);
        assert!(matches!(sig, Some(DetectionSignal::Canary(_))));
    }

    #[test]
    fn generic_canary_marker_detected() {
        let sig = scan_canary("<div data-canary-token='x'>", &[]);
        assert!(matches!(sig, Some(DetectionSignal::Canary(_))));
    }

    #[test]
    fn clean_body_no_canary() {
        assert!(scan_canary("<html>normal content</html>", &[]).is_none());
    }

    #[test]
    fn proof_of_work_challenge_recognized() {
        let sig = detect_challenge("Making sure you're not a bot...", &[]);
        assert!(matches!(sig, Some(DetectionSignal::ProofOfWork(_))));
    }

    #[test]
    fn cloudflare_challenge_recognized() {
        let sig = detect_challenge("Attention Required! | Cloudflare — checking your browser", &[]);
        assert!(matches!(sig, Some(DetectionSignal::Challenge(_))));
    }

    #[test]
    fn real_content_not_flagged_as_challenge() {
        let sig = detect_challenge("<html><body>Real festival listing content</body></html>", &[]);
        assert!(sig.is_none(), "legit content must not be a false-positive challenge");
    }

    #[test]
    fn absurd_gate_composes_all_phases() {
        // Honeypot URL short-circuits first.
        assert!(absurd_gate("https://x.com/spider-trap/", "", &[], &[]).is_some());
        // Clean everything => no signal.
        assert!(absurd_gate("https://x.com/ok", "<html>ok</html>", &[], &[]).is_none());
    }

    #[test]
    fn state_machine_clean_response_proceeds() {
        let mut gate = AbsurdGate::new(2, vec![]);
        let obs = Observation {
            url: "https://x.com/ok",
            http_status: 200,
            body: "<html>real content</html>",
            headers: &[],
        };
        assert_eq!(gate.observe(&obs), GateAction::Proceed);
        assert_eq!(gate.phase(), GatePhase::Sacrificial);
    }

    #[test]
    fn state_machine_escalates_then_burns() {
        // max 2 rotations: block, block => rotate twice, third block => burn.
        let mut gate = AbsurdGate::new(2, vec![]);
        let blocked = Observation {
            url: "https://x.com/p",
            http_status: 403,
            body: "Attention Required! | Cloudflare",
            headers: &[],
        };
        // Phase 1 block -> rotate (canary analysis)
        let a1 = gate.observe(&blocked);
        assert!(matches!(a1, GateAction::RotatePersona(_)));
        assert_eq!(gate.phase(), GatePhase::CanaryAnalysis);
        // Phase 2 block -> rotate (chameleon shift)
        let a2 = gate.observe(&blocked);
        assert!(matches!(a2, GateAction::RotatePersona(_)));
        assert_eq!(gate.phase(), GatePhase::ChameleonShift);
        // Rotations exhausted -> honest failure + burn
        let a3 = gate.observe(&blocked);
        assert!(matches!(a3, GateAction::BurnAndFail(_)));
        assert_eq!(gate.phase(), GatePhase::HonestFailure);
    }

    #[test]
    fn state_machine_burn_carries_vendor_evidence() {
        let mut gate = AbsurdGate::new(0, vec![]); // zero rotations => immediate burn
        let obs = Observation {
            url: "https://x.com/p",
            http_status: 403,
            body: "datadome challenge",
            headers: &[],
        };
        match gate.observe(&obs) {
            GateAction::BurnAndFail(sig) => {
                // The evidence must name the vendor we detected (honest failure).
                assert!(matches!(sig, DetectionSignal::Challenge(_)));
            }
            other => panic!("expected BurnAndFail, got {other:?}"),
        }
    }

    #[test]
    fn state_machine_recovers_when_rotation_succeeds() {
        // block once (rotate), then a clean response after the persona swap => Proceed.
        let mut gate = AbsurdGate::new(3, vec![]);
        let blocked = Observation { url: "https://x.com/p", http_status: 429, body: "", headers: &[] };
        let clean = Observation { url: "https://x.com/p", http_status: 200, body: "<html>ok</html>", headers: &[] };
        assert!(matches!(gate.observe(&blocked), GateAction::RotatePersona(_)));
        assert_eq!(gate.observe(&clean), GateAction::Proceed, "rotation recovered => proceed");
    }
}
