//! Strictly read-only deterministic time-series execution telemetry projection
//! over existing governance, trajectory, trace, and host-observation artifacts (#1095).
//!
//! Boundary invariant: no background daemon, no new database, no default upload,
//! no secret/transcript leakage, and no fabricated zeros for unavailable metrics.
//! Every projection marks `read_only = true` and `authoritative = false`.

use crate::core::error::DecapodError;
use crate::core::governance_document::GOVERNANCE_PATH;
use crate::core::observability_projections::snapshot::{
    OBSERVE_SCHEMA_VERSION, ObservationSnapshot, capture_observation_with,
    load_raw_governance_json_readonly,
};
use crate::core::observability_projections::timeline::{
    ProofApplicability, TimelineEntry, TimelineProjectionQuery, TrustClass,
    project_activity_timeline,
};
use crate::core::trace::redact_string;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub const DEFAULT_TELEMETRY_LIMIT: usize = 256;
pub const HARD_TELEMETRY_LIMIT: usize = 1024;
pub const DEFAULT_BUCKET_SECONDS: u64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryProjectionQuery {
    #[serde(default)]
    pub after_seq: u64,
    #[serde(default = "default_telemetry_limit")]
    pub limit: usize,
    #[serde(default = "default_bucket_seconds")]
    pub bucket_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_repo_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_head_sha: Option<String>,
}

impl Default for TelemetryProjectionQuery {
    fn default() -> Self {
        Self {
            after_seq: 0,
            limit: default_telemetry_limit(),
            bucket_seconds: default_bucket_seconds(),
            task_id: None,
            expected_repo_id: None,
            expected_head_sha: None,
        }
    }
}

fn default_telemetry_limit() -> usize {
    DEFAULT_TELEMETRY_LIMIT
}

fn default_bucket_seconds() -> u64 {
    DEFAULT_BUCKET_SECONDS
}

/// Distinct evidence origin classes (#1095) so deterministic mocks, instrumented
/// real agents, operator reports, and independent verifications are never conflated.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceOriginClass {
    DeterministicMock,
    InstrumentedRealAgent,
    OperatorReported,
    IndependentVerification,
    RecordedKernelFact,
    Unknown,
}

/// Numerator/denominator model for useful verified work per unit of human attention (#1095).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsefulWorkRatio {
    /// Independently verified/proven completion count only (never raw task counters or self-declared pass).
    pub verified_useful_work_numerator: u64,
    /// Reviewed human attention units; explicit `None` when human review provenance is unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_attention_denominator: Option<u64>,
    /// Computed ratio only when denominator is reviewed and non-zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratio_verified_per_human_attention: Option<f64>,
    pub human_interruptions_observed: u64,
    pub approval_decisions_observed: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unnecessary_questions_observed: Option<u64>,
    pub suppressed_escalation_risk_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_human_seconds: Option<f64>,
    pub definition: String,
}

/// Deterministic time-series bucket over observed execution/governance events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryBucket {
    pub bucket_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bucket_start_epoch: Option<u64>,
    pub event_count: usize,
    pub invocations: u64,
    pub policy_denials: u64,
    pub verification_passes: u64,
    pub verification_failures: u64,
    pub recovery_transitions: u64,
    pub cancelled_or_partial_runs: u64,
}

/// Structured integrity/data-quality finding for collector audits (#1095).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryIntegrityFinding {
    pub code: String,
    pub severity: String,
    pub detail: String,
    pub source_ref: String,
}

/// Bounded pagination metadata for telemetry series.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryPagination {
    pub after_seq: u64,
    pub limit: usize,
    pub returned_events: usize,
    pub total_events: usize,
    pub has_more: bool,
}

