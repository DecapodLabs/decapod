//! Fail-closed reconciliation for fragmented explicit `paths:[...]` scope projections.
//! Preserves recorded intent, custody history, checks, and checkpoints while requiring
//! the prior journal to be archived in Git before repairing scope projections.
use super::*;

#[derive(Clone, Debug, Serialize)]
pub struct ScopeReconciliationReport {
    pub status: String,
    pub marker: String,
    pub task_id: String,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
    pub canonical_scope: Vec<String>,
    pub previous_repo_scope: Vec<String>,
    pub repaired_repo_scope: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_commit: Option<String>,
}

fn normalize_sorted(root: &Path, items: &[String]) -> Vec<String> {
    let mut out: Vec<String> = items
        .iter()
        .map(|item| crate::core::path_policy::normalize_persisted_path(root, item))
        .collect();
    out.sort();
    out.dedup();
    out
}

fn resolve_unambiguous_task_id(
    trajectory: &crate::core::trajectory::TrajectoryArtifact,
    plan: Option<&crate::plan_governance::GovernedPlan>,
    explicit_task_id: Option<&str>,
) -> Result<String, DecapodError> {
    let plan_todos = plan.map(|p| p.todo_ids.as_slice()).unwrap_or(&[]);
    if let Some(explicit) = explicit_task_id {
        let explicit = explicit.trim();
        if explicit.is_empty() {
            return Err(invalid(
                "SCOPE_RECONCILIATION_TASK_REQUIRED: explicit --task-id must not be empty",
            ));
        }
        if let Some(traj_task) = trajectory.task_id.as_deref()
            && traj_task != explicit
        {
            return Err(invalid(format!(
                "SCOPE_RECONCILIATION_AMBIGUOUS_TASK: trajectory binds task '{traj_task}', which conflicts with requested '{explicit}'"
            )));
        }
        if !plan_todos.is_empty() && !plan_todos.iter().any(|id| id == explicit) {
            return Err(invalid(format!(
                "SCOPE_RECONCILIATION_AMBIGUOUS_TASK: governed plan todo_ids {:?} do not include requested '{explicit}'",
                plan_todos
            )));
        }
        return Ok(explicit.to_string());
    }

    if let Some(traj_task) = trajectory.task_id.as_deref() {
        if !plan_todos.is_empty() && (plan_todos.len() != 1 || plan_todos[0] != traj_task) {
            return Err(invalid(format!(
                "SCOPE_RECONCILIATION_AMBIGUOUS_TASK: trajectory task '{traj_task}' disagrees with plan todo_ids {:?}",
                plan_todos
            )));
        }
        return Ok(traj_task.to_string());
    }

    if plan_todos.len() == 1 {
        return Ok(plan_todos[0].clone());
    }

    Err(invalid(
        "SCOPE_RECONCILIATION_AMBIGUOUS_TASK: cannot determine a single task_id from trajectory or plan metadata; pass --task-id or resolve conflicting todo bindings",
    ))
}

fn reconcile_scope_vec(
    root: &Path,
    existing: &[String],
    canonical: &[String],
    legacy: &[String],
) -> Result<Option<Vec<String>>, DecapodError> {
    let existing_norm = normalize_sorted(root, existing);
    if existing_norm == canonical {
        return Ok(None);
    }
    if !legacy.is_empty()
        && legacy != canonical
        && legacy
            .iter()
            .all(|fragment| existing_norm.contains(fragment))
    {
        let mut repaired: Vec<String> = existing_norm
            .into_iter()
            .filter(|entry| !legacy.contains(entry))
            .collect();
        repaired.extend(canonical.iter().cloned());
        repaired.sort();
        repaired.dedup();
        if repaired
            .iter()
            .any(|entry| entry.starts_with("paths:") || entry.contains('"'))
        {
            return Err(invalid(
                "SCOPE_RECONCILIATION_AMBIGUOUS: residual scope fragments remain after replacing legacy task fragments; existing governance journal was preserved",
            ));
        }
        return Ok(Some(repaired));
    }
    Err(invalid(
        "SCOPE_RECONCILIATION_AMBIGUOUS: recorded governance scope entries do not match the task's canonical scope or legacy comma-split fragments; existing governance journal was preserved",
    ))
}

