// Moved from src/decapod/core/trajectory.rs
use super::*;
use tempfile::tempdir;

fn trajectory_git(root: &Path, args: &[&str]) -> Vec<u8> {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .expect("run fixture git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn commit_trajectory_journal(root: &Path) {
    trajectory_git(root, &["init", "-b", "main"]);
    trajectory_git(root, &["add", ".decapod/governance.json"]);
    trajectory_git(
        root,
        &[
            "-c",
            "user.name=Trajectory Test",
            "-c",
            "user.email=trajectory@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "Preserve trajectory journal",
        ],
    );
}

fn replacement_init(run_id: &str) -> TrajectoryInit {
    TrajectoryInit {
        run_id: run_id.to_string(),
        task_id: None,
        intent_id: None,
        original_intent: "original".to_string(),
        derived_intent: "derived".to_string(),
        active_boundaries: vec!["src/**".to_string()],
        repo_scope: vec!["src/decapod/lib.rs".to_string()],
        destination: None,
        current_phase: None,
        next_transitions: Vec::new(),
        blockers: Vec::new(),
    }
}

#[test]
fn trajectory_creation_is_inspectable_and_unproven_without_checks() {
    let temp = tempdir().unwrap();
    let artifact = init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_1".to_string(),
            task_id: Some("task_1".to_string()),
            intent_id: None,
            original_intent: "original intent".to_string(),
            derived_intent: "derived intent".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/lib.rs".to_string()],
            destination: None,
            current_phase: None,
            next_transitions: Vec::new(),
            blockers: Vec::new(),
        },
    )
    .unwrap();

    assert_eq!(artifact.proof_status, TrajectoryProofStatus::NoChecksRun);
    assert_eq!(
        artifact.verdicts.completion_proof,
        TrajectoryVerdict::Unsupported
    );
    assert_eq!(load_trajectory(temp.path(), "run_1").unwrap(), artifact);
}

#[test]
fn new_run_replaces_the_cookie_and_git_preserves_prior_history() {
    let temp = tempdir().unwrap();
    init_trajectory(temp.path(), replacement_init("run_old")).unwrap();
    let prior = fs::read(trajectory_cookie_path(temp.path())).unwrap();
    commit_trajectory_journal(temp.path());
    let replacement = init_trajectory(temp.path(), replacement_init("run_new")).unwrap();
    assert_eq!(replacement.run_id, "run_new");
    assert!(load_trajectory(temp.path(), "run_old").is_err());
    let cookie_path = trajectory_cookie_path(temp.path());
    let governance_dir = cookie_path.parent().unwrap();
    assert!(governance_dir.join("governance.json").exists());
    assert!(!governance_dir.join("governance").exists());
    assert_eq!(
        load_trajectory(temp.path(), "run_new").unwrap(),
        replacement
    );

    assert_eq!(
        trajectory_git(temp.path(), &["show", "HEAD:.decapod/governance.json"]),
        prior,
        "the complete previous run must remain recoverable in Git"
    );
    let current = fs::read(&cookie_path).unwrap();
    let same_run = init_trajectory(temp.path(), replacement_init("run_new"));
    assert!(same_run.is_err());
    assert_eq!(fs::read(cookie_path).unwrap(), current);
}

#[test]
fn new_run_preserves_uncommitted_failures_blockers_custody_and_jev() {
    let temp = tempdir().unwrap();
    init_trajectory(temp.path(), replacement_init("run_old")).unwrap();
    commit_trajectory_journal(temp.path());
    let failed = record_trajectory(
        temp.path(),
        "run_old",
        TrajectoryUpdate {
            checks: vec![TrajectoryCheck {
                name: "required-check".to_string(),
                status: TrajectoryCheckStatus::Failed,
            }],
            blockers: vec!["required check failed".to_string()],
            unresolved_assumptions: vec!["repair still needs verification".to_string()],
            evidence: vec!["artifact:failed-check".to_string()],
            ..Default::default()
        },
    )
    .unwrap();
    crate::core::governance_document::write_section(
        temp.path(),
        "jev",
        &serde_json::json!({
            "$schema": "https://decapod.dev/schemas/jev-observations-1.0.0.schema.json",
            "schema_version": "1.0.0",
            "kind": "jev_observation_ledger",
            "trajectory_run_id": "run_old",
            "runs": {}
        }),
    )
    .unwrap();
    let cookie = trajectory_cookie_path(temp.path());
    let prior = fs::read(&cookie).unwrap();
    // An interrupted checkpoint may have staged the journal without committing
    // it. Neither staging nor a repeated initialization attempt archives it.
    trajectory_git(temp.path(), &["add", ".decapod/governance.json"]);
    for _ in 0..2 {
        let error = init_trajectory(temp.path(), replacement_init("run_new")).unwrap_err();
        assert!(error.to_string().contains("TRAJECTORY_UNARCHIVED_WORK"));
        assert!(error.to_string().contains("run_old"));
        assert_eq!(fs::read(&cookie).unwrap(), prior);
        assert_eq!(load_trajectory(temp.path(), "run_old").unwrap(), failed);
    }

    // The same requested replacement succeeds once the recovery instruction is
    // satisfied; the failure, assumptions, custody and Jev bytes remain in HEAD.
    commit_trajectory_journal(temp.path());
    init_trajectory(temp.path(), replacement_init("run_new")).unwrap();
    assert_eq!(
        trajectory_git(temp.path(), &["show", "HEAD:.decapod/governance.json"]),
        prior
    );
    assert!(
        crate::core::governance_document::read_section(temp.path(), "jev")
            .unwrap()
            .is_none()
    );
}

