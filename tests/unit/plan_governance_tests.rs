// Moved from src/decapod/plan_governance.rs
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

fn phase(id: &str) -> Phase {
    Phase {
        id: id.to_string(),
        name: id.to_string(),
        description: format!("{id} work"),
        entry_gates: vec![],
        exit_gates: vec![],
        entered: false,
        completed: false,
        entered_at: None,
        completed_at: None,
    }
}

#[test]
fn phases_are_entered_in_order_and_done_requires_all_phases() {
    let dir = tempfile::tempdir().unwrap();
    init_plan(
        dir.path(),
        InitPlanInput {
            title: "ordered".to_string(),
            intent: "prove ordered execution".to_string(),
            todo_ids: vec![],
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
    add_phase(dir.path(), phase("plan")).unwrap();
    add_phase(dir.path(), phase("build")).unwrap();
    patch_plan(
        dir.path(),
        PlanPatch {
            state: Some(PlanState::Approved),
            ..Default::default()
        },
    )
    .unwrap();

    let blocked = enter_phase(dir.path(), "build").unwrap_err().to_string();
    assert!(blocked.contains("INVALID_PHASE_TRANSITION"));
    enter_phase(dir.path(), "plan").unwrap();
    complete_phase(dir.path(), "plan").unwrap();
    enter_phase(dir.path(), "build").unwrap();
    let incomplete = patch_plan(
        dir.path(),
        PlanPatch {
            state: Some(PlanState::Done),
            ..Default::default()
        },
    )
    .unwrap_err()
    .to_string();
    assert!(incomplete.contains("PHASES_INCOMPLETE"));
    complete_phase(dir.path(), "build").unwrap();
    assert_eq!(
        load_plan(dir.path()).unwrap().unwrap().state,
        PlanState::Done
    );
}

fn execution_fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    init_plan(
        dir.path(),
        InitPlanInput {
            title: "Request context survives isolation".to_string(),
            intent: "Deliver the already understood request".to_string(),
            todo_ids: vec!["root-task".to_string()],
            proof_hooks: vec!["focused tests".to_string()],
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
    patch_plan(
        dir.path(),
        PlanPatch {
            state: Some(PlanState::Approved),
            ..Default::default()
        },
    )
    .unwrap();
    let store = dir.path().join("isolated-store");
    fs::create_dir_all(&store).unwrap();
    let conn =
        crate::core::db::db_connect(&crate::core::todo::todo_db_path(&store).to_string_lossy())
            .unwrap();
    // Minimal disposable projection fixture: execution checks only task identity.
    conn.execute("CREATE TABLE tasks (id TEXT PRIMARY KEY)", [])
        .unwrap();
    (dir, store)
}

fn execute_check(
    root: &Path,
    store: &Path,
    todo_id: Option<&str>,
) -> Result<GovernedPlan, error::DecapodError> {
    ensure_execute_ready(ExecuteCheckInput {
        project_root: root,
        store_root: store,
        todo_id,
    })
}

fn insert_projection_task(store: &Path, id: &str) {
    let conn =
        crate::core::db::db_connect(&crate::core::todo::todo_db_path(store).to_string_lossy())
            .unwrap();
    conn.execute(
        "INSERT INTO tasks(id) VALUES (?1)",
        crate::core::db::params![id],
    )
    .unwrap();
}

#[test]
fn missing_projection_is_coordination_failure_not_lost_intent() {
    let (dir, store) = execution_fixture();
    let before = fs::read(plan_path(dir.path())).unwrap();
    let message = execute_check(dir.path(), &store, None)
        .unwrap_err()
        .to_string();
    assert!(message.contains("TODO_PROJECTION_MISSING"));
    assert!(!message.contains("NEEDS_HUMAN_INPUT"));
    let payload: serde_json::Value =
        serde_json::from_str(message.split_once("payload=").unwrap().1).unwrap();
    assert_eq!(payload["kind"], "coordination_projection_missing");
    assert_eq!(payload["todo_ids"], json!(["root-task"]));
    assert_eq!(payload["store_root"], json!(store));
    assert_eq!(payload["project_root"], json!(dir.path()));
    assert_eq!(payload["intent_check"], "passed");
    assert_eq!(payload["execution_ready"], false);
    assert_eq!(payload["ownership"], "not_established_by_absence");
    assert!(payload.get("questions").is_none());
    assert!(!payload["recovery"].as_array().unwrap().is_empty());
    assert_eq!(before, fs::read(plan_path(dir.path())).unwrap());
}

#[test]
fn explicit_missing_todo_does_not_fall_back_to_another_plan_task() {
    let (dir, store) = execution_fixture();
    insert_projection_task(&store, "root-task");
    let message = execute_check(dir.path(), &store, Some("other-task"))
        .unwrap_err()
        .to_string();
    assert!(message.contains("TODO_PROJECTION_MISSING"));
    let payload: serde_json::Value =
        serde_json::from_str(message.split_once("payload=").unwrap().1).unwrap();
    assert_eq!(payload["todo_ids"], json!(["other-task"]));
    assert!(execute_check(dir.path(), &store, Some("root-task")).is_ok());
}

#[test]
fn actual_intent_ambiguity_still_requires_human_input_before_projection_lookup() {
    let (dir, store) = execution_fixture();
    for patch in [
        PlanPatch {
            intent: Some(String::new()),
            ..Default::default()
        },
        PlanPatch {
            intent: Some("Clear request".to_string()),
            unknowns: Some(vec!["Scope unknown".to_string()]),
            ..Default::default()
        },
        PlanPatch {
            unknowns: Some(vec![]),
            human_questions: Some(vec!["Which proof is required?".to_string()]),
            ..Default::default()
        },
    ] {
        patch_plan(dir.path(), patch).unwrap();
        let message = execute_check(dir.path(), &store, None)
            .unwrap_err()
            .to_string();
        assert!(message.contains("NEEDS_HUMAN_INPUT"), "{message}");
        assert!(!message.contains("TODO_PROJECTION_MISSING"));
    }
}

#[test]
fn missing_projection_does_not_waive_plan_approval_or_phase_gates() {
    let (dir, store) = execution_fixture();
    patch_plan(
        dir.path(),
        PlanPatch {
            state: Some(PlanState::Draft),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        execute_check(dir.path(), &store, None)
            .unwrap_err()
            .to_string()
            .contains("NEEDS_PLAN_APPROVAL")
    );
    patch_plan(
        dir.path(),
        PlanPatch {
            state: Some(PlanState::Approved),
            ..Default::default()
        },
    )
    .unwrap();
    add_phase(dir.path(), phase("build")).unwrap();
    assert!(
        execute_check(dir.path(), &store, None)
            .unwrap_err()
            .to_string()
            .contains("TODO_PROJECTION_MISSING")
    );
    insert_projection_task(&store, "root-task");
    assert!(
        execute_check(dir.path(), &store, None)
            .unwrap_err()
            .to_string()
            .contains("PHASE_REQUIRED")
    );
    enter_phase(dir.path(), "build").unwrap();
    assert!(execute_check(dir.path(), &store, None).is_ok());
}

#[test]
fn projection_query_failure_is_not_reported_as_absence() {
    let (dir, store) = execution_fixture();
    let conn =
        crate::core::db::db_connect(&crate::core::todo::todo_db_path(&store).to_string_lossy())
            .unwrap();
    conn.execute("DROP TABLE tasks", []).unwrap();
    let error = execute_check(dir.path(), &store, None).unwrap_err();
    assert!(matches!(error, error::DecapodError::StorageError(_)));
    assert!(!error.to_string().contains("TODO_PROJECTION_MISSING"));
}
