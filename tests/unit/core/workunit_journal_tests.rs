use super::*;
use crate::core::{db, events};
use std::sync::{Arc, Barrier};

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    // Broker mutations evaluate policy against the initialized runtime store.
    // An events-only file is not a substitute for the TODO-owned policy schema.
    crate::core::todo::initialize_todo_db(&root.path().join(".decapod/data")).unwrap();
    init_workunit(root.path(), "task_test", "intent:test").unwrap();
    root
}

#[test]
fn journal_initializes_and_fences_prepared_manifest_updates() {
    let root = fixture();
    let status = workunit_journal_status(root.path(), "task_test").unwrap();
    assert_eq!(status.generation, Some(1));
    assert_eq!(status.projection_state, "current");
    assert!(!status.resumable);
    let mut first = load_workunit(root.path(), "task_test").unwrap();
    let mut stale = first.clone();
    first.state_refs.push("state:first".into());
    stale.state_refs.push("state:stale".into());
    write_workunit_checked(root.path(), &first, "first-update", 1).unwrap();
    let error = write_workunit_checked(root.path(), &stale, "stale-update", 1).unwrap_err();
    assert!(error.to_string().contains("WORKUNIT_GENERATION_CONFLICT"));
    assert_eq!(load_workunit(root.path(), "task_test").unwrap(), first);
    assert!(
        write_workunit(root.path(), &stale)
            .unwrap_err()
            .to_string()
            .contains("EXPECTED_GENERATION")
    );
}

#[test]
fn duplicate_operations_are_idempotent_but_changed_bytes_conflict() {
    let root = fixture();
    let mut manifest = load_workunit(root.path(), "task_test").unwrap();
    manifest.state_refs.push("state:first".into());
    write_workunit_checked(root.path(), &manifest, "one-operation", 1).unwrap();
    write_workunit_checked(root.path(), &manifest, "one-operation", 1).unwrap();
    assert_eq!(journal_records(root.path(), "task_test").unwrap().len(), 2);
    manifest.state_refs.push("state:changed".into());
    assert!(
        write_workunit_checked(root.path(), &manifest, "one-operation", 1)
            .unwrap_err()
            .to_string()
            .contains("WORKUNIT_OPERATION_CONFLICT")
    );
}

#[test]
fn interrupted_projection_publication_recovers_exactly_without_another_event() {
    let root = fixture();
    let previous = journal_records(root.path(), "task_test")
        .unwrap()
        .pop()
        .unwrap();
    let mut changed = previous.manifest.clone();
    changed.state_refs.push("state:committed".into());
    append_snapshot(
        root.path(),
        &changed,
        "interrupted",
        "mutation",
        1,
        None,
        Default::default(),
    )
    .unwrap();
    assert!(
        load_workunit(root.path(), "task_test")
            .unwrap_err()
            .to_string()
            .contains("WORKUNIT_RECOVERY_REQUIRED")
    );
    let first = recover_workunit(root.path(), "task_test").unwrap();
    let second = recover_workunit(root.path(), "task_test").unwrap();
    assert_eq!(first, second);
    assert_eq!(first.generation, Some(2));
    assert!(!first.resumable);
    assert_eq!(load_workunit(root.path(), "task_test").unwrap(), changed);
    assert_eq!(journal_records(root.path(), "task_test").unwrap().len(), 2);
}

#[test]
fn truncated_projection_recovers_but_unknown_valid_contents_are_preserved() {
    let root = fixture();
    let path = workunit_path(root.path(), "task_test").unwrap();
    fs::write(&path, b"{\"task_id\":").unwrap();
    recover_workunit(root.path(), "task_test").unwrap();
    let mut unknown = load_workunit(root.path(), "task_test").unwrap();
    unknown.intent_ref = "intent:unrecognized".into();
    fs::write(&path, serde_json::to_vec(&unknown).unwrap()).unwrap();
    assert!(
        recover_workunit(root.path(), "task_test")
            .unwrap_err()
            .to_string()
            .contains("WORKUNIT_PROJECTION_CONFLICT")
    );
    assert_eq!(
        load_workunit_projection(root.path(), "task_test").unwrap(),
        unknown
    );
}