#[test]
fn invalid_replacement_preserves_committed_predecessor_without_partial_writes() {
    let temp = tempdir().unwrap();
    init_trajectory(temp.path(), replacement_init("run_old")).unwrap();
    commit_trajectory_journal(temp.path());
    let cookie = trajectory_cookie_path(temp.path());
    let prior = fs::read(&cookie).unwrap();
    let mut invalid = replacement_init("run_new");
    invalid.original_intent.clear();
    assert!(init_trajectory(temp.path(), invalid).is_err());
    assert_eq!(fs::read(&cookie).unwrap(), prior);
    assert!(load_trajectory(temp.path(), "run_old").is_ok());

    init_trajectory(temp.path(), replacement_init("run_new")).unwrap();
    assert_eq!(
        trajectory_git(temp.path(), &["show", "HEAD:.decapod/governance.json"]),
        prior
    );
}

#[test]
fn new_run_without_git_history_preserves_existing_journal() {
    let temp = tempdir().unwrap();
    init_trajectory(temp.path(), replacement_init("run_old")).unwrap();
    let cookie = trajectory_cookie_path(temp.path());
    let prior = fs::read(&cookie).unwrap();
    let error = init_trajectory(temp.path(), replacement_init("run_new")).unwrap_err();
    assert!(error.to_string().contains("TRAJECTORY_UNARCHIVED_WORK"));
    assert_eq!(fs::read(cookie).unwrap(), prior);
}

#[test]
fn inherited_governance_git_environment_child() {
    let Some(root) = std::env::var_os("DECAPOD_GOVERNANCE_GIT_ENV_FIXTURE") else {
        return;
    };
    let root = Path::new(&root);
    let cookie = trajectory_cookie_path(root);
    let before = fs::read(&cookie).unwrap();
    let error = init_trajectory(root, replacement_init("run_redirected")).unwrap_err();
    assert!(error.to_string().contains("TRAJECTORY_UNARCHIVED_WORK"));
    assert_eq!(fs::read(cookie).unwrap(), before);
}

#[test]
fn foreign_git_environment_cannot_archive_the_current_journal() {
    let actual = tempdir().unwrap();
    init_trajectory(actual.path(), replacement_init("run_old")).unwrap();
    commit_trajectory_journal(actual.path());
    record_trajectory(
        actual.path(),
        "run_old",
        TrajectoryUpdate {
            blockers: vec!["uncommitted failure".into()],
            ..Default::default()
        },
    )
    .unwrap();
    let cookie = trajectory_cookie_path(actual.path());
    let before = fs::read(&cookie).unwrap();
    let foreign = tempdir().unwrap();
    fs::create_dir_all(foreign.path().join(".decapod")).unwrap();
    fs::write(trajectory_cookie_path(foreign.path()), &before).unwrap();
    commit_trajectory_journal(foreign.path());
    assert_eq!(
        trajectory_git(foreign.path(), &["show", "HEAD:.decapod/governance.json"]),
        before
    );

    // The isolated child avoids changing process-global environment in a
    // parallel test binary. Its filesystem root and apparent Git root disagree.
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "core::trajectory::tests::inherited_governance_git_environment_child",
            "--nocapture",
        ])
        .env("DECAPOD_GOVERNANCE_GIT_ENV_FIXTURE", actual.path())
        .env("GIT_DIR", foreign.path().join(".git"))
        .env("GIT_COMMON_DIR", foreign.path().join(".git"))
        .env("GIT_WORK_TREE", foreign.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    assert_eq!(fs::read(cookie).unwrap(), before);
}

