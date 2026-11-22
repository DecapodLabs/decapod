//! Structured read-only agent activity, custody, witnessed-vs-declared path,
//! and proof timelines projection (#1713, #860).

use crate::core::error::DecapodError;
use crate::core::governance_document::{self, GOVERNANCE_PATH};
use crate::core::observability_projections::snapshot::{
    OBSERVE_SCHEMA_VERSION, ObservationSnapshot, capture_observation_with,
    load_raw_governance_json_readonly, read_tasks_readonly, run_git_stdout_trimmed,
};
use crate::core::observability_projections::specs::inspect_validation_receipt_value;
use crate::core::projection_budget::{
    EventProjectionQuery, ProjectionBudget, project_events_read_only,
};
use crate::core::trace;
use crate::core::trajectory::{self, TrajectoryArtifact, TrajectoryLoopStatus};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineProjectionQuery {
    #[serde(default)]
    pub after_seq: u64,
    #[serde(default = "default_timeline_limit")]
    pub limit: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
}

impl Default for TimelineProjectionQuery {
    fn default() -> Self {
        Self {
            after_seq: 0,
            limit: default_timeline_limit(),
            stream: None,
        }
    }
}

fn default_timeline_limit() -> usize {
    256
}

/// Explicit provenance / trust class for every timeline event (#1713).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustClass {
    /// Independently observed by the kernel-mediated runner (`observed-path.json`).
    Witnessed,
    /// Self-reported by an agent in trajectory/plan/claim fields; uncorroborated.
    Declared,
    /// Immutable repository/SQLite record (e.g. Git commit, SQLite event row).
    RecordedFact,
    /// Derived heuristic; visibly distinct from authoritative state.
    Inferred,
    /// Missing or unverifiable provenance.
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProofApplicability {
    Proven,
    NotProven,
    Failed,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelineEntry {
    pub event_id: String,
    pub sequence: u64,
    pub timestamp: String,
    pub stage: String,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_generation: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_revision: Option<String>,
    pub trust_class: TrustClass,
    pub corroborated: bool,
    pub claimed_action: String,
    pub observed_transition: String,
    pub proof_applicability: ProofApplicability,
    pub source_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelineIntegrityFinding {
    pub code: String,
    pub severity: String,
    pub detail: String,
    pub source_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectiveEvidenceEdge {
    pub intent_id: String,
    pub task_id: Option<String>,
    pub claim_id: Option<String>,
    pub evidence_ref: Option<String>,
    pub proof_applicability: ProofApplicability,
    pub broken: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broken_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelineCursor {
    pub stream: String,
    pub after_seq: u64,
    pub next_seq: u64,
    pub retention_floor_seq: u64,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivityTimelineProjection {
    pub schema_version: String,
    pub read_only: bool,
    pub authoritative: bool,
    pub snapshot: ObservationSnapshot,
    pub history_intact: bool,
    pub witnessed_path_present: bool,
    pub declared_only_count: usize,
    pub witnessed_count: usize,
    pub integrity_findings: Vec<TimelineIntegrityFinding>,
    pub entries: Vec<TimelineEntry>,
    pub objective_evidence_graph: Vec<ObjectiveEvidenceEdge>,
    pub cursor: TimelineCursor,
}

fn load_observed_path_judgment(repo_root: &Path) -> Option<trajectory::path::Judgment> {
    let candidates = [
        repo_root.join(".decapod/managed/artifacts/validation/observed-path.json"),
        repo_root.join(".decapod/data/validation/observed-path.json"),
        repo_root.join(".decapod/observed-path.json"),
    ];
    for candidate in candidates {
        if candidate.exists()
            && let Ok(bytes) = fs::read(&candidate)
            && let Ok(judgment) = serde_json::from_slice::<trajectory::path::Judgment>(&bytes)
        {
            return Some(judgment);
        }
    }
    // Also check `.decapod/governance.json#/sections/observed_path` when embedded.
    if let Ok(Some(doc)) = governance_document::load(repo_root)
        && let Some(val) = doc.sections.get("observed_path")
        && let Ok(judgment) = serde_json::from_value::<trajectory::path::Judgment>(val.clone())
    {
        return Some(judgment);
    }
    None
}

fn verify_trajectory_integrity(
    traj: &TrajectoryArtifact,
    findings: &mut Vec<TimelineIntegrityFinding>,
) {
    // 1. Recompute trajectory artifact hash if present to catch silent edits/tampering.
    if !traj.artifact_hash.is_empty()
        && let Ok(expected) = traj.computed_hash_hex()
        && expected != traj.artifact_hash
    {
        findings.push(TimelineIntegrityFinding {
            code: "TRAJECTORY_HASH_MISMATCH".to_string(),
            severity: "error".to_string(),
            detail: format!(
                "recorded trajectory artifact_hash '{}' does not match recomputed '{}'; declared history may have been altered",
                traj.artifact_hash, expected
            ),
            source_ref: format!("{GOVERNANCE_PATH}#/sections/trajectory"),
        });
    }

    // 2. Check contiguous retry attempts per loop_id so removing/replacing an
    // earlier failed attempt is deterministically exposed (#1713 negative test!).
    let mut attempts_by_loop: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
    for l in &traj.loops {
        attempts_by_loop
            .entry(l.loop_id.as_str())
            .or_default()
            .push(l.attempt);
    }
    for (loop_id, mut attempts) in attempts_by_loop {
        attempts.sort_unstable();
        if attempts.first().copied() != Some(1) {
            findings.push(TimelineIntegrityFinding {
                code: "MISSING_INITIAL_LOOP_ATTEMPT".to_string(),
                severity: "error".to_string(),
                detail: format!(
                    "loop '{loop_id}' starts at attempt {:?} instead of 1; earlier attempt(s) removed or missing",
                    attempts.first()
                ),
                source_ref: format!("{GOVERNANCE_PATH}#/sections/trajectory/loops/{loop_id}"),
            });
        }
        for pair in attempts.windows(2) {
            if pair[1] == pair[0] {
                findings.push(TimelineIntegrityFinding {
                    code: "DUPLICATE_LOOP_ATTEMPT".to_string(),
                    severity: "error".to_string(),
                    detail: format!(
                        "loop '{loop_id}' contains duplicate attempt {}; possible replaced history",
                        pair[0]
                    ),
                    source_ref: format!("{GOVERNANCE_PATH}#/sections/trajectory/loops/{loop_id}"),
                });
            } else if pair[1] != pair[0] + 1 {
                findings.push(TimelineIntegrityFinding {
                    code: "NON_CONTIGUOUS_LOOP_ATTEMPTS".to_string(),
                    severity: "error".to_string(),
                    detail: format!(
                        "loop '{loop_id}' has gap between attempt {} and {}; earlier failed attempt may have been omitted",
                        pair[0], pair[1]
                    ),
                    source_ref: format!("{GOVERNANCE_PATH}#/sections/trajectory/loops/{loop_id}"),
                });
            }
        }
    }

    // 3. Check custody_event_id links exist in recorded custody events (forged linked event detection!).
    let known_custody_ids: BTreeSet<String> = traj
        .custody
        .events
        .iter()
        .map(|e| e.event_id.clone())
        .collect();
    for l in &traj.loops {
        if let Some(ref cid) = l.custody_event_id
            && !known_custody_ids.contains(cid)
        {
            findings.push(TimelineIntegrityFinding {
                code: "FORGED_OR_DANGLING_CUSTODY_LINK".to_string(),
                severity: "error".to_string(),
                detail: format!(
                    "loop '{}' attempt {} references custody_event_id '{}' which is absent from canonical custody ledger",
                    l.loop_id, l.attempt, cid
                ),
                source_ref: format!("{GOVERNANCE_PATH}#/sections/trajectory/loops/{}", l.loop_id),
            });
        }
    }
}

fn verify_observed_path_chain(
    judgment: &trajectory::path::Judgment,
    findings: &mut Vec<TimelineIntegrityFinding>,
) {
    let mut expected_prev: Option<String> = None;
    let mut expected_seq: u64 = 1;
    for ev in &judgment.events {
        if ev.sequence != expected_seq {
            findings.push(TimelineIntegrityFinding {
                code: "WITNESSED_PATH_SEQUENCE_GAP".to_string(),
                severity: "error".to_string(),
                detail: format!(
                    "witnessed event '{}' has sequence {} (expected {})",
                    ev.event_id, ev.sequence, expected_seq
                ),
                source_ref: "observed-path.json".to_string(),
            });
        }
        if ev.previous != expected_prev {
            findings.push(TimelineIntegrityFinding {
                code: "WITNESSED_PATH_BROKEN_CHAIN".to_string(),
                severity: "error".to_string(),
                detail: format!(
                    "witnessed event '{}' links to {:?} instead of expected previous {:?}",
                    ev.event_id, ev.previous, expected_prev
                ),
                source_ref: "observed-path.json".to_string(),
            });
        }
        expected_prev = Some(ev.event_id.clone());
        expected_seq = ev.sequence.saturating_add(1);
    }
}

/// Reconstruct a deterministic, read-only multi-agent activity, custody, and
/// proof timeline from a single consistent snapshot (#1713).
pub fn project_activity_timeline(
    repo_root: &Path,
    query: &TimelineProjectionQuery,
) -> Result<ActivityTimelineProjection, DecapodError> {
    let limit = query.limit.clamp(1, 1024);
    let stream = query.stream.clone().unwrap_or_else(|| "todo".to_string());

    let (snapshot, inner) = capture_observation_with(repo_root, |root, data_root| {
        let doc = governance_document::load(root).ok().flatten();
        let raw_gov = load_raw_governance_json_readonly(root);
        let traj = trajectory::load_trajectory_cookie(root)
            .ok()
            .flatten()
            .or_else(|| {
                raw_gov
                    .as_ref()
                    .and_then(|v| v.get("sections"))
                    .and_then(|s| s.get("trajectory"))
                    .and_then(|t| serde_json::from_value::<TrajectoryArtifact>(t.clone()).ok())
            });
        let observed_path = load_observed_path_judgment(root);
        let tasks = read_tasks_readonly(data_root, 256);

        let mut ev_query = EventProjectionQuery::for_stream(&stream);
        ev_query.after_seq = query.after_seq;
        ev_query.budget = ProjectionBudget {
            max_rows: limit.min(256),
            ..ProjectionBudget::default()
        };
        let events_page = project_events_read_only(data_root, &ev_query).ok();

        Ok((doc, raw_gov, traj, observed_path, tasks, events_page))
    })?;

    let (doc, raw_gov, traj, observed_path, tasks, events_page) = inner;
    let mut findings = Vec::new();
    let mut entries = Vec::new();
    let mut seq_counter: u64 = 1;

    let policy_rev = doc
        .as_ref()
        .and_then(|d| d.change.as_ref().map(|c| c.id.clone()));
    let deterministic_gov_ts = doc
        .as_ref()
        .and_then(|d| d.sections.get("plan"))
        .and_then(|p| p.get("updated_at"))
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "governance:recorded".to_string());

    // 1. Intent / custody events
    if let Some(ref traj) = traj {
        verify_trajectory_integrity(traj, &mut findings);

        entries.push(TimelineEntry {
            event_id: format!("intent:{}", traj.run_id),
            sequence: seq_counter,
            timestamp: deterministic_gov_ts.clone(),
            stage: "human_request".to_string(),
            actor: "human_or_caller".to_string(),
            task_id: traj.task_id.clone(),
            intent_id: traj.intent_id.clone(),
            lease_generation: None,
            policy_revision: policy_rev.clone(),
            trust_class: TrustClass::Declared,
            corroborated: false,
            claimed_action: trace::redact_string(&traj.original_intent),
            observed_transition: "intent_recorded".to_string(),
            proof_applicability: ProofApplicability::NotProven,
            source_ref: format!("{GOVERNANCE_PATH}#/sections/trajectory/original_intent"),
        });
        seq_counter += 1;

        if traj.derived_intent != traj.original_intent {
            entries.push(TimelineEntry {
                event_id: format!("interpretation:{}", traj.run_id),
                sequence: seq_counter,
                timestamp: deterministic_gov_ts.clone(),
                stage: "interpretation".to_string(),
                actor: "agent".to_string(),
                task_id: traj.task_id.clone(),
                intent_id: traj.intent_id.clone(),
                lease_generation: None,
                policy_revision: policy_rev.clone(),
                trust_class: TrustClass::Declared,
                corroborated: false,
                claimed_action: trace::redact_string(&traj.derived_intent),
                observed_transition: "agent_interpretation_recorded".to_string(),
                proof_applicability: ProofApplicability::NotProven,
                source_ref: format!("{GOVERNANCE_PATH}#/sections/trajectory/derived_intent"),
            });
            seq_counter += 1;
        }

        // Trajectory loops: strictly `Declared` unless corroborated by witnessed path.
        for l in &traj.loops {
            let stage = match l.status {
                TrajectoryLoopStatus::Failed | TrajectoryLoopStatus::Blocked => {
                    "refused_transition"
                }
                TrajectoryLoopStatus::Retrying => "remediation",
                TrajectoryLoopStatus::Passed => "rule_check",
                TrajectoryLoopStatus::Open => "interpretation",
            };
            let applicability = match l.status {
                TrajectoryLoopStatus::Failed | TrajectoryLoopStatus::Blocked => {
                    ProofApplicability::Failed
                }
                TrajectoryLoopStatus::Passed => ProofApplicability::NotProven, // declared grader != witnessed proof
                _ => ProofApplicability::Unknown,
            };
            entries.push(TimelineEntry {
                event_id: format!("loop:{}:attempt:{}", l.loop_id, l.attempt),
                sequence: seq_counter,
                timestamp: deterministic_gov_ts.clone(),
                stage: stage.to_string(),
                actor: format!("{:?}", l.trigger).to_ascii_lowercase(),
                task_id: traj.task_id.clone(),
                intent_id: Some(l.intent_id.clone()),
                lease_generation: None,
                policy_revision: policy_rev.clone(),
                trust_class: TrustClass::Declared,
                corroborated: false,
                claimed_action: if l.tool_calls.is_empty() {
                    format!("{:?} attempt {}", l.loop_type, l.attempt)
                } else {
                    trace::redact_string(&l.tool_calls.join(", "))
                },
                observed_transition: format!(
                    "loop_status={:?}, grader={:?}",
                    l.status, l.grader_result
                )
                .to_ascii_lowercase(),
                proof_applicability: applicability,
                source_ref: format!(
                    "{GOVERNANCE_PATH}#/sections/trajectory/loops/{}/{}",
                    l.loop_id, l.attempt
                ),
            });
            seq_counter += 1;
        }
    }

    // 2. Witnessed native proof path (#1767 correlation!)
    let witnessed_path_present = observed_path.is_some();
    if let Some(ref judgment) = observed_path {
        verify_observed_path_chain(judgment, &mut findings);
        let binding_actor = judgment
            .binding
            .as_ref()
            .map(|b| b.actor.clone())
            .unwrap_or_else(|| "kernel_mediated_native_runner".to_string());
        let binding_lease = judgment.binding.as_ref().map(|b| b.lease_generation);
        let binding_rev = judgment
            .binding
            .as_ref()
            .map(|b| b.instruction_revision.clone())
            .or_else(|| policy_rev.clone());
        let head_matches = judgment
            .binding
            .as_ref()
            .and_then(|b| {
                snapshot
                    .worktree
                    .head_commit
                    .as_deref()
                    .map(|h| b.subject.head_revision == h)
            })
            .unwrap_or(false);

        for ev in &judgment.events {
            let applicability = if !head_matches {
                ProofApplicability::Stale
            } else if ev.outcome.eq_ignore_ascii_case("pass")
                || ev.outcome.eq_ignore_ascii_case("success")
                || ev.outcome.eq_ignore_ascii_case("authorized")
            {
                ProofApplicability::Proven
            } else if ev.outcome.eq_ignore_ascii_case("fail")
                || ev.outcome.eq_ignore_ascii_case("failed")
                || ev.outcome.eq_ignore_ascii_case("denied")
            {
                ProofApplicability::Failed
            } else {
                ProofApplicability::Unknown
            };
            entries.push(TimelineEntry {
                event_id: format!("witnessed:{}", ev.event_id),
                sequence: seq_counter,
                timestamp: deterministic_gov_ts.clone(),
                stage: "validation".to_string(),
                actor: binding_actor.clone(),
                task_id: traj.as_ref().and_then(|t| t.task_id.clone()),
                intent_id: traj.as_ref().and_then(|t| t.intent_id.clone()),
                lease_generation: binding_lease,
                policy_revision: binding_rev.clone(),
                trust_class: TrustClass::Witnessed,
                corroborated: true,
                claimed_action: format!("{}:{}", ev.kind, ev.check.job),
                observed_transition: format!("witnessed_outcome={}", ev.outcome),
                proof_applicability: applicability,
                source_ref: format!("observed-path.json#seq:{}", ev.sequence),
            });
            seq_counter += 1;
        }
    }

    // 3. SQLite task/lifecycle events (bounded cursor from projection_budget!)
    let mut cursor = TimelineCursor {
        stream: stream.clone(),
        after_seq: query.after_seq,
        next_seq: query.after_seq,
        retention_floor_seq: 0,
        has_more: false,
    };
    if let Some(page) = events_page {
        cursor = TimelineCursor {
            stream: page.cursor.stream,
            after_seq: page.cursor.after_seq,
            next_seq: page.cursor.next_seq,
            retention_floor_seq: page.cursor.retention_floor_seq,
            has_more: page.cursor.has_more,
        };
        for ev in page.items {
            let task_id = ev
                .payload
                .get("task_id")
                .or_else(|| ev.payload.get("id"))
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned);
            entries.push(TimelineEntry {
                event_id: format!("db:{}:{}", ev.stream, ev.event_id),
                sequence: seq_counter,
                timestamp: ev.ts,
                stage: "task_lifecycle".to_string(),
                actor: ev.actor,
                task_id,
                intent_id: traj.as_ref().and_then(|t| t.intent_id.clone()),
                lease_generation: None,
                policy_revision: policy_rev.clone(),
                trust_class: TrustClass::RecordedFact,
                corroborated: true,
                claimed_action: ev.event_type.clone(),
                observed_transition: format!("event_seq={}", ev.seq),
                proof_applicability: ProofApplicability::NotProven,
                source_ref: format!(".decapod/data/decapod.db#events/{}/{}", ev.stream, ev.seq),
            });
            seq_counter += 1;
        }
    }

    // 4. Git commits since merge-base (bounded)
    if let Some(mb) = snapshot.baseline.merge_base_sha.as_deref()
        && let Some(log) = run_git_stdout_trimmed(
            repo_root,
            &[
                "log",
                "--reverse",
                "-n",
                "32",
                "--format=%H\t%aI\t%an\t%s",
                &format!("{mb}..HEAD"),
            ],
        )
    {
        for line in log.lines() {
            let parts: Vec<&str> = line.splitn(4, '\t').collect();
            if parts.len() == 4 {
                entries.push(TimelineEntry {
                    event_id: format!("git:{}", parts[0]),
                    sequence: seq_counter,
                    timestamp: parts[1].to_string(),
                    stage: "remediation".to_string(),
                    actor: parts[2].to_string(),
                    task_id: traj.as_ref().and_then(|t| t.task_id.clone()),
                    intent_id: traj.as_ref().and_then(|t| t.intent_id.clone()),
                    lease_generation: None,
                    policy_revision: policy_rev.clone(),
                    trust_class: TrustClass::RecordedFact,
                    corroborated: true,
                    claimed_action: trace::redact_string(parts[3]),
                    observed_transition: format!(
                        "git_commit:{}",
                        &parts[0][..parts[0].len().min(12)]
                    ),
                    proof_applicability: ProofApplicability::NotProven,
                    source_ref: format!("git:commit:{}", parts[0]),
                });
                seq_counter += 1;
            }
        }
    }

    // 5. Validation receipt (with raw JSON fallback so tampered receipts are still surfaced!)
    let raw_val_section = doc
        .as_ref()
        .and_then(|d| d.sections.get("validation"))
        .or_else(|| {
            raw_gov
                .as_ref()
                .and_then(|v| v.get("sections"))
                .and_then(|s| s.get("validation"))
        });
    let receipt =
        inspect_validation_receipt_value(raw_val_section, snapshot.worktree.head_commit.as_deref());
    if receipt.present {
        if receipt.forged_or_tampered {
            findings.push(TimelineIntegrityFinding {
                code: "FORGED_VALIDATION_RECEIPT".to_string(),
                severity: "error".to_string(),
                detail: receipt
                    .integrity_error
                    .clone()
                    .unwrap_or_else(|| "validation receipt hash mismatch".to_string()),
                source_ref: format!("{GOVERNANCE_PATH}#/sections/validation"),
            });
        }
        let applicability = if receipt.forged_or_tampered {
            ProofApplicability::Failed
        } else if receipt.stale_vs_head {
            ProofApplicability::Stale
        } else if receipt.integrity_valid && receipt.fail_count == 0 {
            ProofApplicability::Proven
        } else {
            ProofApplicability::Failed
        };
        entries.push(TimelineEntry {
            event_id: format!(
                "validation:{}",
                receipt.receipt_hash.as_deref().unwrap_or("unhashed")
            ),
            sequence: seq_counter,
            timestamp: deterministic_gov_ts.clone(),
            stage: "validation".to_string(),
            actor: "decapod_validate".to_string(),
            task_id: traj.as_ref().and_then(|t| t.task_id.clone()),
            intent_id: traj.as_ref().and_then(|t| t.intent_id.clone()),
            lease_generation: None,
            policy_revision: policy_rev.clone(),
            trust_class: if receipt.integrity_valid {
                TrustClass::RecordedFact
            } else {
                TrustClass::Unknown
            },
            corroborated: receipt.integrity_valid && !receipt.stale_vs_head,
            claimed_action: "validation_receipt".to_string(),
            observed_transition: format!(
                "status={} pass={} fail={} stale_vs_head={}",
                receipt.status.as_deref().unwrap_or("unknown"),
                receipt.pass_count,
                receipt.fail_count,
                receipt.stale_vs_head
            ),
            proof_applicability: applicability,
            source_ref: format!("{GOVERNANCE_PATH}#/sections/validation"),
        });
    }

    // Objective -> evidence graph (with raw JSON fallback when governance_document::load fails due to tampering)
    let mut objective_evidence_graph = Vec::new();
    let root_intent = traj
        .as_ref()
        .and_then(|t| t.intent_id.clone())
        .unwrap_or_else(|| "intent:unknown".to_string());
    let claim_entries: Vec<(String, Vec<String>)> = if let Some(ref doc) = doc {
        doc.claims
            .iter()
            .map(|(id, c)| (id.clone(), c.proof_refs.clone()))
            .collect()
    } else if let Some(raw_claims) = raw_gov
        .as_ref()
        .and_then(|v| v.get("claims"))
        .and_then(|c| c.as_object())
    {
        raw_claims
            .iter()
            .map(|(id, v)| {
                let refs = v
                    .get("proof_refs")
                    .and_then(|r| r.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|x| x.as_str().map(ToOwned::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                (id.clone(), refs)
            })
            .collect()
    } else {
        Vec::new()
    };
    for (claim_id, proof_refs) in claim_entries {
        let broken = proof_refs.is_empty() || !findings.is_empty();
        objective_evidence_graph.push(ObjectiveEvidenceEdge {
            intent_id: root_intent.clone(),
            task_id: tasks.first().map(|t| t.id.clone()),
            claim_id: Some(claim_id),
            evidence_ref: proof_refs.first().cloned(),
            proof_applicability: if !findings.is_empty() {
                ProofApplicability::Failed
            } else if proof_refs.is_empty() {
                ProofApplicability::NotProven
            } else if receipt.integrity_valid && !receipt.stale_vs_head {
                ProofApplicability::Proven
            } else {
                ProofApplicability::Stale
            },
            broken,
            broken_reason: if !findings.is_empty() {
                Some("timeline_integrity_anomalies_detected".to_string())
            } else if proof_refs.is_empty() {
                Some("claim_missing_proof_ref".to_string())
            } else {
                None
            },
        });
    }

    let declared_only_count = entries
        .iter()
        .filter(|e| e.trust_class == TrustClass::Declared && !e.corroborated)
        .count();
    let witnessed_count = entries
        .iter()
        .filter(|e| e.trust_class == TrustClass::Witnessed)
        .count();

    // Enforce bounded entries count
    if entries.len() > limit {
        entries.truncate(limit);
    }

    Ok(ActivityTimelineProjection {
        schema_version: OBSERVE_SCHEMA_VERSION.to_string(),
        read_only: true,
        authoritative: false,
        snapshot,
        history_intact: findings.is_empty(),
        witnessed_path_present,
        declared_only_count,
        witnessed_count,
        integrity_findings: findings,
        entries,
        objective_evidence_graph,
        cursor,
    })
}
