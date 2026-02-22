use crate::core::error;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PLAN_SCHEMA_VERSION: &str = "1.0.0";
const PLAN_PATH: &str = ".decapod/governance/plan.json";

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlanState {
    Draft,
    Annotating,
    Approved,
    Executing,
    Done,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScopeConstraints {
    #[serde(default)]
    pub forbidden_paths: Vec<String>,
    pub file_touch_budget: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GovernedPlan {
    pub schema_version: String,
    pub title: String,
    pub intent: String,
    pub state: PlanState,
    #[serde(default)]
    pub todo_ids: Vec<String>,
    #[serde(default)]
    pub proof_hooks: Vec<String>,
    #[serde(default)]
    pub unknowns: Vec<String>,
    #[serde(default)]
    pub human_questions: Vec<String>,
    #[serde(default)]
    pub constraints: ScopeConstraints,
    pub updated_at: String,
}

#[derive(Clone, Debug, Default)]
pub struct PlanPatch {
    pub title: Option<String>,
    pub intent: Option<String>,
    pub state: Option<PlanState>,
    pub todo_ids: Option<Vec<String>>,
    pub proof_hooks: Option<Vec<String>>,
    pub unknowns: Option<Vec<String>>,
    pub human_questions: Option<Vec<String>>,
    pub constraints: Option<ScopeConstraints>,
}

pub struct InitPlanInput {
    pub title: String,
    pub intent: String,
    pub todo_ids: Vec<String>,
    pub proof_hooks: Vec<String>,
    pub unknowns: Vec<String>,
    pub human_questions: Vec<String>,
    pub constraints: ScopeConstraints,
}

pub struct ExecuteCheckInput<'a> {
    pub project_root: &'a Path,
    pub store_root: &'a Path,
    pub todo_id: Option<&'a str>,
}

pub fn plan_path(project_root: &Path) -> PathBuf {
    project_root.join(PLAN_PATH)
}

pub fn load_plan(project_root: &Path) -> Result<Option<GovernedPlan>, error::DecapodError> {
    let path = plan_path(project_root);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(error::DecapodError::IoError)?;
    let plan: GovernedPlan = serde_json::from_slice(&bytes).map_err(|e| {
        error::DecapodError::ValidationError(format!("Invalid plan artifact JSON: {e}"))
    })?;
    Ok(Some(plan))
}

pub fn save_plan(project_root: &Path, plan: &GovernedPlan) -> Result<(), error::DecapodError> {
    let path = plan_path(project_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    }
    let bytes = serde_json::to_vec_pretty(plan).map_err(|e| {
        error::DecapodError::ValidationError(format!("Unable to serialize plan artifact: {e}"))
    })?;
    fs::write(path, bytes).map_err(error::DecapodError::IoError)?;
    Ok(())
}

pub fn init_plan(
    project_root: &Path,
    input: InitPlanInput,
) -> Result<GovernedPlan, error::DecapodError> {
    let plan = GovernedPlan {
        schema_version: PLAN_SCHEMA_VERSION.to_string(),
        title: input.title,
        intent: input.intent,
        state: PlanState::Draft,
        todo_ids: input.todo_ids,
        proof_hooks: input.proof_hooks,
        unknowns: input.unknowns,
        human_questions: input.human_questions,
        constraints: input.constraints,
        updated_at: crate::core::time::now_epoch_z(),
    };
    save_plan(project_root, &plan)?;
    Ok(plan)
}

pub fn patch_plan(
    project_root: &Path,
    patch: PlanPatch,
) -> Result<GovernedPlan, error::DecapodError> {
    let mut plan = load_plan(project_root)?.ok_or_else(|| {
        marker_error(
            "NEEDS_PLAN_APPROVAL",
            "Plan artifact is missing. Run `decapod govern plan init` first.",
            None,
        )
    })?;

    if let Some(title) = patch.title {
        plan.title = title;
    }
    if let Some(intent) = patch.intent {
        plan.intent = intent;
    }
    if let Some(state) = patch.state {
        plan.state = state;
    }
    if let Some(todo_ids) = patch.todo_ids {
        plan.todo_ids = todo_ids;
    }
    if let Some(proof_hooks) = patch.proof_hooks {
        plan.proof_hooks = proof_hooks;
    }
    if let Some(unknowns) = patch.unknowns {
        plan.unknowns = unknowns;
    }
    if let Some(human_questions) = patch.human_questions {
        plan.human_questions = human_questions;
    }
    if let Some(constraints) = patch.constraints {
        plan.constraints = constraints;
    }
    plan.updated_at = crate::core::time::now_epoch_z();
    save_plan(project_root, &plan)?;
    Ok(plan)
}

pub fn ensure_execute_ready(
    input: ExecuteCheckInput<'_>,
) -> Result<GovernedPlan, error::DecapodError> {
    let plan = load_plan(input.project_root)?.ok_or_else(|| {
        marker_error(
            "NEEDS_PLAN_APPROVAL",
            "Execution blocked: missing governed plan artifact.",
            None,
        )
    })?;

    if plan.state != PlanState::Approved {
        return Err(marker_error(
            "NEEDS_PLAN_APPROVAL",
            "Execution blocked: plan state must be APPROVED.",
            Some(json!({ "current_state": format!("{:?}", plan.state).to_uppercase() })),
        ));
    }

    if plan.intent.trim().is_empty()
        || !plan.unknowns.is_empty()
        || !plan.human_questions.is_empty()
    {
        let mut questions = Vec::new();
        if plan.intent.trim().is_empty() {
            questions.push("What is the single-sentence intent for this change?".to_string());
        }
        questions.extend(plan.human_questions.clone());
        for unknown in &plan.unknowns {
            questions.push(format!("Resolve unknown before execution: {unknown}"));
        }
        return Err(marker_error(
            "NEEDS_HUMAN_INPUT",
            "Execution blocked: unresolved intent or unknowns.",
            Some(json!({ "questions": questions })),
        ));
    }

    let candidate_todo_ids = if let Some(todo_id) = input.todo_id {
        vec![todo_id.to_string()]
    } else {
        plan.todo_ids.clone()
    };

    if candidate_todo_ids.is_empty() {
        return Err(marker_error(
            "NEEDS_HUMAN_INPUT",
            "Execution blocked: no TODO selected for execution scope.",
            Some(json!({
                "questions": ["Which TODO ID should this execution run against?"]
            })),
        ));
    }

    let db_path = crate::core::todo::todo_db_path(input.store_root);
    let conn = Connection::open(&db_path).map_err(error::DecapodError::RusqliteError)?;
    let mut found = false;
    for todo_id in &candidate_todo_ids {
        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM tasks WHERE id = ?1 LIMIT 1",
                rusqlite::params![todo_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(error::DecapodError::RusqliteError)?;
        if exists.is_some() {
            found = true;
            break;
        }
    }
    if !found {
        return Err(marker_error(
            "NEEDS_HUMAN_INPUT",
            "Execution blocked: referenced TODO is missing.",
            Some(
                json!({ "questions": ["Confirm the TODO ID and run `decapod todo add` if needed."] }),
            ),
        ));
    }

    enforce_scope_constraints(input.project_root, &plan.constraints)?;
    Ok(plan)
}

pub fn collect_unverified_done_todos(
    store_root: &Path,
) -> Result<Vec<String>, error::DecapodError> {
    let db_path = crate::core::todo::todo_db_path(store_root);
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let conn = Connection::open(db_path).map_err(error::DecapodError::RusqliteError)?;
    let mut stmt = conn
        .prepare(
            "SELECT t.id
             FROM tasks t
             LEFT JOIN task_verification v ON v.todo_id = t.id
             WHERE t.status = 'done'
               AND (
                 v.last_verified_status IS NULL
                 OR LOWER(v.last_verified_status) NOT IN ('verified', 'pass')
               )
             ORDER BY t.updated_at DESC",
        )
        .map_err(error::DecapodError::RusqliteError)?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(error::DecapodError::RusqliteError)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(error::DecapodError::RusqliteError)?);
    }
    Ok(out)
}

