//! Execution continuity, not a second workflow or validation authority.
//!
//! Checkpoints live in the existing trajectory and Git history. Captures reuse
//! the readiness material inventory and validation epoch, but do not claim
//! candidate readiness, task ownership, or permission for a future operation.

use super::{TrajectoryArtifact, load_trajectory, load_trajectory_cookie, write_trajectory};
use crate::core::{
    bounded_process::{BoundedCommand, Cancellation, ProcessLimits, Termination},
    db::{Connection, OptionalExtension, params},
    error::DecapodError,
    governance_document,
    readiness_evidence::material::{self, CaptureIo},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

const SCHEMA: &str = "1.0.0";
const RESUME_OPERATION: &str = "trajectory.resume";
const MAX_CHECKPOINTS: usize = 256;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExecutionApprovalBinding {
    pub approval_id: String,
    /// Includes issuer, action, scope and expiry, without persisting their text.
    pub record_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExecutionStateBinding {
    pub schema_version: String,
    pub task_id: String,
    pub run_id: String,
    pub intent_id: String,
    /// Provenance, not a claim that earlier validation applies to a new commit.
    pub head_revision: String,
    pub work_branch: String,
    pub material_digest: String,
    pub plan_digest: String,
    pub trajectory_digest: String,
    pub validation_epoch_digest: String,
    pub governance_digest: String,
    pub policy_digest: String,
    pub task_custody_digest: String,
    pub required_approvals: Vec<ExecutionApprovalBinding>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExecutionCheckpoint {
    pub checkpoint_id: String,
    pub sequence: u64,
    pub binding: ExecutionStateBinding,
    pub binding_digest: String,
}

fn blocked(code: &str, message: &str) -> DecapodError {
    DecapodError::ValidationError(format!("{code}: {message}"))
}

fn digest(value: &impl Serialize) -> Result<String, DecapodError> {
    let bytes = serde_json::to_vec(value).map_err(|_| {
        blocked(
            "EXECUTION_BINDING_INVALID",
            "cannot encode execution identity",
        )
    })?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

pub(super) fn validate_checkpoints(artifact: &TrajectoryArtifact) -> Result<(), DecapodError> {
    if artifact.execution_checkpoints.len() > MAX_CHECKPOINTS {
        return Err(blocked(
            "EXECUTION_CHECKPOINT_LIMIT",
            "trajectory has too many execution checkpoints",
        ));
    }
    let mut ids = BTreeSet::new();
    for (index, checkpoint) in artifact.execution_checkpoints.iter().enumerate() {
        super::validate_run_id(&checkpoint.checkpoint_id)?;
        if checkpoint.sequence != index as u64 + 1
            || !ids.insert(&checkpoint.checkpoint_id)
            || checkpoint.binding.run_id != artifact.run_id
            || checkpoint.binding.schema_version != SCHEMA
            || checkpoint.binding_digest != digest(&checkpoint.binding)?
        {
            return Err(blocked(
                "EXECUTION_CHECKPOINT_CORRUPT",
                "invalid checkpoint sequence, identity or digest",
            ));
        }
    }
    Ok(())
}

/// No raw file contents, command lines, environment, absolute paths or process
/// output are persisted. New nonignored source files must first be staged so
/// the existing complete tracked-material inventory owns their identity.
pub fn capture_execution_binding(
    project_root: &Path,
    task_id: &str,
) -> Result<ExecutionStateBinding, DecapodError> {
    governance_document::with_lock(project_root, || {
        let trajectory = current_trajectory(project_root, task_id)?;
        let approval_ids = trajectory
            .execution_checkpoints
            .last()
            .map(|checkpoint| {
                checkpoint
                    .binding
                    .required_approvals
                    .iter()
                    .map(|approval| approval.approval_id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        stable_capture(project_root, task_id, &approval_ids)
    })
}

fn current_trajectory(root: &Path, task_id: &str) -> Result<TrajectoryArtifact, DecapodError> {
    let trajectory = load_trajectory_cookie(root)?.ok_or_else(|| {
        blocked(
            "EXECUTION_CONTEXT_MISSING",
            "initialize the current trajectory before checkpointing",
        )
    })?;
    validate_checkpoints(&trajectory)?;
    if task_id.trim().is_empty() || trajectory.task_id.as_deref() != Some(task_id) {
        return Err(blocked(
            "EXECUTION_TASK_MISMATCH",
            "current trajectory must name this task",
        ));
    }
    if trajectory.intent_id.as_deref().is_none_or(str::is_empty)
        || trajectory.original_intent.trim().is_empty()
        || trajectory.derived_intent.trim().is_empty()
    {
        return Err(blocked(
            "EXECUTION_INTENT_MISSING",
            "current accepted intent is required",
        ));
    }
    if !trajectory.blockers.is_empty() || !trajectory.unresolved_assumptions.is_empty() {
        return Err(blocked(
            "EXECUTION_DECISION_REQUIRED",
            "resolve current trajectory blockers and assumptions before resuming",
        ));
    }
    let intent_id = trajectory.intent_id.as_deref().unwrap_or_default();
    let intent = trajectory.custody.intents.get(intent_id).ok_or_else(|| {
        blocked(
            "EXECUTION_INTENT_MISSING",
            "current trajectory has no matching custody intent",
        )
    })?;
    if intent.status.is_terminal() {
        return Err(blocked(
            "EXECUTION_INTENT_TERMINAL",
            "terminal intent cannot authorize continuation",
        ));
    }
    Ok(trajectory)
}

fn accepted_plan(
    root: &Path,
    task_id: &str,
) -> Result<crate::plan_governance::GovernedPlan, DecapodError> {
    use crate::plan_governance::{self, PlanState, SpecReviewDisposition};
    let plan = plan_governance::load_plan(root)?.ok_or_else(|| {
        blocked(
            "NEEDS_PLAN_APPROVAL",
            "execution checkpoint requires an accepted plan",
        )
    })?;
    if !matches!(plan.state, PlanState::Approved | PlanState::Executing)
        || !plan.todo_ids.iter().any(|id| id == task_id)
    {
        return Err(blocked(
            "NEEDS_PLAN_APPROVAL",
            "current task must belong to an approved or executing plan",
        ));
    }
    if plan.intent.trim().is_empty()
        || !plan.unknowns.is_empty()
        || !plan.human_questions.is_empty()
        || !plan.unresolved_contradictions.is_empty()
        || plan
            .spec_reviews
            .iter()
            .any(|review| review.disposition == SpecReviewDisposition::RequiresDecision)
    {
        return Err(blocked(
            "EXECUTION_DECISION_REQUIRED",
            "resolve the current plan's human decision gates before resuming",
        ));
    }
    plan_governance::validate_plan_progress(&plan)?;
    if let Some(phase) = plan
        .phases
        .iter()
        .find(|phase| phase.entered && !phase.completed)
    {
        if !plan_governance::check_phase_entry_gates(root, &phase.id)?.0 {
            return Err(blocked(
                "PHASE_GATE_NOT_SATISFIED",
                "current execution phase entry gates are not satisfied",
            ));
        }
    } else if plan.phases.iter().any(|phase| !phase.completed) {
        return Err(blocked(
            "PHASE_REQUIRED",
            "enter the next accepted phase before resuming",
        ));
    }
    plan_governance::enforce_scope_constraints(root, &plan.constraints)?;
    Ok(plan)
}

fn stable_capture(
    root: &Path,
    task_id: &str,
    approvals: &[String],
) -> Result<ExecutionStateBinding, DecapodError> {
    let first = capture_once(root, task_id, approvals)?;
    let second = capture_once(root, task_id, approvals)?;
    if first != second {
        return Err(blocked(
            "EXECUTION_CAPTURE_CHANGED",
            "source or execution authority changed during capture; retry at a stable boundary",
        ));
    }
    Ok(first)
}

fn capture_once(
    root: &Path,
    task_id: &str,
    approvals: &[String],
) -> Result<ExecutionStateBinding, DecapodError> {
    let mut trajectory = current_trajectory(root, task_id)?;
    let plan = accepted_plan(root, task_id)?;
    let mut io = CaptureBudget::new();
    reject_grafts(root, &mut io)?;
    let material = material::capture_with_io(root, &BTreeSet::new(), &mut io)
        .map_err(|_| blocked("EXECUTION_MATERIAL_UNAVAILABLE", "cannot capture bounded tracked working material; stage new source files, resolve conflicts and retry"))?;
    let epoch = crate::core::validation_epoch::active_validation_epoch_bounded(root)?;
    // Only the checkpoint bookkeeping is omitted. Intent, phase, boundaries,
    // evidence and complete custody history remain bound to their actual bytes.
    trajectory.execution_checkpoints.clear();
    let trajectory_digest = digest(&trajectory.canonicalized())?;
    let mut governance = BTreeMap::new();
    for section in ["claims", "validation"] {
        governance.insert(section, governance_document::read_section(root, section)?);
    }
    let (policy_digest, required_approvals) = capture_policy(root, task_id, approvals, &mut io)?;
    let owner = policy_owner(root, &mut io)?;
    let custody = crate::core::todo::capture_execution_custody(
        &owner.join(".decapod/data"),
        task_id,
        &crate::current_agent_id(),
    )?;
    Ok(ExecutionStateBinding {
        schema_version: SCHEMA.into(),
        task_id: task_id.into(),
        run_id: trajectory.run_id,
        intent_id: trajectory.intent_id.unwrap_or_default(),
        head_revision: material.head_revision.clone(),
        work_branch: material.work_branch.clone(),
        material_digest: material.content_digest().map_err(|_| {
            blocked(
                "EXECUTION_BINDING_INVALID",
                "cannot encode material identity",
            )
        })?,
        plan_digest: digest(&plan)?,
        trajectory_digest,
        validation_epoch_digest: digest(&epoch)?,
        governance_digest: digest(&governance)?,
        policy_digest,
        task_custody_digest: digest(&custody)?,
        required_approvals,
    })
}

/// Re-measures current source and authority. Equality alone is insufficient:
/// expiry/revocation and human gates are evaluated anew by capture_policy and
/// accepted_plan. A checkpoint commit may advance HEAD but cannot rewrite its
/// recorded ancestry or change the working content or branch.
pub fn verify_execution_binding(
    root: &Path,
    task_id: &str,
    expected: &ExecutionStateBinding,
) -> Result<(), DecapodError> {
    governance_document::with_lock(root, || {
        if expected.schema_version != SCHEMA || expected.task_id != task_id {
            return Err(blocked(
                "EXECUTION_BINDING_INVALID",
                "checkpoint schema or task does not match",
            ));
        }
        let approvals = expected
            .required_approvals
            .iter()
            .map(|a| a.approval_id.clone())
            .collect::<Vec<_>>();
        let mut current = stable_capture(root, task_id, &approvals)?;
        let current_head = current.head_revision.clone();
        current.head_revision = expected.head_revision.clone();
        if &current != expected {
            return Err(blocked(
                "EXECUTION_STATE_CHANGED",
                "source, plan, intent, boundaries, policy or required approval changed; review the change and create a fresh checkpoint",
            ));
        }
        if !valid_revision(&expected.head_revision) || !valid_revision(&current_head) {
            return Err(blocked(
                "EXECUTION_HISTORY_CHANGED",
                "invalid checkpoint revision",
            ));
        }
        let mut io = CaptureBudget::new();
        reject_grafts(root, &mut io)?;
        io.git(
            root,
            &[
                "merge-base",
                "--is-ancestor",
                &expected.head_revision,
                &current_head,
            ],
        )
        .map_err(|_| {
            blocked(
                "EXECUTION_HISTORY_CHANGED",
                "checkpoint HEAD is no longer an ancestor of the current branch",
            )
        })?;
        Ok(())
    })
}

fn valid_revision(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Append a deterministic, idempotent checkpoint to the current Git journal.
/// Reusing an ID with different state fails instead of replacing history.
pub fn checkpoint_execution(
    root: &Path,
    run_id: &str,
    checkpoint_id: &str,
    required_approval_ids: &[String],
) -> Result<ExecutionCheckpoint, DecapodError> {
    super::validate_run_id(checkpoint_id)?;
    governance_document::with_lock(root, || {
        let mut trajectory = load_trajectory(root, run_id)?;
        let task_id = trajectory
            .task_id
            .clone()
            .ok_or_else(|| blocked("EXECUTION_TASK_MISMATCH", "trajectory must name a task"))?;
        // A new checkpoint is not a way to discard a previously declared
        // continuation gate. Additional requirements can be added; removing a
        // requirement needs a newly accepted run/intent with preserved history.
        if let Some(previous) = trajectory.execution_checkpoints.last()
            && previous
                .binding
                .required_approvals
                .iter()
                .any(|approval| !required_approval_ids.contains(&approval.approval_id))
        {
            return Err(blocked(
                "EXECUTION_APPROVAL_REQUIRED",
                "new checkpoint cannot remove the current run's required approval references",
            ));
        }
        let binding = stable_capture(root, &task_id, required_approval_ids)?;
        if let Some(previous) = trajectory
            .execution_checkpoints
            .iter()
            .find(|c| c.checkpoint_id == checkpoint_id)
        {
            verify_execution_binding(root, &task_id, &previous.binding)?;
            if previous.binding.required_approvals != binding.required_approvals {
                return Err(blocked(
                    "EXECUTION_CHECKPOINT_CONFLICT",
                    "checkpoint ID already records different approval requirements",
                ));
            }
            return Ok(previous.clone());
        }
        if trajectory.execution_checkpoints.len() >= MAX_CHECKPOINTS {
            return Err(blocked(
                "EXECUTION_CHECKPOINT_LIMIT",
                "preserve this run in Git before starting a new governed run",
            ));
        }
        let checkpoint = ExecutionCheckpoint {
            checkpoint_id: checkpoint_id.into(),
            sequence: trajectory.execution_checkpoints.len() as u64 + 1,
            binding_digest: digest(&binding)?,
            binding,
        };
        trajectory.execution_checkpoints.push(checkpoint.clone());
        write_trajectory(root, &trajectory)?;
        Ok(checkpoint)
    })
}

/// Read-only continuation gate. This never marks work verified or mutates a
/// receipt, plan, task, approval, or external harness.
pub fn verify_execution_resume(
    root: &Path,
    run_id: &str,
    checkpoint_id: &str,
) -> Result<ExecutionCheckpoint, DecapodError> {
    governance_document::with_lock(root, || {
        let trajectory = load_trajectory(root, run_id)?;
        let checkpoint = trajectory
            .execution_checkpoints
            .iter()
            .find(|c| c.checkpoint_id == checkpoint_id)
            .ok_or_else(|| {
                blocked(
                    "EXECUTION_CHECKPOINT_MISSING",
                    "requested checkpoint is not in the current trajectory",
                )
            })?;
        if checkpoint.binding_digest != digest(&checkpoint.binding)? {
            return Err(blocked(
                "EXECUTION_CHECKPOINT_CORRUPT",
                "checkpoint binding digest does not match",
            ));
        }
        let task = trajectory
            .task_id
            .as_deref()
            .ok_or_else(|| blocked("EXECUTION_TASK_MISMATCH", "current task is missing"))?;
        verify_execution_binding(root, task, &checkpoint.binding)?;
        Ok(checkpoint.clone())
    })
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, DecapodError> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            params![table],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some())
}

fn capture_policy(
    root: &Path,
    task_id: &str,
    required: &[String],
    io: &mut CaptureBudget,
) -> Result<(String, Vec<ExecutionApprovalBinding>), DecapodError> {
    // Use the same shared policy owner as task mutation, not the workspace's
    // disposable projection. Store resolution is Decapod-owned.
    let owner = policy_owner(root, io)?;
    let store = owner.join(".decapod/data");
    let mut material = BTreeMap::new();
    let mut config_text = None;
    let mut override_text = None;
    for (label, input_root, path) in [
        (
            "accepted_config",
            owner.as_path(),
            owner.join(".decapod/config.toml"),
        ),
        ("workspace_config", root, root.join(".decapod/config.toml")),
        (
            "policy_override",
            owner.as_path(),
            owner.join(".decapod/OVERRIDE.md"),
        ),
        ("riskmap", owner.as_path(), store.join("RISKMAP.json")),
    ] {
        let bytes = read_policy_input(input_root, &path, io)?;
        material.insert(
            label,
            bytes
                .as_ref()
                .map(|bytes| format!("sha256:{:x}", Sha256::digest(bytes)))
                .unwrap_or_else(|| "absent".into()),
        );
        if matches!(label, "accepted_config" | "policy_override") {
            let text = bytes
                .map(String::from_utf8)
                .transpose()
                .map_err(|_| blocked("EXECUTION_POLICY_INVALID", "policy input is not UTF-8"))?;
            if label == "accepted_config" {
                config_text = text;
            } else {
                override_text = text;
            }
        }
    }
    let db = crate::plugins::policy::policy_db_path(&store);
    let mut zones = Vec::<(String, String, i64)>::new();
    let mut trust = Vec::<(String, String, String)>::new();
    let mut approvals = Vec::new();
    let mut ids = required.to_vec();
    if ids.iter().any(|id| {
        id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    }) {
        return Err(blocked(
            "EXECUTION_APPROVAL_INVALID",
            "approval reference must be a bounded identifier",
        ));
    }
    ids.sort();
    ids.dedup();
    if ids.len() > 64 {
        return Err(blocked(
            "EXECUTION_APPROVAL_LIMIT",
            "too many required approvals",
        ));
    }
    if db.exists() {
        let conn = crate::core::db::db_connect_for_validate(&db.to_string_lossy())?;
        ids.extend(crate::plugins::policy::continuation_policy_on_conn(
            &conn,
            &crate::current_agent_id(),
            RESUME_OPERATION,
            task_id,
            config_text.as_deref(),
            override_text.as_deref(),
        )?);
        ids.sort();
        ids.dedup();
        if ids.len() > 64 {
            return Err(blocked(
                "EXECUTION_APPROVAL_LIMIT",
                "too many required approvals",
            ));
        }
        if table_exists(&conn, "risk_zones")? {
            let mut statement = conn.prepare("SELECT substr(zone_name,1,513), substr(required_trust_level,1,513), requires_approval FROM risk_zones ORDER BY zone_name LIMIT 10001")?;
            for row in statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))? {
                zones.push(row?);
                if zones.len() > 10000 {
                    return Err(blocked(
                        "EXECUTION_POLICY_LIMIT",
                        "policy inventory exceeds bound",
                    ));
                }
            }
            if zones
                .iter()
                .any(|(zone, trust, _)| zone.len() > 512 || trust.len() > 512)
            {
                return Err(blocked(
                    "EXECUTION_POLICY_LIMIT",
                    "policy field exceeds bound",
                ));
            }
        }
        if table_exists(&conn, "agents")? {
            // Heartbeat/presence metadata is not approval authority.
            let mut statement = conn.prepare("SELECT substr(agent_id,1,513), substr(trust_level,1,513), substr(trust_granted_by,1,513) FROM agents ORDER BY agent_id LIMIT 10001")?;
            for row in statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))? {
                trust.push(row?);
                if trust.len() > 10000 {
                    return Err(blocked(
                        "EXECUTION_POLICY_LIMIT",
                        "trust inventory exceeds bound",
                    ));
                }
            }
            if trust.iter().any(|(actor, level, issuer)| {
                actor.len() > 512 || level.len() > 512 || issuer.len() > 512
            }) {
                return Err(blocked(
                    "EXECUTION_POLICY_LIMIT",
                    "trust field exceeds bound",
                ));
            }
        }
        if !ids.is_empty() && !table_exists(&conn, "approvals")? {
            return Err(blocked(
                "EXECUTION_APPROVAL_MISSING",
                "required approval authority is unavailable",
            ));
        }
        for id in ids {
            let record = conn.query_row(
                "SELECT substr(action_fingerprint,1,1025), substr(actor,1,1025), substr(ts,1,1025), substr(scope,1,1025), substr(expires_at,1,1025) FROM approvals WHERE approval_id=?1",
                params![id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, Option<String>>(4)?))
            ).optional()?.ok_or_else(|| blocked("EXECUTION_APPROVAL_MISSING", "required approval was revoked or is absent"))?;
            if [&record.0, &record.1, &record.2, &record.3]
                .iter()
                .any(|value| value.len() > 1024)
                || record.4.as_ref().is_some_and(|value| value.len() > 1024)
            {
                return Err(blocked(
                    "EXECUTION_POLICY_LIMIT",
                    "approval field exceeds bound",
                ));
            }
            let action = crate::plugins::policy::continuation_approval_action(
                RESUME_OPERATION,
                task_id,
                &record.3,
                &record.0,
            );
            if action.is_none() || record.1.trim().is_empty() {
                return Err(blocked(
                    "EXECUTION_APPROVAL_SCOPE",
                    "required approval must apply to trajectory.resume for this task in global scope with an identified issuer",
                ));
            }
            if !approval_live(record.4.as_deref()) {
                return Err(blocked(
                    "EXECUTION_APPROVAL_EXPIRED",
                    "required resume approval has expired",
                ));
            }
            approvals.push(ExecutionApprovalBinding {
                approval_id: id,
                record_digest: digest(&record)?,
            });
        }
    } else if !ids.is_empty() {
        return Err(blocked(
            "EXECUTION_APPROVAL_MISSING",
            "required policy datastore is unavailable",
        ));
    }
    Ok((digest(&(material, zones, trust))?, approvals))
}

fn read_policy_input(
    root: &Path,
    path: &Path,
    io: &mut CaptureBudget,
) -> Result<Option<Vec<u8>>, DecapodError> {
    let reader = crate::core::validation_epoch::input::BoundedEpochInput::new(root)?;
    let mut file = match reader.open_regular(path) {
        Ok(file) => file,
        Err(DecapodError::IoError(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 16384];
    loop {
        io.checkpoint()?;
        let length = file.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        if bytes.len() + length > 1024 * 1024 {
            return Err(blocked(
                "EXECUTION_POLICY_LIMIT",
                "policy input exceeds 1 MiB",
            ));
        }
        bytes.extend_from_slice(&buffer[..length]);
    }
    Ok(Some(bytes))
}

fn approval_live(expiry: Option<&str>) -> bool {
    crate::plugins::policy::approval_expiry_is_live(expiry)
}

fn git_path(root: &Path, bytes: Vec<u8>) -> Result<PathBuf, DecapodError> {
    let value = std::str::from_utf8(&bytes)
        .map_err(|_| {
            blocked(
                "EXECUTION_POLICY_ROOT",
                "Git returned a non-UTF-8 authority path",
            )
        })?
        .trim();
    if value.is_empty() {
        return Err(blocked(
            "EXECUTION_POLICY_ROOT",
            "Git authority path is empty",
        ));
    }
    let path = Path::new(value);
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    })
}

fn policy_owner(root: &Path, io: &mut CaptureBudget) -> Result<PathBuf, DecapodError> {
    if let Some(host) = crate::core::workspace::host_repo_from_canonical_workspace(root) {
        return Ok(host);
    }
    // Equivalent owner resolution to find_governance_root, with the same
    // sanitized, bounded Git adapter that captures the execution source.
    let common = git_path(root, io.git(root, &["rev-parse", "--git-common-dir"])?)?;
    let common = fs::canonicalize(common)?;
    if common.file_name().is_some_and(|name| name == ".git") {
        return common
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| blocked("EXECUTION_POLICY_ROOT", "Git authority root has no parent"));
    }
    Ok(common)
}

fn reject_grafts(root: &Path, io: &mut CaptureBudget) -> Result<(), DecapodError> {
    let grafts = git_path(
        root,
        io.git(root, &["rev-parse", "--git-path", "info/grafts"])?,
    )?;
    if grafts.exists() && fs::metadata(grafts)?.len() != 0 {
        return Err(blocked(
            "EXECUTION_HISTORY_CHANGED",
            "Git grafts cannot establish checkpoint ancestry",
        ));
    }
    Ok(())
}

struct CaptureBudget {
    deadline: Instant,
    remaining_bytes: u64,
}
impl CaptureBudget {
    fn new() -> Self {
        Self {
            deadline: Instant::now() + Duration::from_secs(60),
            remaining_bytes: 1024 * 1024 * 1024,
        }
    }
}
impl CaptureIo for CaptureBudget {
    fn checkpoint(&mut self) -> io::Result<()> {
        if Instant::now() >= self.deadline {
            return Err(io::Error::other("execution capture deadline exceeded"));
        }
        Ok(())
    }
    fn file_digest(&mut self, path: &Path) -> io::Result<String> {
        self.checkpoint()?;
        if !fs::symlink_metadata(path)?.is_file() {
            return Err(io::Error::other("execution material is not a regular file"));
        }
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let mut file = options.open(path)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::other("execution material is not a regular file"));
        }
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            self.checkpoint()?;
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            self.remaining_bytes = self
                .remaining_bytes
                .checked_sub(n as u64)
                .ok_or_else(|| io::Error::other("execution capture byte budget exceeded"))?;
            hash.update(&buffer[..n]);
        }
        Ok(format!("sha256:{:x}", hash.finalize()))
    }
    fn git(&mut self, root: &Path, args: &[&str]) -> io::Result<Vec<u8>> {
        self.checkpoint()?;
        let result = Command::new("git")
            .arg("--no-optional-locks")
            .args([
                "-c",
                "core.fsmonitor=false",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "core.untrackedCache=false",
                "-c",
                "protocol.allow=never",
                "-c",
                "gc.auto=0",
                "-c",
                "maintenance.auto=false",
            ])
            .args(args)
            .current_dir(root)
            .env_clear()
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ALLOW_PROTOCOL", "")
            .env("GIT_PROTOCOL_FROM_USER", "0")
            .bounded_capture(
                ProcessLimits {
                    timeout: self.deadline.saturating_duration_since(Instant::now()),
                    max_output_bytes: 16 * 1024 * 1024,
                },
                &Cancellation::default(),
            )?;
        if result.termination != Termination::Exited || !result.output.status.success() {
            return Err(io::Error::other(
                "bounded execution material Git query failed",
            ));
        }
        self.checkpoint()?;
        Ok(result.output.stdout)
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/core/execution_checkpoint_tests.rs"]
mod tests;