#[test]
fn conflicting_or_corrupt_journal_never_repairs_projection() {
    let root = fixture();
    let path = workunit_path(root.path(), "task_test").unwrap();
    let original = fs::read(&path).unwrap();
    let data = root.path().join(".decapod/data");
    let db_path = events::canonical_db_path(&data);
    let conn = db::db_connect(&db_path.to_string_lossy()).unwrap();
    let mut record = journal_records_on_conn(&conn, "task_test")
        .unwrap()
        .pop()
        .unwrap();
    record.manifest.intent_ref = "intent:corrupt".into();
    conn.execute(
        "UPDATE events SET payload = ?1 WHERE event_id = ?2",
        db::params![serde_json::to_string(&record).unwrap(), record.event_id],
    )
    .unwrap();
    drop(conn);
    assert!(
        recover_workunit(root.path(), "task_test")
            .unwrap_err()
            .to_string()
            .contains("WORKUNIT_JOURNAL_INVALID")
    );
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn reordered_predecessor_is_rejected_even_with_recomputed_record_hash() {
    let root = fixture();
    add_state_ref(root.path(), "task_test", "state:next").unwrap();
    let data = root.path().join(".decapod/data");
    let conn = db::db_connect(&events::canonical_db_path(&data).to_string_lossy()).unwrap();
    let mut record = journal_records_on_conn(&conn, "task_test")
        .unwrap()
        .pop()
        .unwrap();
    record.predecessor = Some("sha256:wrong-predecessor".into());
    record.record_hash = record_hash(&record).unwrap();
    conn.execute(
        "UPDATE events SET payload = ?1 WHERE event_id = ?2",
        db::params![serde_json::to_string(&record).unwrap(), record.event_id],
    )
    .unwrap();
    drop(conn);
    assert!(
        workunit_journal_status(root.path(), "task_test")
            .unwrap_err()
            .to_string()
            .contains("WORKUNIT_JOURNAL_CONFLICT")
    );
}

#[test]
fn concurrent_read_modify_write_preserves_every_attachment() {
    let root = fixture();
    let barrier = Arc::new(Barrier::new(8));
    let mut threads = Vec::new();
    for index in 0..8 {
        let path = root.path().to_path_buf();
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            add_state_ref(&path, "task_test", &format!("state:{index}")).unwrap();
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(
        load_workunit(root.path(), "task_test")
            .unwrap()
            .state_refs
            .len(),
        8
    );
    assert_eq!(journal_records(root.path(), "task_test").unwrap().len(), 9);
}

#[test]
fn legacy_baseline_is_explicit_and_does_not_reanimate_verified_proof() {
    let root = tempfile::tempdir().unwrap();
    crate::core::todo::initialize_todo_db(&root.path().join(".decapod/data")).unwrap();
    let manifest = WorkUnitManifest {
        task_id: "legacy_task".into(),
        intent_ref: "intent:legacy".into(),
        spec_refs: vec![],
        state_refs: vec![],
        proof_plan: vec!["check".into()],
        proof_results: vec![WorkUnitProofResult {
            gate: "check".into(),
            status: "pass".into(),
            artifact_ref: None,
            evaluator_epoch: None,
            validation_epoch: None,
        }],
        validation_epoch: None,
        status: WorkUnitStatus::Verified,
    };
    publish_projection(root.path(), &manifest).unwrap();
    assert_eq!(
        workunit_journal_status(root.path(), "legacy_task")
            .unwrap()
            .projection_state,
        "legacy_unjournaled"
    );
    let baseline = with_workunit_lock(root.path(), || {
        baseline_if_needed(root.path(), "legacy_task")
    })
    .unwrap();
    assert_eq!(baseline.kind, "legacy_baseline");
    assert!(baseline.legacy_baseline);
    assert!(baseline.binding.is_none());
    fs::remove_file(workunit_path(root.path(), "legacy_task").unwrap()).unwrap();
    assert!(
        recover_workunit(root.path(), "legacy_task")
            .unwrap_err()
            .to_string()
            .contains("WORKUNIT_CHECKPOINT_REQUIRED")
    );
    assert!(!workunit_path(root.path(), "legacy_task").unwrap().exists());
}

#[test]
fn proof_hashes_bind_regular_bytes_and_reject_missing_or_external_symlinked_artifacts() {
    let root = fixture();
    let mut manifest = load_workunit(root.path(), "task_test").unwrap();
    fs::write(root.path().join("proof.txt"), "original proof").unwrap();
    manifest.proof_results.push(WorkUnitProofResult {
        gate: "check".into(),
        status: "pass".into(),
        artifact_ref: Some("proof.txt".into()),
        evaluator_epoch: None,
        validation_epoch: None,
    });
    let original = proof_artifact_hashes(root.path(), &manifest).unwrap();
    fs::write(root.path().join("proof.txt"), "altered proof").unwrap();
    assert_ne!(
        proof_artifact_hashes(root.path(), &manifest).unwrap(),
        original
    );
    fs::remove_file(root.path().join("proof.txt")).unwrap();
    assert!(proof_artifact_hashes(root.path(), &manifest).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/dev/null", root.path().join("proof.txt")).unwrap();
        assert!(
            proof_artifact_hashes(root.path(), &manifest)
                .unwrap_err()
                .to_string()
                .contains("WORKUNIT_PROOF_UNVERIFIABLE")
        );
    }
}

#[test]
fn changed_proof_plan_invalidates_prior_results_without_claiming_new_success() {
    let root = fixture();
    set_proof_plan(root.path(), "task_test", &["old-check".into()]).unwrap();
    record_proof_result(root.path(), "task_test", "old-check", "pass", None).unwrap();
    let manifest = set_proof_plan(root.path(), "task_test", &["new-check".into()]).unwrap();
    assert!(manifest.proof_results.is_empty());
    assert!(
        !workunit_journal_status(root.path(), "task_test")
            .unwrap()
            .resumable
    );
}

#[test]
fn checkpoint_cannot_rebind_any_unbound_historical_pass_to_current_source() {
    for status in [
        WorkUnitStatus::Draft,
        WorkUnitStatus::Executing,
        WorkUnitStatus::Claimed,
        WorkUnitStatus::Verified,
    ] {
        let root = tempfile::tempdir().unwrap();
        crate::core::todo::initialize_todo_db(&root.path().join(".decapod/data")).unwrap();
        let manifest = WorkUnitManifest {
            task_id: "historical_task".into(),
            intent_ref: "intent:old-source".into(),
            spec_refs: vec![],
            state_refs: vec![],
            proof_plan: vec!["old-check".into()],
            proof_results: vec![WorkUnitProofResult {
                gate: "old-check".into(),
                status: "pass".into(),
                artifact_ref: Some("proof.txt".into()),
                evaluator_epoch: None,
                validation_epoch: None,
            }],
            validation_epoch: None,
            status,
        };
        fs::write(root.path().join("proof.txt"), "old proof bytes").unwrap();
        publish_projection(root.path(), &manifest).unwrap();
        let error = checkpoint_workunit(
            root.path(),
            "historical_task",
            "new-source-checkpoint",
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("WORKUNIT_CHECKPOINT_REQUIRED"));
        let records = journal_records(root.path(), "historical_task").unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].kind, "legacy_baseline");
        assert!(records[0].binding.is_none());
        assert!(records[0].proof_hashes.is_empty());
    }
}