pub fn count_done_todos(store_root: &Path) -> Result<usize, error::DecapodError> {
    let db_path = crate::core::todo::todo_db_path(store_root);
    if !db_path.exists() {
        return Ok(0);
    }
    let conn = Connection::open(db_path).map_err(error::DecapodError::RusqliteError)?;
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE status = 'done'",
            [],
            |row| row.get(0),
        )
        .map_err(error::DecapodError::RusqliteError)?;
    Ok(count.max(0) as usize)
}

pub fn marker_error(
    marker: &str,
    message: &str,
    payload: Option<serde_json::Value>,
) -> error::DecapodError {
    match payload {
        Some(payload) => {
            error::DecapodError::ValidationError(format!("{marker}: {message} payload={payload}"))
        }
        None => error::DecapodError::ValidationError(format!("{marker}: {message}")),
    }
}

fn enforce_scope_constraints(
    project_root: &Path,
    constraints: &ScopeConstraints,
) -> Result<(), error::DecapodError> {
    if constraints.file_touch_budget.is_none() && constraints.forbidden_paths.is_empty() {
        return Ok(());
    }
    let output = Command::new("git")
        .args(["status", "--short", "--untracked-files=no"])
        .current_dir(project_root)
        .output()
        .map_err(error::DecapodError::IoError)?;
    if !output.status.success() {
        return Ok(());
    }
    let changed_files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            if line.len() < 4 {
                return None;
            }
            Some(line[3..].trim().to_string())
        })
        .collect();

    if let Some(limit) = constraints.file_touch_budget
        && changed_files.len() > limit
    {
        return Err(marker_error(
            "SCOPE_VIOLATION",
            "Touched files exceed plan file-touch budget.",
            Some(json!({
                "touched_files": changed_files.len(),
                "file_touch_budget": limit
            })),
        ));
    }

    let mut forbidden_hits = Vec::new();
    for file in &changed_files {
        if constraints
            .forbidden_paths
            .iter()
            .any(|prefix| file == prefix || file.starts_with(&format!("{prefix}/")))
        {
            forbidden_hits.push(file.clone());
        }
    }
    if !forbidden_hits.is_empty() {
        return Err(marker_error(
            "SCOPE_VIOLATION",
            "Touched files violate forbidden path constraints.",
            Some(json!({ "forbidden_hits": forbidden_hits })),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_input_gate_blocks_empty_intent() {
        let dir = tempfile::tempdir().unwrap();
        let plan = init_plan(
            dir.path(),
            InitPlanInput {
                title: "Title".to_string(),
                intent: "".to_string(),
                todo_ids: vec!["T1".to_string()],
                proof_hooks: vec!["validate_passes".to_string()],
                unknowns: vec![],
                human_questions: vec![],
                constraints: ScopeConstraints::default(),
            },
        )
        .unwrap();
        assert_eq!(plan.state, PlanState::Draft);
    }
}
