//! Unified derived per-todo/PR obligation ledger, stagnation detection,
//! structured agent outcome claims, 4-dimension completion reports, canonical
//! approval binding, trusted grader selection, root requirement conservation,
//! direct main-merge policy evaluation, and offline telemetry/benchmark harnesses.
//!
//! Covers:
//! - `#1084`: Unified derived per-todo/PR obligation ledger (conservation/consistency/proof + async effects)
//! - `#1083`: Lifecycle governed-state stagnation detection (compaction-resistant, provider-wait aware)
//! - `#1729`: Structured agent outcome-claim schema & source-anchor verification
//! - `#1726`: 4-dimension completion report & stale-build / zero-assertion / skip detection
//! - `#1761`: Exact canonicalized approval-vs-execution binding & single-use approval ledger
//! - `#1762`: Trusted grader selection & held-out negative-control adequacy reporting
//! - `#1768`: Root requirement conservation, authorized supersession & freshness-loss detection
//! - `#1468`: Accepted-policy direct `main_merge` authorization check (fail-closed on target movement)
//! - `#1095`, `#1772`, `#1774`, `#1796`: Deterministic offline telemetry & benchmark harnesses

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const OBLIGATION_LEDGER_SCHEMA: &str = "decapod.obligation-ledger.v1";
pub const COMPLETION_REPORT_4D_SCHEMA: &str = "decapod.completion-report-4d.v1";
pub const GRADER_REPORT_SCHEMA: &str = "decapod.trusted-grader-report.v1";