#[test]
fn recording_another_gate_cannot_rehash_retained_passing_proof() {
    let root = fixture();
    let mut previous = load_workunit(root.path(), "task_test").unwrap();
    previous.proof_results.push(WorkUnitProofResult {
        gate: "gate-a".into(),
        status: "pass".into(),
        artifact_ref: Some("proof-a.txt".into()),
        evaluator_epoch: None,
        validation_epoch: None,
    });
    fs::write(root.path().join("proof-a.txt"), "original proof").unwrap();
    let old_hashes = proof_artifact_hashes(root.path(), &previous).unwrap();
    let mut current = previous.clone();
    current.proof_results.push(WorkUnitProofResult {
        gate: "gate-b".into(),
        status: "fail".into(),
        artifact_ref: None,
        evaluator_epoch: None,
        validation_epoch: None,
    });
    fs::write(root.path().join("proof-a.txt"), "changed proof").unwrap();
    let new_hashes = proof_artifact_hashes(root.path(), &current).unwrap();
    assert!(
        verify_retained_proof_hashes(
            &previous,
            &current,
            &old_hashes,
            &new_hashes,
            Some("gate-b")
        )
        .unwrap_err()
        .to_string()
        .contains("WORKUNIT_PROOF_CHANGED")
    );
    // Explicitly re-recording gate A is the only operation allowed to acquire
    // new A bytes (the caller separately verifies the current source binding).
    verify_retained_proof_hashes(
        &previous,
        &previous,
        &old_hashes,
        &new_hashes,
        Some("gate-a"),
    )
    .unwrap();
}