#[test]
fn git_replace_cannot_archive_the_current_journal() {
    let temp = tempdir().unwrap();
    init_trajectory(temp.path(), replacement_init("run_old")).unwrap();
    commit_trajectory_journal(temp.path());
    let original = String::from_utf8(trajectory_git(temp.path(), &["rev-parse", "HEAD"])).unwrap();
    record_trajectory(
        temp.path(),
        "run_old",
        TrajectoryUpdate {
            blockers: vec!["failure absent from actual HEAD".into()],
            ..Default::default()
        },
    )
    .unwrap();
    let cookie = trajectory_cookie_path(temp.path());
    let before = fs::read(&cookie).unwrap();
    commit_trajectory_journal(temp.path());
    let replacement =
        String::from_utf8(trajectory_git(temp.path(), &["rev-parse", "HEAD"])).unwrap();
    trajectory_git(temp.path(), &["reset", "--soft", original.trim()]);
    trajectory_git(
        temp.path(),
        &["replace", original.trim(), replacement.trim()],
    );
    assert_eq!(
        trajectory_git(temp.path(), &["show", "HEAD:.decapod/governance.json"]),
        before,
        "ordinary Git reads demonstrate the false archival source"
    );
    let error = init_trajectory(temp.path(), replacement_init("run_replaced")).unwrap_err();
    assert!(error.to_string().contains("TRAJECTORY_UNARCHIVED_WORK"));
    assert_eq!(fs::read(cookie).unwrap(), before);
}

#[test]
fn grafted_history_cannot_authorize_journal_replacement() {
    let temp = tempdir().unwrap();
    init_trajectory(temp.path(), replacement_init("run_old")).unwrap();
    commit_trajectory_journal(temp.path());
    let head = trajectory_git(temp.path(), &["rev-parse", "HEAD"]);
    fs::write(temp.path().join(".git/info/grafts"), head).unwrap();
    let cookie = trajectory_cookie_path(temp.path());
    let before = fs::read(&cookie).unwrap();
    let error = init_trajectory(temp.path(), replacement_init("run_grafted")).unwrap_err();
    assert!(error.to_string().contains("TRAJECTORY_UNARCHIVED_WORK"));
    assert!(error.to_string().contains("grafts"));
    assert_eq!(fs::read(cookie).unwrap(), before);
}

