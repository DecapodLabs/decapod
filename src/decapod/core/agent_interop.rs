//! Agent interoperability engine:
//! - Four-plane versioned transport-neutral governance envelope v1, replay/import/authority
//!   isolation, and 4-disposition decision-time projection (#870)
//! - Two-way dispatcher-vs-authorization drift check and observed-conformance receipt
//!   generator/verifier bound to binary/source/schema/transport (#871)
//! - Adversarial conformance vectors & summary-fiction/provenance/freshness/tamper rules (#1733)
//! - Bounded stdio MCP JSON-RPC 2.0 adapter (#872) -> [`mcp`]
//! - Bounded A2A Agent Card & task/handoff adapter (#873) -> [`a2a`]
//! - Offline capability registry, skill discovery, manifest compiler, safe fragment composition,
//!   and harness-native packaging (#1715, #1716, #1717, #1718, #875) -> [`registry`]
//! - Coding-rule taxonomy/gap matrix, rule metadata evaluator, and 6-state semantic task-scope
//!   overlap classifier (#1724, #1725, #878) -> [`rules`]

pub mod a2a;
pub mod mcp;
pub mod registry;
pub mod rules;

use crate::core::interop::{self, CapabilityEvidence, InteropManifest, VerificationState};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Composed governance envelope profile identifier (#870).
pub const GOVERNANCE_ENVELOPE_PROFILE_ID: &str = "decapod.governance-envelope";
/// Composed governance envelope profile version (#870).
pub const GOVERNANCE_ENVELOPE_PROFILE_VERSION: &str = "v1";
/// Supported independent plane major versions for v1.
pub const SUPPORTED_PLANE_MAJOR: u32 = 1;
/// Supported optional/mandatory envelope extensions in this build.
pub const SUPPORTED_ENVELOPE_EXTENSIONS: &[&str] = &[
    "receipts",
    "interlocks",
    "trajectory-lineage",
    "delegation-narrowing",
    "decision-disposition",
];

// ============================================================================
// #870: Four-Plane Versioned Transport-Neutral Governance Envelope v1
// ============================================================================

/// Independently versioned semantic planes composed into one declared profile version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaneVersions {
    /// Identity, custody, original human intent vs agent interpretation.
    pub intent: String,
    /// Policy, approval bindings, delegation narrowing, authorization transitions.
    pub authority: String,
    /// Proof references, producer trust class, raw observed receipts, environment integrity.
    pub evidence: String,
    /// Execution attempt vs effect, terminal states, interlocks, decision disposition.
    pub outcome: String,
}

impl Default for PlaneVersions {
    fn default() -> Self {
        Self {
            intent: "1.0.0".to_string(),
            authority: "1.0.0".to_string(),
            evidence: "1.0.0".to_string(),
            outcome: "1.0.0".to_string(),
        }
    }
}

/// Plane 1: Identity, custody, and intent provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentPlane {
    pub repository_id: String,
    pub tenant_id: String,
    pub task_id: String,
    pub run_id: String,
    pub op_id: String,
    pub actor_id: String,
    pub recipient_id: String,
    pub replay_domain: String,
    pub lease_generation: u64,
    pub original_human_request_sha256: String,
    pub agent_interpretation_summary: String,
    /// Whether the human request revision was independently observed vs caller-reported.
    pub human_request_independently_observed: bool,
}

/// Optional parent delegation constraint for narrowing verification (#870).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationGrant {
    pub parent_grant_id: String,
    pub parent_allowed_actions: Vec<String>,
    pub parent_recipient_id: String,
    pub parent_expires_at_epoch: u64,
    pub parent_allowed_paths: Vec<String>,
    pub parent_max_budget: u64,
    pub child_action: String,
    pub child_recipient_id: String,
    pub child_expires_at_epoch: u64,
    pub child_paths: Vec<String>,
    pub child_budget: u64,
}

/// Plane 2: Policy and authorization transition claims (never imported as local authority).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityPlane {
    pub requested_operation: String,
    pub actual_parameters_canonical: String,
    /// If human approval is claimed, exact approved operation and canonical parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_approved_operation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_approved_parameters_canonical: Option<String>,
    /// True if an authenticated human principal explicitly approved this exact call
    /// (distinct from an approved agent plan!).
    pub authenticated_human_approval: bool,
    pub plan_approved_by_agent: bool,
    pub issued_at_epoch: u64,
    pub expires_at_epoch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation: Option<DelegationGrant>,
    #[serde(default)]
    pub mandatory_extensions: Vec<String>,
    #[serde(default)]
    pub optional_extensions: Vec<String>,
}

/// Trust classification of the entity that produced an evidence record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProducerTrustClass {
    IndependentExecutor,
    TrustedCi,
    SelfDeclared,
    Untrusted,
}

/// Actual raw receipt bytes bound by SHA-256 digest (prevents summary-fiction attacks!).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawEvidenceReceipt {
    pub receipt_id: String,
    pub claimed_sha256: String,
    pub raw_bytes_utf8: String,
}

impl RawEvidenceReceipt {
    pub fn compute_sha256(raw: &str) -> String {
        let mut h = Sha256::new();
        h.update(raw.as_bytes());
        format!("{:x}", h.finalize())
    }

    pub fn from_raw(receipt_id: impl Into<String>, raw: impl Into<String>) -> Self {
        let raw_s = raw.into();
        let digest = Self::compute_sha256(&raw_s);
        Self {
            receipt_id: receipt_id.into(),
            claimed_sha256: digest,
            raw_bytes_utf8: raw_s,
        }
    }
}

/// Ordered history event for tail-cut / reorder / omitted-denial detection (#1733).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEventRef {
    pub seq: u64,
    pub event_kind: String,
    pub verdict: String,
}

/// Plane 3: Proof, witness, raw receipts, freshness bindings, and environment integrity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidencePlane {
    pub producer_id: String,
    pub producer_trust_class: ProducerTrustClass,
    pub source_tree_sha: String,
    pub head_commit_sha: String,
    pub base_commit_sha: String,
    pub proof_plan_epoch: u64,
    /// Digests claimed in any summary report (each MUST have a matching raw receipt!).
    #[serde(default)]
    pub claimed_receipt_digests: Vec<String>,
    /// Actual raw evidence receipts whose bytes are hashed and checked by the verifier.
    #[serde(default)]
    pub raw_receipts: Vec<RawEvidenceReceipt>,
    /// True if the only integrity anchor is a checksum file generated alongside the summary.
    #[serde(default)]
    pub self_generated_checksum_only: bool,
    /// True if a newer execution on this candidate failed after an earlier pass.
    #[serde(default)]
    pub newer_suite_failed: bool,
    /// Detected execution environment tamper signals (edited/removed test, hidden env filter, etc.).
    #[serde(default)]
    pub environment_tamper_signals: Vec<String>,
    /// Ordered history events observed for this run.
    #[serde(default)]
    pub history_events: Vec<HistoryEventRef>,
    /// Independently held terminal sequence number, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_terminal_seq: Option<u64>,
    /// Whether an independent observer actually covered the claimed action scope.
    pub independent_effect_observed: bool,
}

