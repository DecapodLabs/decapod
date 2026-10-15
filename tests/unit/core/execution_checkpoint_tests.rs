use super::*;
use crate::{
    core::trajectory::{self, TrajectoryInit, TrajectoryUpdate},
    plan_governance::{self, InitPlanInput, PlanPatch, PlanState, ScopeConstraints},
};
use tempfile::{TempDir, tempdir};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "fixture git failed: {args:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn commit(root: &Path) {
    git(root, &["add", "."]);
    git(
        root,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "execution fixture",
        ],
    );
}

fn fixture() -> TempDir {
    let temp = tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "work"]);
    git(root, &["config", "user.name", "Execution Test"]);
    git(root, &["config", "user.email", "execution@example.invalid"]);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/app.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join(".gitignore"), ".decapod/data/\n").unwrap();
    plan_governance::init_plan(
        root,
        InitPlanInput {
            title: "Continue accepted work".into(),
            intent: "Preserve approved scope".into(),
            todo_ids: vec!["task_execution".into()],
            proof_hooks: vec!["source test".into()],
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
    plan_governance::patch_plan(
        root,
        PlanPatch {
            state: Some(PlanState::Approved),
            ..Default::default()
        },
    )
    .unwrap();
    trajectory::init_trajectory(
        root,
        TrajectoryInit {
            run_id: "run_execution".into(),
            task_id: Some("task_execution".into()),
            intent_id: Some("intent_execution".into()),
            original_intent: "Preserve approved scope".into(),
            derived_intent: "Preserve approved scope".into(),
            active_boundaries: vec!["no publication".into()],
            repo_scope: vec!["src".into()],
            destination: None,
            current_phase: None,
            next_transitions: vec![],
            blockers: vec![],
        },
    )
    .unwrap();
    let store = root.join(".decapod/data");
    crate::core::todo::initialize_todo_db(&store).unwrap();
    {
        let conn =
            crate::core::db::db_connect(&crate::core::todo::todo_db_path(&store).to_string_lossy())
                .unwrap();
        conn.execute("INSERT INTO tasks(id,hash,title,status,created_at,updated_at,dir_path,scope,assigned_to,lease_expires_at,lease_generation,lease_lifecycle,intent_anchor,revision) VALUES(?1,'fixture','Execution','open','1Z','1Z',?2,'repo',?3,?4,1,'claimed','intent_execution',1)",
            params!["task_execution", root.to_string_lossy().as_ref(), crate::current_agent_id(), format!("{}Z", chrono::Utc::now().timestamp()+3600)]).unwrap();
    }
    commit(root);
    temp
}

#[test]
fn execution_checkpoint_commit_resume_is_deterministic_and_read_only() {
    let temp = fixture();
    let root = temp.path();
    let first = checkpoint_execution(root, "run_execution", "checkpoint_1", &[]).unwrap();
    let bytes = fs::read(root.join(super::super::TRAJECTORY_PATH)).unwrap();
    assert_eq!(
        first,
        checkpoint_execution(root, "run_execution", "checkpoint_1", &[]).unwrap()
    );
    assert_eq!(
        bytes,
        fs::read(root.join(super::super::TRAJECTORY_PATH)).unwrap()
    );
    commit(root);
    assert_eq!(
        first,
        verify_execution_resume(root, "run_execution", "checkpoint_1").unwrap()
    );
    assert_eq!(
        bytes,
        fs::read(root.join(super::super::TRAJECTORY_PATH)).unwrap()
    );
    assert_eq!(
        load_trajectory(root, "run_execution").unwrap().proof_status,
        trajectory::TrajectoryProofStatus::NoChecksRun
    );
}

#[test]
fn execution_resume_refuses_dirty_and_committed_source_changes() {
    let temp = fixture();
    let root = temp.path();
    checkpoint_execution(root, "run_execution", "checkpoint_1", &[]).unwrap();
    fs::write(root.join("src/app.rs"), "fn main() { panic!(); }\n").unwrap();
    assert!(
        verify_execution_resume(root, "run_execution", "checkpoint_1")
            .unwrap_err()
            .to_string()
            .contains("EXECUTION_STATE_CHANGED")
    );
    commit(root);
    assert!(verify_execution_resume(root, "run_execution", "checkpoint_1").is_err());
}

#[test]
fn execution_resume_refuses_branch_substitution_and_rewritten_history() {
    let temp = fixture();
    let root = temp.path();
    let binding = capture_execution_binding(root, "task_execution").unwrap();
    git(root, &["checkout", "-b", "substitute"]);
    assert!(verify_execution_binding(root, "task_execution", &binding).is_err());
    git(root, &["checkout", "work"]);
    // A new root with exactly the same tree cannot replace the provenance.
    git(root, &["checkout", "--orphan", "replacement"]);
    git(
        root,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "rewritten root",
        ],
    );
    git(root, &["branch", "-D", "work"]);
    git(root, &["branch", "-m", "work"]);
    assert!(
        verify_execution_binding(root, "task_execution", &binding)
            .unwrap_err()
            .to_string()
            .contains("EXECUTION_HISTORY_CHANGED")
    );
}