#[test]
fn shared_artifact_cannot_be_rebound_while_another_gate_retains_it() {
    let root = fixture();
    let mut previous = load_workunit(root.path(), "task_test").unwrap();
    for gate in ["gate-a", "gate-b"] {
        previous.proof_results.push(WorkUnitProofResult {
            gate: gate.into(),
            status: "pass".into(),
            artifact_ref: Some("shared-proof.txt".into()),
            evaluator_epoch: None,
            validation_epoch: None,
        });
    }
    fs::write(root.path().join("shared-proof.txt"), "original").unwrap();
    let old_hashes = proof_artifact_hashes(root.path(), &previous).unwrap();
    fs::write(root.path().join("shared-proof.txt"), "changed").unwrap();
    let new_hashes = proof_artifact_hashes(root.path(), &previous).unwrap();
    assert!(
        verify_retained_proof_hashes(
            &previous,
            &previous,
            &old_hashes,
            &new_hashes,
            Some("gate-b")
        )
        .unwrap_err()
        .to_string()
        .contains("WORKUNIT_PROOF_CHANGED")
    );
}

#[test]
fn indexed_task_subject_mismatch_is_corruption_not_missing_history() {
    let root = fixture();
    let data = root.path().join(".decapod/data");
    let conn = db::db_connect(&events::canonical_db_path(&data).to_string_lossy()).unwrap();
    conn.execute(
        "UPDATE events SET subject_id = 'other_task' WHERE stream = ?1",
        db::params![events::WORKUNIT],
    )
    .unwrap();
    let error = journal_records_on_conn(&conn, "other_task").unwrap_err();
    assert!(error.to_string().contains("WORKUNIT_JOURNAL_INVALID"));
}

#[test]
fn oversized_manifest_is_rejected_before_read_or_journal_commit() {
    let root = fixture();
    let mut manifest = load_workunit(root.path(), "task_test").unwrap();
    manifest
        .state_refs
        .push("x".repeat(MAX_MANIFEST_BYTES as usize));
    assert!(
        write_workunit_checked(root.path(), &manifest, "oversized", 1)
            .unwrap_err()
            .to_string()
            .contains("WORKUNIT_JOURNAL_CAPACITY")
    );
    assert_eq!(journal_records(root.path(), "task_test").unwrap().len(), 1);
    let path = workunit_path(root.path(), "task_test").unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(MAX_MANIFEST_BYTES + 1)
        .unwrap();
    assert!(
        load_workunit_projection(root.path(), "task_test")
            .unwrap_err()
            .to_string()
            .contains("WORKUNIT_PROJECTION_INVALID")
    );
}

