//! A bounded continuation of mentor/orientation, not another authority engine.
use super::{CandidateSource, MentorEngine, ObligationsContext};
use crate::core::{error::DecapodError, governance_document, store::find_governance_root, todo};
use crate::plan_governance::{self, GovernedPlan, SpecReviewDisposition};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};

pub const ROUND_LIMIT: usize = 3;
const MAX_PATHS: usize = 32;
const MAX_SOURCE_BYTES: u64 = 256 * 1024;
const MAX_ADRS: usize = 32;
const MAX_JOURNAL_BYTES: usize = 64 * 1024;
const PLAN_REF: &str = ".decapod/governance.json#/sections/plan";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub task_id: String,
    pub submission_id: String,
    #[serde(default)]
    pub expected_token: Option<String>,
    pub op: String,
    pub touched_paths: Vec<String>,
    /// Transient agent explanation. Hashed for replay integrity, never a grader.
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub evidence: Vec<SourceRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceRef {
    pub ref_path: String,
    /// May be omitted on input to request current byte identity, never approval.
    #[serde(default)]
    pub hash: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Proceed,
    Reflect,
    HumanDecision,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Challenge {
    pub id: String,
    pub question: String,
    pub trigger: String,
    pub sources: Vec<SourceRef>,
    pub investigation: String,
    /// Only existing plan constraints/authority gates are binding.
    pub enforceable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Outcome {
    pub schema_version: String,
    pub disposition: Disposition,
    pub reason: String,
    pub challenge: Option<Challenge>,
    pub token: String,
    pub round: usize,
    pub remaining_rounds: usize,
    pub terminal: bool,
    pub execution_blocked: bool,
    pub context_digest: String,
    pub next_action: String,
    /// Always false: reflection is neither execution nor validation.
    pub completion_proven: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Receipt {
    pub submission_id: String,
    pub input_digest: String,
    pub progress_digest: String,
    pub touched_paths: Vec<String>,
    pub pending_ids: Vec<String>,
    pub resolved_ids: Vec<String>,
    pub evidence: Vec<SourceRef>,
    pub outcome: Outcome,
}

/// One current task/plan continuation; no explanations or source bodies copied.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Journal {
    pub task_id: String,
    pub custody_digest: String,
    pub receipts: Vec<Receipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<Outcome>,
}

fn invalid(message: &str) -> DecapodError {
    DecapodError::ValidationError(format!("REFLECTION_{message}"))
}

fn digest(value: &impl Serialize) -> Result<String, DecapodError> {
    governance_document::digest(
        &serde_json::to_value(value).map_err(|e| invalid(&format!("INVALID_INPUT: {e}")))?,
    )
}

fn check_path(root: &Path, path: &str) -> Result<(), DecapodError> {
    if path.is_empty()
        || path.len() > 256
        || path.contains('\\')
        || path.split('/').any(|part| matches!(part, "" | "." | ".."))
        || Path::new(path)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(invalid(
            "INVALID_PATH: use bounded literal repository paths",
        ));
    }
    let mut current = root.to_path_buf();
    for part in Path::new(path).components() {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => return Err(invalid("SYMLINK_PATH")),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(DecapodError::IoError(e)),
        }
    }
    Ok(())
}

fn source_hash(root: &Path, path: &str) -> Result<String, DecapodError> {
    check_path(root, path)?;
    let path = root.join(path);
    match fs::metadata(&path) {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_SOURCE_BYTES => {
            let bytes = fs::read(path).map_err(DecapodError::IoError)?;
            if bytes.len() as u64 > MAX_SOURCE_BYTES {
                return Err(invalid("SOURCE_LIMIT"));
            }
            Ok(format!("sha256:{:x}", Sha256::digest(&bytes)))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => digest(&"absent"),
        Err(e) => Err(DecapodError::IoError(e)),
        _ => Err(invalid("SOURCE_LIMIT: expected a bounded regular file")),
    }
}

fn plan_contract(plan: &GovernedPlan) -> Result<Value, DecapodError> {
    let mut value = serde_json::to_value(plan).map_err(|e| invalid(&e.to_string()))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| invalid("INVALID_PLAN"))?;
    object.remove("reflection");
    object.remove("updated_at");
    Ok(value)
}