#[test]
fn fresh_workbranch_seed_remains_recordable_when_replacement_is_blocked() {
    let temp = tempdir().unwrap();
    crate::core::governance_document::migrate(temp.path()).unwrap();
    commit_trajectory_journal(temp.path());
    trajectory_git(temp.path(), &["checkout", "-b", "agent/test/task_seed"]);
    let context = crate::core::governance_document::BranchWorkContext {
        title: "Current task".into(),
        intent: "Preserve current task intent across workspace creation".into(),
        todo_ids: vec!["task_seed".into()],
        scope: vec!["src".into()],
    };
    crate::core::governance_document::initialize_workbranch(temp.path(), "main", Some(&context))
        .unwrap()
        .expect("fresh workbranch governance");
    let seed = load_trajectory_cookie(temp.path()).unwrap().unwrap();
    assert_eq!(seed.task_id.as_deref(), Some("task_seed"));
    assert_eq!(seed.original_intent, context.intent);
    let cookie = trajectory_cookie_path(temp.path());
    let before = fs::read(&cookie).unwrap();
    let error = init_trajectory(temp.path(), replacement_init("replacement")).unwrap_err();
    assert!(error.to_string().contains("TRAJECTORY_UNARCHIVED_WORK"));
    assert!(error.to_string().contains(&seed.run_id));
    assert_eq!(fs::read(&cookie).unwrap(), before);

    let continued = record_trajectory(
        temp.path(),
        &seed.run_id,
        TrajectoryUpdate {
            current_phase: Some("inspection".into()),
            inspected_files: vec!["src/lib.rs".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(continued.run_id, seed.run_id);
    assert_eq!(continued.task_id, seed.task_id);
    assert_eq!(continued.original_intent, seed.original_intent);
    assert_eq!(continued.inspected_files, vec!["src/lib.rs"]);
    assert_eq!(continued.proof_status, TrajectoryProofStatus::NoChecksRun);
    assert_eq!(
        continued.custody.events.len(),
        seed.custody.events.len() + 1
    );
}

#[test]
fn corrupted_current_cookie_is_not_masked_by_secondary_state() {
    let temp = tempdir().unwrap();
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_current".to_string(),
            task_id: None,
            intent_id: None,
            original_intent: "original".to_string(),
            derived_intent: "derived".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/lib.rs".to_string()],
            destination: None,
            current_phase: None,
            next_transitions: Vec::new(),
            blockers: Vec::new(),
        },
    )
    .unwrap();

    fs::write(trajectory_cookie_path(temp.path()), b"{\"corrupt\":true}").unwrap();

    assert!(load_trajectory_cookie(temp.path()).is_err());
    assert!(load_trajectory(temp.path(), "run_current").is_err());
}

#[test]
fn init_preserves_a_malformed_document_for_explicit_recovery() {
    let temp = tempdir().unwrap();
    let init = |run_id: &str| TrajectoryInit {
        run_id: run_id.to_string(),
        task_id: None,
        intent_id: None,
        original_intent: "original".to_string(),
        derived_intent: "derived".to_string(),
        active_boundaries: vec!["src/**".to_string()],
        repo_scope: vec!["src/decapod/core/trajectory.rs".to_string()],
        destination: None,
        current_phase: None,
        next_transitions: Vec::new(),
        blockers: Vec::new(),
    };

    init_trajectory(temp.path(), init("run_old")).unwrap();
    let cookie = trajectory_cookie_path(temp.path());
    let mut raw = fs::read_to_string(&cookie).unwrap();
    raw.push_str("\n{\"stale\":true}\n");
    fs::write(&cookie, &raw).unwrap();

    assert!(init_trajectory(temp.path(), init("run_new")).is_err());
    assert_eq!(fs::read_to_string(cookie).unwrap(), raw);
}

#[test]
fn trajectory_persists_portable_paths() {
    let temp = tempdir().unwrap();
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_paths".to_string(),
            task_id: None,
            intent_id: None,
            original_intent: "original".to_string(),
            derived_intent: "derived".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec![temp.path().join("src").to_string_lossy().to_string()],
            destination: None,
            current_phase: None,
            next_transitions: Vec::new(),
            blockers: Vec::new(),
        },
    )
    .unwrap();

    let outside = temp.path().parent().unwrap().join("secret.txt");
    let artifact = record_trajectory(
        temp.path(),
        "run_paths",
        TrajectoryUpdate {
            inspected_files: vec![temp.path().join("src/lib.rs").to_string_lossy().to_string()],
            modified_files: vec![outside.to_string_lossy().to_string()],
            ..TrajectoryUpdate::default()
        },
    )
    .unwrap();

    assert_eq!(artifact.repo_scope, vec!["src"]);
    assert_eq!(artifact.inspected_files, vec!["src/lib.rs"]);
    assert_eq!(artifact.modified_files, vec!["<external-path>"]);
}