/// Optional collector input record for deterministic collector audits and local
/// comparison exports (duplicate/partial/cancelled runs, missing fields, join checks).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TelemetryEventSample {
    pub event_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epoch_secs: Option<u64>,
    pub stage: String,
    pub action: String,
    pub outcome: String,
    pub origin_class: EvidenceOriginClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_head_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
    #[serde(default)]
    pub independently_verified: bool,
    #[serde(default)]
    pub human_interruption: bool,
    #[serde(default)]
    pub approval_decision: bool,
    #[serde(default)]
    pub cancelled_or_partial: bool,
}

/// Top-level strictly read-only execution telemetry projection (#1095).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionTelemetryProjection {
    pub schema_version: String,
    pub telemetry_schema_version: String,
    pub read_only: bool,
    pub authoritative: bool,
    pub snapshot: ObservationSnapshot,
    pub bucket_seconds: u64,
    pub evidence_classes_present: Vec<EvidenceOriginClass>,
    pub useful_work: UsefulWorkRatio,
    pub total_invocations: u64,
    pub policy_denials: u64,
    pub verification_passes: u64,
    pub verification_failures: u64,
    pub recovery_transitions: u64,
    pub cancelled_or_partial_runs: u64,
    pub retained_governance_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_tokens_observed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_ms_observed: Option<u64>,
    pub missing_measurements: Vec<String>,
    pub series: Vec<TelemetryBucket>,
    pub integrity_findings: Vec<TelemetryIntegrityFinding>,
    pub attribution_limits: Vec<String>,
    pub pagination: TelemetryPagination,
}

fn parse_epoch_secs(ts: &str) -> Option<u64> {
    let trimmed = ts.trim();
    if trimmed.is_empty() {
        return None;
    }
    let stripped = trimmed.strip_suffix('Z').unwrap_or(trimmed);
    if let Ok(v) = stripped.parse::<u64>() {
        return Some(v);
    }
    // Parse ISO-8601 UTC YYYY-MM-DDTHH:MM:SS[Z] without external date crates.
    let (date_part, time_part) = stripped.split_once('T')?;
    let mut d = date_part.split('-');
    let year: i64 = d.next()?.parse().ok()?;
    let month: u32 = d.next()?.parse().ok()?;
    let day: u32 = d.next()?.parse().ok()?;
    let time_core = time_part.split('.').next()?.split('+').next()?;
    let mut t = time_core.split(':');
    let hour: u64 = t.next()?.parse().ok()?;
    let min: u64 = t.next()?.parse().ok()?;
    let sec: u64 = t.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || min > 59 || sec > 59 {
        return None;
    }
    // Howard Hinnant's civil-from-days algorithm (days since 1970-01-01).
    let y = year - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = (y - era * 400) as u64;
    let m = u64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + u64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + (doe as i64) - 719_468;
    if days < 0 {
        return None;
    }
    Some((days as u64) * 86_400 + hour * 3_600 + min * 60 + sec)
}

fn classify_timeline_origin(entry: &TimelineEntry) -> EvidenceOriginClass {
    match entry.trust_class {
        TrustClass::Witnessed => EvidenceOriginClass::IndependentVerification,
        TrustClass::RecordedFact => EvidenceOriginClass::RecordedKernelFact,
        TrustClass::Declared => EvidenceOriginClass::InstrumentedRealAgent,
        TrustClass::Inferred | TrustClass::Unknown => EvidenceOriginClass::Unknown,
    }
}

