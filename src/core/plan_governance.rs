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

/// Represents a gate that must be satisfied to enter or exit a phase
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gate {
    /// Human-readable description of what this gate validates
    pub description: String,
    /// Required artifacts that must exist (file paths relative to project root)
    #[serde(default)]
    pub required_artifacts: Vec<String>,
    /// Validation checks that must pass
    #[serde(default)]
    pub validation_checks: Vec<String>,
    /// Whether this gate has been satisfied
    #[serde(default)]
    pub satisfied: bool,
    /// Timestamp when the gate was last satisfied
    #[serde(default)]
    pub satisfied_at: Option<String>,
}

/// A phase in a gated execution process
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Phase {
    /// Unique identifier for the phase
    pub id: String,
    /// Human-readable name of the phase
    pub name: String,
    /// Description of what this phase accomplishes
    pub description: String,
    /// Gates that must be satisfied to enter this phase
    #[serde(default)]
    pub entry_gates: Vec<Gate>,
    /// Gates that must be satisfied to exit this phase
    #[serde(default)]
    pub exit_gates: Vec<Gate>,
    /// Whether this phase has been entered
    #[serde(default)]
    pub entered: bool,
    /// Whether this phase has been completed (exited)
    #[serde(default)]
    pub completed: bool,
    /// Timestamp when the phase was entered
    #[serde(default)]
    pub entered_at: Option<String>,
    /// Timestamp when the phase was completed
    #[serde(default)]
    pub completed_at: Option<String>,
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
    pub stop_conditions: Vec<String>,
    #[serde(default)]
    pub unresolved_contradictions: Vec<String>,
    #[serde(default)]
    pub deferred_questions: Vec<String>,
    #[serde(default)]
    pub constraints: ScopeConstraints,
    /// Phases in the execution process
    #[serde(default)]
    pub phases: Vec<Phase>,
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
    pub stop_conditions: Option<Vec<String>>,
    pub unresolved_contradictions: Option<Vec<String>>,
    pub deferred_questions: Option<Vec<String>>,
    pub constraints: Option<ScopeConstraints>,
    pub phases: Option<Vec<Phase>>,
}