fn accepted_execution_fixture() -> tempfile::TempDir {
    use crate::core::trajectory::{self, TrajectoryInit};
    use crate::plan_governance::{self, InitPlanInput, PlanPatch, PlanState, ScopeConstraints};
    let root = fixture();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .current_dir(root.path())
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "fixture git failed: {args:?}");
    };
    git(&["init", "-b", "work"]);
    git(&["config", "user.name", "Workunit Test"]);
    git(&["config", "user.email", "workunit@example.invalid"]);
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/app.rs"), "fn main() {}\n").unwrap();
    fs::write(root.path().join(".gitignore"), ".decapod/data/\n").unwrap();
    plan_governance::init_plan(
        root.path(),
        InitPlanInput {
            title: "Continue accepted work".into(),
            intent: "Preserve approved scope".into(),
            todo_ids: vec!["task_test".into()],
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
        root.path(),
        PlanPatch {
            state: Some(PlanState::Approved),
            ..Default::default()
        },
    )
    .unwrap();
    trajectory::init_trajectory(
        root.path(),
        TrajectoryInit {
            run_id: "run_test".into(),
            task_id: Some("task_test".into()),
            intent_id: Some("intent:test".into()),
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
    let store = root.path().join(".decapod/data");
    crate::core::todo::initialize_todo_db(&store).unwrap();
    let conn = db::db_connect(&crate::core::todo::todo_db_path(&store).to_string_lossy()).unwrap();
    conn.execute("INSERT INTO tasks(id,hash,title,status,created_at,updated_at,dir_path,scope,assigned_to,lease_expires_at,lease_generation,lease_lifecycle,intent_anchor,revision) VALUES(?1,'fixture','Execution','open','1Z','1Z',?2,'repo',?3,?4,1,'claimed','intent:test',1)",
        db::params!["task_test", root.path().to_string_lossy().as_ref(), crate::current_agent_id(), format!("{}Z", chrono::Utc::now().timestamp()+3600)]).unwrap();
    drop(conn);
    git(&["add", "."]);
    git(&[
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-m",
        "workunit fixture",
    ]);
    root
}

#[test]
fn current_checkpoint_resume_rejects_source_drift_without_advancing_journal() {
    let root = accepted_execution_fixture();
    let checkpoint = checkpoint_workunit(root.path(), "task_test", "checkpoint", Some(1)).unwrap();
    assert!(checkpoint.resumable);
    fs::write(root.path().join("src/app.rs"), "fn main() { panic!(); }\n").unwrap();
    let error =
        resume_workunit(root.path(), "task_test", "resume", checkpoint.generation).unwrap_err();
    assert!(error.to_string().contains("EXECUTION_STATE_CHANGED"));
    assert_eq!(journal_records(root.path(), "task_test").unwrap().len(), 2);
    assert!(
        !workunit_journal_status(root.path(), "task_test")
            .unwrap()
            .resumable
    );
}

#[test]
fn checkpoint_and_resume_retries_preserve_unknown_valid_projection() {
    for resume in [false, true] {
        let root = accepted_execution_fixture();
        checkpoint_workunit(root.path(), "task_test", "checkpoint", Some(1)).unwrap();
        if resume {
            resume_workunit(root.path(), "task_test", "resume", Some(2)).unwrap();
        }
        let mut divergent = load_workunit(root.path(), "task_test").unwrap();
        divergent.state_refs.push("state:unrecognized".into());
        publish_projection(root.path(), &divergent).unwrap();
        let error = if resume {
            resume_workunit(root.path(), "task_test", "resume", Some(2)).unwrap_err()
        } else {
            checkpoint_workunit(root.path(), "task_test", "checkpoint", Some(1)).unwrap_err()
        };
        assert!(error.to_string().contains("WORKUNIT_RECOVERY_REQUIRED"));
        assert_eq!(
            load_workunit_projection(root.path(), "task_test").unwrap(),
            divergent
        );
        assert!(
            recover_workunit(root.path(), "task_test")
                .unwrap_err()
                .to_string()
                .contains("WORKUNIT_PROJECTION_CONFLICT")
        );
    }
}

#[test]
fn fresh_bound_proof_is_resumable_but_modified_artifacts_cannot_be_reanimated() {
    let root = accepted_execution_fixture();
    set_proof_plan(root.path(), "task_test", &["gate-a".into()]).unwrap();
    checkpoint_workunit(root.path(), "task_test", "checkpoint", Some(2)).unwrap();
    let artifact = ".decapod/data/proof-a.txt";
    fs::write(root.path().join(artifact), "fresh proof").unwrap();
    record_proof_result(
        root.path(),
        "task_test",
        "gate-a",
        "pass",
        Some(artifact.into()),
    )
    .unwrap();
    transition_status(root.path(), "task_test", WorkUnitStatus::Executing).unwrap();
    transition_status(root.path(), "task_test", WorkUnitStatus::Claimed).unwrap();
    transition_status(root.path(), "task_test", WorkUnitStatus::Verified).unwrap();
    assert!(
        workunit_journal_status(root.path(), "task_test")
            .unwrap()
            .resumable
    );
    fs::remove_file(workunit_path(root.path(), "task_test").unwrap()).unwrap();
    let recovered = recover_workunit(root.path(), "task_test").unwrap();
    assert!(recovered.resumable);
    let resumed = resume_workunit(
        root.path(),
        "task_test",
        "resume-verified",
        recovered.generation,
    )
    .unwrap();
    assert!(resumed.resumable);
    assert_eq!(
        load_workunit(root.path(), "task_test").unwrap().status,
        WorkUnitStatus::Verified
    );
    fs::write(root.path().join(artifact), "altered proof").unwrap();
    assert!(
        record_proof_result(root.path(), "task_test", "unrelated", "fail", None)
            .unwrap_err()
            .to_string()
            .contains("WORKUNIT_PROOF_CHANGED")
    );
    fs::remove_file(workunit_path(root.path(), "task_test").unwrap()).unwrap();
    assert!(
        recover_workunit(root.path(), "task_test")
            .unwrap_err()
            .to_string()
            .contains("WORKUNIT_PROOF_CHANGED")
    );
    assert!(!workunit_path(root.path(), "task_test").unwrap().exists());
}

#[test]
fn checkpoint_refuses_workunit_intent_unrelated_to_authoritative_task() {
    let root = accepted_execution_fixture();
    let records = journal_records(root.path(), "task_test").unwrap();
    let mut manifest = records.last().unwrap().manifest.clone();
    manifest.intent_ref = "intent:unrelated".into();
    // Deliberately construct a historical snapshot to test explicit adoption.
    let record = append_snapshot(
        root.path(),
        &manifest,
        "legacy-change",
        "mutation",
        1,
        None,
        Default::default(),
    )
    .unwrap();
    publish_projection(root.path(), &record.manifest).unwrap();
    assert!(
        checkpoint_workunit(root.path(), "task_test", "checkpoint", Some(2))
            .unwrap_err()
            .to_string()
            .contains("WORKUNIT_INTENT_CONFLICT")
    );
}

#[test]
fn resume_fences_changed_authoritative_task_lease() {
    let root = accepted_execution_fixture();
    checkpoint_workunit(root.path(), "task_test", "checkpoint", Some(1)).unwrap();
    let conn = db::db_connect(
        &crate::core::todo::todo_db_path(&root.path().join(".decapod/data")).to_string_lossy(),
    )
    .unwrap();
    conn.execute(
        "UPDATE tasks SET lease_generation = lease_generation + 1 WHERE id = 'task_test'",
        [],
    )
    .unwrap();
    drop(conn);
    assert!(
        resume_workunit(root.path(), "task_test", "resume", Some(2))
            .unwrap_err()
            .to_string()
            .contains("EXECUTION_STATE_CHANGED")
    );
    assert_eq!(journal_records(root.path(), "task_test").unwrap().len(), 2);
}

#[test]
fn newly_failed_proof_downgrades_verified_state() {
    let root = accepted_execution_fixture();
    set_proof_plan(root.path(), "task_test", &["gate-a".into()]).unwrap();
    checkpoint_workunit(root.path(), "task_test", "checkpoint", Some(2)).unwrap();
    let artifact = ".decapod/data/proof-a.txt";
    fs::write(root.path().join(artifact), "fresh proof").unwrap();
    record_proof_result(
        root.path(),
        "task_test",
        "gate-a",
        "pass",
        Some(artifact.into()),
    )
    .unwrap();
    transition_status(root.path(), "task_test", WorkUnitStatus::Executing).unwrap();
    transition_status(root.path(), "task_test", WorkUnitStatus::Claimed).unwrap();
    transition_status(root.path(), "task_test", WorkUnitStatus::Verified).unwrap();
    let failed = record_proof_result(root.path(), "task_test", "gate-a", "fail", None).unwrap();
    assert_eq!(failed.status, WorkUnitStatus::Claimed);
    assert!(transition_status(root.path(), "task_test", WorkUnitStatus::Verified).is_err());
}

#[test]
fn proof_reads_deduplicate_and_enforce_total_count_and_deadline_budgets() {
    let root = fixture();
    let mut manifest = load_workunit(root.path(), "task_test").unwrap();
    let proof = |gate: &str, artifact: &str| WorkUnitProofResult {
        gate: gate.into(),
        status: "pass".into(),
        artifact_ref: Some(artifact.into()),
        evaluator_epoch: None,
        validation_epoch: None,
    };
    fs::write(root.path().join("first.txt"), "one").unwrap();
    fs::write(root.path().join("second.txt"), "two").unwrap();
    manifest.proof_results = vec![proof("a", "first.txt"), proof("b", "first.txt")];
    let timeout = std::time::Duration::from_secs(5);
    let hashes =
        proof_artifact_hashes_with_limits(root.path(), &manifest, 1, 3, 3, timeout).unwrap();
    assert_eq!(hashes.len(), 1, "shared reference is read only once");
    assert!(
        proof_artifact_hashes_with_limits(
            root.path(),
            &manifest,
            1,
            3,
            3,
            std::time::Duration::ZERO
        )
        .unwrap_err()
        .to_string()
        .contains("WORKUNIT_PROOF_LIMIT")
    );
    manifest.proof_results[1] = proof("b", "second.txt");
    assert!(
        proof_artifact_hashes_with_limits(root.path(), &manifest, 1, 3, 6, timeout)
            .unwrap_err()
            .to_string()
            .contains("WORKUNIT_PROOF_LIMIT")
    );
    assert!(
        proof_artifact_hashes_with_limits(root.path(), &manifest, 2, 3, 5, timeout)
            .unwrap_err()
            .to_string()
            .contains("WORKUNIT_PROOF_LIMIT")
    );
    assert!(
        proof_artifact_hashes_with_limits(root.path(), &manifest, 2, 2, 6, timeout)
            .unwrap_err()
            .to_string()
            .contains("WORKUNIT_PROOF_UNVERIFIABLE")
    );
}

#[cfg(unix)]
#[test]
fn proof_opener_refuses_symlinked_ancestors_outside_pinned_root() {
    let root = fixture();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("proof.txt"), "outside proof").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("artifact-dir")).unwrap();
    let mut manifest = load_workunit(root.path(), "task_test").unwrap();
    manifest.proof_results.push(WorkUnitProofResult {
        gate: "check".into(),
        status: "pass".into(),
        artifact_ref: Some("artifact-dir/proof.txt".into()),
        evaluator_epoch: None,
        validation_epoch: None,
    });
    assert!(
        proof_artifact_hashes(root.path(), &manifest)
            .unwrap_err()
            .to_string()
            .contains("WORKUNIT_PROOF_UNVERIFIABLE")
    );
}

