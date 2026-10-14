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
        approve_plan(dir.path());
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
    approve_plan(dir.path());
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

fn scope_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    scope_git(dir.path(), &["init", "-q", "-b", "master"]);
    scope_git(dir.path(), &["config", "user.name", "Scope Test"]);
    scope_git(
        dir.path(),
        &["config", "user.email", "scope@example.invalid"],
    );
    fs::create_dir(dir.path().join("protected")).unwrap();
    fs::write(dir.path().join("protected/original file.txt"), "base\n").unwrap();
    scope_git(dir.path(), &["add", "."]);
    scope_git(dir.path(), &["commit", "-qm", "accepted base"]);
    scope_git(dir.path(), &["checkout", "-qb", "candidate"]);
    dir
}
fn scope_git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}
#[test]
fn scope_rejects_clean_committed_rename_and_deleted_forbidden_source() {
    let dir = scope_repo();
    scope_git(
        dir.path(),
        &["mv", "protected/original file.txt", "allowed.txt"],
    );
    scope_git(dir.path(), &["commit", "-qm", "rename forbidden source"]);
    let constraints = ScopeConstraints {
        forbidden_paths: vec!["protected".into()],
        file_touch_budget: None,
    };
    let error = enforce_scope_constraints(dir.path(), &constraints)
        .unwrap_err()
        .to_string();
    assert!(error.contains("SCOPE_VIOLATION"));
    assert!(error.contains("protected/original file.txt"));
    let budget = ScopeConstraints {
        forbidden_paths: vec![],
        file_touch_budget: Some(1),
    };
    assert!(
        enforce_scope_constraints(dir.path(), &budget)
            .unwrap_err()
            .to_string()
            .contains("file-touch budget")
    );
}
#[test]
fn scope_counts_union_of_committed_staged_unstaged_and_untracked_paths() {
    let dir = scope_repo();
    fs::write(dir.path().join("committed.txt"), "committed").unwrap();
    scope_git(dir.path(), &["add", "."]);
    scope_git(dir.path(), &["commit", "-qm", "committed path"]);
    fs::write(dir.path().join("staged.txt"), "staged").unwrap();
    scope_git(dir.path(), &["add", "staged.txt"]);
    fs::write(dir.path().join("protected/original file.txt"), "unstaged").unwrap();
    fs::write(dir.path().join("untracked.txt"), "untracked").unwrap();
    let mut budget = ScopeConstraints {
        forbidden_paths: vec![],
        file_touch_budget: Some(3),
    };
    assert!(enforce_scope_constraints(dir.path(), &budget).is_err());
    budget.file_touch_budget = Some(4);
    enforce_scope_constraints(dir.path(), &budget).unwrap();
}
#[test]
fn scope_preserves_quoted_paths_and_directory_boundaries_and_fails_without_base() {
    let dir = scope_repo();
    fs::write(dir.path().join("protected/quoted\"name\n.txt"), "new").unwrap();
    let constraints = ScopeConstraints {
        forbidden_paths: vec!["protected".into()],
        file_touch_budget: None,
    };
    assert!(enforce_scope_constraints(dir.path(), &constraints).is_err());
    let paths = std::collections::BTreeSet::from(["protected-other/file".into()]);
    enforce_scope_paths(&constraints, &paths).unwrap();
    scope_git(dir.path(), &["branch", "-D", "master"]);
    assert!(
        enforce_scope_constraints(dir.path(), &constraints)
            .unwrap_err()
            .to_string()
            .contains("SCOPE_EVIDENCE_UNAVAILABLE")
    );
}

#[test]
fn staged_revert_cannot_hide_a_forbidden_committed_head_from_execution_scope() {
    let dir = scope_repo();
    fs::write(
        dir.path().join("protected/original file.txt"),
        "forbidden commit",
    )
    .unwrap();
    scope_git(dir.path(), &["commit", "-am", "forbidden edit"]);
    scope_git(
        dir.path(),
        &["checkout", "master", "--", "protected/original file.txt"],
    );
    let constraints = ScopeConstraints {
        forbidden_paths: vec!["protected".into()],
        file_touch_budget: None,
    };
    assert!(
        enforce_scope_constraints(dir.path(), &constraints)
            .unwrap_err()
            .to_string()
            .contains("SCOPE_VIOLATION")
    );
}