fn save(root: &Path, plan: &GovernedPlan) -> Result<(), DecapodError> {
    if let Some(journal) = &plan.reflection
        && serde_json::to_vec(journal)
            .map_err(|e| invalid(&e.to_string()))?
            .len()
            > MAX_JOURNAL_BYTES
    {
        return Err(invalid("STATE_LIMIT: journal exceeds 64 KiB"));
    }
    plan_governance::save_plan(root, plan)
}

fn adr_candidates(root: &Path) -> Result<Vec<CandidateSource>, DecapodError> {
    check_path(root, "docs/decisions")?;
    let dir = root.join("docs/decisions");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut candidates = Vec::new();
    for entry in fs::read_dir(dir).map_err(DecapodError::IoError)? {
        let path = entry.map_err(DecapodError::IoError)?.path();
        if !path.extension().is_some_and(|ext| ext == "md") {
            continue;
        }
        if candidates.len() == MAX_ADRS {
            return Err(invalid("SOURCE_LIMIT: too many ADRs"));
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| invalid("SOURCE_SCOPE"))?
            .to_string_lossy()
            .into_owned();
        source_hash(root, &relative)?;
        let content = fs::read_to_string(&path).map_err(DecapodError::IoError)?;
        if content.len() as u64 > MAX_SOURCE_BYTES {
            return Err(invalid("SOURCE_LIMIT"));
        }
        candidates.push(CandidateSource::Adr { path, content });
    }
    candidates.sort_by_key(|c| match c {
        CandidateSource::Adr { path, .. } => path.clone(),
        _ => unreachable!(),
    });
    Ok(candidates)
}

fn context(
    root: &Path,
    plan: &GovernedPlan,
    evidence: &[SourceRef],
) -> Result<String, DecapodError> {
    let mut sources = Vec::new();
    for path in ["AGENTS.md", ".decapod/config.toml", ".decapod/OVERRIDE.md"] {
        sources.push((path.to_string(), source_hash(root, path)?));
    }
    for candidate in adr_candidates(root)? {
        if let CandidateSource::Adr { path, .. } = candidate {
            let path = path
                .strip_prefix(root)
                .map_err(|_| invalid("SOURCE_SCOPE"))?
                .to_string_lossy()
                .into_owned();
            sources.push((path.clone(), source_hash(root, &path)?));
        }
    }
    for reference in evidence {
        if reference.ref_path != PLAN_REF {
            sources.push((
                reference.ref_path.clone(),
                source_hash(root, &reference.ref_path)?,
            ));
        }
    }
    sources.sort();
    sources.dedup();
    let document = governance_document::load(root)?;
    digest(&json!({"plan": plan_contract(plan)?, "sources": sources,
        "baseline": document.as_ref().map(|d| &d.baseline),
        "change": document.as_ref().and_then(|d| d.change.as_ref())}))
}

fn bounded(text: &str) -> String {
    text.chars().take(512).collect()
}

fn challenge(
    trigger: &str,
    question: String,
    source: SourceRef,
    enforceable: bool,
) -> Result<Challenge, DecapodError> {
    Ok(Challenge {
        id: digest(&json!({"trigger":trigger, "source":source.ref_path}))?,
        question: bounded(&question),
        trigger: bounded(trigger),
        sources: vec![source],
        investigation: if enforceable {
            "Inspect the cited contract. Narrow the declared action to satisfy it, or resolve the governing question through the existing plan/authority surface. Re-submit current source references; prose alone cannot clear this challenge.".into()
        } else {
            "Inspect the cited ADR and the affected implementation. This literal match is advisory; independently verify the invariant. A narrower scope or separately revised governing source can remove the match. Citation alone proves no behavior.".into()
        },
        enforceable,
    })
}