fn sample_from_timeline_entry(
    entry: &TimelineEntry,
    expected_head_sha: Option<&str>,
) -> TelemetryEventSample {
    let stage_lc = entry.stage.to_ascii_lowercase();
    let transition_lc = entry.observed_transition.to_ascii_lowercase();
    let action_lc = entry.claimed_action.to_ascii_lowercase();
    let outcome = match entry.proof_applicability {
        ProofApplicability::Proven => "passed",
        ProofApplicability::Failed => "failed",
        ProofApplicability::Stale => "stale",
        ProofApplicability::NotProven => "not_proven",
        ProofApplicability::Unknown => {
            if transition_lc.contains("cancel")
                || transition_lc.contains("timeout")
                || transition_lc.contains("partial")
            {
                "cancelled_or_partial"
            } else if transition_lc.contains("deny")
                || transition_lc.contains("refus")
                || transition_lc.contains("block")
            {
                "denied"
            } else {
                "recorded"
            }
        }
    };

    let independently_verified = entry.proof_applicability == ProofApplicability::Proven
        && matches!(
            entry.trust_class,
            TrustClass::Witnessed | TrustClass::RecordedFact
        )
        && entry.corroborated;

    TelemetryEventSample {
        event_id: entry.event_id.clone(),
        epoch_secs: parse_epoch_secs(&entry.timestamp),
        stage: redact_string(&entry.stage),
        action: redact_string(&entry.claimed_action),
        outcome: outcome.to_string(),
        origin_class: classify_timeline_origin(entry),
        task_id: entry.task_id.clone(),
        source_head_sha: expected_head_sha.map(str::to_string),
        context_tokens: None,
        elapsed_ms: None,
        independently_verified,
        human_interruption: stage_lc.contains("escalat") || action_lc.contains("question"),
        approval_decision: stage_lc.contains("approv") || transition_lc.contains("approv"),
        cancelled_or_partial: outcome == "cancelled_or_partial",
    }
}

fn measure_retained_governance_bytes(repo_root: &Path) -> u64 {
    let mut total: u64 = 0;
    let gov = repo_root.join(GOVERNANCE_PATH);
    if let Ok(meta) = fs::metadata(&gov) {
        total = total.saturating_add(meta.len());
    }
    let specs_dir = repo_root.join(".decapod/managed/specs");
    if let Ok(entries) = fs::read_dir(&specs_dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata()
                && meta.is_file()
            {
                total = total.saturating_add(meta.len());
            }
        }
    }
    total
}

fn inspect_governance_for_samples(
    gov_json: Option<&Value>,
    head_sha: Option<&str>,
) -> Vec<TelemetryEventSample> {
    let mut extra = Vec::new();
    let Some(doc) = gov_json else {
        return extra;
    };
    if let Some(loops) = doc
        .pointer("/sections/trajectory/loops")
        .and_then(Value::as_array)
    {
        for (idx, item) in loops.iter().enumerate() {
            let status = item
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let loop_id = item
                .get("loop_id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("trajectory-loop-{idx}"));
            let loop_type = item
                .get("loop_type")
                .and_then(Value::as_str)
                .unwrap_or("verification");
            let proof_refs_nonempty = item
                .get("proof_refs")
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty());
            let status_lc = status.to_ascii_lowercase();
            let independently_verified =
                (status_lc == "passed" || status_lc == "verified") && proof_refs_nonempty;
            let cancelled_or_partial = matches!(
                status_lc.as_str(),
                "partial" | "cancelled" | "canceled" | "unavailable" | "aborted"
            );
            extra.push(TelemetryEventSample {
                event_id: format!("gov-loop:{loop_id}"),
                epoch_secs: item
                    .get("recorded_at")
                    .and_then(Value::as_str)
                    .and_then(parse_epoch_secs),
                stage: redact_string(loop_type),
                action: redact_string(
                    item.get("trigger")
                        .and_then(Value::as_str)
                        .unwrap_or("trajectory_loop"),
                ),
                outcome: status_lc,
                origin_class: if independently_verified {
                    EvidenceOriginClass::IndependentVerification
                } else {
                    EvidenceOriginClass::RecordedKernelFact
                },
                task_id: doc
                    .pointer("/sections/plan/todo_ids/0")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                source_head_sha: head_sha.map(str::to_string),
                context_tokens: item.get("context_tokens").and_then(Value::as_u64),
                elapsed_ms: item.get("elapsed_ms").and_then(Value::as_u64),
                independently_verified,
                human_interruption: loop_type.eq_ignore_ascii_case("human_escalation"),
                approval_decision: loop_type.eq_ignore_ascii_case("approval"),
                cancelled_or_partial,
            });
        }
    }
    extra
}