/// Reconcile fragmented `paths:[...]` scope projections in `.decapod/governance.json`
/// (`sections.trajectory.repo_scope` and `intents.<id>.constraints`) from the single
/// authoritative todo record, after proving the prior journal is committed in Git.
pub fn reconcile_scope_projections(
    root: &Path,
    store_root: &Path,
    explicit_task_id: Option<&str>,
    explicit_run_id: Option<&str>,
) -> Result<ScopeReconciliationReport, DecapodError> {
    with_lock(root, || {
        let mut document = load(root)?.ok_or_else(|| {
            invalid("SCOPE_RECONCILIATION_MISSING_JOURNAL: no .decapod/governance.json document exists to reconcile")
        })?;
        let trajectory_value = get_section(&document, "trajectory")?.ok_or_else(|| {
            invalid("SCOPE_RECONCILIATION_MISSING_TRAJECTORY: no trajectory section exists to reconcile")
        })?;
        let mut trajectory: crate::core::trajectory::TrajectoryArtifact =
            serde_json::from_value(trajectory_value).map_err(|e| invalid(e.to_string()))?;
        if let Some(expected_run_id) = explicit_run_id
            && trajectory.run_id != expected_run_id
        {
            return Err(invalid(format!(
                "SCOPE_RECONCILIATION_RUN_ID_MISMATCH: active trajectory run_id '{}' does not match requested '{expected_run_id}'",
                trajectory.run_id
            )));
        }

        let mut plan: Option<crate::plan_governance::GovernedPlan> =
            get_section(&document, "plan")?
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| invalid(format!("Invalid governed plan: {e}")))?;

        let task_id = resolve_unambiguous_task_id(&trajectory, plan.as_ref(), explicit_task_id)?;
        if !crate::core::todo::todo_db_path(store_root).exists() {
            return Err(invalid(format!(
                "TODO_PROJECTION_MISSING: todo store missing at {}; cannot verify authoritative scope for '{task_id}'",
                store_root.display()
            )));
        }
        let task = crate::core::todo::get_task(store_root, &task_id)?.ok_or_else(|| {
            invalid(format!(
                "TODO_PROJECTION_MISSING: task '{task_id}' not found in {}",
                store_root.display()
            ))
        })?;

        let parsed_canonical = crate::core::fleet_coord::parse_scope_entries(&task.scope)
            .map_err(DecapodError::ValidationError)?;
        let canonical_scope = normalize_sorted(root, &parsed_canonical);
        let legacy_raw: Vec<String> = task
            .scope
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        let legacy_fragments = normalize_sorted(root, &legacy_raw);

        let previous_repo_scope = normalize_sorted(root, &trajectory.repo_scope);
        let intent_id = trajectory
            .intent_id
            .clone()
            .unwrap_or_else(|| format!("intent:{}", trajectory.run_id));
        let new_repo_scope = reconcile_scope_vec(
            root,
            &previous_repo_scope,
            &canonical_scope,
            &legacy_fragments,
        )?;
        let new_constraints = match trajectory.custody.intents.get(&intent_id) {
            Some(intent) => reconcile_scope_vec(
                root,
                &intent.constraints,
                &canonical_scope,
                &legacy_fragments,
            )?,
            None => None,
        };

        if new_repo_scope.is_none() && new_constraints.is_none() {
            let plan_state = plan
                .as_ref()
                .map(|p| format!("{:?}", p.state).to_uppercase());
            return Ok(ScopeReconciliationReport {
                status: "unchanged".to_string(),
                marker: "SCOPE_ALREADY_CANONICAL".to_string(),
                task_id,
                run_id: trajectory.run_id,
                intent_id: trajectory.intent_id,
                canonical_scope,
                previous_repo_scope: previous_repo_scope.clone(),
                repaired_repo_scope: previous_repo_scope,
                plan_state,
                archived_commit: None,
            });
        }

        recoverable_in_git(root, &document).map_err(|err| {
            invalid(format!(
                "SCOPE_RECONCILIATION_ARCHIVE_REQUIRED: preserve and commit the complete current governance journal before reconciling fragmented scope projections; no trajectory, plan, custody, or historical evidence was modified. {err}"
            ))
        })?;
        let archived_commit =
            immutable_archive_git(root, &["rev-parse", "--verify", "HEAD^{commit}"])?;

        if let Some(repaired) = new_repo_scope {
            trajectory.repo_scope = repaired;
        }
        if let Some(repaired_constraints) = new_constraints
            && let Some(intent) = trajectory.custody.intents.get_mut(&intent_id)
        {
            intent.constraints = repaired_constraints;
        }
        let updated_trajectory = trajectory
            .with_recomputed_hash()
            .map_err(|e| invalid(e.to_string()))?;
        let repaired_repo_scope = updated_trajectory.repo_scope.clone();
        let trajectory_json =
            serde_json::to_value(&updated_trajectory).map_err(|e| invalid(e.to_string()))?;
        set_section(&mut document, "trajectory", trajectory_json)?;

        let plan_state = if let Some(mut governed_plan) = plan.take() {
            crate::plan_governance::invalidate_plan_acceptance(&mut governed_plan);
            governed_plan.updated_at = crate::core::time::now_epoch_z();
            let state_label = format!("{:?}", governed_plan.state).to_uppercase();
            let plan_json =
                serde_json::to_value(&governed_plan).map_err(|e| invalid(e.to_string()))?;
            set_section(&mut document, "plan", plan_json)?;
            Some(state_label)
        } else {
            None
        };

        prune_epochs(&mut document)?;
        save(root, &document)?;

        Ok(ScopeReconciliationReport {
            status: "reconciled".to_string(),
            marker: "SCOPE_RECONCILED".to_string(),
            task_id,
            run_id: updated_trajectory.run_id,
            intent_id: updated_trajectory.intent_id,
            canonical_scope,
            previous_repo_scope,
            repaired_repo_scope,
            plan_state,
            archived_commit: Some(archived_commit),
        })
    })
}
