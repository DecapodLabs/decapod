//! Opt-in machine-checked formal obligation adapter and finite-capacity
//! conservation verifier (#1907).
//!
//! Design invariants:
//! - Opt-in only: disabled by default; ordinary CLI/RPC and local repos run
//!   with zero formal-checker dependencies when disabled.
//! - A formal checker is a separately executed, pinned tool; an agent may author
//!   a candidate proof, never declare that proof verified.
//! - Formal proof increases confidence ONLY in the proposition actually checked
//!   under explicitly declared assumptions. Claiming Rust runtime correctness
//!   requires an independent linking argument (executable conformance tests or
//!   correspondence proof).
//! - Admitted holes (`sorry`, `admit`, undeclared `axiom`/`assume`), missing
//!   imports, empty diagnostics, changed proof/model/toolchain digests, or
//!   fabricated transcripts fail closed.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const FORMAL_OBLIGATION_SCHEMA: &str = "decapod.formal-obligation.v1";
pub const FORMAL_VERDICT_SCHEMA: &str = "decapod.formal-obligation-verdict.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FormalCheckerKind {
    Lean,
    Kani,
    Verus,
}

impl FormalCheckerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Lean => "lean",
            Self::Kani => "kani",
            Self::Verus => "verus",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FormalObligationConfig {
    /// Disabled by default: zero effect on ordinary repos when false.
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub pinned_checkers: Vec<PinnedFormalChecker>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PinnedFormalChecker {
    pub kind: FormalCheckerKind,
    pub version: String,
    pub executable_path: String,
    pub executable_sha256: String,
    #[serde(default)]
    pub allowed_flags: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FormalClaimScope {
    /// Proves only the formal mathematical/state-machine model under declared assumptions.
    ModelOnly,
    /// Claims correspondence with implemented Rust code; requires an independent linking argument.
    ImplementationLinked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FormalLinkingArgument {
    pub linked_rust_symbols: Vec<String>,
    pub conformance_test_refs: Vec<String>,
    pub modeled_bounds: Vec<String>,
    pub unmodeled_host_effects: Vec<String>,
    pub conformance_tests_passed: bool,
}

/// Versioned formal-obligation record bound to exact source, policy, model,
/// proof, assumptions, imports, and pinned checker identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FormalObligationRecord {
    pub schema: String,
    pub obligation_id: String,
    pub property_name: String,
    pub scope: FormalClaimScope,
    pub checker_kind: FormalCheckerKind,
    pub checker_version: String,
    pub checker_executable_sha256: String,
    pub checker_flags: Vec<String>,
    pub model_source_path: String,
    pub model_source_sha256: String,
    pub proof_source_path: String,
    pub proof_source_sha256: String,
    pub declared_assumptions: Vec<String>,
    pub declared_imports: Vec<String>,
    pub repository_id: String,
    pub task_id: String,
    pub source_commit: String,
    pub policy_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linking_argument: Option<FormalLinkingArgument>,
}

/// Observed output captured from a separately executed pinned formal checker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FormalCheckerTranscript {
    pub checker_kind: FormalCheckerKind,
    pub checker_version: String,
    pub observed_executable_sha256: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub proof_source_contents: String,
    pub model_source_contents: String,
    pub runtime_ms: u64,
    pub storage_bytes: u64,
    /// Cryptographic digest computed by the trusted runner over
    /// `(obligation_id, executable_sha256, model_sha256, proof_sha256, exit_code, stdout, stderr)`.
    /// Prevents agent-authored fake transcripts from impersonating checker runs.
    pub runner_attestation_sha256: String,
    pub produced_by_trusted_runner: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FormalVerificationStatus {
    Disabled,
    VerifiedModelOnly,
    VerifiedImplementationLinked,
    Stale,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FormalVerificationResult {
    pub schema: String,
    pub obligation_id: String,
    pub status: FormalVerificationStatus,
    pub accepted_for_declared_assumptions_only: bool,
    pub blanket_production_correctness_claimed: bool,
    pub rejection_codes: Vec<String>,
    pub details: Vec<String>,
    pub runtime_ms: u64,
    pub storage_bytes: u64,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Compute the trusted runner attestation digest binding the exact obligation,
/// pinned executable, model/proof sources, exit code, and checker diagnostics.
pub fn compute_runner_attestation(
    obligation_id: &str,
    observed_executable_sha256: &str,
    model_sha256: &str,
    proof_sha256: &str,
    exit_code: i32,
    stdout: &str,
    stderr: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"decapod.formal-runner-attestation.v1\0");
    hasher.update(obligation_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(observed_executable_sha256.as_bytes());
    hasher.update(b"\0");
    hasher.update(model_sha256.as_bytes());
    hasher.update(b"\0");
    hasher.update(proof_sha256.as_bytes());
    hasher.update(b"\0");
    hasher.update(exit_code.to_string().as_bytes());
    hasher.update(b"\0");
    hasher.update(stdout.as_bytes());
    hasher.update(b"\0");
    hasher.update(stderr.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn contains_token(haystack: &str, needle: &str) -> bool {
    let lower = haystack.to_ascii_lowercase();
    let needle_lower = needle.to_ascii_lowercase();
    let mut start = 0;
    while let Some(rel) = lower[start..].find(&needle_lower) {
        let idx = start + rel;
        let before_ok = idx == 0
            || !lower[..idx]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        let end = idx + needle_lower.len();
        let after_ok = end == lower.len()
            || !lower[end..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        if before_ok && after_ok {
            return true;
        }
        start = idx + 1;
    }
    false
}

fn detect_admitted_holes(
    kind: FormalCheckerKind,
    proof_src: &str,
    stdout: &str,
    stderr: &str,
) -> Vec<String> {
    let mut hits = Vec::new();
    let combined = format!("{proof_src}\n{stdout}\n{stderr}");
    match kind {
        FormalCheckerKind::Lean => {
            for kw in ["sorry", "admit", "axiom ", "implemented_by", "unsafe "] {
                if contains_token(&combined, kw.trim()) {
                    hits.push(format!("Lean admitted hole or unsafe escape '{kw}'"));
                }
            }
            if combined
                .to_ascii_lowercase()
                .contains("declaration uses 'sorry'")
            {
                hits.push("Lean diagnostic reports declaration uses 'sorry'".to_string());
            }
        }
        FormalCheckerKind::Kani => {
            if proof_src.contains("kani::assume") {
                hits.push("Kani proof contains unreviewed `kani::assume` hole".to_string());
            }
            if stdout.contains("VERIFICATION:- FAILED") || stderr.contains("VERIFICATION:- FAILED")
            {
                hits.push("Kani reported VERIFICATION:- FAILED".to_string());
            }
        }
        FormalCheckerKind::Verus => {
            for kw in ["assume(", "admit(", "external_body"] {
                if proof_src.contains(kw) || stdout.contains(kw) {
                    hits.push(format!("Verus proof contains admitted hole '{kw}'"));
                }
            }
        }
    }
    hits
}

fn has_checker_diagnostic_coverage(kind: FormalCheckerKind, stdout: &str, stderr: &str) -> bool {
    let combined = format!("{stdout}\n{stderr}");
    if combined.trim().is_empty() {
        return false;
    }
    match kind {
        FormalCheckerKind::Lean => {
            combined.contains("lean")
                || combined.contains("theorem")
                || combined.contains("verified")
        }
        FormalCheckerKind::Kani => combined.contains("VERIFICATION:- SUCCESSFUL"),
        FormalCheckerKind::Verus => {
            combined.contains("verified") && !combined.contains("0 verified")
        }
    }
}

/// Verify an opt-in formal obligation against pinned checker configuration and
/// an observed checker transcript.
pub fn verify_formal_obligation(
    config: &FormalObligationConfig,
    record: &FormalObligationRecord,
    transcript: &FormalCheckerTranscript,
) -> FormalVerificationResult {
    if !config.enabled {
        return FormalVerificationResult {
            schema: FORMAL_VERDICT_SCHEMA.to_string(),
            obligation_id: record.obligation_id.clone(),
            status: FormalVerificationStatus::Disabled,
            accepted_for_declared_assumptions_only: false,
            blanket_production_correctness_claimed: false,
            rejection_codes: Vec::new(),
            details: vec![
                "Formal-obligation adapter is disabled; ordinary validation proceeds without checker requirements.".to_string(),
            ],
            runtime_ms: 0,
            storage_bytes: 0,
        };
    }

    let mut codes = Vec::new();
    let mut details = Vec::new();
    let mut stale = false;

    if record.schema != FORMAL_OBLIGATION_SCHEMA {
        codes.push("INVALID_SCHEMA".to_string());
        details.push(format!(
            "Expected schema '{FORMAL_OBLIGATION_SCHEMA}', got '{}'",
            record.schema
        ));
    }

    let Some(pinned) = config
        .pinned_checkers
        .iter()
        .find(|p| p.kind == record.checker_kind)
    else {
        codes.push("UNPINNED_CHECKER".to_string());
        details.push(format!(
            "No pinned checker configured for '{}'",
            record.checker_kind.as_str()
        ));
        return FormalVerificationResult {
            schema: FORMAL_VERDICT_SCHEMA.to_string(),
            obligation_id: record.obligation_id.clone(),
            status: FormalVerificationStatus::Rejected,
            accepted_for_declared_assumptions_only: false,
            blanket_production_correctness_claimed: false,
            rejection_codes: codes,
            details,
            runtime_ms: transcript.runtime_ms,
            storage_bytes: transcript.storage_bytes,
        };
    };

    if pinned.version != record.checker_version || pinned.version != transcript.checker_version {
        stale = true;
        codes.push("CHECKER_VERSION_DRIFT".to_string());
        details.push(format!(
            "Pinned checker version '{}' does not match record '{}' / transcript '{}'",
            pinned.version, record.checker_version, transcript.checker_version
        ));
    }

    if pinned.executable_sha256 != record.checker_executable_sha256
        || pinned.executable_sha256 != transcript.observed_executable_sha256
    {
        codes.push("UNTRUSTED_EXECUTABLE_SUBSTITUTION".to_string());
        details.push(
            "Observed checker executable digest does not match pinned trusted executable digest."
                .to_string(),
        );
    }

    let actual_model_sha = sha256_hex(transcript.model_source_contents.as_bytes());
    let actual_proof_sha = sha256_hex(transcript.proof_source_contents.as_bytes());
    if actual_model_sha != record.model_source_sha256 {
        stale = true;
        codes.push("MODEL_INPUT_STALE".to_string());
        details.push("Model source digest changed after obligation record creation.".to_string());
    }
    if actual_proof_sha != record.proof_source_sha256 {
        stale = true;
        codes.push("PROOF_INPUT_STALE".to_string());
        details.push("Proof source digest changed after obligation record creation.".to_string());
    }

    if !transcript.produced_by_trusted_runner {
        codes.push("FABRICATED_TRANSCRIPT".to_string());
        details.push(
            "Transcript was not produced by the trusted validation runner; agent-authored fake PASS rejected.".to_string(),
        );
    }

    let expected_attestation = compute_runner_attestation(
        &record.obligation_id,
        &transcript.observed_executable_sha256,
        &actual_model_sha,
        &actual_proof_sha,
        transcript.exit_code,
        &transcript.stdout,
        &transcript.stderr,
    );
    if transcript.runner_attestation_sha256 != expected_attestation {
        codes.push("FABRICATED_TRANSCRIPT_DIGEST".to_string());
        details.push(
            "Runner attestation digest mismatch; transcript cannot be trusted as checker output."
                .to_string(),
        );
    }

    if transcript.exit_code != 0 {
        codes.push("CHECKER_NONZERO_EXIT".to_string());
        details.push(format!(
            "Formal checker exited with non-zero code {}",
            transcript.exit_code
        ));
    }

    if !has_checker_diagnostic_coverage(record.checker_kind, &transcript.stdout, &transcript.stderr)
    {
        codes.push("MISSING_DIAGNOSTIC_COVERAGE".to_string());
        details.push(
            "Exit zero alone is insufficient without explicit checker verification diagnostics."
                .to_string(),
        );
    }

    for imp in &record.declared_imports {
        if !transcript.proof_source_contents.contains(imp)
            && !transcript.model_source_contents.contains(imp)
        {
            codes.push("MISSING_DECLARED_IMPORT".to_string());
            details.push(format!(
                "Declared import '{imp}' missing from checked model/proof sources."
            ));
        }
    }

    let hole_hits = detect_admitted_holes(
        record.checker_kind,
        &transcript.proof_source_contents,
        &transcript.stdout,
        &transcript.stderr,
    );
    if !hole_hits.is_empty() {
        codes.push("ADMITTED_HOLE_REJECTED".to_string());
        details.extend(hole_hits);
    }

    if record.scope == FormalClaimScope::ImplementationLinked {
        match &record.linking_argument {
            None => {
                codes.push("MISSING_LINKING_ARGUMENT".to_string());
                details.push(
                    "Implementation-linked formal claim requires an independent linking argument."
                        .to_string(),
                );
            }
            Some(link) => {
                if link.linked_rust_symbols.is_empty() || link.conformance_test_refs.is_empty() {
                    codes.push("INCOMPLETE_LINKING_ARGUMENT".to_string());
                    details.push(
                        "Linking argument must cite linked Rust symbols and executable conformance tests."
                            .to_string(),
                    );
                }
                if !link.conformance_tests_passed {
                    codes.push("IMPLEMENTATION_BEHAVIOR_MISMATCH".to_string());
                    details.push(
                        "Executable behavioral conformance failed despite formal model check; implementation mismatch blocks acceptance."
                            .to_string(),
                    );
                }
            }
        }
    }

    if !codes.is_empty() {
        let status = if stale
            && codes.iter().all(|c| {
                matches!(
                    c.as_str(),
                    "MODEL_INPUT_STALE" | "PROOF_INPUT_STALE" | "CHECKER_VERSION_DRIFT"
                )
            }) {
            FormalVerificationStatus::Stale
        } else {
            FormalVerificationStatus::Rejected
        };
        return FormalVerificationResult {
            schema: FORMAL_VERDICT_SCHEMA.to_string(),
            obligation_id: record.obligation_id.clone(),
            status,
            accepted_for_declared_assumptions_only: false,
            blanket_production_correctness_claimed: false,
            rejection_codes: codes,
            details,
            runtime_ms: transcript.runtime_ms,
            storage_bytes: transcript.storage_bytes,
        };
    }

    let status = match record.scope {
        FormalClaimScope::ModelOnly => FormalVerificationStatus::VerifiedModelOnly,
        FormalClaimScope::ImplementationLinked => {
            FormalVerificationStatus::VerifiedImplementationLinked
        }
    };
    FormalVerificationResult {
        schema: FORMAL_VERDICT_SCHEMA.to_string(),
        obligation_id: record.obligation_id.clone(),
        status,
        accepted_for_declared_assumptions_only: true,
        blanket_production_correctness_claimed: false,
        rejection_codes: Vec::new(),
        details: vec![format!(
            "Checked property '{}' under {} declared assumption(s).",
            record.property_name,
            record.declared_assumptions.len()
        )],
        runtime_ms: transcript.runtime_ms,
        storage_bytes: transcript.storage_bytes,
    }
}

/// Finite-capacity conservation state machine (#1907 native conservation pilot).
///
/// Conservation invariant across every admitted transition:
/// `committed_or_spent + armed + retained_indeterminate + reserved + available == issued_capacity`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapacityAccountingState {
    pub issued_capacity: u64,
    pub committed_or_spent: u64,
    pub armed: u64,
    pub retained_indeterminate: u64,
    pub reserved: u64,
    pub available: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapacityTransition {
    Allocate { amount: u64 },
    Arm { amount: u64 },
    Settle { amount: u64 },
    DefinitelyAbort { amount: u64 },
    MarkIndeterminate { amount: u64 },
    ReconcileIndeterminate { amount: u64, settled: bool },
    TimeoutWithoutAuthoritativeReconciliation,
    RecoverAfterRestart,
}

impl CapacityAccountingState {
    pub fn new(issued_capacity: u64) -> Self {
        Self {
            issued_capacity,
            committed_or_spent: 0,
            armed: 0,
            retained_indeterminate: 0,
            reserved: 0,
            available: issued_capacity,
        }
    }

    pub fn conservation_holds(&self) -> bool {
        self.committed_or_spent
            .checked_add(self.armed)
            .and_then(|s| s.checked_add(self.retained_indeterminate))
            .and_then(|s| s.checked_add(self.reserved))
            .and_then(|s| s.checked_add(self.available))
            == Some(self.issued_capacity)
    }

    pub fn apply(&mut self, transition: CapacityTransition) -> Result<(), String> {
        match transition {
            CapacityTransition::Allocate { amount } => {
                if amount == 0 || amount > self.available {
                    return Err("INSUFFICIENT_AVAILABLE_CAPACITY".to_string());
                }
                self.available -= amount;
                self.reserved += amount;
            }
            CapacityTransition::Arm { amount } => {
                if amount == 0 || amount > self.reserved {
                    return Err("INSUFFICIENT_RESERVED_CAPACITY".to_string());
                }
                self.reserved -= amount;
                self.armed += amount;
            }
            CapacityTransition::Settle { amount } => {
                if amount == 0 || amount > self.armed {
                    return Err("INSUFFICIENT_ARMED_CAPACITY".to_string());
                }
                self.armed -= amount;
                self.committed_or_spent += amount;
            }
            CapacityTransition::DefinitelyAbort { amount } => {
                if amount == 0 || amount > self.armed {
                    return Err("INSUFFICIENT_ARMED_FOR_ABORT".to_string());
                }
                self.armed -= amount;
                self.available += amount;
            }
            CapacityTransition::MarkIndeterminate { amount } => {
                if amount == 0 || amount > self.armed {
                    return Err("INSUFFICIENT_ARMED_FOR_INDETERMINATE".to_string());
                }
                self.armed -= amount;
                self.retained_indeterminate += amount;
            }
            CapacityTransition::ReconcileIndeterminate { amount, settled } => {
                if amount == 0 || amount > self.retained_indeterminate {
                    return Err("INSUFFICIENT_INDETERMINATE_CAPACITY".to_string());
                }
                self.retained_indeterminate -= amount;
                if settled {
                    self.committed_or_spent += amount;
                } else {
                    self.available += amount;
                }
            }
            CapacityTransition::TimeoutWithoutAuthoritativeReconciliation => {
                // Invariant (#1907): timeout alone MUST NOT replenish available capacity.
                // Indeterminate outcomes remain retained until authoritative reconciliation.
            }
            CapacityTransition::RecoverAfterRestart => {
                // In-flight armed capacity whose post-send outcome is unknown across
                // crash/restart moves to retained_indeterminate rather than double-releasing.
                if self.armed > 0 {
                    let unknown = self.armed;
                    self.armed = 0;
                    self.retained_indeterminate += unknown;
                }
            }
        }
        if !self.conservation_holds() {
            return Err("CONSERVATION_INVARIANT_VIOLATED".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FormalPilotVerdict {
    pub property_name: String,
    pub checker_kind: FormalCheckerKind,
    pub positive_control_passed: bool,
    pub negative_impersonation_rejected: bool,
    pub measured_runtime_ms: u64,
    pub measured_storage_bytes: u64,
    pub justify_broader_integration: bool,
    pub verdict_summary: String,
}