/// Pure deterministic telemetry aggregator over bounded event samples (#1095).
/// Deduplicates duplicate event IDs, retains partial/cancelled runs, flags wrong
/// task/source joins, redacts secret substrings, and preserves missing metrics as `None`.
pub fn aggregate_telemetry_samples(
    samples: &[TelemetryEventSample],
    snapshot: ObservationSnapshot,
    query: &TelemetryProjectionQuery,
    retained_governance_bytes: u64,
    reviewed_human_attention_denominator: Option<u64>,
    reviewed_unnecessary_questions: Option<u64>,
    observed_active_human_seconds: Option<f64>,
) -> ExecutionTelemetryProjection {
    let bucket_seconds = query.bucket_seconds.max(1);
    let mut seen_event_ids: BTreeSet<String> = BTreeSet::new();
    let mut integrity_findings: Vec<TelemetryIntegrityFinding> = Vec::new();
    let mut origin_classes: BTreeSet<EvidenceOriginClass> = BTreeSet::new();
    let mut buckets: BTreeMap<String, TelemetryBucket> = BTreeMap::new();

    let mut total_invocations: u64 = 0;
    let mut policy_denials: u64 = 0;
    let mut verification_passes: u64 = 0;
    let mut verification_failures: u64 = 0;
    let mut recovery_transitions: u64 = 0;
    let mut cancelled_or_partial_runs: u64 = 0;
    let mut verified_useful_work_numerator: u64 = 0;
    let mut human_interruptions_observed: u64 = 0;
    let mut approval_decisions_observed: u64 = 0;
    let mut suppressed_escalation_risk_count: u64 = 0;

    let mut token_sum: u64 = 0;
    let mut any_tokens_observed = false;
    let mut elapsed_sum: u64 = 0;
    let mut any_elapsed_observed = false;
    let mut prev_failed = false;

    for sample in samples {
        let safe_event_id = redact_string(&sample.event_id);
        let safe_action = redact_string(&sample.action);
        let safe_stage = redact_string(&sample.stage);
        if safe_action != sample.action
            || safe_stage != sample.stage
            || safe_event_id != sample.event_id
        {
            integrity_findings.push(TelemetryIntegrityFinding {
                code: "secret_redacted".to_string(),
                severity: "warning".to_string(),
                detail: "Secret-like substring redacted from telemetry sample before projection."
                    .to_string(),
                source_ref: safe_event_id.clone(),
            });
        }

        if !seen_event_ids.insert(safe_event_id.clone()) {
            integrity_findings.push(TelemetryIntegrityFinding {
                code: "duplicate_event_id".to_string(),
                severity: "warning".to_string(),
                detail: format!(
                    "Duplicate event_id '{safe_event_id}' ignored to prevent inflated telemetry counts."
                ),
                source_ref: safe_event_id,
            });
            continue;
        }

        if let Some(ref filter_task) = query.task_id {
            match sample.task_id.as_deref() {
                Some(t) if t != filter_task => {
                    integrity_findings.push(TelemetryIntegrityFinding {
                        code: "mismatched_task_join".to_string(),
                        severity: "warning".to_string(),
                        detail: format!(
                            "Excluded telemetry sample belonging to task '{t}' (requested '{filter_task}')."
                        ),
                        source_ref: safe_event_id,
                    });
                    continue;
                }
                _ => {}
            }
        }

        if let Some(ref expected_head) = query.expected_head_sha
            && let Some(ref sample_head) = sample.source_head_sha
            && sample_head != expected_head
        {
            integrity_findings.push(TelemetryIntegrityFinding {
                code: "mismatched_source_join".to_string(),
                severity: "warning".to_string(),
                detail: format!(
                    "Sample bound to source head '{sample_head}' does not match expected head '{expected_head}'; excluded from current verified numerator."
                ),
                source_ref: safe_event_id.clone(),
            });
        }

        origin_classes.insert(sample.origin_class);
        total_invocations += 1;

        let outcome_lc = sample.outcome.to_ascii_lowercase();
        let is_denial = outcome_lc.contains("denied")
            || outcome_lc.contains("refused")
            || outcome_lc.contains("blocked");
        let is_fail = outcome_lc == "failed" || outcome_lc == "stale" || outcome_lc == "error";
        let is_pass = outcome_lc == "passed" || outcome_lc == "verified";
        let is_cancelled_partial = sample.cancelled_or_partial
            || outcome_lc == "cancelled_or_partial"
            || outcome_lc == "partial"
            || outcome_lc == "cancelled"
            || outcome_lc == "canceled"
            || outcome_lc == "timeout";

        let head_matches = query
            .expected_head_sha
            .as_deref()
            .zip(sample.source_head_sha.as_deref())
            .is_none_or(|(exp, got)| exp == got);

        if is_denial {
            policy_denials += 1;
        }
        if is_fail {
            verification_failures += 1;
            prev_failed = true;
        }
        if is_pass {
            verification_passes += 1;
            if prev_failed {
                recovery_transitions += 1;
                prev_failed = false;
            }
            if sample.independently_verified && head_matches {
                verified_useful_work_numerator += 1;
            } else if !sample.independently_verified {
                integrity_findings.push(TelemetryIntegrityFinding {
                    code: "uncorroborated_declared_claim".to_string(),
                    severity: "info".to_string(),
                    detail: "Declared pass without independent verification excluded from verified_useful_work_numerator.".to_string(),
                    source_ref: safe_event_id.clone(),
                });
            }
        }
        if is_cancelled_partial {
            cancelled_or_partial_runs += 1;
            integrity_findings.push(TelemetryIntegrityFinding {
                code: "partial_or_cancelled_run".to_string(),
                severity: "info".to_string(),
                detail: "Partial or cancelled execution retained in telemetry denominator."
                    .to_string(),
                source_ref: safe_event_id.clone(),
            });
        }
        if sample.human_interruption {
            human_interruptions_observed += 1;
        }
        if sample.approval_decision {
            approval_decisions_observed += 1;
        }
        if safe_stage.to_ascii_lowercase().contains("bypass")
            || safe_action
                .to_ascii_lowercase()
                .contains("suppressed_escalation")
        {
            suppressed_escalation_risk_count += 1;
        }
        if let Some(toks) = sample.context_tokens {
            token_sum = token_sum.saturating_add(toks);
            any_tokens_observed = true;
        }
        if let Some(ms) = sample.elapsed_ms {
            elapsed_sum = elapsed_sum.saturating_add(ms);
            any_elapsed_observed = true;
        }

        let (bucket_key, bucket_start_epoch) = match sample.epoch_secs {
            Some(ts) => {
                let start = (ts / bucket_seconds) * bucket_seconds;
                (format!("epoch:{start}"), Some(start))
            }
            None => ("unbucketed:no_timestamp".to_string(), None),
        };
        let entry = buckets
            .entry(bucket_key.clone())
            .or_insert_with(|| TelemetryBucket {
                bucket_key,
                bucket_start_epoch,
                event_count: 0,
                invocations: 0,
                policy_denials: 0,
                verification_passes: 0,
                verification_failures: 0,
                recovery_transitions: 0,
                cancelled_or_partial_runs: 0,
            });
        entry.event_count += 1;
        entry.invocations += 1;
        if is_denial {
            entry.policy_denials += 1;
        }
        if is_pass {
            entry.verification_passes += 1;
        }
        if is_fail {
            entry.verification_failures += 1;
        }
        if is_cancelled_partial {
            entry.cancelled_or_partial_runs += 1;
        }
    }

    let context_tokens_observed = any_tokens_observed.then_some(token_sum);
    let elapsed_ms_observed = any_elapsed_observed.then_some(elapsed_sum);

    let mut missing_measurements = Vec::new();
    if reviewed_human_attention_denominator.is_none() {
        missing_measurements.push("human_attention_denominator".to_string());
    }
    if reviewed_unnecessary_questions.is_none() {
        missing_measurements.push("unnecessary_questions_observed".to_string());
    }
    if observed_active_human_seconds.is_none() {
        missing_measurements.push("active_human_seconds".to_string());
    }
    if context_tokens_observed.is_none() {
        missing_measurements.push("context_tokens_observed".to_string());
    }
    if elapsed_ms_observed.is_none() {
        missing_measurements.push("elapsed_ms_observed".to_string());
    }

    let ratio_verified_per_human_attention =
        reviewed_human_attention_denominator.and_then(|denom| {
            if denom == 0 {
                None
            } else {
                Some(verified_useful_work_numerator as f64 / denom as f64)
            }
        });

    let returned_events = seen_event_ids.len();
    let total_events = samples.len();

    ExecutionTelemetryProjection {
        schema_version: OBSERVE_SCHEMA_VERSION.to_string(),
        telemetry_schema_version: "decapod.execution-telemetry.v1".to_string(),
        read_only: true,
        authoritative: false,
        snapshot,
        bucket_seconds,
        evidence_classes_present: origin_classes.into_iter().collect(),
        useful_work: UsefulWorkRatio {
            verified_useful_work_numerator,
            human_attention_denominator: reviewed_human_attention_denominator,
            ratio_verified_per_human_attention,
            human_interruptions_observed,
            approval_decisions_observed,
            unnecessary_questions_observed: reviewed_unnecessary_questions,
            suppressed_escalation_risk_count,
            active_human_seconds: observed_active_human_seconds,
            definition: "verified_useful_work_numerator counts only independently corroborated passing outcomes bound to the current candidate; unreviewed human attention remains null rather than fabricated zero.".to_string(),
        },
        total_invocations,
        policy_denials,
        verification_passes,
        verification_failures,
        recovery_transitions,
        cancelled_or_partial_runs,
        retained_governance_bytes,
        context_tokens_observed,
        elapsed_ms_observed,
        missing_measurements,
        series: buckets.into_values().collect(),
        integrity_findings,
        attribution_limits: vec![
            "Read-only local projection over recorded kernel, trajectory, and trace artifacts; never grants validation or publication authority.".to_string(),
            "Unavailable token, elapsed-time, and reviewed human-attention metrics are preserved as explicit nulls.".to_string(),
            "Deterministic test fixtures and real-host runs are tagged separately via evidence_classes_present and cannot be extrapolated across classes.".to_string(),
        ],
        pagination: TelemetryPagination {
            after_seq: query.after_seq,
            limit: query.limit.clamp(1, HARD_TELEMETRY_LIMIT),
            returned_events,
            total_events,
            has_more: total_events > query.limit.clamp(1, HARD_TELEMETRY_LIMIT),
        },
    }
}