fn sha256_str(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ============================================================================
// #1084: Unified Derived Per-Todo / Per-PR Obligation Ledger
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ObligationCategory {
    /// Root requirement & task mapping completeness (#1768).
    Conservation,
    /// Cross-artifact / contract / policy consistency (#1770).
    Consistency,
    /// Independently executed verification & behavioral evidence (#1762/#1730).
    Proof,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ObligationSurface {
    Governance,
    LivingSpec,
    Entrypoint,
    Execution,
    Publication,
    ExternalEffect,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AsyncEffectState {
    Pending,
    Indeterminate,
    ObservedSucceeded,
    ObservedFailed,
    ObservedAborted,
}

impl AsyncEffectState {
    pub fn is_resolved_success(self) -> bool {
        matches!(self, Self::ObservedSucceeded)
    }

    pub fn is_unresolved(self) -> bool {
        matches!(self, Self::Pending | Self::Indeterminate)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DerivedObligationStatus {
    Satisfied,
    Unsatisfied,
    Stale,
    PendingAsync,
    IndeterminateAsync,
    Unavailable,
    HumanRequired,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum InvalidationReason {
    SourceChanged,
    BaseMoved,
    RulesChanged,
    VersionChanged,
    CustodyChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DelayedAsyncEffect {
    pub effect_id: String,
    pub obligation_id: String,
    pub target_system: String,
    pub state: AsyncEffectState,
    pub deadline_epoch: Option<i64>,
    pub evidence_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObligationInput {
    pub obligation_id: String,
    pub requirement_id: String,
    pub task_id: String,
    pub pr_number: Option<u64>,
    pub category: ObligationCategory,
    pub surface: ObligationSurface,
    pub owner: String,
    pub accepted_revision: String,
    pub observed_revision: Option<String>,
    pub rule_id: String,
    pub phase: String,
    pub evidence_ref: Option<String>,
    pub evidence_passed: bool,
    pub model_declared_done: bool,
    pub cached_status_only: bool,
    pub requires_human_authority: bool,
    pub invalidations: Vec<InvalidationReason>,
    pub async_effect: Option<DelayedAsyncEffect>,
    pub next_admissible_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DerivedObligationItem {
    pub obligation_id: String,
    pub requirement_id: String,
    pub category: ObligationCategory,
    pub surface: ObligationSurface,
    pub owner: String,
    pub accepted_revision: String,
    pub rule_id: String,
    pub phase: String,
    pub status: DerivedObligationStatus,
    pub evidence_ref: Option<String>,
    pub invalidations: Vec<InvalidationReason>,
    pub async_state: Option<AsyncEffectState>,
    pub next_admissible_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DerivedObligationLedger {
    pub schema: String,
    pub query_subject: String,
    pub task_id: Option<String>,
    pub pr_number: Option<u64>,
    pub grants_authority: bool,
    pub ready: bool,
    pub ignored_unrelated_backlog: usize,
    pub conservation_unsatisfied: usize,
    pub consistency_unsatisfied: usize,
    pub proof_unsatisfied: usize,
    pub pending_or_indeterminate_effects: usize,
    pub items: Vec<DerivedObligationItem>,
    pub state_fingerprint: String,
}

/// Derive a deterministic, read-only obligation ledger for a specific todo or PR
/// (`Continue PR #N`). Unrelated global backlog items are ignored; cached status
/// and model declarations never satisfy an obligation; source/base/rules/version/
/// custody changes invalidate affected obligations; pending/indeterminate async
/// effects remain unresolved until authoritatively observed.
pub fn derive_obligation_ledger(
    selected_task_id: Option<&str>,
    selected_pr_number: Option<u64>,
    inputs: &[ObligationInput],
) -> DerivedObligationLedger {
    let mut items = Vec::new();
    let mut ignored_unrelated_backlog = 0usize;

    for input in inputs {
        let task_matches = selected_task_id.is_none_or(|tid| input.task_id == tid);
        let pr_matches = selected_pr_number.is_none_or(|pr| input.pr_number == Some(pr));
        if !(task_matches && pr_matches) {
            ignored_unrelated_backlog += 1;
            continue;
        }

        let revision_stale = input
            .observed_revision
            .as_deref()
            .is_some_and(|obs| obs != input.accepted_revision);
        let has_invalidations = !input.invalidations.is_empty() || revision_stale;

        let async_state = input.async_effect.as_ref().map(|e| e.state);
        let status = if let Some(effect_state) = async_state {
            match effect_state {
                AsyncEffectState::Pending => DerivedObligationStatus::PendingAsync,
                AsyncEffectState::Indeterminate => DerivedObligationStatus::IndeterminateAsync,
                AsyncEffectState::ObservedFailed | AsyncEffectState::ObservedAborted => {
                    DerivedObligationStatus::Unsatisfied
                }
                AsyncEffectState::ObservedSucceeded => {
                    if has_invalidations {
                        DerivedObligationStatus::Stale
                    } else if input.evidence_passed
                        && !input.cached_status_only
                        && input.evidence_ref.is_some()
                    {
                        DerivedObligationStatus::Satisfied
                    } else {
                        DerivedObligationStatus::Unsatisfied
                    }
                }
            }
        } else if input.requires_human_authority && !input.evidence_passed {
            DerivedObligationStatus::HumanRequired
        } else if has_invalidations {
            DerivedObligationStatus::Stale
        } else if input.cached_status_only || !input.evidence_passed || input.evidence_ref.is_none()
        {
            // Model declarations (`model_declared_done`) and cached status alone NEVER satisfy.
            let _ = input.model_declared_done;
            DerivedObligationStatus::Unsatisfied
        } else {
            DerivedObligationStatus::Satisfied
        };

        let mut invalidations = input.invalidations.clone();
        if revision_stale && !invalidations.contains(&InvalidationReason::SourceChanged) {
            invalidations.push(InvalidationReason::SourceChanged);
        }
        invalidations.sort();
        invalidations.dedup();

        items.push(DerivedObligationItem {
            obligation_id: input.obligation_id.clone(),
            requirement_id: input.requirement_id.clone(),
            category: input.category,
            surface: input.surface,
            owner: input.owner.clone(),
            accepted_revision: input.accepted_revision.clone(),
            rule_id: input.rule_id.clone(),
            phase: input.phase.clone(),
            status,
            evidence_ref: input.evidence_ref.clone(),
            invalidations,
            async_state,
            next_admissible_action: if status == DerivedObligationStatus::Satisfied {
                "none".to_string()
            } else {
                input.next_admissible_action.clone()
            },
        });
    }

    items.sort_by(|a, b| a.obligation_id.cmp(&b.obligation_id));

    let conservation_unsatisfied = items
        .iter()
        .filter(|i| {
            i.category == ObligationCategory::Conservation
                && i.status != DerivedObligationStatus::Satisfied
        })
        .count();
    let consistency_unsatisfied = items
        .iter()
        .filter(|i| {
            i.category == ObligationCategory::Consistency
                && i.status != DerivedObligationStatus::Satisfied
        })
        .count();
    let proof_unsatisfied = items
        .iter()
        .filter(|i| {
            i.category == ObligationCategory::Proof
                && i.status != DerivedObligationStatus::Satisfied
        })
        .count();
    let pending_or_indeterminate_effects = items
        .iter()
        .filter(|i| i.async_state.is_some_and(AsyncEffectState::is_unresolved))
        .count();

    let ready = !items.is_empty()
        && items
            .iter()
            .all(|i| i.status == DerivedObligationStatus::Satisfied);

    let fp_material = serde_json::to_string(&items).unwrap_or_default();
    let state_fingerprint = sha256_str(&fp_material);

    let query_subject = match (selected_task_id, selected_pr_number) {
        (Some(t), Some(pr)) => format!("todo:{t}:pr#{pr}"),
        (Some(t), None) => format!("todo:{t}"),
        (None, Some(pr)) => format!("pr#{pr}"),
        (None, None) => "all".to_string(),
    };

    DerivedObligationLedger {
        schema: OBLIGATION_LEDGER_SCHEMA.to_string(),
        query_subject,
        task_id: selected_task_id.map(str::to_string),
        pr_number: selected_pr_number,
        grants_authority: false,
        ready,
        ignored_unrelated_backlog,
        conservation_unsatisfied,
        consistency_unsatisfied,
        proof_unsatisfied,
        pending_or_indeterminate_effects,
        items,
        state_fingerprint,
    }
}

// ============================================================================
// #1083: Lifecycle Governed-State Stagnation Detection
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderWaitState {
    pub provider: String,
    pub check_id: String,
    pub deadline_epoch: i64,
    pub recheck_after_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StagnationObservation {
    pub task_id: String,
    pub head_commit: String,
    pub ledger_fingerprint: String,
    pub satisfied_obligations: BTreeSet<String>,
    pub unresolved_obligations: BTreeMap<String, String>,
    pub evidence_digests: BTreeSet<String>,
    /// Paraphrased agent prose; intentionally excluded from governed-state progress fingerprint!
    pub agent_proposal_prose: String,
    pub now_epoch: i64,
    pub provider_wait: Option<ProviderWaitState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum StagnationDisposition {
    Progressing {
        satisfied_count: usize,
        unresolved_count: usize,
    },
    WaitingOnProvider {
        provider: String,
        check_id: String,
        deadline_epoch: i64,
        recheck_after_secs: u64,
        guidance: String,
    },
    Stagnant {
        repeated_rounds: usize,
        remaining_budget: usize,
        unresolved_obligations: Vec<String>,
        next_admissible_action: String,
    },
    Regressed {
        previously_satisfied_lost: Vec<String>,
        next_admissible_action: String,
    },
    TerminalExhausted {
        budget: usize,
        blockers: Vec<String>,
        recovery_guidance: String,
    },
}

/// Compaction-resistant stagnation tracker per task_id (#1083).
/// Serializes cleanly to JSON so it survives context compaction or agent restart.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StagnationTracker {
    pub task_id: String,
    pub max_stagnant_rounds: usize,
    pub stagnant_rounds: usize,
    pub last_governed_fingerprint: Option<String>,
    pub last_satisfied: BTreeSet<String>,
    pub last_evidence_count: usize,
}

impl StagnationTracker {
    pub fn new(task_id: impl Into<String>, max_stagnant_rounds: usize) -> Self {
        Self {
            task_id: task_id.into(),
            max_stagnant_rounds: max_stagnant_rounds.max(1),
            stagnant_rounds: 0,
            last_governed_fingerprint: None,
            last_satisfied: BTreeSet::new(),
            last_evidence_count: 0,
        }
    }

    fn governed_fingerprint(obs: &StagnationObservation) -> String {
        // Deliberately excludes `agent_proposal_prose` so paraphrased proposals
        // or repeated equivalent queries CANNOT fabricate progress or reset budget.
        let payload = serde_json::json!({
            "task_id": obs.task_id,
            "head_commit": obs.head_commit,
            "ledger_fingerprint": obs.ledger_fingerprint,
            "satisfied": obs.satisfied_obligations,
            "unresolved": obs.unresolved_obligations,
            "evidence_digests": obs.evidence_digests,
        });
        sha256_str(&payload.to_string())
    }

    pub fn observe(
        &mut self,
        obs: &StagnationObservation,
    ) -> Result<StagnationDisposition, String> {
        if obs.task_id != self.task_id {
            return Err(format!(
                "STAGNATION_TASK_ISOLATION_VIOLATION: tracker owns '{}', received '{}'",
                self.task_id, obs.task_id
            ));
        }

        // Known running provider/check with finite unexpired deadline is waiting, not stagnation.
        if let Some(wait) = &obs.provider_wait
            && wait.deadline_epoch > obs.now_epoch
        {
            return Ok(StagnationDisposition::WaitingOnProvider {
                provider: wait.provider.clone(),
                check_id: wait.check_id.clone(),
                deadline_epoch: wait.deadline_epoch,
                recheck_after_secs: wait.recheck_after_secs,
                guidance: format!(
                    "Provider '{}' check '{}' is actively running until epoch {}; recheck after {}s without escalating.",
                    wait.provider, wait.check_id, wait.deadline_epoch, wait.recheck_after_secs
                ),
            });
        }

        let fp = Self::governed_fingerprint(obs);
        let lost: Vec<String> = self
            .last_satisfied
            .difference(&obs.satisfied_obligations)
            .cloned()
            .collect();

        let next_action = obs
            .unresolved_obligations
            .values()
            .next()
            .cloned()
            .unwrap_or_else(|| "Run `decapod validate` on the exact candidate".to_string());

        if !lost.is_empty() {
            self.stagnant_rounds += 1;
            self.last_governed_fingerprint = Some(fp);
            self.last_satisfied = obs.satisfied_obligations.clone();
            self.last_evidence_count = obs.evidence_digests.len();
            if self.stagnant_rounds >= self.max_stagnant_rounds {
                return Ok(StagnationDisposition::TerminalExhausted {
                    budget: self.max_stagnant_rounds,
                    blockers: lost,
                    recovery_guidance: format!(
                        "Finite progress budget ({}) exhausted after regression; repair regressed obligations before retrying.",
                        self.max_stagnant_rounds
                    ),
                });
            }
            return Ok(StagnationDisposition::Regressed {
                previously_satisfied_lost: lost,
                next_admissible_action: next_action,
            });
        }

        let materially_unchanged = self
            .last_governed_fingerprint
            .as_deref()
            .is_some_and(|prev| prev == fp);

        if materially_unchanged && !obs.unresolved_obligations.is_empty() {
            self.stagnant_rounds += 1;
            let unresolved_keys: Vec<String> = obs.unresolved_obligations.keys().cloned().collect();
            if self.stagnant_rounds >= self.max_stagnant_rounds {
                return Ok(StagnationDisposition::TerminalExhausted {
                    budget: self.max_stagnant_rounds,
                    blockers: unresolved_keys,
                    recovery_guidance: format!(
                        "Stagnation budget ({}) exhausted with no material governed progress. Next safe action: {next_action}",
                        self.max_stagnant_rounds
                    ),
                });
            }
            return Ok(StagnationDisposition::Stagnant {
                repeated_rounds: self.stagnant_rounds,
                remaining_budget: self
                    .max_stagnant_rounds
                    .saturating_sub(self.stagnant_rounds),
                unresolved_obligations: unresolved_keys,
                next_admissible_action: next_action,
            });
        }

        // Material governed progress resets stagnation counter.
        self.stagnant_rounds = 0;
        self.last_governed_fingerprint = Some(fp);
        self.last_satisfied = obs.satisfied_obligations.clone();
        self.last_evidence_count = obs.evidence_digests.len();
        Ok(StagnationDisposition::Progressing {
            satisfied_count: obs.satisfied_obligations.len(),
            unresolved_count: obs.unresolved_obligations.len(),
        })
    }
}

// ============================================================================
// #1729: Structured Agent Outcome Claims & Source Anchor Verification
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentOutcomeClaim {
    pub claim_id: String,
    pub declared_change_id: String,
    pub source_path: String,
    pub symbol_or_callsite: String,
    pub claimed_head_revision: String,
    pub original_request_revision: String,
    pub interpretation_revision: String,
    pub linked_root_obligations: Vec<String>,
    pub claimed_effect: String,
    #[serde(default)]
    pub trajectory_loop_refs: Vec<String>,
    /// Always false: an agent outcome claim is a declaration, never proof (#1729).
    pub is_proof: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceAnchorStatus {
    Matched,
    MissingFile,
    SymbolAbsentAtSite,
    HeadChangedStale,
    UnverifiableUnsupportedLanguage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceAnchorVerification {
    pub claim_id: String,
    pub source_path: String,
    pub symbol_or_callsite: String,
    pub status: SourceAnchorStatus,
    pub missing_root_obligations: Vec<String>,
    pub detail: String,
}

const SUPPORTED_ANCHOR_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "py", "go", "md", "toml", "json", "sh", "sql",
];

/// Independently verify an agent's declared source path/symbol anchor against
/// actual workspace bytes, current HEAD, and accepted root obligations (#1729).
pub fn verify_outcome_claim_anchor(
    workspace_root: &Path,
    claim: &AgentOutcomeClaim,
    current_head_revision: &str,
    accepted_root_obligations: &BTreeSet<String>,
) -> SourceAnchorVerification {
    let missing_roots: Vec<String> = accepted_root_obligations
        .iter()
        .filter(|req| !claim.linked_root_obligations.contains(*req))
        .cloned()
        .collect();

    if claim.claimed_head_revision != current_head_revision {
        return SourceAnchorVerification {
            claim_id: claim.claim_id.clone(),
            source_path: claim.source_path.clone(),
            symbol_or_callsite: claim.symbol_or_callsite.clone(),
            status: SourceAnchorStatus::HeadChangedStale,
            missing_root_obligations: missing_roots,
            detail: format!(
                "Claim targets head '{}' but current workspace head is '{}'; prior anchor check is stale.",
                claim.claimed_head_revision, current_head_revision
            ),
        };
    }

    let full_path = workspace_root.join(&claim.source_path);
    if !full_path.is_file() {
        return SourceAnchorVerification {
            claim_id: claim.claim_id.clone(),
            source_path: claim.source_path.clone(),
            symbol_or_callsite: claim.symbol_or_callsite.clone(),
            status: SourceAnchorStatus::MissingFile,
            missing_root_obligations: missing_roots,
            detail: format!(
                "Declared source path '{}' does not exist.",
                claim.source_path
            ),
        };
    }

    let ext = full_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !SUPPORTED_ANCHOR_EXTENSIONS.contains(&ext.as_str()) {
        return SourceAnchorVerification {
            claim_id: claim.claim_id.clone(),
            source_path: claim.source_path.clone(),
            symbol_or_callsite: claim.symbol_or_callsite.clone(),
            status: SourceAnchorStatus::UnverifiableUnsupportedLanguage,
            missing_root_obligations: missing_roots,
            detail: format!(
                "Extension '.{ext}' is outside supported deterministic source-anchor parsers; reported honestly as unverifiable."
            ),
        };
    }

    let Ok(contents) = std::fs::read_to_string(&full_path) else {
        return SourceAnchorVerification {
            claim_id: claim.claim_id.clone(),
            source_path: claim.source_path.clone(),
            symbol_or_callsite: claim.symbol_or_callsite.clone(),
            status: SourceAnchorStatus::UnverifiableUnsupportedLanguage,
            missing_root_obligations: missing_roots,
            detail: "Source file is not readable UTF-8.".to_string(),
        };
    };

    if contents.contains(&claim.symbol_or_callsite) {
        SourceAnchorVerification {
            claim_id: claim.claim_id.clone(),
            source_path: claim.source_path.clone(),
            symbol_or_callsite: claim.symbol_or_callsite.clone(),
            status: SourceAnchorStatus::Matched,
            missing_root_obligations: missing_roots,
            detail: "Declared symbol/callsite found at asserted path on current HEAD (anchor match is not behavioral proof).".to_string(),
        }
    } else {
        SourceAnchorVerification {
            claim_id: claim.claim_id.clone(),
            source_path: claim.source_path.clone(),
            symbol_or_callsite: claim.symbol_or_callsite.clone(),
            status: SourceAnchorStatus::SymbolAbsentAtSite,
            missing_root_obligations: missing_roots,
            detail: format!(
                "Declared symbol/callsite '{}' is absent from '{}'; renamed, moved, or unimplemented.",
                claim.symbol_or_callsite, claim.source_path
            ),
        }
    }
}

// ============================================================================
// #1726: 4-Dimension Completion Report & Stale-Build / Zero-Assertion Checks
// ============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DimensionStatus {
    Passed,
    Failed,
    Stale,
    NotProven,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DimensionVerdict {
    pub status: DimensionStatus,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompletionReportInput {
    pub candidate_head: String,
    pub built_binary_source_digest: String,
    pub current_source_digest: String,
    pub executed_assertion_count: usize,
    pub path_filter_skipped_required_checks: bool,
    pub behavioral_tests_passed: bool,
    pub anchor_verifications: Vec<SourceAnchorVerification>,
    pub unauthorized_path_transitions: Vec<String>,
    pub observer_missing_events: Vec<String>,
    pub pr_admission_gate_passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompletionReport4D {
    pub schema: String,
    pub candidate_head: String,
    /// Dimension 1: Actual behavioral execution & non-stale build evidence.
    pub outcome_proof: DimensionVerdict,
    /// Dimension 2: Observed execution-path authority integrity.
    pub path_integrity: DimensionVerdict,
    /// Dimension 3: Completeness of independent observer/anchor telemetry.
    pub observer_completeness: DimensionVerdict,
    /// Dimension 4: PR / publication admission readiness.
    pub pr_admission: DimensionVerdict,
    pub overall_admissible: bool,
}

pub fn build_completion_report_4d(input: &CompletionReportInput) -> CompletionReport4D {
    let mut outcome_reasons = Vec::new();
    let mut outcome_stale = false;

    if input.built_binary_source_digest != input.current_source_digest {
        outcome_stale = true;
        outcome_reasons.push(
            "STALE_EXECUTABLE_BUILD: tested executable digest does not match current workspace source digest.".to_string(),
        );
    }
    if input.executed_assertion_count == 0 {
        outcome_reasons.push(
            "ZERO_ASSERTION_EXECUTION: zero assertions executed; green exit with 0 assertions is not behavioral proof.".to_string(),
        );
    }
    if input.path_filter_skipped_required_checks {
        outcome_reasons.push(
            "PATH_FILTER_SKIP_DETECTED: path filter skipped mandatory checks for modified surfaces."
                .to_string(),
        );
    }
    if !input.behavioral_tests_passed {
        outcome_reasons.push("BEHAVIORAL_CHECKS_FAILED: required tests failed.".to_string());
    }

    let outcome_status = if !outcome_reasons.is_empty() {
        if outcome_stale && outcome_reasons.len() == 1 {
            DimensionStatus::Stale
        } else {
            DimensionStatus::Failed
        }
    } else {
        DimensionStatus::Passed
    };

    let path_status = if input.unauthorized_path_transitions.is_empty() {
        DimensionStatus::Passed
    } else {
        DimensionStatus::Failed
    };

    let mut observer_reasons = input.observer_missing_events.clone();
    for anchor in &input.anchor_verifications {
        if anchor.status != SourceAnchorStatus::Matched {
            observer_reasons.push(format!(
                "ANCHOR_UNVERIFIED({}): {:?} - {}",
                anchor.claim_id, anchor.status, anchor.detail
            ));
        }
    }
    let observer_status = if observer_reasons.is_empty() {
        DimensionStatus::Passed
    } else if input.anchor_verifications.iter().any(|a| {
        matches!(
            a.status,
            SourceAnchorStatus::HeadChangedStale | SourceAnchorStatus::SymbolAbsentAtSite
        )
    }) {
        DimensionStatus::Failed
    } else {
        DimensionStatus::NotProven
    };

    let mut pr_reasons = Vec::new();
    if !input.pr_admission_gate_passed {
        pr_reasons.push("PR_ADMISSION_GATE_NOT_PASSED".to_string());
    }
    if outcome_status != DimensionStatus::Passed {
        pr_reasons.push("OUTCOME_PROOF_NOT_PASSED".to_string());
    }
    if path_status != DimensionStatus::Passed {
        pr_reasons.push("PATH_INTEGRITY_NOT_PASSED".to_string());
    }
    if observer_status != DimensionStatus::Passed {
        pr_reasons.push("OBSERVER_COMPLETENESS_NOT_PASSED".to_string());
    }
    let pr_status = if pr_reasons.is_empty() {
        DimensionStatus::Passed
    } else {
        DimensionStatus::Failed
    };

    CompletionReport4D {
        schema: COMPLETION_REPORT_4D_SCHEMA.to_string(),
        candidate_head: input.candidate_head.clone(),
        outcome_proof: DimensionVerdict {
            status: outcome_status,
            reasons: outcome_reasons,
        },
        path_integrity: DimensionVerdict {
            status: path_status,
            reasons: input.unauthorized_path_transitions.clone(),
        },
        observer_completeness: DimensionVerdict {
            status: observer_status,
            reasons: observer_reasons,
        },
        pr_admission: DimensionVerdict {
            status: pr_status,
            reasons: pr_reasons,
        },
        overall_admissible: outcome_status == DimensionStatus::Passed
            && path_status == DimensionStatus::Passed
            && observer_status == DimensionStatus::Passed
            && pr_status == DimensionStatus::Passed,
    }
}

// ============================================================================
// #1761: Exact Canonicalized Approval-vs-Execution & Single-Use Binding
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CanonicalInvocation {
    pub program: String,
    pub args: Vec<String>,
    pub subject_commit: String,
    pub target_base: String,
}

impl CanonicalInvocation {
    /// Deterministic canonical digest binding exact program, exact argument vector
    /// (order-sensitive, no shell-splitting ambiguity), subject commit, and target base.
    pub fn canonical_digest(&self) -> String {
        let serialized = serde_json::to_string(&(
            "decapod.canonical-invocation.v1",
            &self.program,
            &self.args,
            &self.subject_commit,
            &self.target_base,
        ))
        .unwrap_or_default();
        sha256_str(&serialized)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CanonicalApprovalRecord {
    pub approval_id: String,
    pub principal_id: String,
    pub canonical_invocation_digest: String,
    pub subject_commit: String,
    pub target_base: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub single_use: bool,
    pub decision_allow: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalLedger {
    pub approvals: Vec<CanonicalApprovalRecord>,
    pub revoked_principals: BTreeSet<String>,
    pub consumed_approval_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalExecutionVerdict {
    pub admitted: bool,
    pub consumed_approval_id: Option<String>,
    pub rejection_code: Option<String>,
    pub detail: String,
}

impl ApprovalLedger {
    /// Authorize and consume a single-use approval strictly matching the exact
    /// canonicalized invocation. Rejects: argument drift, revoked principal,
    /// expired approval, single-use replay, and two-principal conflicts.
    pub fn authorize_and_consume(
        &mut self,
        invocation: &CanonicalInvocation,
        now_epoch: i64,
        independent_read_digest: &str,
    ) -> ApprovalExecutionVerdict {
        let expected_digest = invocation.canonical_digest();
        if expected_digest != independent_read_digest {
            return ApprovalExecutionVerdict {
                admitted: false,
                consumed_approval_id: None,
                rejection_code: Some("INDEPENDENT_READ_PARITY_MISMATCH".to_string()),
                detail: "Independent read-side canonical invocation digest disagrees with execution request.".to_string(),
            };
        }

        let matching_subject: Vec<&CanonicalApprovalRecord> = self
            .approvals
            .iter()
            .filter(|a| {
                a.subject_commit == invocation.subject_commit
                    && a.target_base == invocation.target_base
                    && a.canonical_invocation_digest == expected_digest
            })
            .collect();

        // Two-principal conflict handling: if any active principal explicitly denied
        // or two principals disagree on the same invocation, fail closed.
        let active_allows: BTreeSet<&str> = matching_subject
            .iter()
            .filter(|a| {
                a.decision_allow
                    && !self.revoked_principals.contains(&a.principal_id)
                    && a.expires_at > now_epoch
            })
            .map(|a| a.principal_id.as_str())
            .collect();
        let active_denies: BTreeSet<&str> = matching_subject
            .iter()
            .filter(|a| !a.decision_allow && !self.revoked_principals.contains(&a.principal_id))
            .map(|a| a.principal_id.as_str())
            .collect();

        if !active_allows.is_empty() && !active_denies.is_empty() {
            return ApprovalExecutionVerdict {
                admitted: false,
                consumed_approval_id: None,
                rejection_code: Some("TWO_PRINCIPAL_CONFLICT".to_string()),
                detail: format!(
                    "Conflicting approvals across principals (allow={:?}, deny={:?}); failing closed.",
                    active_allows, active_denies
                ),
            };
        }

        for approval in &self.approvals {
            if approval.canonical_invocation_digest != expected_digest {
                continue;
            }
            if self.revoked_principals.contains(&approval.principal_id) {
                return ApprovalExecutionVerdict {
                    admitted: false,
                    consumed_approval_id: None,
                    rejection_code: Some("REVOKED_PRINCIPAL".to_string()),
                    detail: format!(
                        "Principal '{}' has been revoked; approval '{}' refused.",
                        approval.principal_id, approval.approval_id
                    ),
                };
            }
            if !approval.decision_allow {
                return ApprovalExecutionVerdict {
                    admitted: false,
                    consumed_approval_id: None,
                    rejection_code: Some("PRINCIPAL_DENIED".to_string()),
                    detail: format!(
                        "Approval '{}' explicitly denied execution.",
                        approval.approval_id
                    ),
                };
            }
            if approval.expires_at <= now_epoch || approval.issued_at > now_epoch {
                return ApprovalExecutionVerdict {
                    admitted: false,
                    consumed_approval_id: None,
                    rejection_code: Some("APPROVAL_EXPIRED".to_string()),
                    detail: format!(
                        "Approval '{}' is outside its valid freshness window.",
                        approval.approval_id
                    ),
                };
            }
            if approval.single_use && self.consumed_approval_ids.contains(&approval.approval_id) {
                return ApprovalExecutionVerdict {
                    admitted: false,
                    consumed_approval_id: None,
                    rejection_code: Some("SINGLE_USE_REPLAY_REJECTED".to_string()),
                    detail: format!(
                        "Single-use approval '{}' was already consumed; replay rejected.",
                        approval.approval_id
                    ),
                };
            }

            let id = approval.approval_id.clone();
            if approval.single_use {
                self.consumed_approval_ids.insert(id.clone());
            }
            return ApprovalExecutionVerdict {
                admitted: true,
                consumed_approval_id: Some(id),
                rejection_code: None,
                detail: "Exact canonicalized invocation matched active approval and was consumed."
                    .to_string(),
            };
        }

        ApprovalExecutionVerdict {
            admitted: false,
            consumed_approval_id: None,
            rejection_code: Some("CANONICAL_ARGUMENT_MISMATCH".to_string()),
            detail: "No approval matches the exact canonicalized command, arguments, subject, and target.".to_string(),
        }
    }
}

// ============================================================================
// #1762: Trusted Grader Selection & Held-Out Negative-Control Reporting
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraderSelectionInput {
    pub subject_commit: String,
    pub base_commit: String,
    pub test_selection: String,
    pub accepted_test_count: usize,
    pub actual_executable: String,
    pub observed_executable_sha256: String,
    pub trusted_executable_sha256: String,
    pub environment_fingerprint: String,
    pub producer_source: String,
    pub checked_obligations: Vec<String>,
    pub accepted_negative_control_executed: bool,
    pub heldout_counterexample_id: Option<String>,
    pub heldout_mutation_detected: bool,
    pub candidate_authored_tests_only: bool,
    pub checks_passed: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GraderOutcome {
    Proven,
    Failed,
    NotProven,
    EvaluatorTampered,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraderSelectionReport {
    pub schema: String,
    pub outcome: GraderOutcome,
    pub subject_commit: String,
    pub base_commit: String,
    pub test_selection: String,
    pub accepted_test_count: usize,
    pub actual_executable: String,
    pub environment_fingerprint: String,
    pub producer_source: String,
    pub checked_obligations: Vec<String>,
    pub counterexample_id: Option<String>,
    pub rejection_codes: Vec<String>,
    pub detail: String,
}

/// Evaluate trusted-grader selection & held-out negative-control adequacy (#1762).
pub fn evaluate_trusted_grader(input: &GraderSelectionInput) -> GraderSelectionReport {
    let mut codes = Vec::new();
    let mut tampered = false;

    if input.observed_executable_sha256 != input.trusted_executable_sha256 {
        tampered = true;
        codes.push("UNTRUSTED_EVALUATOR_SUBSTITUTION".to_string());
    }
    if input.accepted_test_count == 0 || input.test_selection.trim().is_empty() {
        codes.push("EMPTY_TEST_SELECTION".to_string());
    }
    if !input.checks_passed {
        codes.push("ACCEPTED_BEHAVIORAL_CHECK_FAILED".to_string());
    }
    if input.heldout_counterexample_id.is_some() && !input.heldout_mutation_detected {
        codes.push("HELDOUT_MUTATION_UNDETECTED".to_string());
    }
    if input.candidate_authored_tests_only && !input.accepted_negative_control_executed {
        codes.push("SEMANTIC_ALIGNMENT_NOT_PROVEN".to_string());
    }

    let outcome = if tampered {
        GraderOutcome::EvaluatorTampered
    } else if codes.iter().any(|c| {
        c == "EMPTY_TEST_SELECTION"
            || c == "ACCEPTED_BEHAVIORAL_CHECK_FAILED"
            || c == "HELDOUT_MUTATION_UNDETECTED"
    }) {
        GraderOutcome::Failed
    } else if codes.iter().any(|c| c == "SEMANTIC_ALIGNMENT_NOT_PROVEN") {
        GraderOutcome::NotProven
    } else {
        GraderOutcome::Proven
    };

    let detail = match outcome {
        GraderOutcome::Proven => format!(
            "Trusted evaluator executed {} accepted test(s) with negative-control adequacy verified.",
            input.accepted_test_count
        ),
        GraderOutcome::NotProven => {
            "Candidate self-tests passed without an independent accepted negative control; semantic alignment remains not_proven."
                .to_string()
        }
        GraderOutcome::EvaluatorTampered => {
            "Observed test runner executable digest does not match accepted trusted evaluator."
                .to_string()
        }
        GraderOutcome::Failed => format!(
            "Trusted grader rejected candidate behavior: {}",
            codes.join(", ")
        ),
    };

    GraderSelectionReport {
        schema: GRADER_REPORT_SCHEMA.to_string(),
        outcome,
        subject_commit: input.subject_commit.clone(),
        base_commit: input.base_commit.clone(),
        test_selection: input.test_selection.clone(),
        accepted_test_count: input.accepted_test_count,
        actual_executable: input.actual_executable.clone(),
        environment_fingerprint: input.environment_fingerprint.clone(),
        producer_source: input.producer_source.clone(),
        checked_obligations: input.checked_obligations.clone(),
        counterexample_id: input.heldout_counterexample_id.clone(),
        rejection_codes: codes,
        detail,
    }
}

// ============================================================================
// #1768: Root Requirement Conservation, Supersession & Freshness-Loss Checks
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RootRequirement {
    pub req_id: String,
    pub source_revision: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RootSupersessionRecord {
    pub superseded_req_id: String,
    pub replacement_req_id: Option<String>,
    pub authorized_by: String,
    pub authority_ref: String,
    pub reason: String,
    pub preserved_original_description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MappedRootObligation {
    pub req_id: String,
    pub owner_task_id: String,
    pub lease_id: String,
    pub agent_id: String,
    pub mapped_head_commit: String,
    pub mapped_base_commit: String,
    pub proof_executed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveCustodySnapshot {
    pub task_leases: BTreeMap<String, (String, String)>, // task_id -> (lease_id, agent_id)
    pub current_head_commit: String,
    pub current_base_commit: String,
    pub unrelated_backlog_task_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RootConservationReport {
    pub conserved: bool,
    pub all_proven: bool,
    pub uncovered_roots: Vec<String>,
    pub unmapped_or_silently_deleted: Vec<String>,
    pub freshness_lost: Vec<String>,
    pub superseded_history: Vec<RootSupersessionRecord>,
    pub ignored_unrelated_backlog_count: usize,
}

/// Evaluate root requirement conservation across delegated tasks (#1768).
/// Distinguishes mapping vs proof, preserves history on authorized supersession,
/// detects freshness loss on lease/agent-swap/head-move/base-move, and ensures
/// unrelated unattempted backlog never blocks readiness.
pub fn evaluate_root_conservation(
    selected_roots: &[RootRequirement],
    mappings: &[MappedRootObligation],
    supersessions: &[RootSupersessionRecord],
    custody: &ActiveCustodySnapshot,
) -> RootConservationReport {
    let mut uncovered = Vec::new();
    let mut silently_deleted = Vec::new();
    let mut freshness_lost = Vec::new();
    let mut valid_supersessions = Vec::new();
    let mut all_proven = true;

    for root in selected_roots {
        if let Some(sup) = supersessions
            .iter()
            .find(|s| s.superseded_req_id == root.req_id)
        {
            if !sup.authorized_by.trim().is_empty()
                && !sup.authority_ref.trim().is_empty()
                && !sup.preserved_original_description.trim().is_empty()
            {
                valid_supersessions.push(sup.clone());
                continue;
            }
            silently_deleted.push(format!(
                "{}: unauthorized supersession missing authority or preserved history",
                root.req_id
            ));
            uncovered.push(root.req_id.clone());
            all_proven = false;
            continue;
        }

        let Some(mapping) = mappings.iter().find(|m| m.req_id == root.req_id) else {
            uncovered.push(root.req_id.clone());
            silently_deleted.push(format!(
                "{}: selected accepted root requirement omitted from delegated task graph",
                root.req_id
            ));
            all_proven = false;
            continue;
        };

        if mapping.mapped_head_commit != custody.current_head_commit {
            freshness_lost.push(format!(
                "{}: head moved from '{}' to '{}'",
                root.req_id, mapping.mapped_head_commit, custody.current_head_commit
            ));
            all_proven = false;
        }
        if mapping.mapped_base_commit != custody.current_base_commit {
            freshness_lost.push(format!(
                "{}: base moved from '{}' to '{}'",
                root.req_id, mapping.mapped_base_commit, custody.current_base_commit
            ));
            all_proven = false;
        }
        match custody.task_leases.get(&mapping.owner_task_id) {
            Some((active_lease, active_agent)) => {
                if active_lease != &mapping.lease_id {
                    freshness_lost.push(format!(
                        "{}: lease identity changed from '{}' to '{}'",
                        root.req_id, mapping.lease_id, active_lease
                    ));
                    all_proven = false;
                }
                if active_agent != &mapping.agent_id {
                    freshness_lost.push(format!(
                        "{}: agent swapped from '{}' to '{}'",
                        root.req_id, mapping.agent_id, active_agent
                    ));
                    all_proven = false;
                }
            }
            None => {
                freshness_lost.push(format!(
                    "{}: owner task '{}' has no active custody lease",
                    root.req_id, mapping.owner_task_id
                ));
                all_proven = false;
            }
        }
        if !mapping.proof_executed {
            all_proven = false;
        }
    }

    RootConservationReport {
        conserved: uncovered.is_empty() && silently_deleted.is_empty() && freshness_lost.is_empty(),
        all_proven: uncovered.is_empty()
            && silently_deleted.is_empty()
            && freshness_lost.is_empty()
            && all_proven,
        uncovered_roots: uncovered,
        unmapped_or_silently_deleted: silently_deleted,
        freshness_lost,
        superseded_history: valid_supersessions,
        ignored_unrelated_backlog_count: custody.unrelated_backlog_task_ids.len(),
    }
}

// ============================================================================
// #1468: Accepted-Policy Direct Main-Merge Authorization Evaluator
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MainMergeAuthorizationInput {
    pub accepted_base_main_merge: bool,
    pub candidate_main_merge: bool,
    pub validated_candidate_commit: String,
    pub current_candidate_commit: String,
    pub validated_target_commit: String,
    pub current_target_commit: String,
    pub full_validation_passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MainMergeAuthorizationDecision {
    pub permitted: bool,
    pub accepted_policy_opted_in: bool,
    pub target_moved: bool,
    pub proof_fresh: bool,
    pub rejection_code: Option<String>,
    pub detail: String,
}

/// Evaluate whether a validated workbranch may merge directly to main (#1468).
/// Fails closed on default `main_merge = false`, candidate self-authorization,
/// stale proof, or target movement.
pub fn evaluate_main_merge_authorization(
    input: &MainMergeAuthorizationInput,
) -> MainMergeAuthorizationDecision {
    let target_moved = input.validated_target_commit != input.current_target_commit;
    let candidate_moved = input.validated_candidate_commit != input.current_candidate_commit;
    let proof_fresh = input.full_validation_passed && !target_moved && !candidate_moved;

    if !input.accepted_base_main_merge && input.candidate_main_merge {
        return MainMergeAuthorizationDecision {
            permitted: false,
            accepted_policy_opted_in: false,
            target_moved,
            proof_fresh,
            rejection_code: Some("POLICY_SELF_AUTHORIZATION_REJECTED".to_string()),
            detail: "Candidate branch cannot self-authorize governance.main_merge = true beyond accepted base policy.".to_string(),
        };
    }
    if !input.accepted_base_main_merge {
        return MainMergeAuthorizationDecision {
            permitted: false,
            accepted_policy_opted_in: false,
            target_moved,
            proof_fresh,
            rejection_code: Some("MAIN_MERGE_DISABLED_BY_DEFAULT".to_string()),
            detail: "Direct merge to main is disabled (accepted governance.main_merge = false); use normal PR flow.".to_string(),
        };
    }
    if !input.full_validation_passed {
        return MainMergeAuthorizationDecision {
            permitted: false,
            accepted_policy_opted_in: true,
            target_moved,
            proof_fresh: false,
            rejection_code: Some("VALIDATION_PROOF_REQUIRED".to_string()),
            detail: "Full exact-candidate validation proof must pass before direct merge to main."
                .to_string(),
        };
    }
    if target_moved {
        return MainMergeAuthorizationDecision {
            permitted: false,
            accepted_policy_opted_in: true,
            target_moved: true,
            proof_fresh: false,
            rejection_code: Some("TARGET_MOVED_FAIL_CLOSED".to_string()),
            detail: format!(
                "Accepted target moved from '{}' to '{}'; re-validate against the new target before merging.",
                input.validated_target_commit, input.current_target_commit
            ),
        };
    }
    if candidate_moved {
        return MainMergeAuthorizationDecision {
            permitted: false,
            accepted_policy_opted_in: true,
            target_moved: false,
            proof_fresh: false,
            rejection_code: Some("CANDIDATE_MOVED_FAIL_CLOSED".to_string()),
            detail: "Candidate HEAD changed after validation; prior proof is stale.".to_string(),
        };
    }

    MainMergeAuthorizationDecision {
        permitted: true,
        accepted_policy_opted_in: true,
        target_moved: false,
        proof_fresh: true,
        rejection_code: None,
        detail: "Accepted base policy opts in to governance.main_merge = true and exact candidate/target proof is fresh.".to_string(),
    }
}

// ============================================================================
// #1095, #1772, #1774, #1796: Deterministic Offline Telemetry & Benchmark Harnesses
// ============================================================================

/// #1095: Time-series execution telemetry record with human-attention denominator
/// and optional JSONL / OTel-compatible export.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionTelemetryRecord {
    pub run_id: String,
    pub mode: String,      // "cold" | "warm" | "recovery"
    pub condition: String, // "baseline" | "decapod"
    pub active_human_minutes: f64,
    pub model_calls: u64,
    pub estimated_cost_usd: f64,
    pub harmful_context_events: u64,
    pub verification_passed: bool,
    pub live_real_agent_run: bool,
}

impl ExecutionTelemetryRecord {
    pub fn to_jsonl_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn to_otel_span_json(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "decapod.execution.run",
            "attributes": {
                "decapod.run_id": self.run_id,
                "decapod.mode": self.mode,
                "decapod.condition": self.condition,
                "decapod.active_human_minutes": self.active_human_minutes,
                "decapod.model_calls": self.model_calls,
                "decapod.estimated_cost_usd": self.estimated_cost_usd,
                "decapod.harmful_context_events": self.harmful_context_events,
                "decapod.verification_passed": self.verification_passed,
                "decapod.live_real_agent_run": self.live_real_agent_run,
            }
        })
    }
}

/// #1772: Offline deterministic constitution relevance-gate benchmark record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelevanceGateBenchmarkCase {
    pub case_id: String,
    pub repeated_irrelevant_term: String,
    pub repetitions: usize,
    pub baseline_no_file_injected_irrelevant: bool,
    pub static_file_injected_irrelevant: bool,
    pub decapod_gate_rejected_irrelevant: bool,
    pub mandatory_policy_preserved: bool,
    pub requires_live_paired_agent_eval: bool,
}

/// #1774: Offline constitution contribution promotion & real-enforcement check.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConstitutionPromotionCheck {
    pub node_id: String,
    pub declared_binding: String,
    pub claims_root_router: bool,
    pub cited_enforcement_gate: Option<String>,
    pub gate_exists_in_binary: bool,
    pub admitted: bool,
    pub rejection_reason: Option<String>,
    pub maintainer_review_required: bool,
}

pub fn evaluate_constitution_promotion(
    node_id: &str,
    declared_binding: &str,
    claims_root_router: bool,
    cited_enforcement_gate: Option<&str>,
    gate_exists_in_binary: bool,
) -> ConstitutionPromotionCheck {
    if node_id.starts_with("research/") && (declared_binding == "binding" || claims_root_router) {
        return ConstitutionPromotionCheck {
            node_id: node_id.to_string(),
            declared_binding: declared_binding.to_string(),
            claims_root_router,
            cited_enforcement_gate: cited_enforcement_gate.map(str::to_string),
            gate_exists_in_binary,
            admitted: false,
            rejection_reason: Some(
                "RESEARCH_AUTHORITY_ESCALATION_REJECTED: research/* advisory node cannot self-promote to binding or root-router status.".to_string(),
            ),
            maintainer_review_required: true,
        };
    }
    if cited_enforcement_gate.is_some() && !gate_exists_in_binary {
        return ConstitutionPromotionCheck {
            node_id: node_id.to_string(),
            declared_binding: declared_binding.to_string(),
            claims_root_router,
            cited_enforcement_gate: cited_enforcement_gate.map(str::to_string),
            gate_exists_in_binary,
            admitted: false,
            rejection_reason: Some(
                "UNSUPPORTED_ENFORCEMENT_CLAIM: cited enforcement gate is absent from actual binary.".to_string(),
            ),
            maintainer_review_required: true,
        };
    }
    ConstitutionPromotionCheck {
        node_id: node_id.to_string(),
        declared_binding: declared_binding.to_string(),
        claims_root_router,
        cited_enforcement_gate: cited_enforcement_gate.map(str::to_string),
        gate_exists_in_binary,
        admitted: true,
        rejection_reason: None,
        maintainer_review_required: declared_binding == "binding",
    }
}

/// #1796: Offline paired task-first independent review benchmark harness & decision record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskFirstReviewBenchmark {
    pub benchmark_id: String,
    pub falsifier_defects_seeded: usize,
    pub defects_caught_without_review: usize,
    pub defects_caught_with_task_first_review: usize,
    pub false_positive_blocks: usize,
    pub added_human_minutes: f64,
    pub live_paired_eval_pending: bool,
    pub preliminary_recommendation: String,
}

impl TaskFirstReviewBenchmark {
    pub fn evaluate_offline(
        benchmark_id: impl Into<String>,
        falsifier_defects_seeded: usize,
        defects_caught_without_review: usize,
        defects_caught_with_task_first_review: usize,
        false_positive_blocks: usize,
        added_human_minutes: f64,
    ) -> Self {
        let net_gain =
            defects_caught_with_task_first_review.saturating_sub(defects_caught_without_review);
        let preliminary_recommendation = if net_gain > 0 && false_positive_blocks == 0 {
            "candidate_optional_step_pending_live_paired_eval".to_string()
        } else {
            "defer_insufficient_signal_or_false_positives".to_string()
        };
        Self {
            benchmark_id: benchmark_id.into(),
            falsifier_defects_seeded,
            defects_caught_without_review,
            defects_caught_with_task_first_review,
            false_positive_blocks,
            added_human_minutes,
            live_paired_eval_pending: true,
            preliminary_recommendation,
        }
    }
}