/// Semantic kind of claim carried in the outcome plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    Attempt,
    Effect,
    PrecommitContainment,
    GoalCompletion,
}

/// Plane 4: Attempt vs effect, behavioral vs process integrity, and terminal state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomePlane {
    pub claim_kind: ClaimKind,
    pub declared_status: String,
    pub observed_status: String,
    /// Number of assertions actually executed (0 assertions with declared "passed" is semantic mismatch).
    #[serde(default)]
    pub observed_assertions: Option<u64>,
    /// HTTP status if remote call (404 proves reachability, NOT business success).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_http_status: Option<u16>,
    /// Whether the observed execution path obeyed repository rules (separate from final output!).
    pub process_path_authorized: bool,
    /// Whether final behavioral output passed independently.
    pub behavioral_passed: bool,
    /// Optional raw URL or path argument checked for ambiguity/traversal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_uri_or_path: Option<String>,
}

/// Complete transport-neutral Governance Envelope v1 (#870).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceEnvelopeV1 {
    pub profile_id: String,
    pub profile_version: String,
    pub plane_versions: PlaneVersions,
    pub intent: IntentPlane,
    pub authority: AuthorityPlane,
    pub evidence: EvidencePlane,
    pub outcome: OutcomePlane,
    /// Canonical SHA-256 over all planes (empty while computing).
    pub envelope_sha256: String,
}

impl GovernanceEnvelopeV1 {
    /// Compute deterministic sorted-key canonical SHA-256 over the envelope payload
    /// (excluding `envelope_sha256`) so independent languages produce identical digests
    /// regardless of field declaration order.
    pub fn compute_envelope_sha256(&self) -> String {
        let mut clone = self.clone();
        clone.envelope_sha256 = String::new();
        let canonical_value = serde_json::to_value(&clone).unwrap_or(serde_json::Value::Null);
        let bytes = serde_json::to_vec(&canonical_value).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("sha256:{:x}", hasher.finalize())
    }

    pub fn with_computed_digest(mut self) -> Self {
        self.envelope_sha256 = self.compute_envelope_sha256();
        self
    }
}

/// Repository-local trust policy used to evaluate incoming external envelopes (#870).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalTrustPolicy {
    pub repository_id: String,
    pub tenant_id: String,
    pub expected_recipient_id: String,
    pub expected_head_commit_sha: String,
    pub expected_base_commit_sha: String,
    pub expected_source_tree_sha: String,
    pub min_lease_generation: u64,
    pub current_proof_plan_epoch: u64,
    pub accepted_producers: BTreeSet<String>,
    pub require_human_approval_for_ops: BTreeSet<String>,
}

/// Deterministic normalized verdict agreed upon by Decapod and independent consumers (#870/#1733).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizedVerdict {
    AcceptedExternalEvidence,
    Rejected,
    NotProven,
}

/// Decisive reason class (compared across independent consumers, not just pass/fail!).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonClass {
    ValidExternalEvidence,
    IncompatiblePlaneVersion,
    UnsupportedMandatoryExtension,
    EnvelopeTampered,
    UntrustedProducer,
    SummaryFictionMissingRawReceipts,
    RawReceiptDigestMismatch,
    StaleEvidence,
    ReplayDetected,
    CrossTenantOrRepoDenied,
    AuthorityWideningDenied,
    ApprovalParameterMismatch,
    MissingHumanApproval,
    TamperedExecutionEnvironment,
    HistoryIncompleteOrReordered,
    ProhibitedExecutionPath,
    SemanticOutcomeMismatch,
    PrecommitContainmentNotGoalCompletion,
    InputAmbiguityRejected,
}

/// Replay protection ledger tracking seen `(replay_domain, repo, tenant, run_id, op_id)` -> payload digest.
#[derive(Debug, Clone, Default)]
pub struct ReplayLedger {
    seen_ops: BTreeMap<(String, String, String, String, String), String>,
    retired_runs: BTreeSet<(String, String)>,
}

impl ReplayLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn retire_run(&mut self, replay_domain: impl Into<String>, run_id: impl Into<String>) {
        self.retired_runs
            .insert((replay_domain.into(), run_id.into()));
    }

    pub fn check_and_record(
        &mut self,
        replay_domain: &str,
        repo: &str,
        tenant: &str,
        run_id: &str,
        op_id: &str,
        payload_digest: &str,
    ) -> Result<(), String> {
        if self
            .retired_runs
            .contains(&(replay_domain.to_string(), run_id.to_string()))
        {
            return Err(format!(
                "run_id '{run_id}' was already retired across restart"
            ));
        }
        let key = (
            replay_domain.to_string(),
            repo.to_string(),
            tenant.to_string(),
            run_id.to_string(),
            op_id.to_string(),
        );
        if let Some(prev_digest) = self.seen_ops.get(&key) {
            if prev_digest == payload_digest {
                return Err(format!("exact replay detected for op_id '{op_id}'"));
            }
            return Err(format!(
                "duplicate op_id '{op_id}' with mutated payload digest"
            ));
        }
        self.seen_ops.insert(key, payload_digest.to_string());
        Ok(())
    }
}

/// Distinctions mandated by #1733 (2026-11-21 summary-fiction negative control).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceVerificationLayers {
    pub manifest_syntax_checked: bool,
    pub raw_receipt_digests_checked: bool,
    pub producer_authenticated: bool,
    pub effect_independently_observed: bool,
    pub process_integrity_ok: bool,
    pub behavioral_outcome_ok: bool,
}

/// Structured result of importing/verifying a Governance Envelope v1 (#870/#1733).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvelopeImportReport {
    pub verdict: NormalizedVerdict,
    pub reason_class: ReasonClass,
    pub detail: String,
    pub accepted_as_external_evidence: bool,
    /// INVARIANT: An imported envelope NEVER automatically grants local mutation/publication authority!
    pub imported_authority_granted: bool,
    pub omitted_optional_extensions: Vec<String>,
    pub verification_layers: EvidenceVerificationLayers,
    pub authority_note: String,
}

fn parse_major(v: &str) -> Option<u32> {
    let mut parts = v.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let _minor = parts.next()?.parse::<u32>().ok()?;
    let _patch = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(major)
}