/// Project deterministic read-only execution telemetry for `repo_root` (#1095).
pub fn project_execution_telemetry(
    repo_root: &Path,
    query: &TelemetryProjectionQuery,
) -> Result<ExecutionTelemetryProjection, DecapodError> {
    let clamped_limit = query.limit.clamp(1, HARD_TELEMETRY_LIMIT);
    let (snapshot, (timeline_proj, gov_json, retained_bytes)) =
        capture_observation_with(repo_root, |_, _| {
            let timeline_q = TimelineProjectionQuery {
                after_seq: query.after_seq,
                limit: clamped_limit,
                stream: None,
            };
            let timeline = project_activity_timeline(repo_root, &timeline_q)?;
            let gov = load_raw_governance_json_readonly(repo_root);
            let bytes = measure_retained_governance_bytes(repo_root);
            Ok((timeline, gov, bytes))
        })?;

    if let Some(ref expected_repo) = query.expected_repo_id
        && expected_repo != &snapshot.repository.repo_id
    {
        return Err(DecapodError::ValidationError(format!(
            "CROSS_SCOPE_DENIED: telemetry query expected repo_id '{expected_repo}' but repository is '{}'",
            snapshot.repository.repo_id
        )));
    }

    let head_sha = snapshot.worktree.head_commit.as_deref();
    let mut samples: Vec<TelemetryEventSample> = timeline_proj
        .entries
        .iter()
        .map(|entry| sample_from_timeline_entry(entry, head_sha))
        .collect();
    samples.extend(inspect_governance_for_samples(gov_json.as_ref(), head_sha));

    Ok(aggregate_telemetry_samples(
        &samples,
        snapshot,
        query,
        retained_bytes,
        None,
        None,
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::observability_projections::snapshot::capture_observation_with;
    use tempfile::tempdir;

    fn sample_snapshot(dir: &Path) -> ObservationSnapshot {
        capture_observation_with(dir, |_, _| Ok(())).unwrap().0
    }

    #[test]
    fn telemetry_dedupes_duplicates_retains_cancelled_redacts_secrets_and_keeps_missing_null() {
        let dir = tempdir().unwrap();
        let snap = sample_snapshot(dir.path());
        let samples = vec![
            TelemetryEventSample {
                event_id: "evt-1".to_string(),
                epoch_secs: Some(1_700_000_010),
                stage: "validation".to_string(),
                action: "run check with Bearer ghp_1234567890abcdefghijklmnop".to_string(),
                outcome: "failed".to_string(),
                origin_class: EvidenceOriginClass::IndependentVerification,
                task_id: Some("code_1".to_string()),
                source_head_sha: Some("aaa".to_string()),
                context_tokens: None,
                elapsed_ms: None,
                independently_verified: false,
                human_interruption: false,
                approval_decision: false,
                cancelled_or_partial: false,
            },
            // Duplicate event_id must be ignored and flagged.
            TelemetryEventSample {
                event_id: "evt-1".to_string(),
                epoch_secs: Some(1_700_000_010),
                stage: "validation".to_string(),
                action: "duplicate".to_string(),
                outcome: "passed".to_string(),
                origin_class: EvidenceOriginClass::IndependentVerification,
                task_id: Some("code_1".to_string()),
                source_head_sha: Some("aaa".to_string()),
                context_tokens: Some(999),
                elapsed_ms: Some(100),
                independently_verified: true,
                human_interruption: false,
                approval_decision: false,
                cancelled_or_partial: false,
            },
            // Recovery pass with independent verification.
            TelemetryEventSample {
                event_id: "evt-2".to_string(),
                epoch_secs: Some(1_700_000_040),
                stage: "validation".to_string(),
                action: "rerun check".to_string(),
                outcome: "passed".to_string(),
                origin_class: EvidenceOriginClass::IndependentVerification,
                task_id: Some("code_1".to_string()),
                source_head_sha: Some("aaa".to_string()),
                context_tokens: None,
                elapsed_ms: None,
                independently_verified: true,
                human_interruption: false,
                approval_decision: true,
                cancelled_or_partial: false,
            },
            // Self-declared pass without independent verification must NOT increment numerator.
            TelemetryEventSample {
                event_id: "evt-3".to_string(),
                epoch_secs: Some(1_700_000_080),
                stage: "completion".to_string(),
                action: "agent claims done".to_string(),
                outcome: "passed".to_string(),
                origin_class: EvidenceOriginClass::InstrumentedRealAgent,
                task_id: Some("code_1".to_string()),
                source_head_sha: Some("aaa".to_string()),
                context_tokens: None,
                elapsed_ms: None,
                independently_verified: false,
                human_interruption: false,
                approval_decision: false,
                cancelled_or_partial: false,
            },
            // Cancelled run retained in denominator.
            TelemetryEventSample {
                event_id: "evt-4".to_string(),
                epoch_secs: Some(1_700_000_090),
                stage: "execution".to_string(),
                action: "timeout".to_string(),
                outcome: "cancelled_or_partial".to_string(),
                origin_class: EvidenceOriginClass::InstrumentedRealAgent,
                task_id: Some("code_1".to_string()),
                source_head_sha: Some("aaa".to_string()),
                context_tokens: None,
                elapsed_ms: None,
                independently_verified: false,
                human_interruption: true,
                approval_decision: false,
                cancelled_or_partial: true,
            },
            // Wrong task join excluded and flagged.
            TelemetryEventSample {
                event_id: "evt-5".to_string(),
                epoch_secs: Some(1_700_000_095),
                stage: "validation".to_string(),
                action: "other task".to_string(),
                outcome: "passed".to_string(),
                origin_class: EvidenceOriginClass::DeterministicMock,
                task_id: Some("code_other".to_string()),
                source_head_sha: Some("aaa".to_string()),
                context_tokens: Some(500),
                elapsed_ms: Some(50),
                independently_verified: true,
                human_interruption: false,
                approval_decision: false,
                cancelled_or_partial: false,
            },
            // Wrong source head join excluded from verified numerator and flagged.
            TelemetryEventSample {
                event_id: "evt-6".to_string(),
                epoch_secs: Some(1_700_000_099),
                stage: "validation".to_string(),
                action: "stale commit pass".to_string(),
                outcome: "passed".to_string(),
                origin_class: EvidenceOriginClass::IndependentVerification,
                task_id: Some("code_1".to_string()),
                source_head_sha: Some("bbb_stale".to_string()),
                context_tokens: None,
                elapsed_ms: None,
                independently_verified: true,
                human_interruption: false,
                approval_decision: false,
                cancelled_or_partial: false,
            },
        ];
        let query = TelemetryProjectionQuery {
            task_id: Some("code_1".to_string()),
            expected_head_sha: Some("aaa".to_string()),
            bucket_seconds: 60,
            ..Default::default()
        };
        let proj = aggregate_telemetry_samples(&samples, snap, &query, 1024, None, None, None);
        assert!(proj.read_only);
        assert!(!proj.authoritative);
        assert_eq!(proj.useful_work.verified_useful_work_numerator, 1);
        assert_eq!(proj.useful_work.human_attention_denominator, None);
        assert_eq!(proj.useful_work.ratio_verified_per_human_attention, None);
        assert_eq!(proj.useful_work.approval_decisions_observed, 1);
        assert_eq!(proj.useful_work.human_interruptions_observed, 1);
        assert_eq!(proj.recovery_transitions, 1);
        assert_eq!(proj.cancelled_or_partial_runs, 1);
        assert_eq!(proj.context_tokens_observed, None);
        assert_eq!(proj.elapsed_ms_observed, None);
        assert!(
            proj.missing_measurements
                .contains(&"context_tokens_observed".to_string())
        );
        assert!(
            proj.missing_measurements
                .contains(&"human_attention_denominator".to_string())
        );
        assert_eq!(proj.series.len(), 2);

        let codes: BTreeSet<&str> = proj
            .integrity_findings
            .iter()
            .map(|f| f.code.as_str())
            .collect();
        assert!(codes.contains("secret_redacted"));
        assert!(codes.contains("duplicate_event_id"));
        assert!(codes.contains("mismatched_task_join"));
        assert!(codes.contains("mismatched_source_join"));
        assert!(codes.contains("uncorroborated_declared_claim"));
        assert!(codes.contains("partial_or_cancelled_run"));
        let serialized = serde_json::to_string(&proj).unwrap();
        assert!(!serialized.contains("ghp_1234567890abcdefghijklmnop"));
    }
}
