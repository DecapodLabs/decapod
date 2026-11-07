//! Governed transition kernel.
//!
//! Decapod does not choose the next unit of work. It resolves canonical state,
//! decides whether an attempted transition is admissible, and decides whether
//! the resulting claim is proven. Agent-written status, notes, and booleans are
//! declarations. They are never evidence.

use crate::core::events;
use crate::core::state::{self, CanonicalSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub const TRANSITION_OP: &str = "govern.transition";

/// Lifecycle of one governed attempt. These are not interchangeable with success.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionPhase {
    Requested,
    Admissible,
    Rejected,
    Attempted,
    Applied,
    Validated,
    Proven,
    Failed,
    Stale,
    Unknown,
}

/// Whether a claim about resulting state is proven.
///
/// `NotProven` means the claim was not established. `Failed` means evidence
/// shows the claim is false. `Unknown` means canonical state or evidence is
/// missing, contradictory, or corrupt. `Stale` means evidence was for a
/// different governing identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofJudgment {
    Proven,
    Failed,
    NotProven,
    Stale,
    Unknown,
}

impl ProofJudgment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proven => "proven",
            Self::Failed => "failed",
            Self::NotProven => "not_proven",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
        }
    }

    pub fn is_success(self) -> bool {
        matches!(self, Self::Proven)
    }
}

/// Identity of the governing inputs a proof or authorization was bound to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateIdentity {
    pub head: String,
    pub rules_fingerprint: String,
    pub verifier: String,
}