fn has_ambiguous_path_or_url(s: &str) -> bool {
    // Reject traversal, backslashes, userinfo `@` authority confusion, percent-encoded slashes, or leading-zero IPv4 octets.
    let lower = s.to_ascii_lowercase();
    if s.contains("..")
        || s.contains('\\')
        || lower.contains("%2f")
        || lower.contains("%5c")
        || s.contains('\0')
    {
        return true;
    }
    if let Some(rest) = s
        .strip_prefix("http://")
        .or_else(|| s.strip_prefix("https://"))
    {
        let authority = rest.split('/').next().unwrap_or("");
        if authority.contains('@') {
            return true;
        }
        let host = authority.split(':').next().unwrap_or("");
        let octets: Vec<&str> = host.split('.').collect();
        if octets.len() == 4
            && octets
                .iter()
                .all(|o| !o.is_empty() && o.bytes().all(|b| b.is_ascii_digit()))
            && octets.iter().any(|o| o.len() > 1 && o.starts_with('0'))
        {
            return true;
        }
    }
    false
}

/// Evaluate an external Governance Envelope v1 against local repository trust policy & replay ledger.
/// Preserves the core #870 invariant: imported conformant evidence NEVER grants local authority!
pub fn import_external_envelope(
    env: &GovernanceEnvelopeV1,
    policy: &LocalTrustPolicy,
    replay_ledger: &mut ReplayLedger,
    now_epoch: u64,
) -> EnvelopeImportReport {
    let mut omitted_optional_extensions: Vec<String> = env
        .authority
        .optional_extensions
        .iter()
        .filter(|e| !SUPPORTED_ENVELOPE_EXTENSIONS.contains(&e.as_str()))
        .cloned()
        .collect();
    omitted_optional_extensions.sort();
    omitted_optional_extensions.dedup();

    let mut layers = EvidenceVerificationLayers {
        manifest_syntax_checked: false,
        raw_receipt_digests_checked: false,
        producer_authenticated: false,
        effect_independently_observed: env.evidence.independent_effect_observed,
        process_integrity_ok: env.outcome.process_path_authorized
            && env.evidence.environment_tamper_signals.is_empty(),
        behavioral_outcome_ok: env.outcome.behavioral_passed,
    };

    let make_report = |verdict: NormalizedVerdict,
                       reason_class: ReasonClass,
                       detail: String,
                       layers: EvidenceVerificationLayers,
                       omitted: Vec<String>| {
        EnvelopeImportReport {
        accepted_as_external_evidence: verdict == NormalizedVerdict::AcceptedExternalEvidence,
        imported_authority_granted: false,
        verdict,
        reason_class,
        detail,
        omitted_optional_extensions: omitted,
        verification_layers: layers,
        authority_note: "Imported governance envelopes remain external evidence only; explicit local policy acceptance and kernel reauthorization are required for any local action.".to_string(),
    }
    };

    // 1. Profile & plane version compatibility (fail closed on major mismatch).
    if env.profile_id != GOVERNANCE_ENVELOPE_PROFILE_ID
        || env.profile_version != GOVERNANCE_ENVELOPE_PROFILE_VERSION
    {
        return make_report(
            NormalizedVerdict::Rejected,
            ReasonClass::IncompatiblePlaneVersion,
            format!(
                "unsupported profile {}:{}",
                env.profile_id, env.profile_version
            ),
            layers,
            omitted_optional_extensions,
        );
    }
    for (name, ver) in [
        ("intent", &env.plane_versions.intent),
        ("authority", &env.plane_versions.authority),
        ("evidence", &env.plane_versions.evidence),
        ("outcome", &env.plane_versions.outcome),
    ] {
        if parse_major(ver) != Some(SUPPORTED_PLANE_MAJOR) {
            return make_report(
                NormalizedVerdict::Rejected,
                ReasonClass::IncompatiblePlaneVersion,
                format!("incompatible major version '{ver}' on plane '{name}'"),
                layers,
                omitted_optional_extensions,
            );
        }
    }

    // 2. Mandatory extensions must be supported; unknown optional extensions degrade safely.
    for ext in &env.authority.mandatory_extensions {
        if !SUPPORTED_ENVELOPE_EXTENSIONS.contains(&ext.as_str()) {
            return make_report(
                NormalizedVerdict::Rejected,
                ReasonClass::UnsupportedMandatoryExtension,
                format!("unsupported mandatory extension '{ext}'"),
                layers,
                omitted_optional_extensions,
            );
        }
    }
    layers.manifest_syntax_checked = true;

    // 3. Envelope tamper check.
    let computed_digest = env.compute_envelope_sha256();
    if env.envelope_sha256 != computed_digest {
        return make_report(
            NormalizedVerdict::Rejected,
            ReasonClass::EnvelopeTampered,
            format!(
                "envelope_sha256 '{}' does not match computed '{computed_digest}'",
                env.envelope_sha256
            ),
            layers,
            omitted_optional_extensions,
        );
    }

    // 4. Input ambiguity check (URLs / paths).
    if let Some(ref target) = env.outcome.target_uri_or_path
        && has_ambiguous_path_or_url(target)
    {
        return make_report(
            NormalizedVerdict::Rejected,
            ReasonClass::InputAmbiguityRejected,
            format!("ambiguous or traversal target URI/path rejected: '{target}'"),
            layers,
            omitted_optional_extensions,
        );
    }

    // 5. Cross-repository / cross-tenant / wrong-recipient isolation.
    if env.intent.repository_id != policy.repository_id
        || env.intent.tenant_id != policy.tenant_id
        || env.intent.recipient_id != policy.expected_recipient_id
    {
        return make_report(
            NormalizedVerdict::Rejected,
            ReasonClass::CrossTenantOrRepoDenied,
            "envelope repository, tenant, or recipient does not match local policy".to_string(),
            layers,
            omitted_optional_extensions,
        );
    }

    // 6. Replay & duplicate op protection.
    if let Err(err) = replay_ledger.check_and_record(
        &env.intent.replay_domain,
        &env.intent.repository_id,
        &env.intent.tenant_id,
        &env.intent.run_id,
        &env.intent.op_id,
        &env.envelope_sha256,
    ) {
        return make_report(
            NormalizedVerdict::Rejected,
            ReasonClass::ReplayDetected,
            err,
            layers,
            omitted_optional_extensions,
        );
    }

    // 7. Summary-fiction & raw receipt byte digest verification (#1733).
    if env.evidence.self_generated_checksum_only {
        return make_report(
            NormalizedVerdict::NotProven,
            ReasonClass::SummaryFictionMissingRawReceipts,
            "checksum list generated alongside summary without raw receipts cannot establish proof"
                .to_string(),
            layers,
            omitted_optional_extensions,
        );
    }
    if env.evidence.claimed_receipt_digests.is_empty() && env.evidence.raw_receipts.is_empty() {
        return make_report(
            NormalizedVerdict::NotProven,
            ReasonClass::SummaryFictionMissingRawReceipts,
            "no raw evidence receipts supplied".to_string(),
            layers,
            omitted_optional_extensions,
        );
    }
    let mut verified_raw_digests = BTreeSet::new();
    for raw in &env.evidence.raw_receipts {
        let actual = RawEvidenceReceipt::compute_sha256(&raw.raw_bytes_utf8);
        if raw.claimed_sha256 != actual {
            return make_report(
                NormalizedVerdict::Rejected,
                ReasonClass::RawReceiptDigestMismatch,
                format!(
                    "raw receipt '{}' digest mismatch: claimed '{}' != actual '{actual}'",
                    raw.receipt_id, raw.claimed_sha256
                ),
                layers,
                omitted_optional_extensions,
            );
        }
        verified_raw_digests.insert(actual);
    }
    for claimed in &env.evidence.claimed_receipt_digests {
        if !verified_raw_digests.contains(claimed) {
            return make_report(
                NormalizedVerdict::NotProven,
                ReasonClass::SummaryFictionMissingRawReceipts,
                format!(
                    "claimed receipt digest '{claimed}' has no matching verified raw receipt bytes"
                ),
                layers,
                omitted_optional_extensions,
            );
        }
    }
    layers.raw_receipt_digests_checked = true;

    // 8. Producer trust & authentication check.
    let producer_trusted = matches!(
        env.evidence.producer_trust_class,
        ProducerTrustClass::IndependentExecutor | ProducerTrustClass::TrustedCi
    ) && policy
        .accepted_producers
        .contains(&env.evidence.producer_id);
    if !producer_trusted {
        return make_report(
            NormalizedVerdict::Rejected,
            ReasonClass::UntrustedProducer,
            format!(
                "producer '{}' (class {:?}) is not accepted by repository trust policy",
                env.evidence.producer_id, env.evidence.producer_trust_class
            ),
            layers,
            omitted_optional_extensions,
        );
    }
    layers.producer_authenticated = true;

    // 9. Freshness checks (expiry, lease, head/base/tree, proof-plan epoch, newer failed suite).
    if env.authority.expires_at_epoch <= env.authority.issued_at_epoch
        || now_epoch > env.authority.expires_at_epoch
        || env.intent.lease_generation < policy.min_lease_generation
        || env.evidence.head_commit_sha != policy.expected_head_commit_sha
        || env.evidence.base_commit_sha != policy.expected_base_commit_sha
        || env.evidence.source_tree_sha != policy.expected_source_tree_sha
        || env.evidence.proof_plan_epoch != policy.current_proof_plan_epoch
        || env.evidence.newer_suite_failed
    {
        return make_report(
            NormalizedVerdict::Rejected,
            ReasonClass::StaleEvidence,
            "evidence is expired, bound to a stale commit/tree/base/epoch/lease, or superseded by a newer failure".to_string(),
            layers,
            omitted_optional_extensions,
        );
    }

    // 10. Delegation narrowing check (child grant must never widen parent bounds).
    if let Some(ref del) = env.authority.delegation {
        let action_ok = del.parent_allowed_actions.contains(&del.child_action);
        let recipient_ok = del.child_recipient_id == del.parent_recipient_id;
        let expiry_ok = del.child_expires_at_epoch <= del.parent_expires_at_epoch;
        let budget_ok = del.child_budget <= del.parent_max_budget;
        let paths_ok = del.child_paths.iter().all(|cp| {
            del.parent_allowed_paths
                .iter()
                .any(|pp| cp == pp || cp.starts_with(&format!("{pp}/")))
        });
        if !(action_ok && recipient_ok && expiry_ok && budget_ok && paths_ok) {
            return make_report(
                NormalizedVerdict::Rejected,
                ReasonClass::AuthorityWideningDenied,
                "child delegation grant widens parent action, recipient, expiry, paths, or budget"
                    .to_string(),
                layers,
                omitted_optional_extensions,
            );
        }
    }

    // 11. Exact human approval binding (plan approval is NOT human consent; params must match!).
    if policy
        .require_human_approval_for_ops
        .contains(&env.authority.requested_operation)
        && !env.authority.authenticated_human_approval
    {
        return make_report(
            NormalizedVerdict::Rejected,
            ReasonClass::MissingHumanApproval,
            format!(
                "operation '{}' requires authenticated human approval (agent plan approval is insufficient)",
                env.authority.requested_operation
            ),
            layers,
            omitted_optional_extensions,
        );
    }
    if let Some(ref approved_op) = env.authority.claimed_approved_operation {
        let params_match = env
            .authority
            .claimed_approved_parameters_canonical
            .as_deref()
            == Some(env.authority.actual_parameters_canonical.as_str());
        if approved_op != &env.authority.requested_operation || !params_match {
            return make_report(
                NormalizedVerdict::Rejected,
                ReasonClass::ApprovalParameterMismatch,
                "actual operation or parameters differ from the approved call binding".to_string(),
                layers,
                omitted_optional_extensions,
            );
        }
    }

    // 12. Tampered execution environment & history completeness checks (#1733).
    if !env.evidence.environment_tamper_signals.is_empty() {
        return make_report(
            NormalizedVerdict::Rejected,
            ReasonClass::TamperedExecutionEnvironment,
            format!(
                "execution environment tampering detected: {:?}",
                env.evidence.environment_tamper_signals
            ),
            layers,
            omitted_optional_extensions,
        );
    }
    if !env.evidence.history_events.is_empty() {
        // Require strictly contiguous 1..=N sequence numbers and match independently held terminal seq.
        for (idx, ev) in env.evidence.history_events.iter().enumerate() {
            let expected_seq = (idx as u64) + 1;
            if ev.seq != expected_seq {
                return make_report(
                    NormalizedVerdict::Rejected,
                    ReasonClass::HistoryIncompleteOrReordered,
                    format!(
                        "history sequence gap or reorder: expected seq {expected_seq}, observed {}",
                        ev.seq
                    ),
                    layers,
                    omitted_optional_extensions,
                );
            }
        }
        if let Some(term) = env.evidence.expected_terminal_seq {
            let last_seq = env
                .evidence
                .history_events
                .last()
                .map(|e| e.seq)
                .unwrap_or(0);
            if last_seq != term {
                return make_report(
                    NormalizedVerdict::Rejected,
                    ReasonClass::HistoryIncompleteOrReordered,
                    format!(
                        "history truncated before independently held terminal seq {term} (last={last_seq})"
                    ),
                    layers,
                    omitted_optional_extensions,
                );
            }
        }
    }

    // 13. Process vs outcome integrity & semantic mismatch checks (#1733).
    if !env.outcome.process_path_authorized {
        return make_report(
            NormalizedVerdict::Rejected,
            ReasonClass::ProhibitedExecutionPath,
            "final behavioral result was reached via a prohibited execution path".to_string(),
            layers,
            omitted_optional_extensions,
        );
    }
    if env.outcome.claim_kind == ClaimKind::PrecommitContainment
        && env
            .outcome
            .declared_status
            .eq_ignore_ascii_case("goal_completed")
    {
        return make_report(
            NormalizedVerdict::NotProven,
            ReasonClass::PrecommitContainmentNotGoalCompletion,
            "pre-commit refusal or safe containment proves containment, not goal completion"
                .to_string(),
            layers,
            omitted_optional_extensions,
        );
    }
    let declared_ok = matches!(
        env.outcome.declared_status.to_ascii_lowercase().as_str(),
        "passed" | "verified" | "completed" | "success"
    );
    let observed_ok = matches!(
        env.outcome.observed_status.to_ascii_lowercase().as_str(),
        "passed" | "verified" | "completed" | "success"
    );
    let zero_assertions = env.outcome.observed_assertions == Some(0);
    let http_error_as_success = env
        .outcome
        .observed_http_status
        .is_some_and(|code| code >= 400);
    if declared_ok
        && (!observed_ok
            || zero_assertions
            || http_error_as_success
            || !env.outcome.behavioral_passed)
    {
        return make_report(
            NormalizedVerdict::Rejected,
            ReasonClass::SemanticOutcomeMismatch,
            "declared passing status conflicts with observed status, zero assertions, or HTTP error response".to_string(),
            layers,
            omitted_optional_extensions,
        );
    }
    if !env.evidence.independent_effect_observed {
        return make_report(
            NormalizedVerdict::NotProven,
            ReasonClass::SemanticOutcomeMismatch,
            "effect was not independently observed by an accepted executor/readback".to_string(),
            layers,
            omitted_optional_extensions,
        );
    }

    make_report(
        NormalizedVerdict::AcceptedExternalEvidence,
        ReasonClass::ValidExternalEvidence,
        "envelope verified as external evidence under local trust policy; local reauthorization still required for action".to_string(),
        layers,
        omitted_optional_extensions,
    )
}