fn challenges(
    root: &Path,
    plan: &GovernedPlan,
    request: &Request,
) -> Result<Vec<Challenge>, DecapodError> {
    let mut result = Vec::new();
    let plan_source = SourceRef {
        ref_path: PLAN_REF.into(),
        hash: digest(&plan_contract(plan)?)?,
    };
    for question in &plan.human_questions {
        result.push(challenge(
            "human_question",
            question.clone(),
            plan_source.clone(),
            true,
        )?);
    }
    for review in &plan.spec_reviews {
        if review.disposition == SpecReviewDisposition::RequiresDecision {
            result.push(challenge(
                "human_spec_review",
                review.reason.clone(),
                plan_source.clone(),
                true,
            )?);
        }
    }
    for path in &request.touched_paths {
        for forbidden in &plan.constraints.forbidden_paths {
            let forbidden = forbidden.trim_end_matches('/').to_ascii_lowercase();
            let folded = path.to_ascii_lowercase();
            if folded == forbidden
                || folded.starts_with(&format!("{forbidden}/"))
                || forbidden.starts_with(&format!("{folded}/"))
            {
                result.push(challenge(&format!("forbidden_path:{path}"),
                    format!("How will the proposed action avoid changing {path}, which the current plan excludes?"),
                    plan_source.clone(), true)?);
            }
        }
    }
    if plan
        .constraints
        .file_touch_budget
        .is_some_and(|limit| request.touched_paths.len() > limit)
    {
        result.push(challenge(
            "file_touch_budget",
            "Which smaller action fits the current plan's file-touch budget?".into(),
            plan_source.clone(),
            true,
        )?);
    }
    for item in plan.unknowns.iter().chain(&plan.unresolved_contradictions) {
        result.push(challenge(
            &format!("plan_gap:{}", digest(item)?),
            format!("What inspectable contract change resolves this recorded plan gap: {item}?"),
            plan_source.clone(),
            true,
        )?);
    }
    let engine = MentorEngine::new(root);
    let ctx = ObligationsContext {
        op: request.op.clone(),
        params: json!({}),
        touched_paths: request.touched_paths.clone(),
        diff_summary: None,
        project_profile_id: None,
        session_id: None,
        high_risk: false,
    };
    // Share the existing detector, not the unrelated ranked memory/coplayer scan.
    let candidates = adr_candidates(root)?;
    for conflict in engine.detect_contradictions(&engine.score_candidates(&candidates, &ctx), &ctx)
    {
        if let Some(source) = conflict.source {
            result.push(challenge(
                &format!("mentor:{}", conflict.description),
                format!(
                    "How will this proposed action preserve the cited repository decision? {}",
                    conflict.description
                ),
                SourceRef {
                    hash: source_hash(root, &source.id)?,
                    ref_path: source.id,
                },
                false,
            )?);
        }
    }
    result.dedup_by(|a, b| a.id == b.id);
    Ok(result)
}

fn custody(root: &Path, task_id: &str) -> Result<String, DecapodError> {
    let store = find_governance_root(root).join(".decapod/data");
    let actor = std::env::var("DECAPOD_AGENT_ID").unwrap_or_else(|_| "unknown".into());
    digest(&todo::capture_execution_custody(&store, task_id, &actor)?)
}

fn validate_request(root: &Path, request: &Request) -> Result<(), DecapodError> {
    for text in [&request.task_id, &request.submission_id, &request.op] {
        if text.trim().is_empty() || text.len() > 128 {
            return Err(invalid("INVALID_INPUT"));
        }
    }
    if request.rationale.len() > 2048
        || request.touched_paths.len() > MAX_PATHS
        || request.evidence.len() > 8
    {
        return Err(invalid("INPUT_LIMIT"));
    }
    if request
        .expected_token
        .as_ref()
        .is_some_and(|token| token.len() > 71)
        || request
            .evidence
            .iter()
            .any(|source| source.ref_path.len() > 256 || source.hash.len() > 71)
    {
        return Err(invalid("INPUT_LIMIT"));
    }
    if !matches!(request.op.as_str(), "change" | "modify" | "update") {
        return Err(invalid(
            "INVALID_OPERATION: reflection covers change, modify or update; use ordinary orientation for informational queries",
        ));
    }
    let mut seen = BTreeSet::new();
    for path in &request.touched_paths {
        check_path(root, path)?;
        if !seen.insert(path) {
            return Err(invalid("DUPLICATE_PATH"));
        }
    }
    Ok(())
}