#[test]
fn execution_resume_rechecks_plan_acceptance_and_human_decisions() {
    let temp = fixture();
    let root = temp.path();
    let binding = capture_execution_binding(root, "task_execution").unwrap();
    plan_governance::patch_plan(
        root,
        PlanPatch {
            human_questions: Some(vec!["Choose scope".into()]),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(verify_execution_binding(root, "task_execution", &binding).is_err());
    plan_governance::patch_plan(
        root,
        PlanPatch {
            state: Some(PlanState::Approved),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        verify_execution_binding(root, "task_execution", &binding)
            .unwrap_err()
            .to_string()
            .contains("EXECUTION_DECISION_REQUIRED")
    );
}

#[test]
fn execution_resume_binds_meaningful_trajectory_and_task_identity() {
    let temp = fixture();
    let root = temp.path();
    let binding = capture_execution_binding(root, "task_execution").unwrap();
    assert!(capture_execution_binding(root, "other_task").is_err());
    trajectory::record_trajectory(
        root,
        "run_execution",
        TrajectoryUpdate {
            evidence: vec!["a new measured observation".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(verify_execution_binding(root, "task_execution", &binding).is_err());
}

#[test]
fn execution_capture_requires_staging_new_material_and_binds_deletions() {
    let temp = fixture();
    let root = temp.path();
    fs::write(root.join("new-source.txt"), "new source").unwrap();
    assert!(capture_execution_binding(root, "task_execution").is_err());
    git(root, &["add", "new-source.txt"]);
    let binding = capture_execution_binding(root, "task_execution").unwrap();
    fs::remove_file(root.join("src/app.rs")).unwrap();
    assert!(verify_execution_binding(root, "task_execution", &binding).is_err());
}

#[test]
fn execution_checkpoints_do_not_persist_secret_source_or_paths() {
    let temp = fixture();
    let root = temp.path();
    let secret = "example-test-secret-not-a-real-credential";
    fs::write(root.join("src/app.rs"), secret).unwrap();
    let checkpoint = checkpoint_execution(root, "run_execution", "checkpoint_1", &[]).unwrap();
    let json = serde_json::to_string(&checkpoint).unwrap();
    assert!(!json.contains(secret));
    assert!(!json.contains(root.to_str().unwrap()));
    assert!(!json.contains("src/app.rs"));
    assert_eq!(
        checkpoint.binding_digest,
        digest(&checkpoint.binding).unwrap()
    );
}

fn approval_fixture(root: &Path, operation: &str, scope: &str, expiry: &str) -> String {
    let store = crate::core::store::Store {
        kind: crate::core::store::StoreKind::Repo,
        root: root.join(".decapod/data"),
    };
    crate::plugins::policy::initialize_policy_db(&store.root).unwrap();
    crate::plugins::policy::approve_action_with_expiry(
        &store,
        operation,
        Some("task_execution"),
        "operator",
        scope,
        Some(expiry),
    )
    .unwrap()
}

#[test]
fn execution_resume_rejects_revoked_and_wrong_scope_approvals() {
    let temp = fixture();
    let root = temp.path();
    let future = format!("{}Z", chrono::Utc::now().timestamp() + 3600);
    let wrong = approval_fixture(root, "todo.delete", "global", &future);
    assert!(
        checkpoint_execution(root, "run_execution", "wrong", &[wrong])
            .unwrap_err()
            .to_string()
            .contains("EXECUTION_APPROVAL_SCOPE")
    );
    let id = approval_fixture(root, RESUME_OPERATION, "global", &future);
    checkpoint_execution(root, "run_execution", "checkpoint_1", &[id.clone()]).unwrap();
    verify_execution_resume(root, "run_execution", "checkpoint_1").unwrap();
    let store = crate::core::store::Store {
        kind: crate::core::store::StoreKind::Repo,
        root: root.join(".decapod/data"),
    };
    crate::plugins::policy::revoke_approval(&store, &id).unwrap();
    assert!(
        verify_execution_resume(root, "run_execution", "checkpoint_1")
            .unwrap_err()
            .to_string()
            .contains("EXECUTION_APPROVAL_MISSING")
    );
}

#[test]
fn execution_approval_expiry_and_record_changes_are_not_hash_only() {
    assert!(!approval_live(Some("1Z")));
    assert!(!approval_live(Some("malformed")));
    assert!(approval_live(None));
    let temp = fixture();
    let root = temp.path();
    let future = format!("{}Z", chrono::Utc::now().timestamp() + 3600);
    let id = approval_fixture(root, RESUME_OPERATION, "global", &future);
    checkpoint_execution(root, "run_execution", "checkpoint_1", &[id.clone()]).unwrap();
    let db = crate::plugins::policy::policy_db_path(&root.join(".decapod/data"));
    {
        let conn = crate::core::db::db_connect(&db.to_string_lossy()).unwrap();
        conn.execute(
            "UPDATE approvals SET expires_at='1Z' WHERE approval_id=?1",
            params![id],
        )
        .unwrap();
    }
    assert!(
        verify_execution_resume(root, "run_execution", "checkpoint_1")
            .unwrap_err()
            .to_string()
            .contains("EXECUTION_APPROVAL_EXPIRED")
    );
}

#[test]
fn execution_resume_rechecks_task_revision_owner_generation_and_live_expiry() {
    for mutation in [
        "revision=revision+1",
        "assigned_to='different_actor'",
        "lease_generation=lease_generation+1",
        "lease_expires_at='1Z'",
        "lease_lifecycle='yielded'",
        "depends_on='missing_dependency'",
    ] {
        let temp = fixture();
        let root = temp.path();
        let binding = capture_execution_binding(root, "task_execution").unwrap();
        {
            let conn = crate::core::db::db_connect(
                &crate::core::todo::todo_db_path(&root.join(".decapod/data")).to_string_lossy(),
            )
            .unwrap();
            conn.execute(
                &format!("UPDATE tasks SET {mutation} WHERE id='task_execution'"),
                [],
            )
            .unwrap();
        }
        assert!(
            verify_execution_binding(root, "task_execution", &binding).is_err(),
            "must reject changed custody: {mutation}"
        );
    }
}

#[test]
fn execution_checkpoint_cannot_drop_prior_approval_requirements() {
    let temp = fixture();
    let root = temp.path();
    let future = format!("{}Z", chrono::Utc::now().timestamp() + 3600);
    let id = approval_fixture(root, RESUME_OPERATION, "global", &future);
    checkpoint_execution(root, "run_execution", "checkpoint_1", &[id]).unwrap();
    assert!(
        checkpoint_execution(root, "run_execution", "checkpoint_2", &[])
            .unwrap_err()
            .to_string()
            .contains("EXECUTION_APPROVAL_REQUIRED")
    );
}

#[test]
fn execution_checkpoint_rechecks_existing_scope_constraint_gate() {
    let temp = fixture();
    let root = temp.path();
    git(root, &["branch", "main"]);
    plan_governance::patch_plan(
        root,
        PlanPatch {
            constraints: Some(ScopeConstraints {
                forbidden_paths: vec!["src/**".into()],
                file_touch_budget: None,
            }),
            ..Default::default()
        },
    )
    .unwrap();
    plan_governance::patch_plan(
        root,
        PlanPatch {
            state: Some(PlanState::Approved),
            ..Default::default()
        },
    )
    .unwrap();
    fs::write(root.join("src/app.rs"), "changed implementation").unwrap();
    assert!(capture_execution_binding(root, "task_execution").is_err());
}

#[test]
fn execution_resume_accepts_applicable_zone_approval_without_widening_action() {
    let temp = fixture();
    let root = temp.path();
    let future = format!("{}Z", chrono::Utc::now().timestamp() + 3600);
    let id = approval_fixture(root, "control.mutate", "global", &future);
    checkpoint_execution(root, "run_execution", "checkpoint_1", &[id]).unwrap();
    verify_execution_resume(root, "run_execution", "checkpoint_1").unwrap();
}