fn approve_plan(root: &Path) {
    patch_plan(
        root,
        PlanPatch {
            state: Some(PlanState::Approved),
            ..Default::default()
        },
    )
    .unwrap();
}

fn approval_gate() -> Gate {
    Gate {
        description: "Current contract reviewed".into(),
        required_artifacts: vec![],
        validation_checks: vec![],
        satisfied: false,
        satisfied_at: None,
    }
}

fn progressed_plan(state: PlanState) -> (tempfile::TempDir, PathBuf) {
    let (dir, store) = execution_fixture();
    insert_projection_task(&store, "root-task");
    let mut first = phase("first");
    first.entry_gates.push(approval_gate());
    first.exit_gates.push(approval_gate());
    add_phase(dir.path(), first).unwrap();
    add_phase(dir.path(), phase("second")).unwrap();
    approve_plan(dir.path());
    satisfy_gate(dir.path(), "first", "entry", 0).unwrap();
    satisfy_gate(dir.path(), "first", "exit", 0).unwrap();
    enter_phase(dir.path(), "first").unwrap();
    complete_phase(dir.path(), "first").unwrap();
    if state == PlanState::Done {
        enter_phase(dir.path(), "second").unwrap();
        complete_phase(dir.path(), "second").unwrap();
    } else if state == PlanState::Approved {
        approve_plan(dir.path());
    }
    assert_eq!(load_plan(dir.path()).unwrap().unwrap().state, state);
    (dir, store)
}

fn assert_acceptance_invalidated(plan: &GovernedPlan) {
    assert_eq!(plan.state, PlanState::Draft);
    for phase in &plan.phases {
        assert!(!phase.entered && !phase.completed);
        assert!(phase.entered_at.is_none() && phase.completed_at.is_none());
        for gate in phase.entry_gates.iter().chain(&phase.exit_gates) {
            assert!(!gate.satisfied);
            assert!(gate.satisfied_at.is_none());
        }
    }
}

#[test]
fn contract_edits_revoke_every_accepted_state_and_all_phase_evidence() {
    let patches = vec![
        PlanPatch {
            title: Some("Revised title".into()),
            ..Default::default()
        },
        PlanPatch {
            intent: Some("Different accepted work".into()),
            ..Default::default()
        },
        PlanPatch {
            todo_ids: Some(vec!["replacement".into()]),
            ..Default::default()
        },
        PlanPatch {
            proof_hooks: Some(vec!["different proof".into()]),
            ..Default::default()
        },
        PlanPatch {
            unknowns: Some(vec!["new unknown".into()]),
            ..Default::default()
        },
        PlanPatch {
            human_questions: Some(vec!["new question".into()]),
            ..Default::default()
        },
        PlanPatch {
            stop_conditions: Some(vec!["new stop".into()]),
            ..Default::default()
        },
        PlanPatch {
            unresolved_contradictions: Some(vec!["new conflict".into()]),
            ..Default::default()
        },
        PlanPatch {
            deferred_questions: Some(vec!["new deferral".into()]),
            ..Default::default()
        },
        PlanPatch {
            constraints: Some(ScopeConstraints {
                forbidden_paths: vec!["private/**".into()],
                file_touch_budget: Some(1),
            }),
            ..Default::default()
        },
        PlanPatch {
            phases: Some(vec![phase("replacement")]),
            ..Default::default()
        },
    ];
    for state in [PlanState::Approved, PlanState::Executing, PlanState::Done] {
        for patch in &patches {
            let (dir, store) = progressed_plan(state.clone());
            let changed = patch_plan(dir.path(), patch.clone()).unwrap();
            assert_acceptance_invalidated(&changed);
            assert!(
                execute_check(dir.path(), &store, None)
                    .unwrap_err()
                    .to_string()
                    .contains("NEEDS_PLAN_APPROVAL")
            );
            assert!(
                enter_phase(dir.path(), &changed.phases[0].id)
                    .unwrap_err()
                    .to_string()
                    .contains("NEEDS_PLAN_APPROVAL")
            );
            assert!(
                complete_phase(dir.path(), &changed.phases[0].id)
                    .unwrap_err()
                    .to_string()
                    .contains("NEEDS_PLAN_APPROVAL")
            );
        }
    }
}