// ============================================================================
// #870: Decision-Time 4-Disposition Read-Only Projection
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionDisposition {
    Admit,
    RepairRequired,
    AuthorityRequired,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationTransportState {
    Evaluated,
    Invalid,
    Unknown,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionBoundaryInput {
    pub operation: String,
    pub subject_ref: String,
    pub transport_state: EvaluationTransportState,
    pub hard_constraint_violated: bool,
    pub requires_principal_decision: bool,
    pub bounded_repair_available: bool,
    pub proposed_repair_expands_scope_or_args: bool,
    pub deterministic_requirements_satisfied: bool,
    pub underlying_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionProjection {
    pub transport_state: EvaluationTransportState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disposition: Option<DecisionDisposition>,
    pub underlying_reason: String,
    pub subject_ref: String,
    pub effect_occurred: bool,
    pub next_action: String,
}

/// Pure read-only projection mapping a decision boundary to the 4 normative dispositions
/// while keeping `invalid`/`unknown`/`unavailable` evaluation errors distinct from admission (#870).
pub fn project_decision_disposition(input: &DecisionBoundaryInput) -> DecisionProjection {
    if input.transport_state != EvaluationTransportState::Evaluated {
        return DecisionProjection {
            transport_state: input.transport_state,
            disposition: None,
            underlying_reason: input.underlying_reason.clone(),
            subject_ref: input.subject_ref.clone(),
            effect_occurred: false,
            next_action: "fail_closed_do_not_admit".to_string(),
        };
    }

    if input.hard_constraint_violated {
        return DecisionProjection {
            transport_state: EvaluationTransportState::Evaluated,
            disposition: Some(DecisionDisposition::Reject),
            underlying_reason: input.underlying_reason.clone(),
            subject_ref: input.subject_ref.clone(),
            effect_occurred: false,
            next_action: "refuse_operation".to_string(),
        };
    }

    if input.proposed_repair_expands_scope_or_args {
        return DecisionProjection {
            transport_state: EvaluationTransportState::Evaluated,
            disposition: Some(DecisionDisposition::Reject),
            underlying_reason: format!(
                "proposed repair expands scope or mutates arguments outside bounds: {}",
                input.underlying_reason
            ),
            subject_ref: input.subject_ref.clone(),
            effect_occurred: false,
            next_action: "refuse_repair_until_rejudged".to_string(),
        };
    }

    if input.requires_principal_decision {
        return DecisionProjection {
            transport_state: EvaluationTransportState::Evaluated,
            disposition: Some(DecisionDisposition::AuthorityRequired),
            underlying_reason: input.underlying_reason.clone(),
            subject_ref: input.subject_ref.clone(),
            effect_occurred: false,
            next_action: "stop_and_request_authorized_principal_decision".to_string(),
        };
    }

    if input.bounded_repair_available && !input.deterministic_requirements_satisfied {
        return DecisionProjection {
            transport_state: EvaluationTransportState::Evaluated,
            disposition: Some(DecisionDisposition::RepairRequired),
            underlying_reason: input.underlying_reason.clone(),
            subject_ref: input.subject_ref.clone(),
            effect_occurred: false,
            next_action: "return_typed_correction_and_reevaluate_before_effect".to_string(),
        };
    }

    if input.deterministic_requirements_satisfied {
        return DecisionProjection {
            transport_state: EvaluationTransportState::Evaluated,
            disposition: Some(DecisionDisposition::Admit),
            underlying_reason: input.underlying_reason.clone(),
            subject_ref: input.subject_ref.clone(),
            effect_occurred: false,
            next_action: "invoke_already_authorized_operation_under_owning_gate".to_string(),
        };
    }

    DecisionProjection {
        transport_state: EvaluationTransportState::Evaluated,
        disposition: Some(DecisionDisposition::Reject),
        underlying_reason: input.underlying_reason.clone(),
        subject_ref: input.subject_ref.clone(),
        effect_occurred: false,
        next_action: "refuse_unsatisfied_requirements".to_string(),
    }
}

// ============================================================================
// #871: Two-Way Dispatcher-vs-Authorization Drift Check & Observed Conformance Receipt
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriftFinding {
    pub code: String,
    pub op: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatcherAuthorizationDriftReport {
    pub clean: bool,
    pub advertised_count: usize,
    pub dispatched_count: usize,
    pub findings: Vec<DriftFinding>,
}

/// Two-way drift check comparing the manifest catalog, actual RPC dispatcher ops,
/// actual session entry-guard enforcement, and read-only profile role boundaries (#871).
pub fn check_dispatcher_authorization_drift(
    manifest: &InteropManifest,
    dispatched_ops: &[&str],
    session_required_fn: impl Fn(&str) -> bool,
) -> DispatcherAuthorizationDriftReport {
    let mut findings = Vec::new();
    let dispatched_set: BTreeSet<&str> = dispatched_ops.iter().copied().collect();
    let mut advertised_ops: BTreeSet<String> = BTreeSet::new();

    for cap in &manifest.capabilities {
        if cap.surface != "rpc" {
            continue;
        }
        let Some(ref mapping) = cap.implementation else {
            findings.push(DriftFinding {
                code: "missing_mapping".to_string(),
                op: cap.id.clone(),
                detail: "RPC capability lacks implementation mapping".to_string(),
            });
            continue;
        };
        advertised_ops.insert(mapping.op.clone());

        // Direction 1: advertised op must actually exist in dispatcher (except early-routed inspect/maintenance ops).
        if !dispatched_set.contains(mapping.op.as_str()) {
            findings.push(DriftFinding {
                code: "advertised_but_undispatched".to_string(),
                op: mapping.op.clone(),
                detail: format!(
                    "manifest advertises '{}' which is absent from dispatcher",
                    mapping.op
                ),
            });
        }

        // Direction 2: advertised session_required must match actual entry guard.
        let actual_session_required = session_required_fn(&mapping.op);
        if mapping.session_required != actual_session_required {
            findings.push(DriftFinding {
                code: "session_guard_drift".to_string(),
                op: mapping.op.clone(),
                detail: format!(
                    "manifest session_required={} differs from actual entry guard={}",
                    mapping.session_required, actual_session_required
                ),
            });
        }
    }

    // Direction 3: every dispatched op must be advertised in the manifest.
    for op in &dispatched_set {
        if !advertised_ops.contains(*op) {
            findings.push(DriftFinding {
                code: "dispatched_but_unadvertised".to_string(),
                op: (*op).to_string(),
                detail: format!("dispatcher accepts '{op}' which is omitted from manifest"),
            });
        }
    }

    // Direction 4: read-only profiles must never include mutating operations.
    for profile in interop::profiles() {
        if profile.id.starts_with("read-only") {
            for op in profile
                .required_ops
                .iter()
                .chain(profile.optional_ops.iter())
            {
                if let Some(cap) = manifest
                    .capabilities
                    .iter()
                    .find(|c| c.implementation.as_ref().is_some_and(|m| &m.op == op))
                    && !cap.implementation.as_ref().is_some_and(|m| m.read_only)
                {
                    findings.push(DriftFinding {
                        code: "read_only_profile_mutating_op".to_string(),
                        op: op.clone(),
                        detail: format!(
                            "read-only profile '{}' includes mutating operation '{op}'",
                            profile.id
                        ),
                    });
                }
            }
        }
    }

    findings.sort_by(|a, b| a.op.cmp(&b.op).then_with(|| a.code.cmp(&b.code)));
    DispatcherAuthorizationDriftReport {
        clean: findings.is_empty(),
        advertised_count: advertised_ops.len(),
        dispatched_count: dispatched_set.len(),
        findings,
    }
}

/// Single capability observation inside an [`ObservedConformanceReceipt`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedCapabilityResult {
    pub capability_id: String,
    pub conformance_test: String,
    pub passed: bool,
    pub raw_evidence: RawEvidenceReceipt,
}

/// Authenticated observed-conformance receipt bound to exact binary, source, schema,
/// profile, transport, producer, and timestamp (#871).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedConformanceReceipt {
    pub receipt_id: String,
    pub schema_version: String,
    pub profile_id: String,
    pub profile_version: String,
    pub implementation_version: String,
    pub binary_sha256: String,
    pub source_commit_sha: String,
    pub transport: String,
    pub producer_id: String,
    pub producer_trust_class: ProducerTrustClass,
    pub executed_at_epoch: u64,
    pub max_age_secs: u64,
    pub results: Vec<ObservedCapabilityResult>,
    pub receipt_sha256: String,
}

impl ObservedConformanceReceipt {
    pub fn compute_receipt_sha256(&self) -> String {
        let mut clone = self.clone();
        clone.receipt_sha256 = String::new();
        let bytes = serde_json::to_vec(&clone).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("sha256:{:x}", hasher.finalize())
    }

    pub fn with_computed_digest(mut self) -> Self {
        self.receipt_sha256 = self.compute_receipt_sha256();
        self
    }
}

/// Current execution context against which an observed-conformance receipt is evaluated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentBuildContext {
    pub schema_version: String,
    pub profile_id: String,
    pub profile_version: String,
    pub implementation_version: String,
    pub binary_sha256: String,
    pub source_commit_sha: String,
    pub transport: String,
    pub now_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptProjectionReport {
    pub receipt_accepted: bool,
    pub reason: String,
    pub manifest: InteropManifest,
}

/// Project an [`ObservedConformanceReceipt`] onto a static [`InteropManifest`] (#871).
/// - Static manifest entries start at `VerificationState::NotRun`.
/// - Self-declared or untrusted producers, tampered receipts, or missing raw evidence keep `NotRun`.
/// - Mismatched binary/source/schema/profile/transport or expired receipts mark observed entries `Stale`.
/// - ONLY an accepted independent producer bound to the exact current build promotes capabilities
///   to `VerificationState::Verified` (or `Failed`)!
pub fn project_manifest_with_receipt(
    base_manifest: &InteropManifest,
    receipt: &ObservedConformanceReceipt,
    current: &CurrentBuildContext,
    accepted_producers: &BTreeSet<String>,
) -> ReceiptProjectionReport {
    let mut projected = base_manifest.clone();

    // 1. Tamper check.
    if receipt.receipt_sha256 != receipt.compute_receipt_sha256() {
        return ReceiptProjectionReport {
            receipt_accepted: false,
            reason: "receipt_tampered".to_string(),
            manifest: projected,
        };
    }

    // 2. Producer trust check (never promote self-declared / untrusted observations!).
    let trusted = matches!(
        receipt.producer_trust_class,
        ProducerTrustClass::IndependentExecutor | ProducerTrustClass::TrustedCi
    ) && accepted_producers.contains(&receipt.producer_id);
    if !trusted {
        return ReceiptProjectionReport {
            receipt_accepted: false,
            reason: "untrusted_or_self_declared_producer".to_string(),
            manifest: projected,
        };
    }

    // 3. Raw evidence integrity check for every claimed observation.
    for obs in &receipt.results {
        let actual = RawEvidenceReceipt::compute_sha256(&obs.raw_evidence.raw_bytes_utf8);
        if obs.raw_evidence.raw_bytes_utf8.is_empty() || obs.raw_evidence.claimed_sha256 != actual {
            return ReceiptProjectionReport {
                receipt_accepted: false,
                reason: format!("invalid_raw_evidence_for_{}", obs.capability_id),
                manifest: projected,
            };
        }
    }

    // 4. Currentness & exact-build binding check -> Stale if mismatched or expired.
    let expired = current.now_epoch
        > receipt
            .executed_at_epoch
            .saturating_add(receipt.max_age_secs);
    let mismatched = receipt.binary_sha256 != current.binary_sha256
        || receipt.source_commit_sha != current.source_commit_sha
        || receipt.schema_version != current.schema_version
        || receipt.profile_id != current.profile_id
        || receipt.profile_version != current.profile_version
        || receipt.implementation_version != current.implementation_version
        || receipt.transport != current.transport;

    if expired || mismatched {
        for obs in &receipt.results {
            if let Some(cap) = projected
                .capabilities
                .iter_mut()
                .find(|c| c.id == obs.capability_id)
            {
                cap.evidence = CapabilityEvidence {
                    conformance_test: obs.conformance_test.clone(),
                    state: VerificationState::Stale,
                    evidence_ref: Some(format!(
                        "receipt:{}:{}",
                        receipt.receipt_id, obs.raw_evidence.claimed_sha256
                    )),
                };
            }
        }
        return ReceiptProjectionReport {
            receipt_accepted: false,
            reason: if expired {
                "receipt_expired_stale".to_string()
            } else {
                "receipt_build_or_transport_mismatch_stale".to_string()
            },
            manifest: projected,
        };
    }

    // 5. All bindings verified for current build: promote observed capabilities truthfully.
    for obs in &receipt.results {
        if let Some(cap) = projected
            .capabilities
            .iter_mut()
            .find(|c| c.id == obs.capability_id)
        {
            cap.evidence = CapabilityEvidence {
                conformance_test: obs.conformance_test.clone(),
                state: if obs.passed {
                    VerificationState::Verified
                } else {
                    VerificationState::Failed
                },
                evidence_ref: Some(format!(
                    "receipt:{}:{}",
                    receipt.receipt_id, obs.raw_evidence.claimed_sha256
                )),
            };
        }
    }

    ReceiptProjectionReport {
        receipt_accepted: true,
        reason: "observed_conformance_verified_for_current_build".to_string(),
        manifest: projected,
    }
}

/// Embedded normative JSON Schema for Governance Envelope v1 (loaded from `assets/schemas/`).
pub const AGENT_INTEROP_ENVELOPE_SCHEMA_JSON: &str =
    include_str!("../../../assets/schemas/agent-interop-envelope-v1.schema.json");

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_policy() -> LocalTrustPolicy {
        let mut producers = BTreeSet::new();
        producers.insert("ci-executor-1".to_string());
        let mut approval_ops = BTreeSet::new();
        approval_ops.insert("workspace.publish".to_string());
        LocalTrustPolicy {
            repository_id: "DecapodLabs/decapod".to_string(),
            tenant_id: "tenant-main".to_string(),
            expected_recipient_id: "decapod-kernel".to_string(),
            expected_head_commit_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            expected_base_commit_sha: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            expected_source_tree_sha: "cccccccccccccccccccccccccccccccccccccccc".to_string(),
            min_lease_generation: 2,
            current_proof_plan_epoch: 5,
            accepted_producers: producers,
            require_human_approval_for_ops: approval_ops,
        }
    }

    fn valid_envelope() -> GovernanceEnvelopeV1 {
        let raw = RawEvidenceReceipt::from_raw("rcpt-1", r#"{"suite":"core","passed":true}"#);
        let digest = raw.claimed_sha256.clone();
        GovernanceEnvelopeV1 {
            profile_id: GOVERNANCE_ENVELOPE_PROFILE_ID.to_string(),
            profile_version: GOVERNANCE_ENVELOPE_PROFILE_VERSION.to_string(),
            plane_versions: PlaneVersions::default(),
            intent: IntentPlane {
                repository_id: "DecapodLabs/decapod".to_string(),
                tenant_id: "tenant-main".to_string(),
                task_id: "code_01test".to_string(),
                run_id: "run_01".to_string(),
                op_id: "op_01".to_string(),
                actor_id: "agent-a".to_string(),
                recipient_id: "decapod-kernel".to_string(),
                replay_domain: "local-repo".to_string(),
                lease_generation: 2,
                original_human_request_sha256: "d".repeat(64),
                agent_interpretation_summary: "Implement interop profile".to_string(),
                human_request_independently_observed: true,
            },
            authority: AuthorityPlane {
                requested_operation: "validate.run".to_string(),
                actual_parameters_canonical: r#"{"store":"repo"}"#.to_string(),
                claimed_approved_operation: Some("validate.run".to_string()),
                claimed_approved_parameters_canonical: Some(r#"{"store":"repo"}"#.to_string()),
                authenticated_human_approval: false,
                plan_approved_by_agent: true,
                issued_at_epoch: 1000,
                expires_at_epoch: 2000,
                delegation: None,
                mandatory_extensions: vec!["receipts".to_string()],
                optional_extensions: vec!["future-ext".to_string()],
            },
            evidence: EvidencePlane {
                producer_id: "ci-executor-1".to_string(),
                producer_trust_class: ProducerTrustClass::TrustedCi,
                source_tree_sha: "cccccccccccccccccccccccccccccccccccccccc".to_string(),
                head_commit_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                base_commit_sha: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                proof_plan_epoch: 5,
                claimed_receipt_digests: vec![digest],
                raw_receipts: vec![raw],
                self_generated_checksum_only: false,
                newer_suite_failed: false,
                environment_tamper_signals: vec![],
                history_events: vec![
                    HistoryEventRef {
                        seq: 1,
                        event_kind: "claim".to_string(),
                        verdict: "ok".to_string(),
                    },
                    HistoryEventRef {
                        seq: 2,
                        event_kind: "validate".to_string(),
                        verdict: "passed".to_string(),
                    },
                ],
                expected_terminal_seq: Some(2),
                independent_effect_observed: true,
            },
            outcome: OutcomePlane {
                claim_kind: ClaimKind::Effect,
                declared_status: "passed".to_string(),
                observed_status: "passed".to_string(),
                observed_assertions: Some(12),
                observed_http_status: None,
                process_path_authorized: true,
                behavioral_passed: true,
                target_uri_or_path: Some("src/decapod/core/agent_interop.rs".to_string()),
            },
            envelope_sha256: String::new(),
        }
        .with_computed_digest()
    }

    #[test]
    fn valid_envelope_imports_as_external_evidence_only_never_authority() {
        let env = valid_envelope();
        let policy = sample_policy();
        let mut ledger = ReplayLedger::new();
        let report = import_external_envelope(&env, &policy, &mut ledger, 1500);
        assert_eq!(report.verdict, NormalizedVerdict::AcceptedExternalEvidence);
        assert_eq!(report.reason_class, ReasonClass::ValidExternalEvidence);
        assert!(report.accepted_as_external_evidence);
        assert!(!report.imported_authority_granted);
        assert_eq!(report.omitted_optional_extensions, vec!["future-ext"]);

        // Replaying the identical envelope MUST fail closed.
        let replay = import_external_envelope(&env, &policy, &mut ledger, 1501);
        assert_eq!(replay.verdict, NormalizedVerdict::Rejected);
        assert_eq!(replay.reason_class, ReasonClass::ReplayDetected);
    }

    #[test]
    fn summary_fiction_without_raw_receipts_is_rejected() {
        let mut env = valid_envelope();
        env.evidence.raw_receipts.clear();
        env.evidence.claimed_receipt_digests = (0..15).map(|i| format!("{i:064x}")).collect();
        env.evidence.self_generated_checksum_only = true;
        env = env.with_computed_digest();
        let mut ledger = ReplayLedger::new();
        let report = import_external_envelope(&env, &sample_policy(), &mut ledger, 1500);
        assert_eq!(report.verdict, NormalizedVerdict::NotProven);
        assert_eq!(
            report.reason_class,
            ReasonClass::SummaryFictionMissingRawReceipts
        );
        assert!(!report.verification_layers.raw_receipt_digests_checked);
    }

    #[test]
    fn decision_disposition_distinguishes_all_four_plus_transport_errors() {
        let admit = project_decision_disposition(&DecisionBoundaryInput {
            operation: "store.upsert".to_string(),
            subject_ref: "head:abc".to_string(),
            transport_state: EvaluationTransportState::Evaluated,
            hard_constraint_violated: false,
            requires_principal_decision: false,
            bounded_repair_available: false,
            proposed_repair_expands_scope_or_args: false,
            deterministic_requirements_satisfied: true,
            underlying_reason: "all checks green".to_string(),
        });
        assert_eq!(admit.disposition, Some(DecisionDisposition::Admit));

        let repair = project_decision_disposition(&DecisionBoundaryInput {
            operation: "validate.run".to_string(),
            subject_ref: "head:abc".to_string(),
            transport_state: EvaluationTransportState::Evaluated,
            hard_constraint_violated: false,
            requires_principal_decision: false,
            bounded_repair_available: true,
            proposed_repair_expands_scope_or_args: false,
            deterministic_requirements_satisfied: false,
            underlying_reason: "fmt drift".to_string(),
        });
        assert_eq!(
            repair.disposition,
            Some(DecisionDisposition::RepairRequired)
        );

        let scope_expanding_repair = project_decision_disposition(&DecisionBoundaryInput {
            operation: "validate.run".to_string(),
            subject_ref: "head:abc".to_string(),
            transport_state: EvaluationTransportState::Evaluated,
            hard_constraint_violated: false,
            requires_principal_decision: false,
            bounded_repair_available: true,
            proposed_repair_expands_scope_or_args: true,
            deterministic_requirements_satisfied: false,
            underlying_reason: "attempts to rewrite protected path".to_string(),
        });
        assert_eq!(
            scope_expanding_repair.disposition,
            Some(DecisionDisposition::Reject)
        );

        let timeout = project_decision_disposition(&DecisionBoundaryInput {
            operation: "validate.run".to_string(),
            subject_ref: "head:abc".to_string(),
            transport_state: EvaluationTransportState::Unavailable,
            hard_constraint_violated: false,
            requires_principal_decision: false,
            bounded_repair_available: false,
            proposed_repair_expands_scope_or_args: false,
            deterministic_requirements_satisfied: false,
            underlying_reason: "verifier timed out".to_string(),
        });
        assert_eq!(timeout.disposition, None);
        assert_eq!(
            timeout.transport_state,
            EvaluationTransportState::Unavailable
        );
    }

    #[test]
    fn observed_conformance_receipt_enforces_binary_source_transport_and_producer_trust() {
        let base = interop::builtin_manifest();
        let mut accepted = BTreeSet::new();
        accepted.insert("trusted-ci-runner".to_string());
        let ctx = CurrentBuildContext {
            schema_version: interop::INTEROP_MANIFEST_SCHEMA_VERSION.to_string(),
            profile_id: interop::INTEROP_PROFILE_ID.to_string(),
            profile_version: interop::INTEROP_PROFILE_VERSION.to_string(),
            implementation_version: env!("CARGO_PKG_VERSION").to_string(),
            binary_sha256: "1".repeat(64),
            source_commit_sha: "2".repeat(40),
            transport: "rpc-stdin".to_string(),
            now_epoch: 1500,
        };
        let receipt = ObservedConformanceReceipt {
            receipt_id: "obs-01".to_string(),
            schema_version: ctx.schema_version.clone(),
            profile_id: ctx.profile_id.clone(),
            profile_version: ctx.profile_version.clone(),
            implementation_version: ctx.implementation_version.clone(),
            binary_sha256: ctx.binary_sha256.clone(),
            source_commit_sha: ctx.source_commit_sha.clone(),
            transport: ctx.transport.clone(),
            producer_id: "trusted-ci-runner".to_string(),
            producer_trust_class: ProducerTrustClass::TrustedCi,
            executed_at_epoch: 1400,
            max_age_secs: 600,
            results: vec![ObservedCapabilityResult {
                capability_id: "rpc:agent.init".to_string(),
                conformance_test: "interop_conformance::discovery_and_negotiation".to_string(),
                passed: true,
                raw_evidence: RawEvidenceReceipt::from_raw("ev-1", "test ok"),
            }],
            receipt_sha256: String::new(),
        }
        .with_computed_digest();

        // Current valid receipt promotes ONLY the observed capability to Verified.
        let ok = project_manifest_with_receipt(&base, &receipt, &ctx, &accepted);
        assert!(ok.receipt_accepted);
        let init_cap = ok
            .manifest
            .capabilities
            .iter()
            .find(|c| c.id == "rpc:agent.init")
            .unwrap();
        assert_eq!(init_cap.evidence.state, VerificationState::Verified);

        // Wrong binary SHA marks it Stale, never Verified.
        let mut wrong_bin_ctx = ctx.clone();
        wrong_bin_ctx.binary_sha256 = "9".repeat(64);
        let stale = project_manifest_with_receipt(&base, &receipt, &wrong_bin_ctx, &accepted);
        assert!(!stale.receipt_accepted);
        let stale_cap = stale
            .manifest
            .capabilities
            .iter()
            .find(|c| c.id == "rpc:agent.init")
            .unwrap();
        assert_eq!(stale_cap.evidence.state, VerificationState::Stale);

        // Self-declared producer keeps NotRun.
        let mut self_decl = receipt.clone();
        self_decl.producer_trust_class = ProducerTrustClass::SelfDeclared;
        self_decl = self_decl.with_computed_digest();
        let denied = project_manifest_with_receipt(&base, &self_decl, &ctx, &accepted);
        assert!(!denied.receipt_accepted);
        let denied_cap = denied
            .manifest
            .capabilities
            .iter()
            .find(|c| c.id == "rpc:agent.init")
            .unwrap();
        assert_eq!(denied_cap.evidence.state, VerificationState::NotRun);
    }

    #[test]
    fn rule_taxonomy_contains_all_eight_domains_and_zero_third_party_product_names() {
        let matrix = rules::coding_rule_taxonomy_matrix();
        assert_eq!(matrix.domains_covered.len(), 8);
        let json = serde_json::to_string(&matrix).unwrap().to_ascii_lowercase();
        for forbidden in ["cursor", "windsurf", "copilot", "devin", "cline", "aider"] {
            assert!(
                !json.contains(forbidden),
                "coding-rule taxonomy must not reference third-party product name '{forbidden}'"
            );
        }
    }
}