#[test]
fn trajectory_proof_status_distinguishes_check_outcomes() {
    let temp = tempdir().unwrap();
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_2".to_string(),
            task_id: None,
            intent_id: None,
            original_intent: "original".to_string(),
            derived_intent: "derived".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/lib.rs".to_string()],
            destination: None,
            current_phase: None,
            next_transitions: Vec::new(),
            blockers: Vec::new(),
        },
    )
    .unwrap();

    let partial = record_trajectory(
        temp.path(),
        "run_2",
        TrajectoryUpdate {
            checks: vec![
                TrajectoryCheck {
                    name: "cargo test".to_string(),
                    status: TrajectoryCheckStatus::Passed,
                },
                TrajectoryCheck {
                    name: "cargo clippy".to_string(),
                    status: TrajectoryCheckStatus::Unavailable,
                },
            ],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(partial.proof_status, TrajectoryProofStatus::Partial);

    let failed = record_trajectory(
        temp.path(),
        "run_2",
        TrajectoryUpdate {
            checks: vec![TrajectoryCheck {
                name: "cargo test".to_string(),
                status: TrajectoryCheckStatus::Failed,
            }],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(failed.proof_status, TrajectoryProofStatus::Failed);
}

#[test]
fn shortcut_signal_emits_caution_and_completion_claim_is_not_proof() {
    let temp = tempdir().unwrap();
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_3".to_string(),
            task_id: None,
            intent_id: None,
            original_intent: "original".to_string(),
            derived_intent: "derived".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/lib.rs".to_string()],
            destination: None,
            current_phase: None,
            next_transitions: Vec::new(),
            blockers: Vec::new(),
        },
    )
    .unwrap();
    let artifact = record_trajectory(
        temp.path(),
        "run_3",
        TrajectoryUpdate {
            completion_claim: Some("done".to_string()),
            shortcut_risk_signals: vec!["completion claimed before checks".to_string()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(artifact.completion_claim.as_deref(), Some("done"));
    assert_eq!(artifact.proof_status, TrajectoryProofStatus::NoChecksRun);
    assert_eq!(artifact.verdicts.shortcut_risk, TrajectoryVerdict::Caution);
    assert_eq!(
        artifact.verdicts.completion_proof,
        TrajectoryVerdict::Unsupported
    );
}

#[test]
fn check_specs_are_typed_and_reject_unknown_statuses() {
    assert_eq!(
        parse_check_spec("cargo test=passed").unwrap().status,
        TrajectoryCheckStatus::Passed
    );
    assert!(parse_check_spec("cargo test=maybe").is_err());
}

#[test]
fn motion_context_is_recorded_and_state_stays_proof_backed() {
    let temp = tempdir().unwrap();
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_motion".to_string(),
            task_id: None,
            intent_id: None,
            original_intent: "original".to_string(),
            derived_intent: "derived".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/lib.rs".to_string()],
            destination: Some("published PR".to_string()),
            current_phase: Some("implementation".to_string()),
            next_transitions: vec!["validate".to_string(), "publish".to_string()],
            blockers: vec!["awaiting CI".to_string()],
        },
    )
    .unwrap();
    let blocked = load_trajectory(temp.path(), "run_motion").unwrap();
    assert_eq!(motion_state(&blocked), TrajectoryMotionState::Blocked);

    let completed = record_trajectory(
        temp.path(),
        "run_motion",
        TrajectoryUpdate {
            blockers: Vec::new(),
            clear_blockers: true,
            checks: vec![TrajectoryCheck {
                name: "validate".to_string(),
                status: TrajectoryCheckStatus::Passed,
            }],
            completion_claim: Some("complete".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(motion_state(&completed), TrajectoryMotionState::Completed);
    assert_eq!(completed.destination.as_deref(), Some("published PR"));
    assert_eq!(completed.current_phase.as_deref(), Some("implementation"));
    assert_eq!(completed.next_transitions, vec!["publish", "validate"]);
}

#[test]
fn motion_state_detects_stalled_and_abandoned() {
    let temp = tempdir().unwrap();
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_stall".to_string(),
            task_id: None,
            intent_id: None,
            original_intent: "original".to_string(),
            derived_intent: "derived".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/lib.rs".to_string()],
            destination: Some("published PR".to_string()),
            current_phase: Some("implementation".to_string()),
            next_transitions: vec!["validate".to_string()],
            blockers: vec![],
        },
    )
    .unwrap();

    let stalled = record_trajectory(
        temp.path(),
        "run_stall",
        TrajectoryUpdate {
            loops: vec![
                loop_record(
                    "run_stall",
                    "intent:run_stall",
                    "verify-loop",
                    TrajectoryLoopType::Verification,
                    1,
                    TrajectoryGraderResult::Fail,
                    "first fail",
                    vec![],
                    TrajectoryMutationProposal::None,
                    TrajectoryLoopStatus::Failed,
                ),
                loop_record(
                    "run_stall",
                    "intent:run_stall",
                    "verify-loop",
                    TrajectoryLoopType::Verification,
                    2,
                    TrajectoryGraderResult::Fail,
                    "second fail",
                    vec![],
                    TrajectoryMutationProposal::None,
                    TrajectoryLoopStatus::Failed,
                ),
            ],
            checks: vec![
                TrajectoryCheck {
                    name: "gate-a".to_string(),
                    status: TrajectoryCheckStatus::Failed,
                },
                TrajectoryCheck {
                    name: "gate-b".to_string(),
                    status: TrajectoryCheckStatus::Failed,
                },
            ],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(motion_state(&stalled), TrajectoryMotionState::Stalled);

    let abandoned = record_trajectory(
        temp.path(),
        "run_stall",
        TrajectoryUpdate {
            completion_claim: Some("done".to_string()),
            checks: vec![TrajectoryCheck {
                name: "validate".to_string(),
                status: TrajectoryCheckStatus::Failed,
            }],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(motion_state(&abandoned), TrajectoryMotionState::Abandoned);

    let abandoned_blocker = record_trajectory(
        temp.path(),
        "run_stall",
        TrajectoryUpdate {
            clear_blockers: true,
            blockers: vec!["abandoned by agent timeout".to_string()],
            completion_claim: None,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        motion_state(&abandoned_blocker),
        TrajectoryMotionState::Abandoned
    );
}

#[allow(clippy::too_many_arguments)]
fn loop_record(
    run_id: &str,
    intent_id: &str,
    loop_id: &str,
    loop_type: TrajectoryLoopType,
    attempt: u32,
    grader_result: TrajectoryGraderResult,
    feedback: &str,
    proof_refs: Vec<&str>,
    mutation_proposal: TrajectoryMutationProposal,
    status: TrajectoryLoopStatus,
) -> TrajectoryLoop {
    TrajectoryLoop {
        intent_id: intent_id.to_string(),
        trajectory_id: run_id.to_string(),
        loop_id: loop_id.to_string(),
        loop_type,
        attempt,
        parent_loop_id: None,
        trigger: TrajectoryTrigger::Human,
        tool_calls: vec!["agent.call".to_string()],
        observations: vec!["bounded observation".to_string()],
        grader_result,
        feedback: feedback.to_string(),
        proof_refs: proof_refs.into_iter().map(str::to_string).collect(),
        mutation_proposal,
        status,
        custody_event_id: None,
    }
}

#[test]
fn loop_attempts_preserve_feedback_and_proof_custody() {
    let temp = tempdir().unwrap();
    let intent_id = "intent:human-request";
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_loops".to_string(),
            task_id: None,
            intent_id: Some(intent_id.to_string()),
            original_intent: "human request".to_string(),
            derived_intent: "bounded loop execution".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/lib.rs".to_string()],
            destination: None,
            current_phase: None,
            next_transitions: Vec::new(),
            blockers: Vec::new(),
        },
    )
    .unwrap();
    let artifact = record_trajectory(
        temp.path(),
        "run_loops",
        TrajectoryUpdate {
            loops: vec![
                loop_record(
                    "run_loops",
                    intent_id,
                    "verify",
                    TrajectoryLoopType::Verification,
                    1,
                    TrajectoryGraderResult::Fail,
                    "assertion mismatch; rerun the bounded check",
                    Vec::new(),
                    TrajectoryMutationProposal::None,
                    TrajectoryLoopStatus::Retrying,
                ),
                loop_record(
                    "run_loops",
                    intent_id,
                    "verify",
                    TrajectoryLoopType::Verification,
                    2,
                    TrajectoryGraderResult::Pass,
                    "",
                    vec!["check:cargo-test", "artifact:trajectory"],
                    TrajectoryMutationProposal::None,
                    TrajectoryLoopStatus::Passed,
                ),
            ],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(artifact.loops.len(), 2);
    assert_eq!(artifact.loops[0].attempt, 1);
    assert_eq!(artifact.loops[0].feedback.len(), 43);
    assert_eq!(artifact.loops[1].proof_refs.len(), 2);
}

#[test]
fn event_and_improvement_loops_remain_inside_intent_boundary() {
    let temp = tempdir().unwrap();
    let intent_id = "intent:boundary";
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_event".to_string(),
            task_id: None,
            intent_id: Some(intent_id.to_string()),
            original_intent: "original".to_string(),
            derived_intent: "event follow-up".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/lib.rs".to_string()],
            destination: None,
            current_phase: None,
            next_transitions: Vec::new(),
            blockers: Vec::new(),
        },
    )
    .unwrap();
    let mut event = loop_record(
        "run_event",
        intent_id,
        "event",
        TrajectoryLoopType::Event,
        1,
        TrajectoryGraderResult::Skipped,
        "",
        Vec::new(),
        TrajectoryMutationProposal::None,
        TrajectoryLoopStatus::Open,
    );
    event.trigger = TrajectoryTrigger::Webhook;
    let mut improvement = loop_record(
        "run_event",
        intent_id,
        "improve",
        TrajectoryLoopType::Improvement,
        1,
        TrajectoryGraderResult::Skipped,
        "candidate harness change",
        Vec::new(),
        TrajectoryMutationProposal::Rubric,
        TrajectoryLoopStatus::Open,
    );
    improvement.parent_loop_id = Some("event".to_string());
    let artifact = record_trajectory(
        temp.path(),
        "run_event",
        TrajectoryUpdate {
            loops: vec![event, improvement],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        artifact
            .loops
            .iter()
            .all(|loop_record| loop_record.intent_id == intent_id)
    );
    assert_eq!(
        artifact.loops[1].mutation_proposal,
        TrajectoryMutationProposal::Rubric
    );
}

#[test]
fn loop_validation_rejects_unproven_passes_and_unbounded_feedback() {
    let temp = tempdir().unwrap();
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_invalid_loop".to_string(),
            task_id: None,
            intent_id: None,
            original_intent: "original".to_string(),
            derived_intent: "derived".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/lib.rs".to_string()],
            destination: None,
            current_phase: None,
            next_transitions: Vec::new(),
            blockers: Vec::new(),
        },
    )
    .unwrap();
    let invalid = loop_record(
        "run_invalid_loop",
        "intent:run_invalid_loop",
        "verify",
        TrajectoryLoopType::Verification,
        1,
        TrajectoryGraderResult::Pass,
        "",
        Vec::new(),
        TrajectoryMutationProposal::None,
        TrajectoryLoopStatus::Passed,
    );
    let error = record_trajectory(
        temp.path(),
        "run_invalid_loop",
        TrajectoryUpdate {
            loops: vec![invalid],
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("requires at least one proof reference")
    );
}

#[test]
fn trajectory_execution_carries_intent_custody_across_records() {
    let temp = tempdir().unwrap();
    let intent_id = "intent:custody-flow";
    init_trajectory(
        temp.path(),
        TrajectoryInit {
            run_id: "run_custody".to_string(),
            task_id: Some("task_custody".to_string()),
            intent_id: Some(intent_id.to_string()),
            original_intent: "preserve the human request exactly".to_string(),
            derived_intent: "record one bounded execution step".to_string(),
            active_boundaries: vec!["src/**".to_string()],
            repo_scope: vec!["src/decapod/core/trajectory.rs".to_string()],
            destination: None,
            current_phase: Some("refinement".to_string()),
            next_transitions: vec!["approve".to_string()],
            blockers: Vec::new(),
        },
    )
    .unwrap();

    let artifact = record_trajectory(
        temp.path(),
        "run_custody",
        TrajectoryUpdate {
            loops: vec![loop_record(
                "run_custody",
                intent_id,
                "custody-step",
                TrajectoryLoopType::Agent,
                1,
                TrajectoryGraderResult::Skipped,
                "",
                Vec::new(),
                TrajectoryMutationProposal::None,
                TrajectoryLoopStatus::Open,
            )],
            ..Default::default()
        },
    )
    .unwrap();

    let record = &artifact.custody.intents[intent_id];
    assert_eq!(record.raw_intent, "preserve the human request exactly");
    assert_eq!(record.status, crate::core::custody::IntentStatus::Refined);
    assert_eq!(artifact.custody.events.len(), 3);
    assert_eq!(artifact.custody.trajectories["run_custody"].steps.len(), 1);
    assert_eq!(
        artifact.loops[0].custody_event_id.as_deref(),
        Some(artifact.custody.events[2].event_id.as_str())
    );
    assert_eq!(
        load_trajectory(temp.path(), "run_custody").unwrap(),
        artifact
    );
}

fn native_proof_blocker() -> ProofBlocker {
    ProofBlocker {
        id: ProofBlockerId::NativeAppleMetal,
        detail: "Real Apple-arm64 Candle compilation and Metal execution remain unproven".into(),
        proof_refs: vec!["evidence:missing-native-execution".into()],
    }
}

#[test]
fn typed_proof_blockers_are_finite_bounded_and_legacy_compatible() {
    let blocker = native_proof_blocker();
    let value = serde_json::to_string(&blocker).unwrap();
    assert_eq!(parse_proof_blocker_json(&value).unwrap(), blocker);
    assert!(
        parse_proof_blocker_json(&value.replace("native-apple-metal", "arbitrary-proof")).is_err()
    );
    assert!(
        parse_proof_blocker_json(
            r#"{"id":"native-apple-metal","detail":"missing","proof_refs":[]}"#
        )
        .is_err()
    );
    assert!(parse_proof_blocker_json(r#"{"id":"native-apple-metal","detail":"missing","proof_refs":["evidence"],"ignore_human_stop":true}"#).is_err());
    assert!(ProofBlockerId::parse("native-vega-gpu").is_err());
    assert_eq!(blocker.id.required_checks().len(), 4);
    let temp = tempdir().unwrap();
    let artifact = init_trajectory(temp.path(), replacement_init("legacy")).unwrap();
    let legacy = serde_json::to_value(&artifact).unwrap();
    assert!(legacy.get("proof_blockers").is_none());
    let decoded: TrajectoryArtifact = serde_json::from_value(legacy).unwrap();
    assert!(decoded.proof_blockers.is_empty());
    assert_eq!(decoded.artifact_hash, artifact.artifact_hash);
}

#[test]
fn deliberate_classification_preserves_exact_detail_evidence_and_other_stops() {
    let temp = tempdir().unwrap();
    let blocker = native_proof_blocker();
    let mut init = replacement_init("classify");
    init.blockers = vec![
        blocker.detail.clone(),
        "Human authorization withheld".into(),
    ];
    init_trajectory(temp.path(), init).unwrap();
    let before = fs::read(trajectory_cookie_path(temp.path())).unwrap();
    let mut wrong = blocker.clone();
    wrong.detail.push('!');
    assert!(
        record_trajectory(
            temp.path(),
            "classify",
            TrajectoryUpdate {
                classify_proof_blockers: vec![wrong],
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(
        fs::read(trajectory_cookie_path(temp.path())).unwrap(),
        before
    );
    let artifact = record_trajectory(
        temp.path(),
        "classify",
        TrajectoryUpdate {
            classify_proof_blockers: vec![blocker.clone()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(artifact.blockers, ["Human authorization withheld"]);
    assert_eq!(
        artifact.proof_blockers.as_slice(),
        std::slice::from_ref(&blocker)
    );
    assert!(artifact.evidence.contains(&blocker.proof_refs[0]));
    assert!(
        serde_json::to_string(&artifact.custody)
            .unwrap()
            .contains("trajectory.classify_proof_blocker")
    );
    assert_eq!(motion_state(&artifact), TrajectoryMotionState::Blocked);
    let cleared = record_trajectory(
        temp.path(),
        "classify",
        TrajectoryUpdate {
            clear_blockers: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(cleared.blockers.is_empty());
    assert_eq!(cleared.proof_blockers, [blocker]);
    assert_eq!(motion_state(&cleared), TrajectoryMotionState::Blocked);
}

#[test]
fn typed_obligation_resolution_requires_all_fresh_checks_and_never_erases_history() {
    let temp = tempdir().unwrap();
    init_trajectory(temp.path(), replacement_init("resolve")).unwrap();
    let blocker = native_proof_blocker();
    let blocked = record_trajectory(
        temp.path(),
        "resolve",
        TrajectoryUpdate {
            proof_blockers: vec![blocker.clone()],
            checks: vec![TrajectoryCheck {
                name: "unrelated".into(),
                status: TrajectoryCheckStatus::Passed,
            }],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        blocked.verdicts.completion_proof,
        TrajectoryVerdict::Unsupported
    );
    let checks: Vec<_> = blocker
        .id
        .required_checks()
        .iter()
        .map(|name| TrajectoryCheck {
            name: (*name).into(),
            status: TrajectoryCheckStatus::Passed,
        })
        .collect();
    let before = fs::read(trajectory_cookie_path(temp.path())).unwrap();
    for count in 0..checks.len() {
        assert!(
            record_trajectory(
                temp.path(),
                "resolve",
                TrajectoryUpdate {
                    resolve_proof_blockers: vec![blocker.id],
                    checks: checks[..count].to_vec(),
                    evidence: vec!["provider:collected-native-checks".into()],
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert_eq!(
            fs::read(trajectory_cookie_path(temp.path())).unwrap(),
            before
        );
    }
    for status in [
        TrajectoryCheckStatus::Failed,
        TrajectoryCheckStatus::Partial,
        TrajectoryCheckStatus::Unavailable,
    ] {
        let mut conflicting = checks.clone();
        conflicting.push(TrajectoryCheck {
            name: checks[0].name.clone(),
            status,
        });
        assert!(
            record_trajectory(
                temp.path(),
                "resolve",
                TrajectoryUpdate {
                    resolve_proof_blockers: vec![blocker.id],
                    checks: conflicting,
                    evidence: vec!["provider:collected-native-checks".into()],
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert_eq!(
            fs::read(trajectory_cookie_path(temp.path())).unwrap(),
            before
        );
    }
    assert!(
        record_trajectory(
            temp.path(),
            "resolve",
            TrajectoryUpdate {
                resolve_proof_blockers: vec![blocker.id],
                checks: checks.clone(),
                ..Default::default()
            }
        )
        .is_err()
    );
    let resolved = record_trajectory(
        temp.path(),
        "resolve",
        TrajectoryUpdate {
            resolve_proof_blockers: vec![blocker.id],
            checks,
            evidence: vec!["provider:collected-native-checks".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(resolved.proof_blockers.is_empty());
    assert!(
        resolved
            .checks
            .iter()
            .any(|check| check.name == "unrelated")
    );
    assert!(
        resolved
            .evidence
            .contains(&"provider:collected-native-checks".into())
    );
    // These are journal declarations only; publication requires independent
    // full source-bound runtime evidence, never a trajectory status string.
}
