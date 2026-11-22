//! Offline verifiable maintainer authority and truthful publication judgments
//! for historical checkpoint exceptions (#1569).
//!
//! Invariants:
//! - An exception NEVER turns unproven history into `history_proven = true`.
//! - Signer trust roots are loaded ONLY from an independently accepted target
//!   ref, never from the requesting candidate worktree.
//! - Every decision binds immutable repository, base, candidate, affected
//!   commit/tree, obligation, authorizer key, reason, evidence, unique ID,
//!   bounded TTL, single-use replay limit, and revocation semantics.

use ring::signature::{ED25519, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

pub const HISTORICAL_EXCEPTION_SIGNERS_PATH: &str = ".decapod/historical-exception-signers.json";
pub const HISTORICAL_EXCEPTION_SIGNERS_SCHEMA: &str = "decapod.historical-exception-signers.v1";
pub const HISTORICAL_EXCEPTION_SCHEMA: &str = "decapod.historical-exception.v1";
pub const HISTORICAL_EXCEPTION_JUDGMENT_SCHEMA: &str = "decapod.historical-exception-judgment.v1";
pub const HISTORICAL_EXCEPTION_ENV: &str = "DECAPOD_HISTORICAL_EXCEPTION_DECISION";
pub const MAX_EXCEPTION_TTL_SECONDS: i64 = 7 * 86_400;
const MAX_SIGNERS_BYTES: usize = 32 * 1024;
const MAX_DECISION_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HistoricalExceptionSigners {
    pub schema: String,
    #[serde(default)]
    pub policy_permits_exceptional_admission: bool,
    #[serde(default)]
    pub keys: BTreeMap<String, String>,
    #[serde(default)]
    pub revoked_key_ids: BTreeSet<String>,
    #[serde(default)]
    pub revoked_decision_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HistoricalExceptionDecision {
    pub schema: String,
    pub decision_id: String,
    pub repository_id: String,
    pub base_commit: String,
    pub candidate_commit: String,
    pub affected_commits: Vec<String>,
    pub affected_trees: Vec<String>,
    pub obligations: Vec<String>,
    pub authorizer_key_id: String,
    pub authorizer_authority: String,
    pub reason: String,
    pub evidence_refs: Vec<String>,
    pub issued_at: i64,
    pub expires_at: i64,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalExceptionContext {
    pub repository_id: String,
    pub base_commit: String,
    pub candidate_commit: String,
    pub affected_commits: Vec<String>,
    pub affected_trees: Vec<String>,
    pub required_obligations: Vec<String>,
    pub now_epoch: i64,
    pub consumed_decision_ids: BTreeSet<String>,
    /// Must always be false for accepted trust roots; true forces fail-closed rejection.
    pub signers_from_candidate_worktree: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizedHistoricalException {
    pub decision_id: String,
    pub authorizer_key_id: String,
    pub authorizer_authority: String,
    pub obligations: Vec<String>,
    pub affected_commits: Vec<String>,
    pub affected_trees: Vec<String>,
    pub reason: String,
    pub evidence_refs: Vec<String>,
    pub issued_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoricalExceptionPublicationJudgment {
    pub schema: String,
    /// Truthfulness invariant (#1569): historical exceptions NEVER make unproven history proven.
    pub history_proven: bool,
    pub exception_authorized: bool,
    pub policy_permits_exceptional_admission: bool,
    pub identity_assurance: String,
    pub authorized_exceptions: Vec<AuthorizedHistoricalException>,
    pub rejection_reasons: Vec<String>,
    pub provenance: Vec<Value>,
}

impl HistoricalExceptionPublicationJudgment {
    pub fn unverified(reason: impl Into<String>) -> Self {
        Self {
            schema: HISTORICAL_EXCEPTION_JUDGMENT_SCHEMA.to_string(),
            history_proven: false,
            exception_authorized: false,
            policy_permits_exceptional_admission: false,
            identity_assurance: "none".to_string(),
            authorized_exceptions: Vec::new(),
            rejection_reasons: vec![reason.into()],
            provenance: Vec::new(),
        }
    }
}

/// Canonical signing payload for an offline historical exception decision.
pub fn canonical_signing_payload(
    decision: &HistoricalExceptionDecision,
) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&(
        HISTORICAL_EXCEPTION_SCHEMA,
        &decision.decision_id,
        &decision.repository_id,
        &decision.base_commit,
        &decision.candidate_commit,
        &decision.affected_commits,
        &decision.affected_trees,
        &decision.obligations,
        &decision.authorizer_key_id,
        &decision.authorizer_authority,
        &decision.reason,
        &decision.evidence_refs,
        decision.issued_at,
        decision.expires_at,
    ))
    .map_err(|e| format!("failed to serialize canonical exception payload: {e}"))
}

fn decode_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = [0u8; N];
    for (idx, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[idx * 2..idx * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

fn valid_id(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.'))
}

fn verify_decision_signature(
    signers: &HistoricalExceptionSigners,
    decision: &HistoricalExceptionDecision,
) -> Result<(), String> {
    let key_hex = signers
        .keys
        .get(&decision.authorizer_key_id)
        .ok_or_else(|| {
            format!(
                "SELF_DECLARED_OR_UNKNOWN_AUTHORIZER: key_id '{}' is not present in accepted trust root",
                decision.authorizer_key_id
            )
        })?;
    let pubkey_bytes = decode_hex::<32>(key_hex).ok_or_else(|| {
        format!(
            "INVALID_SIGNER_PUBLIC_KEY: key_id '{}' is not a 32-byte hex Ed25519 key",
            decision.authorizer_key_id
        )
    })?;
    let sig_bytes = decode_hex::<64>(&decision.signature).ok_or_else(|| {
        "INVALID_SIGNATURE_ENCODING: expected 64-byte hex Ed25519 signature".to_string()
    })?;
    let payload = canonical_signing_payload(decision)?;
    UnparsedPublicKey::new(&ED25519, pubkey_bytes)
        .verify(&payload, &sig_bytes)
        .map_err(|_| {
            format!(
                "INVALID_SIGNATURE: Ed25519 verification failed for decision '{}'",
                decision.decision_id
            )
        })
}

/// Evaluate offline signed historical exception decisions against an independently
/// accepted signer trust root and immutable git/obligation context.
///
/// Guarantees:
/// - `history_proven` is ALWAYS `false`.
/// - Candidate-worktree policy/signers are unconditionally rejected.
/// - Conflicting decisions for the same candidate fail closed.
pub fn verify_historical_exceptions(
    signers: Option<&HistoricalExceptionSigners>,
    decisions: &[HistoricalExceptionDecision],
    ctx: &HistoricalExceptionContext,
) -> HistoricalExceptionPublicationJudgment {
    if ctx.signers_from_candidate_worktree {
        return HistoricalExceptionPublicationJudgment::unverified(
            "CANDIDATE_POLICY_REJECTED: historical exception signers must come from an independently accepted target ref, never the candidate worktree",
        );
    }

    let Some(signers) = signers else {
        return HistoricalExceptionPublicationJudgment::unverified(
            "NO_ACCEPTED_AUTHORITY: no independently accepted historical-exception signer trust root is configured",
        );
    };

    if signers.schema != HISTORICAL_EXCEPTION_SIGNERS_SCHEMA {
        return HistoricalExceptionPublicationJudgment::unverified(format!(
            "INVALID_SIGNERS_SCHEMA: expected '{HISTORICAL_EXCEPTION_SIGNERS_SCHEMA}', found '{}'",
            signers.schema
        ));
    }

    if signers.keys.is_empty() {
        return HistoricalExceptionPublicationJudgment::unverified(
            "NO_ACCEPTED_AUTHORITY: accepted historical-exception signer trust root contains zero keys",
        );
    }

    if decisions.is_empty() {
        return HistoricalExceptionPublicationJudgment {
            schema: HISTORICAL_EXCEPTION_JUDGMENT_SCHEMA.to_string(),
            history_proven: false,
            exception_authorized: false,
            policy_permits_exceptional_admission: signers.policy_permits_exceptional_admission,
            identity_assurance: "none".to_string(),
            authorized_exceptions: Vec::new(),
            rejection_reasons: vec![
                "NO_EXCEPTION_DECISION: no offline historical-exception decision supplied"
                    .to_string(),
            ],
            provenance: Vec::new(),
        };
    }

    let mut rejection_reasons = Vec::new();
    let mut authorized = Vec::new();
    let mut provenance = Vec::new();
    let mut seen_ids = BTreeSet::new();
    let expected_commits: BTreeSet<&str> =
        ctx.affected_commits.iter().map(String::as_str).collect();
    let expected_trees: BTreeSet<&str> = ctx.affected_trees.iter().map(String::as_str).collect();
    let required_obligations: BTreeSet<&str> = ctx
        .required_obligations
        .iter()
        .map(String::as_str)
        .collect();

    // Detect conflicting authority across supplied decisions before admitting any.
    let mut seen_authority_fingerprints: BTreeSet<(&str, &str, Vec<&str>)> = BTreeSet::new();
    for decision in decisions {
        let mut obs: Vec<&str> = decision.obligations.iter().map(String::as_str).collect();
        obs.sort_unstable();
        seen_authority_fingerprints.insert((
            decision.candidate_commit.as_str(),
            decision.reason.as_str(),
            obs,
        ));
    }
    if decisions.len() > 1 && seen_authority_fingerprints.len() > 1 {
        let mut judgment = HistoricalExceptionPublicationJudgment::unverified(
            "CONFLICTING_AUTHORITY: multiple historical-exception decisions disagree on candidate obligations or rationale",
        );
        judgment.policy_permits_exceptional_admission =
            signers.policy_permits_exceptional_admission;
        return judgment;
    }

    for decision in decisions {
        let mut decision_errors = Vec::new();

        if decision.schema != HISTORICAL_EXCEPTION_SCHEMA {
            decision_errors.push(format!(
                "INVALID_DECISION_SCHEMA: expected '{HISTORICAL_EXCEPTION_SCHEMA}'"
            ));
        }
        if !valid_id(&decision.decision_id, 128) {
            decision_errors.push("INVALID_DECISION_ID: malformed or empty decision_id".to_string());
        }
        if !valid_id(&decision.authorizer_key_id, 64) {
            decision_errors
                .push("INVALID_KEY_ID: malformed or empty authorizer_key_id".to_string());
        }
        if decision.reason.trim().is_empty() {
            decision_errors.push("MISSING_REASON: exception reason must not be empty".to_string());
        }
        if decision.evidence_refs.is_empty()
            || decision.evidence_refs.iter().any(|r| r.trim().is_empty())
        {
            decision_errors
                .push("MISSING_EVIDENCE_REFS: exception must cite concrete evidence".to_string());
        }
        if ctx.consumed_decision_ids.contains(&decision.decision_id)
            || !seen_ids.insert(decision.decision_id.clone())
        {
            decision_errors.push(format!(
                "REPLAY_REJECTED: decision_id '{}' has already been consumed",
                decision.decision_id
            ));
        }
        if signers.revoked_decision_ids.contains(&decision.decision_id) {
            decision_errors.push(format!(
                "DECISION_REVOKED: decision_id '{}' is revoked in accepted policy",
                decision.decision_id
            ));
        }
        if signers
            .revoked_key_ids
            .contains(&decision.authorizer_key_id)
        {
            decision_errors.push(format!(
                "AUTHORIZER_KEY_REVOKED: key_id '{}' is revoked in accepted policy",
                decision.authorizer_key_id
            ));
        }
        if decision.issued_at > ctx.now_epoch {
            decision_errors.push(format!(
                "FUTURE_ISSUED_AT: decision '{}' issued_at {} is after current epoch {}",
                decision.decision_id, decision.issued_at, ctx.now_epoch
            ));
        }
        if decision.expires_at <= ctx.now_epoch {
            decision_errors.push(format!(
                "STALE_OR_EXPIRED: decision '{}' expired at {}",
                decision.decision_id, decision.expires_at
            ));
        }
        let ttl = decision.expires_at.saturating_sub(decision.issued_at);
        if ttl <= 0 || ttl > MAX_EXCEPTION_TTL_SECONDS {
            decision_errors.push(format!(
                "INVALID_TTL: decision '{}' TTL {}s exceeds bounded limit {}s",
                decision.decision_id, ttl, MAX_EXCEPTION_TTL_SECONDS
            ));
        }
        if decision.repository_id != ctx.repository_id {
            decision_errors.push(format!(
                "REPOSITORY_MISMATCH: decision binds '{}', expected '{}'",
                decision.repository_id, ctx.repository_id
            ));
        }
        if decision.base_commit != ctx.base_commit {
            decision_errors.push(format!(
                "BASE_COMMIT_MISMATCH: decision binds '{}', expected '{}'",
                decision.base_commit, ctx.base_commit
            ));
        }
        if decision.candidate_commit != ctx.candidate_commit {
            decision_errors.push(format!(
                "CANDIDATE_COMMIT_MISMATCH: decision binds '{}', expected '{}'",
                decision.candidate_commit, ctx.candidate_commit
            ));
        }
        let decision_commits: BTreeSet<&str> = decision
            .affected_commits
            .iter()
            .map(String::as_str)
            .collect();
        if decision_commits.is_empty() || decision_commits != expected_commits {
            decision_errors.push(
                "AFFECTED_COMMIT_MISMATCH: decision affected_commits do not exactly match observed gap commits"
                    .to_string(),
            );
        }
        let decision_trees: BTreeSet<&str> =
            decision.affected_trees.iter().map(String::as_str).collect();
        if decision_trees.is_empty() || decision_trees != expected_trees {
            decision_errors.push(
                "AFFECTED_TREE_MISMATCH: decision affected_trees do not exactly match immutable commit trees"
                    .to_string(),
            );
        }
        let decision_obligations: BTreeSet<&str> =
            decision.obligations.iter().map(String::as_str).collect();
        if decision_obligations.is_empty() || !required_obligations.is_subset(&decision_obligations)
        {
            decision_errors.push(
                "OBLIGATION_MISMATCH: decision obligations do not cover all required gap obligations"
                    .to_string(),
            );
        }
        if let Err(sig_err) = verify_decision_signature(signers, decision) {
            decision_errors.push(sig_err);
        }

        let admitted = decision_errors.is_empty();
        provenance.push(json!({
            "decision_id": decision.decision_id,
            "authorizer_key_id": decision.authorizer_key_id,
            "authorizer_authority": decision.authorizer_authority,
            "admitted": admitted,
            "errors": decision_errors,
            "evaluated_at": ctx.now_epoch,
            "history_proven": false,
        }));

        if admitted {
            authorized.push(AuthorizedHistoricalException {
                decision_id: decision.decision_id.clone(),
                authorizer_key_id: decision.authorizer_key_id.clone(),
                authorizer_authority: decision.authorizer_authority.clone(),
                obligations: decision.obligations.clone(),
                affected_commits: decision.affected_commits.clone(),
                affected_trees: decision.affected_trees.clone(),
                reason: decision.reason.clone(),
                evidence_refs: decision.evidence_refs.clone(),
                issued_at: decision.issued_at,
                expires_at: decision.expires_at,
            });
        } else {
            rejection_reasons.extend(decision_errors);
        }
    }

    let exception_authorized = !authorized.is_empty() && rejection_reasons.is_empty();
    let policy_permits = signers.policy_permits_exceptional_admission;
    if exception_authorized && !policy_permits {
        rejection_reasons.push(
            "POLICY_DISALLOWS_EXCEPTIONAL_ADMISSION: accepted trust root has policy_permits_exceptional_admission=false"
                .to_string(),
        );
    }

    HistoricalExceptionPublicationJudgment {
        schema: HISTORICAL_EXCEPTION_JUDGMENT_SCHEMA.to_string(),
        history_proven: false, // ALWAYS false: exceptions never fabricate historical proof.
        exception_authorized,
        policy_permits_exceptional_admission: policy_permits,
        identity_assurance: if exception_authorized {
            "offline_ed25519_accepted_trust_root".to_string()
        } else {
            "none".to_string()
        },
        authorized_exceptions: if exception_authorized {
            authorized
        } else {
            Vec::new()
        },
        rejection_reasons,
        provenance,
    }
}

/// Load accepted historical-exception signers strictly from an immutable accepted
/// git ref (`<accepted_base>:.decapod/historical-exception-signers.json`), never
/// from the candidate worktree filesystem.
pub fn load_accepted_signers_from_git_ref(
    repo_root: &Path,
    accepted_base_ref: &str,
) -> Option<HistoricalExceptionSigners> {
    let spec = format!("{accepted_base_ref}:{HISTORICAL_EXCEPTION_SIGNERS_PATH}");
    let out = Command::new("git")
        .current_dir(repo_root)
        .args(["show", &spec])
        .output()
        .ok()?;
    if !out.status.success() || out.stdout.len() > MAX_SIGNERS_BYTES {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

/// Evaluate any environment-supplied offline exception envelope against the
/// accepted base ref's trust root for a history-recovery observation.
pub fn evaluate_observation_exception(
    repo_root: &Path,
    accepted_base_ref: &str,
    source_commit: &str,
    source_tree: &str,
    destination_base: &str,
) -> HistoricalExceptionPublicationJudgment {
    let Some(raw_env) = std::env::var_os(HISTORICAL_EXCEPTION_ENV) else {
        return HistoricalExceptionPublicationJudgment::unverified(
            "NO_EXCEPTION_DECISION: no offline historical-exception decision supplied",
        );
    };
    let raw_bytes = raw_env.to_string_lossy().into_owned().into_bytes();
    if raw_bytes.len() > MAX_DECISION_BYTES {
        return HistoricalExceptionPublicationJudgment::unverified(
            "OVERSIZED_EXCEPTION_DECISION: envelope exceeds maximum byte limit",
        );
    }
    let Ok(decision) = serde_json::from_slice::<HistoricalExceptionDecision>(&raw_bytes) else {
        return HistoricalExceptionPublicationJudgment::unverified(
            "MALFORMED_EXCEPTION_DECISION: invalid JSON schema for historical exception",
        );
    };
    let signers = load_accepted_signers_from_git_ref(repo_root, accepted_base_ref);
    let repository_id = crate::core::repo_identity::resolve_repository_identity(repo_root)
        .map(|id| id.canonical_name)
        .unwrap_or_else(|_| "unknown-repo".to_string());
    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let ctx = HistoricalExceptionContext {
        repository_id,
        base_commit: destination_base.to_string(),
        candidate_commit: source_commit.to_string(),
        affected_commits: vec![source_commit.to_string()],
        affected_trees: vec![source_tree.to_string()],
        required_obligations: vec!["checkpoint_history".to_string()],
        now_epoch,
        consumed_decision_ids: BTreeSet::new(),
        signers_from_candidate_worktree: false,
    };
    verify_historical_exceptions(signers.as_ref(), &[decision], &ctx)
}
