//! Deterministic recovery of the old comma-split task-scope projection.
//! The task remains authoritative; this cannot edit its scope or grant proof.
use crate::{
    core::{
        custody::Custody, error::DecapodError, fleet_coord, governance_document, todo, workspace,
    },
    plan_governance,
};
use std::path::Path;

fn blocked(code: &str, reason: &str) -> DecapodError {
    DecapodError::ValidationError(format!("{code}: {reason}"))
}
fn ordered(mut paths: Vec<String>) -> Vec<String> {
    paths.sort();
    paths.dedup();
    paths
}

/// Called only from claimed-workbranch preparation with canonical task data.
/// Preserve a recoverable complete journal before correcting known generated
/// fragments. Any independently changed constraints require explicit review.
pub(crate) fn reconcile_task_scope_projection(
    root: &Path,
    task_id: &str,
    raw_scope: &str,
) -> Result<(), DecapodError> {
    let Some(canonical) = fleet_coord::declared_paths(raw_scope)
        .map_err(|reason| blocked("SCOPE_DECLARATION_INVALID", &reason))?
    else {
        return Ok(());
    };
    governance_document::with_lock(root, || {
        let mut artifact = super::load_trajectory_cookie(root)?.ok_or_else(|| {
            blocked(
                "SCOPE_PROJECTION_MISSING",
                "current task trajectory is absent",
            )
        })?;
        if artifact.task_id.as_deref() != Some(task_id) {
            return Err(blocked(
                "SCOPE_RECONCILIATION_TASK_MISMATCH",
                "the active journal belongs to another task; preserve its history",
            ));
        }
        let intent_id = artifact.intent_id.clone().ok_or_else(|| {
            blocked(
                "SCOPE_PROJECTION_MISSING",
                "current intent identity is absent",
            )
        })?;
        let record = artifact.custody.intents.get(&intent_id).ok_or_else(|| {
            blocked(
                "SCOPE_PROJECTION_MISSING",
                "current intent constraints are absent",
            )
        })?;
        let projected = ordered(artifact.repo_scope.clone());
        let constraints = ordered(record.constraints.clone());
        if projected == canonical && constraints == canonical {
            return Ok(());
        }
        let legacy = ordered(
            raw_scope
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| crate::core::path_policy::normalize_persisted_path(root, value))
                .collect(),
        );
        // This is a migration of one known producer bug, not a general scope
        // editor. Preserve independently authored projections without guessing.
        if projected != legacy && constraints != legacy {
            return Ok(());
        }
        if record.status.is_terminal()
            || (projected != legacy && projected != canonical)
            || (constraints != legacy && constraints != canonical)
        {
            return Err(blocked(
                "SCOPE_RECONCILIATION_AMBIGUOUS",
                "only the exact known comma-split projection is repairable automatically; preserve independent constraints and resolve their authority first",
            ));
        }
        let document = governance_document::load(root)?.ok_or_else(|| {
            blocked(
                "SCOPE_PROJECTION_MISSING",
                "current governance document is absent",
            )
        })?;
        governance_document::recoverable_in_git(root, &document).map_err(|_| {
            blocked("SCOPE_RECONCILIATION_ARCHIVE_REQUIRED", "checkpoint and commit the complete current governance journal, then retry workspace ensure or agent.init; no evidence was replaced")
        })?;
        let owner = workspace::get_main_repo_root(root)?;
        let store = owner.join(".decapod/data");
        let task = todo::get_task(&store, task_id)?
            .ok_or_else(|| blocked("SCOPE_SOURCE_MISSING", "canonical task is unavailable"))?;
        if task.scope != raw_scope {
            return Err(blocked(
                "SCOPE_SOURCE_CHANGED",
                "canonical task scope changed during preparation; reread it before retrying",
            ));
        }
        let mut plan = plan_governance::load_plan(root)?.ok_or_else(|| {
            blocked(
                "SCOPE_PLAN_MISSING",
                "initialize a task-bound plan before reconciliation",
            )
        })?;
        if !plan.todo_ids.iter().any(|id| id == task_id) {
            return Err(blocked(
                "SCOPE_RECONCILIATION_TASK_MISMATCH",
                "plan and trajectory must retain the same current task",
            ));
        }
        if constraints != canonical {
            let expected = record.constraints.clone();
            let mut custody = Custody::from_state(artifact.custody.clone());
            custody.reconcile_scope_projection(&intent_id, &expected, &canonical,
                format!("Deterministic task.scope projection repair for {task_id} at task revision {}; original instruction and prior events retained; no approval or proof granted", task.revision))
                .map_err(|error| blocked("SCOPE_RECONCILIATION_AMBIGUOUS", &error.to_string()))?;
            artifact.custody = custody.into_state();
        }
        artifact.repo_scope = canonical;
        artifact = artifact
            .with_recomputed_hash()
            .map_err(|error| blocked("SCOPE_PROJECTION_INVALID", &error.to_string()))?;
        plan_governance::invalidate_plan_acceptance(&mut plan);
        let current = todo::get_task(&store, task_id)?.ok_or_else(|| {
            blocked(
                "SCOPE_SOURCE_MISSING",
                "canonical task disappeared during preparation",
            )
        })?;
        if current.scope != task.scope || current.revision != task.revision {
            return Err(blocked(
                "SCOPE_SOURCE_CHANGED",
                "task authority changed during reconciliation; retry from its current record",
            ));
        }
        governance_document::write_sections(
            root,
            &[
                (
                    "trajectory",
                    Some(
                        serde_json::to_value(artifact)
                            .map_err(|e| blocked("SCOPE_PROJECTION_INVALID", &e.to_string()))?,
                    ),
                ),
                (
                    "plan",
                    Some(
                        serde_json::to_value(plan)
                            .map_err(|e| blocked("SCOPE_PROJECTION_INVALID", &e.to_string()))?,
                    ),
                ),
            ],
        )
    })
}