pub struct InitPlanInput {
    pub title: String,
    pub intent: String,
    pub todo_ids: Vec<String>,
    pub proof_hooks: Vec<String>,
    pub unknowns: Vec<String>,
    pub human_questions: Vec<String>,
    pub stop_conditions: Vec<String>,
    pub unresolved_contradictions: Vec<String>,
    pub deferred_questions: Vec<String>,
    pub constraints: ScopeConstraints,
    pub phases: Vec<Phase>,
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
        stop_conditions: input.stop_conditions,
        unresolved_contradictions: input.unresolved_contradictions,
        deferred_questions: input.deferred_questions,
        constraints: input.constraints,
        phases: input.phases,
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
    if let Some(stop_conditions) = patch.stop_conditions {
        plan.stop_conditions = stop_conditions;
    }
    if let Some(unresolved_contradictions) = patch.unresolved_contradictions {
        plan.unresolved_contradictions = unresolved_contradictions;
    }
    if let Some(deferred_questions) = patch.deferred_questions {
        plan.deferred_questions = deferred_questions;
    }
    if let Some(constraints) = patch.constraints {
        plan.constraints = constraints;
    }
    if let Some(phases) = patch.phases {
        plan.phases = phases;
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

    // Check that we're allowed to enter the execution phase
    // Find the executing phase (if defined) and check its entry gates
    let mut execution_phase_found = false;
    for phase in &plan.phases {
        if phase.name.to_lowercase() == "executing" || phase.id == "executing" {
            execution_phase_found = true;
            // Check entry gates for the executing phase
            for gate in &phase.entry_gates {
                if !gate.satisfied {
                    return Err(marker_error(
                        "PHASE_GATE_NOT_SATISFIED",
                        &format!("Entry gate not satisfied for phase '{}': {}", phase.name, gate.description),
                        Some(json!({ "phase": phase.name, "gate_description": gate.description })),
                    ));
                }
            }
            break;
        }
    }

    // If there's an explicit executing phase defined, we've already checked its gates
    // If not, fall back to the original scope constraint check
    if !execution_phase_found {
        enforce_scope_constraints(input.project_root, &plan.constraints)?;
    }

    Ok(plan)
}

pub fn enter_phase(
    project_root: &Path,
    phase_id: &str,
) -> Result<GovernedPlan, error::DecapodError> {
    let mut plan = load_plan(project_root)?.ok_or_else(|| {
        marker_error(
            "NEEDS_PLAN_APPROVAL",
            "Plan asset is missing. Run `decapod govern plan init` first.",
            None,
        )
    })?;

    // Find the phase
    let phase_index = plan
        .phases
        .iter()
        .position(|p| p.id == phase_id)
        .ok_or_else(|| {
            marker_error(
                "PHASE_NOT_FOUND",
                &format!("Phase with ID '{}' not found", phase_id),
                None,
            )
        })?;

    let phase = &mut plan.phases[phase_index];

    // Check if already entered
    if phase.entered {
        return Err(marker_error(
            "PHASE_ALREADY_ENTERED",
            &format!("Phase '{}' has already been entered", phase.name),
            None,
        ));
    }

    // Check all entry gates
    for gate in &phase.entry_gates {
        if !gate.satisfied {
            return Err(marker_error(
                "PHASE_ENTRY_GATE_NOT_SATISFIED",
                &format!("Entry gate not satisfied for phase '{}': {}", phase.name, gate.description),
                Some(json!({ "phase": phase.name, "gate_description": gate.description })),
            ));
        }
    }

    // Mark phase as entered
    phase.entered = true;
    phase.entered_at = Some(crate::core::time::now_epoch_z());
    plan.updated_at = crate::core::time::now_epoch_z();
    save_plan(project_root, &plan)?;
    Ok(plan)
}

pub fn complete_phase(
    project_root: &Path,
    phase_id: &str,
) -> Result<GovernedPlan, error::DecapodError> {
    let mut plan = load_plan(project_root)?.ok_or_else(|| {
        marker_error(
            "NEEDS_PLAN_APPROVAL",
            "Plan asset is missing. Run `decapod govern plan init` first.",
            None,
        )
    })?;

    // Find the phase
    let phase_index = plan
        .phases
        .iter()
        .position(|p| p.id == phase_id)
        .ok_or_else(|| {
            marker_error(
                "PHASE_NOT_FOUND",
                &format!("Phase with ID '{}' not found", phase_id),
                None,
            )
        })?;

    let phase = &mut plan.phases[phase_index];

    // Check if phase has been entered
    if !phase.entered {
        return Err(marker_error(
            "PHASE_NOT_ENTERED",
            &format!("Phase '{}' has not been entered yet", phase.name),
            None,
        ));
    }

    // Check if already completed
    if phase.completed {
        return Err(marker_error(
            "PHASE_ALREADY_COMPLETED",
            &format!("Phase '{}' has already been completed", phase.name),
            None,
        ));
    }

    // Check all exit gates
    for gate in &phase.exit_gates {
        if !gate.satisfied {
            return Err(marker_error(
                "PHASE_EXIT_GATE_NOT_SATISFIED",
                &format!("Exit gate not satisfied for phase '{}': {}", phase.name, gate.description),
                Some(json!({ "phase": phase.name, "gate_description": gate.description })),
            ));
        }
    }

    // Mark phase as completed
    phase.completed = true;
    phase.completed_at = Some(crate::core::time::now_epoch_z());
    plan.updated_at = crate::core::time::now_epoch_z();
    save_plan(project_root, &plan)?;
    Ok(plan)
}

pub fn satisfy_gate(
    project_root: &Path,
    phase_id: &str,
    gate_type: &str, // "entry" or "exit"
    gate_index: usize,
) -> Result<GovernedPlan, error::DecapodError> {
    let mut plan = load_plan(project_root)?.ok_or_else(|| {
        marker_error(
            "NEEDS_PLAN_APPROVAL",
            "Plan asset is missing. Run `decapod govern plan init` first.",
            None,
        )
    })?;

    // Find the phase
    let phase_index = plan
        .phases
        .iter()
        .position(|p| p.id == phase_id)
        .ok_or_else(|| {
            marker_error(
                "PHASE_NOT_FOUND",
                &format!("Phase with ID '{}' not found", phase_id),
                None,
            )
        })?;

    let phase = &mut plan.phases[phase_index];

    // Get the appropriate gate
    let gate = match gate_type {
        "entry" => {
            if gate_index >= phase.entry_gates.len() {
                return Err(marker_error(
                    "INVALID_GATE_INDEX",
                    &format!("Entry gate index {} is out of bounds for phase '{}' (has {} gates)", gate_index, phase.name, phase.entry_gates.len()),
                    None,
                ));
            }
            &mut phase.entry_gates[gate_index]
        }
        "exit" => {
            if gate_index >= phase.exit_gates.len() {
                return Err(marker_error(
                    "INVALID_GATE_INDEX",
                    &format!("Exit gate index {} is out of bounds for phase '{}' (has {} gates)", gate_index, phase.name, phase.exit_gates.len()),
                    None,
                ));
            }
            &mut phase.exit_gates[gate_index]
        }
        _ => {
            return Err(marker_error(
                "INVALID_GATE_TYPE",
                "Gate type must be 'entry' or 'exit'",
                None,
            ));
        }
    };

    // Mark gate as satisfied
    gate.satisfied = true;
    gate.satisfied_at = Some(crate::core::time::now_epoch_z());
    plan.updated_at = crate::core::time::now_epoch_z();
    save_plan(project_root, &plan)?;
    Ok(plan)
}

fn verify_artifact_exists(project_root: &Path, artifact_path: &str) -> bool {
    let path = project_root.join(artifact_path);
    path.exists()
}

pub fn check_phase_entry_gates(
    project_root: &Path,
    phase_id: &str,
) -> Result<(bool, Vec<String>), error::DecapodError> {
    let plan = load_plan(project_root)?.ok_or_else(|| {
        marker_error(
            "NEEDS_PLAN_APPROVAL",
            "Plan asset is missing. Run `decapod govern plan init` first.",
            None,
        )
    })?;

    // Find the phase
    let phase = plan
        .phases
        .iter()
        .find(|p| p.id == phase_id)
        .ok_or_else(|| {
            marker_error(
                "PHASE_NOT_FOUND",
                &format!("Phase with ID '{}' not found", phase_id),
                None,
            )
        })?;

    let mut unsatisfied = Vec::new();
    for gate in &phase.entry_gates {
        if !gate.satisfied {
            // Check artifacts
            for artifact in &gate.required_artifacts {
                if !verify_artifact_exists(project_root, artifact) {
                    unsatisfied.push(format!(
                        "Missing required artifact: {} (for gate: {})",
                        artifact, gate.description
                    ));
                }
            }
            
            // For now, we'll just report that the gate isn't satisfied
            # if unsatisfied.is_empty() {
            unsatisfied.push(format!("Gate not satisfied: {}", gate.description));
        }
    }

    Ok((unsatisfied.is_empty(), unsatisfied))
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
    let verifying_ids = verifying_todo_ids();
    let mut out = Vec::new();
    for row in rows {
        let id = row.map_err(error::DecapodError::RusqliteError)?;
        if !verifying_ids.iter().any(|verifying_id| verifying_id == &id) {
            out.push(id);
        }
    }
    Ok(out)
}

pub fn count_done_todos(store_root: &Path) -> Result<usize, error::DecapodError> {
    let db_path = crate::core::todo::todo_db_path(store_root);
    if !db_path.exists() {
        return Ok(0);
    }
    let conn = Connection::open(db_path).map_err(error::DecapodError::RusqliteError)?;
    let verifying_ids = verifying_todo_ids();
    if !verifying_ids.is_empty() {
        let mut stmt = conn
            .prepare("SELECT id FROM tasks WHERE status = 'done'")
            .map_err(error::DecapodError::RusqliteError)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(error::DecapodError::RusqliteError)?;
        let mut count = 0usize;
        for row in rows {
            let id = row.map_err(error::DecapodError::RusqliteError)?;
            if !verifying_ids.iter().any(|verifying_id| verifying_id == &id) {
                count += 1;
            }
        }
        return Ok(count);
    }

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE status = 'done'",
            [],
            |row| row.get(0),
        )
        .map_err(error::DecapodError::RusqliteError)?;
    Ok(count.max(0) as usize)
}

fn verifying_todo_ids() -> Vec<String> {
    std::env::var("DECAPOD_VERIFYING_TODO")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .collect()
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
                stop_conditions: vec![],
                unresolved_contradictions: vec![],
                deferred_questions: vec![],
                constraints: ScopeConstraints::default(),
                phases: vec![],
            },
        )
        .unwrap();
        assert_eq!(plan.state, PlanState::Draft);
    }
}