#[test]
fn identical_contract_updates_preserve_acceptance_progress_and_reviews() {
    for state in [PlanState::Approved, PlanState::Executing, PlanState::Done] {
        let (dir, _) = progressed_plan(state);
        let before = load_plan(dir.path()).unwrap().unwrap();
        let changed = patch_plan(
            dir.path(),
            PlanPatch {
                title: Some(before.title.clone()),
                intent: Some(before.intent.clone()),
                todo_ids: Some(before.todo_ids.clone()),
                proof_hooks: Some(before.proof_hooks.clone()),
                unknowns: Some(before.unknowns.clone()),
                human_questions: Some(before.human_questions.clone()),
                stop_conditions: Some(before.stop_conditions.clone()),
                unresolved_contradictions: Some(before.unresolved_contradictions.clone()),
                deferred_questions: Some(before.deferred_questions.clone()),
                constraints: Some(before.constraints.clone()),
                phases: Some(before.phases.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(changed.state, before.state);
        assert_eq!(changed.phases, before.phases);
        assert_eq!(changed.spec_reviews, before.spec_reviews);
        let updated = update_phase(
            dir.path(),
            "first",
            Some("first".into()),
            Some("first work".into()),
        )
        .unwrap();
        assert_eq!(updated.state, before.state);
        assert_eq!(updated.phases, before.phases);
        let updated = update_gate(
            dir.path(),
            "first",
            true,
            0,
            "Current contract reviewed".into(),
        )
        .unwrap();
        assert_eq!(updated.state, before.state);
        assert_eq!(updated.phases, before.phases);
    }
}

#[test]
fn phase_and_gate_contract_editors_revoke_done_and_require_fresh_progress() {
    for editor in 0..6 {
        let (dir, store) = progressed_plan(PlanState::Done);
        let updated = match editor {
            0 => add_phase(dir.path(), phase("third")),
            1 => update_phase(dir.path(), "first", Some("Revised name".into()), None),
            2 => update_phase(dir.path(), "first", None, Some("Revised contract".into())),
            3 => add_gate(dir.path(), "first", true, approval_gate()),
            4 => add_gate(dir.path(), "first", false, approval_gate()),
            _ => update_gate(dir.path(), "first", false, 0, "Revised gate".into()),
        }
        .unwrap();
        assert_acceptance_invalidated(&updated);
        approve_plan(dir.path());
        assert!(
            execute_check(dir.path(), &store, None)
                .unwrap_err()
                .to_string()
                .contains("PHASE_REQUIRED")
        );
        assert!(
            enter_phase(dir.path(), "first")
                .unwrap_err()
                .to_string()
                .contains("PHASE_ENTRY_GATE_NOT_SATISFIED")
        );
        assert!(
            patch_plan(
                dir.path(),
                PlanPatch {
                    state: Some(PlanState::Done),
                    ..Default::default()
                }
            )
            .unwrap_err()
            .to_string()
            .contains("PHASES_INCOMPLETE")
        );
    }
}

#[test]
fn contract_change_cannot_be_accepted_in_the_same_patch() {
    for state in [PlanState::Approved, PlanState::Executing, PlanState::Done] {
        let (dir, _) = progressed_plan(PlanState::Done);
        let before = fs::read(plan_path(dir.path())).unwrap();
        let error = patch_plan(
            dir.path(),
            PlanPatch {
                state: Some(state),
                phases: Some(vec![phase("unperformed")]),
                ..Default::default()
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("PLAN_CHANGED_REQUIRES_APPROVAL"));
        assert_eq!(before, fs::read(plan_path(dir.path())).unwrap());
    }
}

#[test]
fn unapproved_phase_progress_cannot_manufacture_done() {
    for state in [PlanState::Draft, PlanState::Annotating] {
        let (dir, _) = execution_fixture();
        add_phase(dir.path(), phase("build")).unwrap();
        patch_plan(
            dir.path(),
            PlanPatch {
                state: Some(state.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        let before = fs::read(plan_path(dir.path())).unwrap();
        for error in [
            enter_phase(dir.path(), "build").unwrap_err(),
            complete_phase(dir.path(), "build").unwrap_err(),
        ] {
            assert!(error.to_string().contains("NEEDS_PLAN_APPROVAL"));
        }
        assert_eq!(before, fs::read(plan_path(dir.path())).unwrap());
        approve_plan(dir.path());
        enter_phase(dir.path(), "build").unwrap();
        patch_plan(
            dir.path(),
            PlanPatch {
                state: Some(state),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            complete_phase(dir.path(), "build")
                .unwrap_err()
                .to_string()
                .contains("NEEDS_PLAN_APPROVAL")
        );
        approve_plan(dir.path());
        assert_eq!(
            complete_phase(dir.path(), "build").unwrap().state,
            PlanState::Done
        );
    }
}

#[test]
fn legacy_inconsistent_progress_is_rejected_at_shared_acceptance_boundary() {
    let (dir, store) = progressed_plan(PlanState::Done);
    let mut plan = load_plan(dir.path()).unwrap().unwrap();
    plan.phases[0].exit_gates[0].satisfied = false;
    save_plan(dir.path(), &plan).unwrap();
    assert!(
        validate_plan_progress(&plan)
            .unwrap_err()
            .to_string()
            .contains("PHASE_GATE_NOT_SATISFIED")
    );
    assert!(
        execute_check(dir.path(), &store, None)
            .unwrap_err()
            .to_string()
            .contains("PHASE_GATE_NOT_SATISFIED")
    );
    plan.phases[0].exit_gates[0].satisfied = true;
    plan.phases[1].completed = false;
    assert!(
        validate_plan_progress(&plan)
            .unwrap_err()
            .to_string()
            .contains("PHASES_INCOMPLETE")
    );
}

#[test]
fn completing_final_phase_cannot_persist_inconsistent_legacy_done() {
    let (dir, _) = progressed_plan(PlanState::Executing);
    enter_phase(dir.path(), "second").unwrap();
    let mut plan = load_plan(dir.path()).unwrap().unwrap();
    // The old gate editor could leave a completed predecessor unsatisfied.
    plan.phases[0].exit_gates[0].satisfied = false;
    save_plan(dir.path(), &plan).unwrap();
    let before = fs::read(plan_path(dir.path())).unwrap();
    let error = complete_phase(dir.path(), "second")
        .unwrap_err()
        .to_string();
    assert!(error.contains("PHASE_GATE_NOT_SATISFIED"));
    assert_eq!(before, fs::read(plan_path(dir.path())).unwrap());
}

#[test]
fn contract_revision_preserves_pending_spec_decisions() {
    let (dir, _) = progressed_plan(PlanState::Done);
    let mut plan = load_plan(dir.path()).unwrap().unwrap();
    plan.spec_reviews.push(SpecReview {
        path: PUBLICATION_REVIEW_SPECS[0].into(),
        spec_material_hash: "prior material".into(),
        reviewed_code_fingerprint: "prior code".into(),
        disposition: SpecReviewDisposition::RequiresDecision,
        reason: "Unresolved external authority question".into(),
        decision_ref: None,
    });
    save_plan(dir.path(), &plan).unwrap();
    let updated = patch_plan(
        dir.path(),
        PlanPatch {
            intent: Some("Revised work".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_acceptance_invalidated(&updated);
    assert_eq!(updated.spec_reviews, plan.spec_reviews);
}
