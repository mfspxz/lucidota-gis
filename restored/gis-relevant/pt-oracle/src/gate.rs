//! pt-oracle gate — Verification gate organelle.
//! Promotes Candidate→Proven through receipt-based rule evaluation.

use std::collections::HashMap;

/// Re-exported from GO-50 ontology — the canonical ternary valence type.
pub use go_ontology::TernaryValence as Valence;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OracleReceipt {
    pub ts: u64, pub artifact_hash: [u8; 32], pub agent: String,
    pub verdict: Valence, pub evidence_hashes: Vec<[u8; 32]>,
    pub findings: Vec<String>, pub absurd_tag: String,
}

pub trait VerificationGate {
    type Error: std::fmt::Debug;
    fn verify(&mut self, hash: &[u8;32], agent: &str, claim: &[u8], evidence: &[OracleReceipt]) -> Result<OracleReceipt, Self::Error>;
    fn promote(&mut self, hash: &[u8;32], receipts: &[OracleReceipt], quorum: usize) -> Result<Valence, Self::Error>;
    fn demote(&mut self, hash: &[u8;32], refutation: &[u8]) -> Result<Valence, Self::Error>;
}

pub struct VerificationResult { pub rule_name: String, pub passed: bool, pub confidence: f32, pub detail: String }

pub struct RuleEngine { rules: HashMap<String, Box<dyn Fn(&[u8], &[OracleReceipt]) -> VerificationResult>> }

impl RuleEngine {
    pub fn new() -> Self { Self { rules: HashMap::new() } }
    pub fn register<F: Fn(&[u8], &[OracleReceipt]) -> VerificationResult + 'static>(&mut self, name: &str, rule: F) {
        self.rules.insert(name.to_string(), Box::new(rule));
    }
    pub fn evaluate(&self, claim: &[u8], evidence: &[OracleReceipt]) -> (Valence, Vec<VerificationResult>) {
        let results: Vec<_> = self.rules.values().map(|r| r(claim, evidence)).collect();
        if results.is_empty() { return (Valence::Candidate, results); }
        let passed = results.iter().filter(|r| r.passed).count();
        let ratio = passed as f32 / results.len() as f32;
        (if ratio >= 0.67 { Valence::Proven } else if ratio <= 0.33 { Valence::Tainted } else { Valence::Candidate }, results)
    }
}

impl Default for RuleEngine { fn default() -> Self { Self::new() } }

pub fn rule_non_empty(claim: &[u8], _: &[OracleReceipt]) -> VerificationResult {
    VerificationResult { rule_name: "non_empty".into(), passed: !claim.is_empty(), confidence: if claim.is_empty() { 1.0 } else { 0.99 }, detail: if claim.is_empty() { "empty rejected".into() } else { "non-empty".into() } }
}

pub struct Oracle { engine: RuleEngine, receipts: Vec<OracleReceipt> }

impl Oracle {
    pub fn new() -> Self { let mut e = RuleEngine::new(); e.register("non_empty", rule_non_empty); Self { engine: e, receipts: vec![] } }
}

impl VerificationGate for Oracle {
    type Error = OracleError;
    fn verify(&mut self, hash: &[u8;32], agent: &str, claim: &[u8], evidence: &[OracleReceipt]) -> Result<OracleReceipt, OracleError> {
        let (verdict, results) = self.engine.evaluate(claim, evidence);
        let findings: Vec<_> = results.iter().map(|r| format!("[{}] pass={} conf={:.2}: {}", r.rule_name, r.passed, r.confidence, r.detail)).collect();
        let receipt = OracleReceipt {
            ts: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
            artifact_hash: *hash, agent: agent.to_string(), verdict, evidence_hashes: evidence.iter().map(|r| r.artifact_hash).collect(),
            findings, absurd_tag: format!("oracle:v1:{}", verdict.as_int()),
        };
        self.receipts.push(receipt.clone());
        Ok(receipt)
    }
    fn promote(&mut self, hash: &[u8;32], receipts: &[OracleReceipt], quorum: usize) -> Result<Valence, OracleError> {
        let count = receipts.iter().filter(|r| r.artifact_hash == *hash && r.verdict == Valence::Proven).count();
        Ok(if count >= quorum { Valence::Proven } else { Valence::Candidate })
    }
    fn demote(&mut self, _hash: &[u8;32], refutation: &[u8]) -> Result<Valence, OracleError> {
        if refutation.is_empty() { return Err(OracleError::InvalidRefutation); }
        Ok(Valence::Tainted)
    }
}

impl Default for Oracle { fn default() -> Self { Self::new() } }

#[derive(Debug)] pub enum OracleError { InvalidRefutation, InsufficientEvidence, StoreError(String) }
impl std::fmt::Display for OracleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { Self::InvalidRefutation => write!(f, "invalid refutation"), Self::InsufficientEvidence => write!(f, "insufficient evidence"), Self::StoreError(m) => write!(f, "store: {m}") }
    }
}
impl std::error::Error for OracleError {}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn empty_tainted() { let mut o = Oracle::new(); assert_eq!(o.verify(&[0;32],"t",b"",&[]).unwrap().verdict, Valence::Tainted); }
    #[test] fn nonempty_proven() { let mut o = Oracle::new(); assert_eq!(o.verify(&[1;32],"t",b"x",&[]).unwrap().verdict, Valence::Proven); }
    #[test] fn quorum() { let mut o = Oracle::new(); let r: Vec<_> = (0..5).map(|i| OracleReceipt{ts:i,artifact_hash:[9;32],agent:format!("a{i}"),verdict:Valence::Proven,evidence_hashes:vec![],findings:vec![],absurd_tag:"".into()}).collect(); assert_eq!(o.promote(&[9;32],&r,3).unwrap(),Valence::Proven); assert_eq!(o.promote(&[9;32],&r,6).unwrap(),Valence::Candidate); }
}