impl StateIdentity {
    pub fn complete(&self) -> bool {
        !blank(&self.head) && !blank(&self.rules_fingerprint) && !blank(&self.verifier)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRecord {
    pub state: StateIdentity,
    pub output_hash: String,
    pub inputs_hash: String,
    /// Observed validator outcome. This is not an agent-set completion flag.
    pub observed_pass: bool,
    pub corrupt: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionClaim {
    pub declared_status: Option<String>,
    pub passes: Option<bool>,
    pub notes: Option<String>,
    pub evidence: Option<EvidenceRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionTrace {
    pub op: String,
    pub requested: String,
    pub actor: String,
    pub prior_state: String,
    pub rules_fingerprint: String,
    pub verifier: String,
    pub admission: String,
    pub scope: Vec<String>,
    pub expected_effects: Vec<String>,
    pub observed_effects: Vec<String>,
    pub evidence_hash: Option<String>,
    pub resulting_state: String,
    pub outcome: String,
    pub reason: String,
    pub freshness: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationDecision {
    pub eligible: bool,
    pub judgment: ProofJudgment,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeClaim {
    pub actor: String,
    pub paths: BTreeSet<String>,
    pub exclusive: bool,
    pub lease_expired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionRequest {
    pub actor: String,
    pub paths: BTreeSet<String>,
    pub forbidden: BTreeSet<String>,
    pub file_touch_budget: Option<usize>,
    pub authorized: StateIdentity,
    pub other_claims: Vec<ScopeClaim>,
    pub prerequisites_proven: bool,
    pub own_lease_expired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admission {
    pub phase: TransitionPhase,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableWork {
    pub operation_id: String,
    pub intent: String,
    pub authorized: StateIdentity,
    pub phase: TransitionPhase,
    pub mutations: Vec<String>,
    pub evidence: Option<EvidenceRecord>,
    /// Human-readable notes. Recovery must not read this field as authority.
    pub notes: String,
    pub outstanding: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovery {
    pub judgment: ProofJudgment,
    pub phase: TransitionPhase,
    pub outstanding: Vec<String>,
    pub can_resume: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub id: String,
    pub digest: String,
    pub effect: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied,
    Replay,
    Conflict,
    Unknown,
}

#[derive(Debug, Default, Clone)]
pub struct ApplyLog {
    effects: Vec<String>,
    seen: BTreeMap<String, String>,
}

impl ApplyLog {
    pub fn effects(&self) -> &[String] {
        &self.effects
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerSession {
    pub caller_id: String,
    pub foreground_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerResolution {
    pub caller_id: String,
    pub judgment: ProofJudgment,
    pub identity: Option<StateIdentity>,
    /// Own session note. Another caller's note is not copied here.
    pub own_note: String,
    pub reason: String,
}

pub fn judge_completion(claim: &CompletionClaim, current: &StateIdentity) -> ProofJudgment {
    let _declared = &claim.declared_status;
    let _passes = claim.passes;
    let _notes = &claim.notes;
    let Some(evidence) = &claim.evidence else {
        return ProofJudgment::NotProven;
    };
    if evidence.corrupt || !digest_ok(&evidence.output_hash) || !digest_ok(&evidence.inputs_hash) {
        return ProofJudgment::Unknown;
    }
    if !current.complete() || !evidence.state.complete() {
        return ProofJudgment::Unknown;
    }
    if evidence.state != *current {
        return ProofJudgment::Stale;
    }
    if evidence.observed_pass {
        ProofJudgment::Proven
    } else {
        ProofJudgment::Failed
    }
}

pub fn substantive_task_proof(artifacts: &Value) -> bool {
    let Some(results) = artifacts
        .get("proof_plan_results")
        .and_then(Value::as_array)
    else {
        return false;
    };
    results.iter().any(|result| {
        let Some(object) = result.as_object() else {
            return false;
        };
        let gate = object
            .get("proof_gate")
            .and_then(Value::as_str)
            .unwrap_or("");
        let hash = object
            .get("output_hash")
            .and_then(Value::as_str)
            .unwrap_or("");
        let status_ok = match object.get("status").and_then(Value::as_str) {
            None => true,
            Some(status) => success_word(status),
        };
        status_ok && !blank(gate) && digest_ok(hash)
    })
}

pub fn decide_publication(
    judgment: ProofJudgment,
    canonical_known: bool,
    prior_receipt: Option<&str>,
    receipt: Option<&str>,
) -> PublicationDecision {
    if !canonical_known {
        return PublicationDecision {
            eligible: false,
            judgment: ProofJudgment::Unknown,
            reason: "canonical state is missing".to_string(),
        };
    }
    if let (Some(prior), Some(receipt)) = (prior_receipt, receipt) {
        if !digest_ok(prior) || !digest_ok(receipt) {
            return PublicationDecision {
                eligible: false,
                judgment: ProofJudgment::Unknown,
                reason: "publication receipt is not a digest".to_string(),
            };
        }
        if prior != receipt {
            return PublicationDecision {
                eligible: false,
                judgment: ProofJudgment::Unknown,
                reason: "duplicate publication uses a different receipt".to_string(),
            };
        }
    }
    match judgment {
        ProofJudgment::Proven => PublicationDecision {
            eligible: true,
            judgment,
            reason: if prior_receipt.is_some() {
                "fresh proof matches the recorded publication receipt".to_string()
            } else {
                "fresh proof satisfies publication".to_string()
            },
        },
        ProofJudgment::Stale => PublicationDecision {
            eligible: false,
            judgment,
            reason: "proof is stale for the current governing identity".to_string(),
        },
        ProofJudgment::Failed => PublicationDecision {
            eligible: false,
            judgment,
            reason: "evidence shows the claim failed".to_string(),
        },
        ProofJudgment::NotProven => PublicationDecision {
            eligible: false,
            judgment,
            reason: "completion is not proven".to_string(),
        },
        ProofJudgment::Unknown => PublicationDecision {
            eligible: false,
            judgment,
            reason: "publication evidence is unknown".to_string(),
        },
    }
}

pub fn project_report_publication(report: &Value) -> PublicationDecision {
    if !report.is_object() {
        return decide_publication(ProofJudgment::Unknown, false, None, None);
    }
    let status = report.get("status").and_then(Value::as_str);
    let head = first_string(report, &["head", "candidate", "git_revision"]);
    let receipt = first_string(
        report,
        &["receipt_hash", "validation_receipt_hash", "evidence_hash"],
    );
    let blockers = blocker_codes(report);
    let stale_marker = status.is_some_and(|value| value.eq_ignore_ascii_case("stale"))
        || blockers
            .iter()
            .any(|code| code.to_ascii_lowercase().contains("stale"));
    let canonical_known = head.as_deref().is_some_and(|value| !blank(value));
    if status.is_none() && head.is_none() && receipt.is_none() {
        return decide_publication(ProofJudgment::Unknown, false, None, None);
    }
    if report.get("status").is_some() && status.is_none() {
        return decide_publication(ProofJudgment::Unknown, canonical_known, None, None);
    }
    let current_rules = first_string(report, &["current_rules_fingerprint"]);
    let proof_rules = first_string(report, &["rules_fingerprint"]);
    let current_verifier = first_string(report, &["current_verifier"]);
    let proof_verifier = first_string(report, &["verifier"]);
    let judgment = if stale_marker {
        ProofJudgment::Stale
    } else if identity_pair_unknown(proof_rules.as_deref(), current_rules.as_deref())
        || identity_pair_unknown(proof_verifier.as_deref(), current_verifier.as_deref())
    {
        ProofJudgment::Unknown
    } else if identity_pair_stale(proof_rules.as_deref(), current_rules.as_deref())
        || identity_pair_stale(proof_verifier.as_deref(), current_verifier.as_deref())
    {
        ProofJudgment::Stale
    } else if status.is_some_and(failure_word) {
        ProofJudgment::Failed
    } else if !receipt.as_deref().is_some_and(digest_ok) || !status.is_some_and(success_word) {
        ProofJudgment::NotProven
    } else if !canonical_known {
        ProofJudgment::Unknown
    } else {
        ProofJudgment::Proven
    };
    let _declared_ready = report.get("publication_ready").and_then(Value::as_bool);
    let _declared_passes = report.get("passes").and_then(Value::as_bool);
    decide_publication(
        judgment,
        canonical_known
            || matches!(
                judgment,
                ProofJudgment::Unknown | ProofJudgment::Stale | ProofJudgment::Failed
            ),
        None,
        receipt.as_deref(),
    )
}

pub fn admit(current: &StateIdentity, request: &AdmissionRequest) -> Admission {
    if !current.complete() || !request.authorized.complete() {
        return Admission {
            phase: TransitionPhase::Unknown,
            reason: "governing identity is incomplete".to_string(),
        };
    }
    if request.authorized != *current {
        return Admission {
            phase: TransitionPhase::Stale,
            reason: "authorization was not issued for the current state".to_string(),
        };
    }
    if request.own_lease_expired {
        return Admission {
            phase: TransitionPhase::Rejected,
            reason: "caller lease is expired".to_string(),
        };
    }
    if let Some(limit) = request.file_touch_budget
        && request.paths.len() > limit
    {
        return Admission {
            phase: TransitionPhase::Rejected,
            reason: "change exceeds the file-touch budget".to_string(),
        };
    }
    if request.paths.iter().any(|path| {
        request
            .forbidden
            .iter()
            .any(|forbidden| path == forbidden || path.starts_with(&format!("{forbidden}/")))
    }) {
        return Admission {
            phase: TransitionPhase::Rejected,
            reason: "change includes a forbidden path".to_string(),
        };
    }
    if !request.prerequisites_proven {
        return Admission {
            phase: TransitionPhase::Rejected,
            reason: "prerequisites are not proven".to_string(),
        };
    }
    for claim in &request.other_claims {
        if claim.actor == request.actor || !claim.exclusive || claim.lease_expired {
            continue;
        }
        if claim.paths.intersection(&request.paths).next().is_some() {
            return Admission {
                phase: TransitionPhase::Rejected,
                reason: format!("exclusive scope conflicts with {}", claim.actor),
            };
        }
    }
    Admission {
        phase: TransitionPhase::Admissible,
        reason: "attempt is inside the authorized governing identity".to_string(),
    }
}

pub fn recover(work: Option<&DurableWork>, current: &StateIdentity, prose: &str) -> Recovery {
    let _ = prose;
    let Some(work) = work else {
        return Recovery {
            judgment: ProofJudgment::Unknown,
            phase: TransitionPhase::Unknown,
            outstanding: Vec::new(),
            can_resume: false,
            reason: "no durable governed record; conversational notes are not state".to_string(),
        };
    };
    if !current.complete() {
        return Recovery {
            judgment: ProofJudgment::Unknown,
            phase: TransitionPhase::Unknown,
            outstanding: work.outstanding.clone(),
            can_resume: false,
            reason: "current canonical identity is incomplete".to_string(),
        };
    }
    let judgment = judge_completion(
        &CompletionClaim {
            declared_status: None,
            passes: None,
            notes: Some(work.notes.clone()),
            evidence: work.evidence.clone(),
        },
        current,
    );
    let incomplete = matches!(
        work.phase,
        TransitionPhase::Attempted | TransitionPhase::Applied | TransitionPhase::Requested
    ) && !judgment.is_success();
    let can_resume = judgment.is_success()
        || (work.authorized == *current
            && matches!(
                work.phase,
                TransitionPhase::Admissible
                    | TransitionPhase::Attempted
                    | TransitionPhase::Applied
                    | TransitionPhase::Validated
            ));
    let reason = if incomplete {
        "mutations were recorded without a fresh proof".to_string()
    } else if work.authorized != *current && !judgment.is_success() {
        "authorization was bound to a previous state".to_string()
    } else {
        match judgment {
            ProofJudgment::Proven => "evidence still matches canonical state".to_string(),
            ProofJudgment::NotProven => "durable record does not prove completion".to_string(),
            ProofJudgment::Stale => "recorded evidence is stale".to_string(),
            ProofJudgment::Failed => "recorded evidence failed".to_string(),
            ProofJudgment::Unknown => "recorded evidence cannot be interpreted".to_string(),
        }
    };
    Recovery {
        judgment,
        phase: if judgment.is_success() {
            TransitionPhase::Proven
        } else if incomplete {
            TransitionPhase::Applied
        } else {
            work.phase
        },
        outstanding: if judgment.is_success() {
            Vec::new()
        } else {
            work.outstanding.clone()
        },
        can_resume,
        reason,
    }
}

pub fn apply_operation(log: &mut ApplyLog, operation: &Operation) -> ApplyOutcome {
    if blank(&operation.id) || blank(&operation.digest) {
        return ApplyOutcome::Unknown;
    }
    if let Some(previous) = log.seen.get(&operation.id) {
        return if previous == &operation.digest {
            ApplyOutcome::Replay
        } else {
            ApplyOutcome::Conflict
        };
    }
    log.seen
        .insert(operation.id.clone(), operation.digest.clone());
    log.effects.push(operation.effect.clone());
    ApplyOutcome::Applied
}

pub fn resolve_caller(
    canonical: Option<&StateIdentity>,
    caller: &CallerSession,
    _other: &CallerSession,
) -> CallerResolution {
    match canonical {
        Some(identity) if identity.complete() => CallerResolution {
            caller_id: caller.caller_id.clone(),
            judgment: ProofJudgment::NotProven,
            identity: Some(identity.clone()),
            own_note: caller.foreground_note.clone(),
            reason: "canonical identity is shared; session notes are not authority".to_string(),
        },
        Some(_) | None => CallerResolution {
            caller_id: caller.caller_id.clone(),
            judgment: ProofJudgment::Unknown,
            identity: None,
            own_note: caller.foreground_note.clone(),
            reason: "canonical state is missing or incomplete".to_string(),
        },
    }
}

/// Governing identity taken only from the canonical state resolver.
pub fn identity_from_snapshot(snapshot: &CanonicalSnapshot) -> StateIdentity {
    let rules_fingerprint = snapshot
        .provenance
        .governance_document_hash
        .clone()
        .or_else(|| snapshot.provenance.validation_receipt_hash.clone())
        .unwrap_or_default();
    StateIdentity {
        head: snapshot.revision.head_sha.clone(),
        rules_fingerprint,
        verifier: snapshot.provenance.decapod_version.clone(),
    }
}

/// Judge a persisted verification row by reconstructing its evidence.
///
/// The row is an index. `last_verified_status` and a self-asserted digest do
/// not establish `Proven`. File bytes are hashed again, and the proof digest
/// must equal that reconstruction bound to the canonical snapshot.
pub fn judge_stored_verification(
    project_root: &Path,
    stored_status: &str,
    artifacts: &Value,
) -> Result<ProofJudgment, crate::core::error::DecapodError> {
    let snapshot = state::canonical_snapshot(project_root)?;
    Ok(judge_reconstructed(
        project_root,
        &identity_from_snapshot(&snapshot),
        stored_status,
        artifacts,
    ))
}

pub fn judge_reconstructed(
    project_root: &Path,
    identity: &StateIdentity,
    stored_status: &str,
    artifacts: &Value,
) -> ProofJudgment {
    let _ = stored_status;
    if !identity.complete() {
        return ProofJudgment::Unknown;
    }
    let Some(files) = artifacts.get("file_artifacts").and_then(Value::as_array) else {
        return ProofJudgment::NotProven;
    };
    if files.is_empty() {
        return ProofJudgment::NotProven;
    }
    let mut observed = BTreeMap::new();
    for file in files {
        let Some(path) = file.get("path").and_then(Value::as_str) else {
            return ProofJudgment::Unknown;
        };
        if !safe_relative(path) {
            return ProofJudgment::Unknown;
        }
        let claimed = file.get("hash").and_then(Value::as_str).unwrap_or("");
        if !digest_ok(claimed) {
            return ProofJudgment::NotProven;
        }
        let actual = match hash_file(&project_root.join(path)) {
            Ok(hash) => hash,
            Err(_) => return ProofJudgment::Unknown,
        };
        if actual != claimed.trim() && format!("sha256:{actual}") != claimed.trim() {
            return ProofJudgment::NotProven;
        }
        observed.insert(path.to_string(), actual);
    }
    let bound_head = artifacts.get("canonical_head").and_then(Value::as_str);
    let bound_rules = artifacts.get("rules_fingerprint").and_then(Value::as_str);
    let bound_verifier = artifacts.get("verifier").and_then(Value::as_str);
    match (bound_head, bound_rules, bound_verifier) {
        (Some(head), Some(rules), Some(verifier))
            if head == identity.head
                && rules == identity.rules_fingerprint
                && verifier == identity.verifier => {}
        (Some(_), Some(_), Some(_)) => return ProofJudgment::Stale,
        _ => return ProofJudgment::NotProven,
    }
    let recomputed = reconstruction_digest(&observed, identity);
    let claimed_digest = artifacts
        .get("receipt_hash")
        .or_else(|| artifacts.get("output_hash"))
        .and_then(Value::as_str)
        .or_else(|| {
            artifacts
                .pointer("/proof_plan_results/0/output_hash")
                .and_then(Value::as_str)
        });
    match claimed_digest {
        Some(digest) if digest == recomputed || digest == format!("sha256:{recomputed}") => {
            ProofJudgment::Proven
        }
        Some(_) => ProofJudgment::NotProven,
        None => ProofJudgment::NotProven,
    }
}

fn reconstruction_digest(files: &BTreeMap<String, String>, identity: &StateIdentity) -> String {
    let mut hasher = Sha256::new();
    for (path, hash) in files {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(hash.as_bytes());
        hasher.update([0]);
    }
    hasher.update(identity.head.as_bytes());
    hasher.update([0]);
    hasher.update(identity.rules_fingerprint.as_bytes());
    hasher.update([0]);
    hasher.update(identity.verifier.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn hash_file(path: &Path) -> Result<String, std::io::Error> {
    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn safe_relative(path: &str) -> bool {
    let path = Path::new(path);
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

/// Append one governed-transition decision to the existing traces stream.
///
/// The flight recorder renders this stream. It is provenance, not canonical state.
pub fn record_transition(
    store_root: &Path,
    trace: &TransitionTrace,
) -> Result<u64, crate::core::error::DecapodError> {
    let mut event = trace_json(trace);
    let Some(object) = event.as_object_mut() else {
        return Err(crate::core::error::DecapodError::ValidationError(
            "transition trace is not a JSON object".into(),
        ));
    };
    object.insert(
        "event_type".to_string(),
        Value::String(TRANSITION_OP.to_string()),
    );
    object.insert("op".to_string(), Value::String(TRANSITION_OP.to_string()));
    object.insert(
        "ts".to_string(),
        Value::String(crate::core::time::now_epoch_z()),
    );
    object.insert("actor".to_string(), Value::String(trace.actor.clone()));
    object.insert("status".to_string(), Value::String(trace.outcome.clone()));
    object.insert(
        "event_id".to_string(),
        Value::String(format!(
            "govern-{}-{}",
            trace.prior_state,
            trace.evidence_hash.as_deref().unwrap_or("none")
        )),
    );
    events::append(store_root, events::TRACES, &event)
}

pub fn commit_freshness(committed: &StateIdentity, current: &StateIdentity) -> ProofJudgment {
    if !committed.complete() || !current.complete() {
        ProofJudgment::Unknown
    } else if committed != current {
        ProofJudgment::Stale
    } else {
        ProofJudgment::Proven
    }
}

pub struct TraceInput<'a> {
    pub actor: &'a str,
    pub requested: &'a str,
    pub prior: &'a StateIdentity,
    pub admission: &'a Admission,
    pub scope: &'a [String],
    pub expected: &'a [String],
    pub observed: &'a [String],
    pub evidence_hash: Option<&'a str>,
    pub judgment: ProofJudgment,
    pub reason: &'a str,
}

pub fn transition_trace(input: TraceInput<'_>) -> TransitionTrace {
    TransitionTrace {
        op: TRANSITION_OP.to_string(),
        requested: input.requested.to_string(),
        actor: input.actor.to_string(),
        prior_state: input.prior.head.clone(),
        rules_fingerprint: input.prior.rules_fingerprint.clone(),
        verifier: input.prior.verifier.clone(),
        admission: format!("{:?}", input.admission.phase).to_ascii_lowercase(),
        scope: input.scope.to_vec(),
        expected_effects: input.expected.to_vec(),
        observed_effects: input.observed.to_vec(),
        evidence_hash: input.evidence_hash.map(str::to_string),
        resulting_state: input.prior.head.clone(),
        outcome: input.judgment.as_str().to_string(),
        reason: input.reason.to_string(),
        freshness: input.judgment.as_str().to_string(),
    }
}

pub fn trace_json(trace: &TransitionTrace) -> Value {
    let mut value = serde_json::to_value(trace).unwrap_or_else(|_| Value::Object(Map::new()));
    if let Some(object) = value.as_object_mut() {
        object.remove("notes");
        object.remove("passes");
        object.remove("declared_status");
    }
    value
}

fn blank(value: &str) -> bool {
    value.trim().is_empty()
}

fn digest_ok(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() || sentinel(value) {
        return false;
    }
    let body = value.strip_prefix("sha256:").unwrap_or(value);
    !body.is_empty()
        && !sentinel(body)
        && body.len() >= 8
        && body
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn sentinel(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "true"
            | "false"
            | "pass"
            | "passed"
            | "verified"
            | "present"
            | "yes"
            | "done"
            | "ok"
            | "success"
            | "complete"
    )
}

fn success_word(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "pass" | "passed" | "ok" | "proven" | "verified"
    )
}

fn failure_word(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "fail" | "failed" | "error"
    )
}

fn first_string(report: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = report.get(*key).and_then(Value::as_str) {
            return Some(value.to_string());
        }
    }
    None
}

fn blocker_codes(report: &Value) -> Vec<String> {
    report
        .pointer("/readiness/blockers")
        .and_then(Value::as_array)
        .map(|blockers| {
            blockers
                .iter()
                .filter_map(|blocker| {
                    blocker
                        .get("code")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn identity_pair_unknown(proof: Option<&str>, current: Option<&str>) -> bool {
    match (proof, current) {
        (Some(proof), Some(current)) => blank(proof) || blank(current),
        (Some(_), None) | (None, Some(_)) => true,
        (None, None) => false,
    }
}

fn identity_pair_stale(proof: Option<&str>, current: Option<&str>) -> bool {
    match (proof, current) {
        (Some(proof), Some(current)) => !blank(proof) && !blank(current) && proof != current,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(head: &str) -> StateIdentity {
        StateIdentity {
            head: head.to_string(),
            rules_fingerprint: "rules-1".to_string(),
            verifier: "decapod-0.130.2".to_string(),
        }
    }

    fn evidence(head: &str, pass: bool) -> EvidenceRecord {
        EvidenceRecord {
            state: identity(head),
            output_hash: "sha256:0123456789abcdef".to_string(),
            inputs_hash: "sha256:fedcba9876543210".to_string(),
            observed_pass: pass,
            corrupt: false,
        }
    }

    #[test]
    fn declaration_without_evidence_is_not_proven() {
        let judgment = judge_completion(
            &CompletionClaim {
                declared_status: Some("done".to_string()),
                passes: Some(true),
                notes: Some("all tests passed".to_string()),
                evidence: None,
            },
            &identity("s0"),
        );
        assert_eq!(judgment, ProofJudgment::NotProven);
        assert!(!decide_publication(judgment, true, None, None).eligible);
    }

    #[test]
    fn status_field_and_notes_do_not_override_missing_proof() {
        let artifacts = serde_json::json!({
            "task_id": "task-1",
            "task_revision": 4,
            "passes": true,
            "notes": "marked done by the previous agent"
        });
        assert!(!substantive_task_proof(&artifacts));
        let judgment = judge_completion(
            &CompletionClaim {
                declared_status: Some("done".to_string()),
                passes: Some(true),
                notes: Some("progress.txt says complete".to_string()),
                evidence: None,
            },
            &identity("s0"),
        );
        assert_eq!(judgment, ProofJudgment::NotProven);
    }

    #[test]
    fn governed_file_change_stales_proof() {
        let judgment = judge_completion(
            &CompletionClaim {
                declared_status: Some("pass".to_string()),
                passes: Some(true),
                notes: None,
                evidence: Some(evidence("s0", true)),
            },
            &identity("s1"),
        );
        assert_eq!(judgment, ProofJudgment::Stale);
        assert!(!decide_publication(judgment, true, None, None).eligible);
    }

    #[test]
    fn rule_or_verifier_change_stales_proof() {
        let mut moved_rules = identity("s0");
        moved_rules.rules_fingerprint = "rules-2".to_string();
        assert_eq!(
            judge_completion(
                &CompletionClaim {
                    declared_status: None,
                    passes: None,
                    notes: None,
                    evidence: Some(evidence("s0", true)),
                },
                &moved_rules,
            ),
            ProofJudgment::Stale
        );
        let mut moved_verifier = identity("s0");
        moved_verifier.verifier = "decapod-0.131.0".to_string();
        assert_eq!(
            commit_freshness(&identity("s0"), &moved_verifier),
            ProofJudgment::Stale
        );
    }

    #[test]
    fn fresh_evidence_can_publish_and_stale_cannot() {
        let proven = judge_completion(
            &CompletionClaim {
                declared_status: None,
                passes: None,
                notes: None,
                evidence: Some(evidence("s0", true)),
            },
            &identity("s0"),
        );
        assert_eq!(proven, ProofJudgment::Proven);
        assert!(decide_publication(proven, true, None, None).eligible);
        let report = serde_json::json!({
            "status": "pass",
            "publication_ready": true,
            "passes": true,
            "head": "aaaaaaaaaaaaaaaa",
            "receipt_hash": "sha256:0123456789abcdef"
        });
        assert!(project_report_publication(&report).eligible);
        let stale = serde_json::json!({
            "status": "pass",
            "publication_ready": true,
            "head": "aaaaaaaaaaaaaaaa",
            "receipt_hash": "sha256:0123456789abcdef",
            "rules_fingerprint": "rules-1",
            "current_rules_fingerprint": "rules-2"
        });
        let decision = project_report_publication(&stale);
        assert_eq!(decision.judgment, ProofJudgment::Stale);
        assert!(!decision.eligible);
    }

    #[test]
    fn boolean_report_is_not_publication() {
        let report = serde_json::json!({
            "passes": true,
            "publication_ready": true,
            "status": "done"
        });
        let decision = project_report_publication(&report);
        assert!(!decision.eligible);
        assert_ne!(decision.judgment, ProofJudgment::Proven);
    }

    #[test]
    fn session_loss_reconstructs_from_durable_record_not_prose() {
        let work = DurableWork {
            operation_id: "op-1".to_string(),
            intent: "govern transitions".to_string(),
            authorized: identity("s0"),
            phase: TransitionPhase::Applied,
            mutations: vec!["src/decapod/core/governed_transition.rs".to_string()],
            evidence: None,
            notes: "finished, passes true, safe to publish".to_string(),
            outstanding: vec!["proof".to_string()],
        };
        let recovery = recover(
            Some(&work),
            &identity("s0"),
            "the previous agent said the task was done",
        );
        assert_eq!(recovery.judgment, ProofJudgment::NotProven);
        assert_eq!(recovery.outstanding, vec!["proof".to_string()]);
        assert!(recovery.can_resume);
        assert!(
            recover(None, &identity("s0"), "continue from memory").judgment
                == ProofJudgment::Unknown
        );
    }

    #[test]
    fn handoff_proves_only_when_the_next_session_has_fresh_evidence() {
        let mut work = DurableWork {
            operation_id: "op-2".to_string(),
            intent: "bounded change".to_string(),
            authorized: identity("s0"),
            phase: TransitionPhase::Validated,
            mutations: vec!["src/lib.rs".to_string()],
            evidence: Some(evidence("s0", true)),
            notes: "ignore me".to_string(),
            outstanding: vec!["publish".to_string()],
        };
        let resumed = recover(Some(&work), &identity("s0"), "");
        assert_eq!(resumed.judgment, ProofJudgment::Proven);
        assert!(resumed.outstanding.is_empty());
        work.evidence.as_mut().unwrap().state = identity("s0");
        work.authorized = identity("s0");
        assert_eq!(
            recover(Some(&work), &identity("s1"), "still done").judgment,
            ProofJudgment::Stale
        );
    }

    #[test]
    fn conflicting_exclusive_claims_are_rejected() {
        let mut paths = BTreeSet::new();
        paths.insert("src/decapod/core/todo.rs".to_string());
        let request = AdmissionRequest {
            actor: "agent-a".to_string(),
            paths: paths.clone(),
            forbidden: BTreeSet::new(),
            file_touch_budget: Some(8),
            authorized: identity("s0"),
            other_claims: vec![ScopeClaim {
                actor: "agent-b".to_string(),
                paths,
                exclusive: true,
                lease_expired: false,
            }],
            prerequisites_proven: true,
            own_lease_expired: false,
        };
        assert_eq!(
            admit(&identity("s0"), &request).phase,
            TransitionPhase::Rejected
        );
        let mut shared = request.clone();
        shared.other_claims[0].exclusive = false;
        assert_eq!(
            admit(&identity("s0"), &shared).phase,
            TransitionPhase::Admissible
        );
    }

    #[test]
    fn authorization_against_s0_is_stale_at_s1() {
        let request = AdmissionRequest {
            actor: "agent-a".to_string(),
            paths: BTreeSet::from(["src/main.rs".to_string()]),
            forbidden: BTreeSet::new(),
            file_touch_budget: Some(3),
            authorized: identity("s0"),
            other_claims: Vec::new(),
            prerequisites_proven: true,
            own_lease_expired: false,
        };
        assert_eq!(
            admit(&identity("s1"), &request).phase,
            TransitionPhase::Stale
        );
    }

    #[test]
    fn crash_before_validation_stays_unproven_and_retry_is_idempotent() {
        let work = DurableWork {
            operation_id: "op-crash".to_string(),
            intent: "edit".to_string(),
            authorized: identity("s0"),
            phase: TransitionPhase::Applied,
            mutations: vec!["src/main.rs".to_string()],
            evidence: None,
            notes: String::new(),
            outstanding: vec!["validate".to_string()],
        };
        let recovery = recover(Some(&work), &identity("s0"), "");
        assert_eq!(recovery.phase, TransitionPhase::Applied);
        assert_eq!(recovery.judgment, ProofJudgment::NotProven);
        let operation = Operation {
            id: "op-crash".to_string(),
            digest: "sha256:0123456789abcdef".to_string(),
            effect: "write src/main.rs".to_string(),
        };
        let mut log = ApplyLog::default();
        assert_eq!(apply_operation(&mut log, &operation), ApplyOutcome::Applied);
        assert_eq!(apply_operation(&mut log, &operation), ApplyOutcome::Replay);
        assert_eq!(log.effects().len(), 1);
        let mut conflict = operation.clone();
        conflict.digest = "sha256:ffffffffffffffff".to_string();
        assert_eq!(apply_operation(&mut log, &conflict), ApplyOutcome::Conflict);
        assert_eq!(log.effects().len(), 1);
    }

    #[test]
    fn corrupt_or_missing_canonical_state_is_unknown() {
        let mut broken = evidence("s0", true);
        broken.corrupt = true;
        assert_eq!(
            judge_completion(
                &CompletionClaim {
                    declared_status: Some("pass".to_string()),
                    passes: Some(true),
                    notes: None,
                    evidence: Some(broken),
                },
                &identity("s0"),
            ),
            ProofJudgment::Unknown
        );
        assert_eq!(
            project_report_publication(&Value::Null).judgment,
            ProofJudgment::Unknown
        );
        let missing = serde_json::json!({});
        assert_eq!(
            project_report_publication(&missing).judgment,
            ProofJudgment::Unknown
        );
        assert_eq!(
            commit_freshness(&identity(""), &identity("s0")),
            ProofJudgment::Unknown
        );
    }

    #[test]
    fn contradictory_success_flag_cannot_promote_a_failed_observation() {
        let judgment = judge_completion(
            &CompletionClaim {
                declared_status: Some("done".to_string()),
                passes: Some(true),
                notes: Some("ship it".to_string()),
                evidence: Some(evidence("s0", false)),
            },
            &identity("s0"),
        );
        assert_eq!(judgment, ProofJudgment::Failed);
        assert_ne!(judgment, ProofJudgment::NotProven);
        assert!(!decide_publication(judgment, true, None, None).eligible);
    }

    #[test]
    fn duplicate_publication_conflicts_and_same_receipt_replays() {
        let proven = ProofJudgment::Proven;
        let receipt = "sha256:0123456789abcdef";
        assert!(decide_publication(proven, true, Some(receipt), Some(receipt)).eligible);
        let conflict =
            decide_publication(proven, true, Some(receipt), Some("sha256:ffffffffffffffff"));
        assert!(!conflict.eligible);
        assert_eq!(conflict.judgment, ProofJudgment::Unknown);
    }

    #[test]
    fn callers_do_not_inherit_each_others_foreground_notes() {
        let canonical = identity("s0");
        let a = CallerSession {
            caller_id: "a".to_string(),
            foreground_note: "agent a scratch".to_string(),
        };
        let b = CallerSession {
            caller_id: "b".to_string(),
            foreground_note: "agent b says publish".to_string(),
        };
        let resolved = resolve_caller(Some(&canonical), &a, &b);
        assert_eq!(resolved.identity.unwrap().head, "s0");
        assert_eq!(resolved.own_note, "agent a scratch");
        assert!(!resolved.own_note.contains("publish"));
        assert_eq!(
            resolve_caller(None, &a, &b).judgment,
            ProofJudgment::Unknown
        );
    }

    #[test]
    fn expired_lease_budget_and_forbidden_paths_reject_admission() {
        let request = AdmissionRequest {
            actor: "agent-a".to_string(),
            paths: BTreeSet::from(["src/a.rs".to_string(), "src/b.rs".to_string()]),
            forbidden: BTreeSet::from(["secrets".to_string()]),
            file_touch_budget: Some(1),
            authorized: identity("s0"),
            other_claims: Vec::new(),
            prerequisites_proven: true,
            own_lease_expired: false,
        };
        assert_eq!(
            admit(&identity("s0"), &request).phase,
            TransitionPhase::Rejected
        );
        let mut expired = request.clone();
        expired.paths = BTreeSet::from(["src/a.rs".to_string()]);
        expired.own_lease_expired = true;
        assert_eq!(
            admit(&identity("s0"), &expired).phase,
            TransitionPhase::Rejected
        );
        let mut forbidden = expired.clone();
        forbidden.own_lease_expired = false;
        forbidden.paths = BTreeSet::from(["secrets/key".to_string()]);
        assert_eq!(
            admit(&identity("s0"), &forbidden).phase,
            TransitionPhase::Rejected
        );
    }

    #[test]
    fn trace_records_the_decision_and_drops_declarations() {
        let prior = identity("s0");
        let admission = admit(
            &prior,
            &AdmissionRequest {
                actor: "agent-a".to_string(),
                paths: BTreeSet::from(["src/main.rs".to_string()]),
                forbidden: BTreeSet::new(),
                file_touch_budget: Some(2),
                authorized: prior.clone(),
                other_claims: Vec::new(),
                prerequisites_proven: true,
                own_lease_expired: false,
            },
        );
        let scope = ["src/main.rs".to_string()];
        let expected = ["proof".to_string()];
        let observed = ["receipt".to_string()];
        let trace = transition_trace(TraceInput {
            actor: "agent-a",
            requested: "validate",
            prior: &prior,
            admission: &admission,
            scope: &scope,
            expected: &expected,
            observed: &observed,
            evidence_hash: Some("sha256:0123456789abcdef"),
            judgment: ProofJudgment::Proven,
            reason: "fresh",
        });
        let json = trace_json(&trace);
        assert_eq!(json["op"], TRANSITION_OP);
        assert_eq!(json["actor"], "agent-a");
        assert_eq!(json["prior_state"], "s0");
        assert_eq!(json["outcome"], "proven");
        assert!(json.get("notes").is_none());
        assert!(json.get("passes").is_none());
    }

    #[test]
    fn unchanged_paths_are_not_taken_from_an_assertion() {
        let request = AdmissionRequest {
            actor: "agent-a".to_string(),
            paths: BTreeSet::from(["src/changed.rs".to_string()]),
            forbidden: BTreeSet::new(),
            file_touch_budget: Some(1),
            authorized: identity("s0"),
            other_claims: Vec::new(),
            prerequisites_proven: false,
            own_lease_expired: false,
        };
        let decision = admit(&identity("s0"), &request);
        assert_eq!(decision.phase, TransitionPhase::Rejected);
        assert!(decision.reason.contains("prerequisites"));
    }

    #[test]
    fn canonical_identity_comes_from_the_resolver_snapshot() {
        let snapshot = state::CanonicalSnapshot {
            revision: state::RevisionInfo {
                head_sha: "abc123abc123".to_string(),
                base_sha: None,
                branch: Some("master".to_string()),
                is_dirty: false,
                dirty_files: Vec::new(),
            },
            provenance: state::ProvenanceInfo {
                state_commit_root: None,
                scope_record_hash: None,
                governance_document_hash: Some("sha256:0123456789abcdef".to_string()),
                trajectory_artifact_hash: None,
                workunit_manifest_hash: None,
                validation_receipt_hash: None,
                resolved_at: "0Z".to_string(),
                decapod_version: "0.130.2".to_string(),
            },
        };
        let identity = identity_from_snapshot(&snapshot);
        assert_eq!(identity.head, snapshot.revision.head_sha);
        assert_eq!(
            identity.rules_fingerprint,
            snapshot.provenance.governance_document_hash.unwrap()
        );
        assert_eq!(identity.verifier, snapshot.provenance.decapod_version);
    }

    #[test]
    fn forged_verification_row_does_not_prove_completion() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("evidence.txt"), b"actual-bytes").unwrap();
        let identity = StateIdentity {
            head: "abc123abc123".to_string(),
            rules_fingerprint: "sha256:0123456789abcdef".to_string(),
            verifier: "0.130.2".to_string(),
        };
        let forged = serde_json::json!({
            "task_id": "task-1",
            "task_revision": 1,
            "passes": true,
            "canonical_head": identity.head,
            "rules_fingerprint": identity.rules_fingerprint,
            "verifier": identity.verifier,
            "file_artifacts": [{
                "path": "evidence.txt",
                "hash": "sha256:deadbeefdeadbeef"
            }],
            "proof_plan_results": [{
                "proof_gate": "validate_passes",
                "output_hash": "sha256:deadbeefdeadbeef"
            }]
        });
        assert_eq!(
            judge_reconstructed(root.path(), &identity, "pass", &forged),
            ProofJudgment::NotProven
        );
        let status_only = serde_json::json!({
            "passes": true,
            "last_verified_status": "pass"
        });
        assert_eq!(
            judge_reconstructed(root.path(), &identity, "pass", &status_only),
            ProofJudgment::NotProven
        );
    }

    #[test]
    fn reconstructed_evidence_can_prove_and_a_mutated_digest_cannot() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("evidence.txt"), b"actual-bytes").unwrap();
        let identity = StateIdentity {
            head: "abc123abc123".to_string(),
            rules_fingerprint: "sha256:0123456789abcdef".to_string(),
            verifier: "0.130.2".to_string(),
        };
        let actual = hash_file(&root.path().join("evidence.txt")).unwrap();
        let mut files = BTreeMap::new();
        files.insert("evidence.txt".to_string(), actual.clone());
        let digest = reconstruction_digest(&files, &identity);
        let proven = serde_json::json!({
            "canonical_head": identity.head,
            "rules_fingerprint": identity.rules_fingerprint,
            "verifier": identity.verifier,
            "file_artifacts": [{ "path": "evidence.txt", "hash": format!("sha256:{actual}") }],
            "receipt_hash": format!("sha256:{digest}")
        });
        assert_eq!(
            judge_reconstructed(root.path(), &identity, "pass", &proven),
            ProofJudgment::Proven
        );
        let mut mutated = proven.clone();
        mutated["receipt_hash"] = serde_json::json!("sha256:ffffffffffffffff");
        assert_eq!(
            judge_reconstructed(root.path(), &identity, "pass", &mutated),
            ProofJudgment::NotProven
        );
        assert!(!decide_publication(ProofJudgment::NotProven, true, None, None).eligible);
    }

    #[test]
    fn resolver_snapshot_rejects_a_forged_row_in_a_git_repository() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("evidence.txt"), b"actual-bytes").unwrap();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(root.path())
                .status()
                .unwrap();
            assert!(status.success());
        };
        git(&["init"]);
        git(&["config", "user.email", "governed@example.com"]);
        git(&["config", "user.name", "governed"]);
        git(&["add", "evidence.txt"]);
        git(&["-c", "commit.gpgsign=false", "commit", "-m", "evidence"]);
        let forged = serde_json::json!({
            "passes": true,
            "file_artifacts": [{ "path": "evidence.txt", "hash": "sha256:deadbeefdeadbeef" }],
            "receipt_hash": "sha256:deadbeefdeadbeef"
        });
        let judgment = judge_stored_verification(root.path(), "pass", &forged).unwrap();
        assert_ne!(judgment, ProofJudgment::Proven);
    }

    #[test]
    fn flight_recorder_persists_the_transition_decision() {
        let root = tempfile::tempdir().unwrap();
        let prior = identity("s0");
        let admission = Admission {
            phase: TransitionPhase::Admissible,
            reason: "in scope".to_string(),
        };
        let trace = transition_trace(TraceInput {
            actor: "agent-a",
            requested: "complete",
            prior: &prior,
            admission: &admission,
            scope: &[],
            expected: &["proof".to_string()],
            observed: &["receipt".to_string()],
            evidence_hash: Some("sha256:0123456789abcdef"),
            judgment: ProofJudgment::NotProven,
            reason: "verification row is not evidence",
        });
        record_transition(root.path(), &trace).unwrap();
        let records = events::query(root.path(), events::TRACES, 10).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].event_type, TRANSITION_OP);
        assert_eq!(records[0].actor, "agent-a");
        assert_eq!(records[0].payload["outcome"], "not_proven");
        assert_eq!(records[0].payload["prior_state"], "s0");
        assert_eq!(records[0].payload["rules_fingerprint"], "rules-1");
        assert_eq!(records[0].payload["verifier"], "decapod-0.130.2");
        assert!(records[0].payload.get("notes").is_none());
    }
}