pub fn submit(root: &Path, request: &Request) -> Result<Outcome, DecapodError> {
    validate_request(root, request)?;
    crate::core::workspace::ensure_isolated_workspace_for_projection_mutation(root)?;
    governance_document::with_lock(root, || {
        let mut plan = plan_governance::load_plan(root)?.ok_or_else(|| invalid("PLAN_REQUIRED"))?;
        if !plan.todo_ids.contains(&request.task_id) {
            return Err(invalid("TASK_UNBOUND"));
        }
        let custody_digest = custody(root, &request.task_id)?;
        let input_digest = digest(request)?;
        let previous = plan.reflection.as_ref();
        if let Some(journal) = previous {
            if journal.task_id != request.task_id || journal.custody_digest != custody_digest {
                return Err(invalid(
                    "CUSTODY_CHANGED: inspect current task and retained journal",
                ));
            }
            if journal.receipts.is_empty() || journal.receipts.len() > ROUND_LIMIT {
                return Err(invalid("INVALID_JOURNAL"));
            }
            if let Some(stop) = &journal.stop {
                return Ok(stop.clone());
            }
            if let Some(receipt) = journal
                .receipts
                .iter()
                .find(|r| r.submission_id == request.submission_id)
            {
                if receipt.input_digest != input_digest {
                    return Err(invalid("SUBMISSION_REUSED"));
                }
                if receipt.outcome.terminal {
                    return Ok(receipt.outcome.clone());
                }
                // Do not replay a conclusion against changed governing bytes.
                let current = context(root, &plan, &receipt.evidence)?;
                if current != receipt.outcome.context_digest {
                    return Err(invalid(
                        "CONTEXT_CHANGED: re-query with a new submission and the latest token",
                    ));
                }
                return Ok(receipt.outcome.clone());
            }
            let last = journal
                .receipts
                .last()
                .ok_or_else(|| invalid("INVALID_JOURNAL"))?;
            if request.expected_token.as_deref() != Some(last.outcome.token.as_str()) {
                return Err(invalid(
                    "STALE_TOKEN: inspect the current plan reflection before retrying",
                ));
            }
            if last.outcome.terminal {
                return Ok(last.outcome.clone());
            }
            if journal.receipts.len() == ROUND_LIMIT {
                let mut stop = last.outcome.clone();
                stop.disposition = Disposition::Blocked;
                stop.reason = "budget_exhausted".into();
                stop.terminal = true;
                stop.execution_blocked = true;
                stop.next_action = "Stop: the lifetime three-round budget is exhausted. Preserve receipts and resolve through a new governed workstream.".into();
                plan.reflection
                    .as_mut()
                    .ok_or_else(|| invalid("INVALID_JOURNAL"))?
                    .stop = Some(stop.clone());
                save(root, &plan)?;
                return Ok(stop);
            }
        } else if request.expected_token.is_some() {
            return Err(invalid("STALE_TOKEN: no reflection exists"));
        }
        let mut evidence = request.evidence.clone();
        for reference in &mut evidence {
            let expected = if reference.ref_path == PLAN_REF {
                digest(&plan_contract(&plan)?)?
            } else {
                source_hash(root, &reference.ref_path)?
            };
            if !reference.hash.is_empty() && reference.hash != expected {
                return Err(invalid(
                    "STALE_EVIDENCE: inspect current source; omit hash to capture current identity",
                ));
            }
            reference.hash = expected;
        }
        let context_digest = context(root, &plan, &evidence)?;
        let mut pending = challenges(root, &plan, request)?;
        let database = todo::todo_db_path(&find_governance_root(root).join(".decapod/data"));
        let conn = crate::core::db::db_connect_for_validate(
            database.to_str().ok_or_else(|| invalid("STORE_PATH"))?,
        )?;
        let title: String = conn.query_row(
            "SELECT title FROM tasks WHERE id=?1",
            [&request.task_id],
            |row| row.get(0),
        )?;
        drop(conn);
        if crate::core::rpc::architectural_alignment_required(&title) {
            pending.insert(0, challenge("human_orientation_alignment",
                "Which architectural direction has the human accepted for this task? Preserve the existing orientation decision gate.".into(),
                SourceRef { ref_path: PLAN_REF.into(), hash: digest(&plan_contract(&plan)?)? }, true)?);
        }
        let pending_ids: Vec<String> = pending.iter().map(|c| c.id.clone()).collect();
        if pending_ids.len() > 64 {
            return Err(invalid("STATE_LIMIT: too many applicable gaps"));
        }
        let progress_digest = digest(&pending_ids)?;
        let mut paths = request.touched_paths.clone();
        paths.sort();
        let last = previous.and_then(|j| j.receipts.last());
        let resolved_ids: Vec<String> = last
            .map(|last| {
                last.pending_ids
                    .iter()
                    .filter(|id| !pending_ids.contains(id))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let narrowed = last.is_some_and(|last| {
            paths.len() < last.touched_paths.len()
                && paths.iter().all(|path| last.touched_paths.contains(path))
        });
        // Evidence records current source identity, never a passing proof receipt.
        let addressed_source = last
            .and_then(|r| r.outcome.challenge.as_ref())
            .is_none_or(|c| {
                request
                    .evidence
                    .iter()
                    .any(|e| c.sources.iter().any(|s| s.ref_path == e.ref_path))
            });
        let round = previous.map_or(1, |j| j.receipts.len() + 1);
        let human = pending
            .first()
            .is_some_and(|c| c.trigger.starts_with("human_"));
        let stagnant = last.is_some_and(|last| {
            !last.pending_ids.is_empty() && resolved_ids.is_empty() && !narrowed
        });
        let (disposition, reason, terminal) = if round > ROUND_LIMIT {
            (Disposition::Blocked, "budget_exhausted", true)
        } else if human {
            (
                Disposition::HumanDecision,
                "existing_human_gate",
                round == ROUND_LIMIT,
            )
        } else if stagnant {
            (Disposition::Blocked, "no_observable_progress", true)
        } else if last.is_some_and(|r| !r.pending_ids.is_empty()) && !addressed_source {
            (Disposition::Blocked, "resolution_source_required", true)
        } else if pending.is_empty() {
            (Disposition::Proceed, "no_outstanding_reflective_gap", false)
        } else if round == ROUND_LIMIT {
            (Disposition::Blocked, "budget_exhausted", true)
        } else {
            (Disposition::Reflect, "investigate_current_gap", false)
        };
        let execution_blocked =
            disposition == Disposition::Blocked || human || pending.iter().any(|c| c.enforceable);
        let focus = pending.into_iter().next();
        let token = digest(&json!({"task":request.task_id, "custody":custody_digest,
            "input":input_digest, "context":context_digest, "round":round}))?;
        let outcome = Outcome {
            schema_version: "decapod.reflection.v1".into(), disposition, reason: reason.into(),
            challenge: focus, token, round: round.min(ROUND_LIMIT),
            remaining_rounds: ROUND_LIMIT.saturating_sub(round), terminal, execution_blocked,
            context_digest, next_action: match disposition {
                Disposition::Proceed => "Reflection has no current gap. Check existing plan approval, custody, scope and execution gates; implement and independently validate before claiming completion.",
                Disposition::Reflect => "Investigate the single challenge. Submit a current source reference and an observably revised action using expected_token; changing wording cannot establish progress.",
                Disposition::HumanDecision => "Obtain the human decision through the existing authority surface. Reflection cannot supply or invent approval.",
                Disposition::Blocked => "Stop this reflection. Preserve its blocker and receipts; do not reset its budget, assert success or execute a blocked action.",
            }.into(),
            completion_proven: false,
        };
        let receipt = Receipt {
            submission_id: request.submission_id.clone(),
            input_digest,
            progress_digest,
            touched_paths: paths,
            pending_ids,
            resolved_ids,
            evidence: evidence.clone(),
            outcome: outcome.clone(),
        };
        let mut journal = plan.reflection.take().unwrap_or(Journal {
            task_id: request.task_id.clone(),
            custody_digest: custody_digest.clone(),
            receipts: vec![],
            stop: None,
        });
        if journal.receipts.len() == ROUND_LIMIT {
            return Err(invalid("BUDGET_EXHAUSTED"));
        }
        journal.receipts.push(receipt);
        plan.reflection = Some(journal);
        // Recheck independent task/source state immediately before atomic save.
        if custody(root, &request.task_id)? != custody_digest
            || context(root, &plan, &evidence)? != outcome.context_digest
        {
            return Err(invalid("CONTEXT_CHANGED: no submission committed"));
        }
        save(root, &plan)?;
        Ok(outcome)
    })
}

/// Called at the existing execution boundary. No reflection means no new cost.
pub fn check_execute(
    root: &Path,
    _store: &Path,
    plan: &GovernedPlan,
    task: Option<&str>,
) -> Result<(), DecapodError> {
    let Some(journal) = &plan.reflection else {
        return Ok(());
    };
    if journal.stop.is_some() {
        return Err(invalid("BUDGET_EXHAUSTED"));
    }
    if task.is_some_and(|task| task != journal.task_id) {
        return Err(invalid("TASK_MISMATCH"));
    }
    let last = journal
        .receipts
        .last()
        .ok_or_else(|| invalid("INVALID_JOURNAL"))?;
    if last.outcome.execution_blocked {
        return Err(invalid(&format!("BLOCKED: {}", last.outcome.reason)));
    }
    if custody(root, &journal.task_id)? != journal.custody_digest
        || context(root, plan, &last.evidence)? != last.outcome.context_digest
    {
        return Err(invalid("CONTEXT_CHANGED: re-query before execution"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../../tests/unit/core/reflection_tests.rs"]
mod tests;