#[test]
fn bare_or_partial_runtime_is_rejected_before_creating_workunit_state() {
    let bare = tempfile::tempdir().unwrap();
    let error = init_workunit(bare.path(), "task_test", "intent:test").unwrap_err();
    assert!(error.to_string().contains("WORKUNIT_RUNTIME_UNINITIALIZED"));
    assert!(error.to_string().contains("decapod init"));
    assert!(!bare.path().join(".decapod").exists());

    let partial = tempfile::tempdir().unwrap();
    let data = partial.path().join(".decapod/data");
    fs::create_dir_all(&data).unwrap();
    let path = events::canonical_db_path(&data);
    let conn = db::db_connect(&path.to_string_lossy()).unwrap();
    events::ensure_tables(&conn).unwrap();
    drop(conn);
    let original = fs::read(&path).unwrap();
    let error = init_workunit(partial.path(), "task_test", "intent:test").unwrap_err();
    assert!(error.to_string().contains("WORKUNIT_RUNTIME_UNINITIALIZED"));
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(!workunits_dir(partial.path()).exists());
    let conn = db::db_connect_for_validate(&path.to_string_lossy()).unwrap();
    assert!(!conn.has_table("risk_zones").unwrap());
    assert!(!conn.has_table("agents").unwrap());
    assert!(
        journal_records_on_conn(&conn, "task_test")
            .unwrap()
            .is_empty()
    );
}
