use super::*;
use crate::core::db::Connection;
use crate::core::store::StoreKind;
use tempfile::{TempDir, tempdir};

const ID: &str = "bugs_completion_fixture";

fn fixture() -> (TempDir, Store) {
    let dir = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: dir.path().join("store"),
    };
    todo::initialize_todo_db(&store.root).unwrap();
    fs::write(dir.path().join("proof.txt"), "verified material").unwrap();
    with_conn(&store, |conn| {
        conn.execute(
            "INSERT INTO tasks(id, hash, title, status, created_at, updated_at, dir_path, scope, revision)
             VALUES(?1, 'completion', 'Completion fixture', 'claimed', '1Z', '1Z', '.', '', 4)",
            [ID],
        ).unwrap();
    });
    (dir, store)
}

fn with_conn<T>(store: &Store, f: impl FnOnce(&Connection) -> T) -> T {
    DbBroker::new(&store.root)
        .with_conn(
            &todo::todo_db_path(&store.root),
            "test",
            None,
            "test.verify",
            |conn| Ok(f(conn)),
        )
        .unwrap()
}

fn prepare(store: &Store, root: &Path) -> PreparedTodoVerification {
    prepare_verified_completion_with(store, root, ID, vec!["proof.txt".into()], || {
        Ok((true, "sha256:passing-output".into()))
    })
    .unwrap()
}

fn count(store: &Store, sql: &str) -> i64 {
    with_conn(store, |conn| {
        conn.query_row(sql, [], |row| row.get(0)).unwrap()
    })
}

fn complete(conn: &Connection) -> Result<(), error::DecapodError> {
    conn.execute(
        "UPDATE tasks SET status = 'done', revision = revision + 1 WHERE id = ?1",
        [ID],
    )?;
    Ok(())
}

fn target(store: &Store, id: &str) -> VerifyTarget {
    load_targets(store, Some(id)).unwrap().remove(0)
}

fn capture_done(store: &Store, root: &Path, id: &str) -> VerifyTarget {
    let prepared =
        prepare_verified_completion_with(store, root, id, vec!["proof.txt".into()], || {
            Ok((true, "sha256:passing-output".into()))
        })
        .unwrap();
    DbBroker::new(&store.root)
        .with_transaction(
            &todo::todo_db_path(&store.root),
            "test",
            None,
            "test.complete",
            |conn| {
                conn.execute(
                    "UPDATE tasks SET status = 'done', completed_at = '2Z', revision = revision + 1 WHERE id = ?1",
                    [id],
                )?;
                prepared.commit_on_connection(conn)
            },
        )
        .unwrap();
    target(store, id)
}

fn passing_snapshot() -> ValidationSnapshot {
    ValidationSnapshot {
        passed: true,
        output_hash: "sha256:passing-output".into(),
    }
}

fn artifacts(target: &VerifyTarget) -> serde_json::Value {
    serde_json::from_str(target.artifacts.as_deref().unwrap()).unwrap()
}

fn target_snapshot(target: &VerifyTarget) -> serde_json::Value {
    serde_json::json!({
        "todo_id": target.todo_id,
        "revision": target.revision,
        "status": target.status,
        "proof_plan": target.proof_plan,
        "artifacts": target.artifacts,
        "last_verified_at": target.last_verified_at,
        "last_verified_status": target.last_verified_status,
        "verification_policy_days": target.verification_policy_days,
    })
}

fn baseline_rows(store: &Store) -> Vec<serde_json::Value> {
    with_conn(store, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT todo_id, proof_plan, verification_artifacts, last_verified_at,
                    last_verified_status, last_verified_notes, verification_policy_days, updated_at
             FROM task_verification ORDER BY todo_id",
            )
            .unwrap();
        stmt.query_map([], |row| {
            Ok(serde_json::json!([
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<String>>(7)?,
            ]))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    })
}

fn verification_events(store: &Store, event_type: &str) -> Vec<events::StoredEvent> {
    events::query(&store.root, events::TODO, usize::MAX)
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type == event_type)
        .collect()
}

// Faults are confined to each temporary fixture database. Fail the canonical
// event insert after projection mutations to prove the whole write rolls back.
fn reject_verification_event(store: &Store, event_type: &str, id: &str) {
    with_conn(store, |conn| {
        conn.execute(&format!(
            "CREATE TRIGGER reject_verification_event BEFORE INSERT ON events
             WHEN NEW.stream = 'todo' AND NEW.event_type = '{event_type}' AND NEW.subject_id = '{id}'
             BEGIN SELECT RAISE(ABORT, 'injected canonical verification event failure'); END;"
        ), []).unwrap();
    });
}

fn batch_fixture() -> (TempDir, Store, Vec<VerifyTarget>) {
    let (dir, store) = fixture();
    let first = capture_done(&store, dir.path(), ID);
    const SECOND: &str = "bugs_completion_second";
    with_conn(&store, |conn| {
        conn.execute(
            "INSERT INTO tasks(id, hash, title, status, created_at, updated_at, dir_path, scope, revision)
             VALUES(?1, 'second', 'Second fixture', 'claimed', '1Z', '1Z', '.', '', 8)",
            [SECOND],
        ).unwrap();
    });
    let second = capture_done(&store, dir.path(), SECOND);
    (dir, store, vec![first, second])
}

#[test]
fn failed_validation_preserves_task_and_passing_baseline_with_failure_history() {
    let (dir, store) = fixture();
    with_conn(&store, |conn| {
        conn.execute("INSERT INTO task_verification(todo_id, proof_plan, verification_artifacts, last_verified_status, updated_at) VALUES(?1, '[]', 'old-evidence', 'pass', '1Z')", [ID]).unwrap();
    });
    let result =
        prepare_verified_completion_with(&store, dir.path(), ID, vec!["proof.txt".into()], || {
            Ok((false, "sha256:failed-output".into()))
        });
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("validate did not pass")
    );
    assert_eq!(
        todo::get_task(&store.root, ID).unwrap().unwrap().status,
        "claimed"
    );
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM task_verification WHERE last_verified_status = 'pass' AND verification_artifacts = 'old-evidence'"
        ),
        1
    );
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM events WHERE event_type = 'task.verify.capture_failed'"
        ),
        1
    );
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM events WHERE event_type = 'task.verify.capture'"
        ),
        0
    );
}

#[test]
fn missing_artifact_fails_before_validation_and_records_attempt() {
    let (dir, store) = fixture();
    let result = prepare_verified_completion_with(
        &store,
        dir.path(),
        ID,
        vec!["missing.txt".into()],
        || panic!("missing artifact must prevent validation execution"),
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("artifact file not found")
    );
    assert_eq!(
        todo::get_task(&store.root, ID).unwrap().unwrap().revision,
        4
    );
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM events WHERE event_type = 'task.verify.capture_failed'"
        ),
        1
    );
}

#[test]
fn preparation_is_non_completing_and_binds_task_snapshot() {
    let (dir, store) = fixture();
    let prepared = prepare(&store, dir.path());
    assert!(prepared.matches_task(ID, 4, "claimed"));
    assert!(!prepared.matches_task(ID, 5, "claimed"));
    assert!(!prepared.matches_task("different", 4, "claimed"));
    assert_eq!(prepared.expected_revision(), 4);
    assert_eq!(prepared.expected_status(), "claimed");
    assert_eq!(count(&store, "SELECT COUNT(*) FROM task_verification"), 0);
    assert_eq!(
        todo::get_task(&store.root, ID).unwrap().unwrap().status,
        "claimed"
    );
}

#[test]
fn prepared_capture_and_status_commit_together() {
    let (dir, store) = fixture();
    let prepared = prepare(&store, dir.path());
    DbBroker::new(&store.root)
        .with_transaction(
            &todo::todo_db_path(&store.root),
            "test",
            None,
            "test.complete",
            |conn| {
                complete(conn)?;
                prepared.commit_on_connection(conn)
            },
        )
        .unwrap();
    assert_eq!(
        todo::get_task(&store.root, ID).unwrap().unwrap().status,
        "done"
    );
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM task_verification WHERE last_verified_status = 'pass'"
        ),
        1
    );
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM events WHERE event_type = 'task.verify.capture'"
        ),
        1
    );
    let captured = target(&store, ID);
    assert_eq!(captured.revision, 5);
    assert_eq!(artifacts(&captured)["task_id"], ID);
    assert_eq!(artifacts(&captured)["task_revision"], 5);
    let events = verification_events(&store, "task.verify.capture");
    assert_eq!(events[0].payload["task_id"], ID);
    assert_eq!(
        events[0].payload["payload"]["verification_artifacts"],
        artifacts(&captured)
    );
}

#[test]
fn transaction_failure_rolls_back_status_baseline_and_capture_event() {
    let (dir, store) = fixture();
    let prepared = prepare(&store, dir.path());
    let result: Result<(), error::DecapodError> = DbBroker::new(&store.root).with_transaction(
        &todo::todo_db_path(&store.root),
        "test",
        None,
        "test.complete",
        |conn| {
            complete(conn)?;
            prepared.commit_on_connection(conn)?;
            Err(error::DecapodError::ValidationError(
                "injected subsequent event failure".into(),
            ))
        },
    );
    assert!(result.is_err());
    assert_eq!(
        todo::get_task(&store.root, ID).unwrap().unwrap().status,
        "claimed"
    );
    assert_eq!(count(&store, "SELECT COUNT(*) FROM task_verification"), 0);
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM events WHERE event_type = 'task.verify.capture'"
        ),
        0
    );
}

#[test]
fn changed_artifact_aborts_completion_transaction() {
    let (dir, store) = fixture();
    let prepared = prepare(&store, dir.path());
    fs::write(dir.path().join("proof.txt"), "changed after validation").unwrap();
    let result = DbBroker::new(&store.root).with_transaction(
        &todo::todo_db_path(&store.root),
        "test",
        None,
        "test.complete",
        |conn| {
            complete(conn)?;
            prepared.commit_on_connection(conn)
        },
    );
    assert!(result.unwrap_err().to_string().contains("artifact changed"));
    assert_eq!(
        todo::get_task(&store.root, ID).unwrap().unwrap().status,
        "claimed"
    );
    assert_eq!(count(&store, "SELECT COUNT(*) FROM task_verification"), 0);
}

#[test]
fn changed_revision_cannot_consume_prepared_capture() {
    let (dir, store) = fixture();
    let prepared = prepare(&store, dir.path());
    let result = DbBroker::new(&store.root).with_transaction(
        &todo::todo_db_path(&store.root),
        "test",
        None,
        "test.complete",
        |conn| {
            conn.execute(
                "UPDATE tasks SET status = 'done', revision = 6 WHERE id = ?1",
                [ID],
            )?;
            prepared.commit_on_connection(conn)
        },
    );
    assert!(result.unwrap_err().to_string().contains("Task changed"));
    assert_eq!(count(&store, "SELECT COUNT(*) FROM task_verification"), 0);
}

#[test]
fn capture_rejects_autocommit_and_validation_time_material_drift() {
    let (dir, store) = fixture();
    let prepared = prepare(&store, dir.path());
    with_conn(&store, |conn| {
        assert!(
            prepared
                .commit_on_connection(conn)
                .unwrap_err()
                .to_string()
                .contains("existing transaction")
        );
    });
    let result =
        prepare_verified_completion_with(&store, dir.path(), ID, vec!["proof.txt".into()], || {
            fs::write(dir.path().join("proof.txt"), "changed while validating").unwrap();
            Ok((true, "sha256:passing-output".into()))
        });
    assert!(result.unwrap_err().to_string().contains("artifact changed"));
}

#[test]
fn changed_epoch_aborts_completion_without_replacing_evidence() {
    let (dir, store) = fixture();
    let prepared = prepare(&store, dir.path());
    fs::write(dir.path().join("AGENTS.md"), "changed governance material").unwrap();
    let result = DbBroker::new(&store.root).with_transaction(
        &todo::todo_db_path(&store.root),
        "test",
        None,
        "test.complete",
        |conn| {
            complete(conn)?;
            prepared.commit_on_connection(conn)
        },
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Validation material changed")
    );
    assert_eq!(
        todo::get_task(&store.root, ID).unwrap().unwrap().status,
        "claimed"
    );
    assert_eq!(count(&store, "SELECT COUNT(*) FROM task_verification"), 0);
}

#[test]
fn qa_recapture_requires_done_and_retains_task_revision() {
    let (dir, store) = fixture();
    with_conn(&store, |conn| {
        conn.execute("UPDATE tasks SET status = 'done' WHERE id = ?1", [ID])
            .unwrap();
    });
    let prepared = prepare(&store, dir.path());
    DbBroker::new(&store.root)
        .with_transaction(
            &todo::todo_db_path(&store.root),
            "test",
            None,
            "test.recapture",
            |conn| prepared.commit_recapture_on_connection(conn),
        )
        .unwrap();
    assert_eq!(
        todo::get_task(&store.root, ID).unwrap().unwrap().revision,
        4
    );
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM task_verification WHERE last_verified_status = 'pass'"
        ),
        1
    );
    let captured = target(&store, ID);
    assert_eq!(artifacts(&captured)["task_id"], ID);
    assert_eq!(artifacts(&captured)["task_revision"], 4);
    assert_eq!(
        verification_events(&store, "task.verify.capture")[0].payload["payload"]["verification_artifacts"]
            ["task_revision"],
        4
    );
}

#[test]
fn validator_execution_error_is_not_successful_capture() {
    let (dir, store) = fixture();
    let result =
        prepare_verified_completion_with(&store, dir.path(), ID, vec!["proof.txt".into()], || {
            Err(error::DecapodError::ValidationError(
                "validator unavailable".into(),
            ))
        });
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("validator unavailable")
    );
    assert_eq!(
        todo::get_task(&store.root, ID).unwrap().unwrap().status,
        "claimed"
    );
    assert_eq!(count(&store, "SELECT COUNT(*) FROM task_verification"), 0);
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM events WHERE event_type = 'task.verify.capture_failed'"
        ),
        1
    );
}

#[test]
fn matching_validator_output_cannot_pass_missing_or_stale_task_binding() {
    let (dir, store) = fixture();
    let current = capture_done(&store, dir.path(), ID);
    let snapshot = passing_snapshot();
    assert_eq!(
        verify_target(&current, &store.root, dir.path(), Some(&snapshot))
            .unwrap()
            .status,
        "pass"
    );
    for (label, task_id, revision) in [
        ("legacy unbound", None, None),
        ("missing identity", None, Some(5)),
        ("missing revision", Some(ID), None),
        ("another task", Some("bugs_other"), Some(5)),
        ("older revision", Some(ID), Some(4)),
        ("future revision", Some(ID), Some(6)),
        ("invalid revision", Some(ID), Some(-1)),
    ] {
        let mut unbound = current.clone();
        let mut proof = artifacts(&current);
        proof["task_id"] = serde_json::json!(task_id);
        proof["task_revision"] = serde_json::json!(revision);
        unbound.artifacts = Some(proof.to_string());
        let result = verify_target(&unbound, &store.root, dir.path(), Some(&snapshot)).unwrap();
        assert_eq!(result.status, "fail", "{label}");
        assert!(
            result
                .notes
                .iter()
                .any(|note| note.contains("STALE_TASK_VERIFICATION")),
            "{label}"
        );
        assert!(
            is_stale(
                &unbound,
                epoch_secs(current.last_verified_at.as_deref().unwrap()).unwrap()
            ),
            "{label}"
        );
    }
}

#[test]
fn current_pass_persists_exact_snapshot_and_canonical_result_event() {
    let (dir, store) = fixture();
    let captured = capture_done(&store, dir.path(), ID);
    let result = verify_target(
        &captured,
        &store.root,
        dir.path(),
        Some(&passing_snapshot()),
    )
    .unwrap();
    assert_eq!(result.status, "pass");
    let written =
        persist_result(&store, &captured, &result.status, "current proof passed").unwrap();
    assert_eq!(
        target_snapshot(&written),
        target_snapshot(&target(&store, ID))
    );
    assert_eq!(written.artifacts, captured.artifacts);
    assert_eq!(written.proof_plan, captured.proof_plan);
    assert_eq!(written.revision, 5);
    assert_eq!(written.last_verified_status.as_deref(), Some("pass"));
    let events = verification_events(&store, "task.verify.result");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].payload["task_id"], ID);
    assert_eq!(
        events[0].payload["payload"]["verification_artifacts"],
        artifacts(&written)
    );
    assert_eq!(events[0].payload["payload"]["last_verified_status"], "pass");
    assert_eq!(
        events[0].payload["payload"]["last_verified_notes"],
        "current proof passed"
    );
    assert_eq!(
        Some(events[0].ts.as_str()),
        written.last_verified_at.as_deref()
    );

    // Recovery's final aggregate result must use the just-written snapshot.
    let rejected = persist_result(&store, &written, "fail", "final aggregate failed").unwrap();
    assert_eq!(
        target_snapshot(&rejected),
        target_snapshot(&target(&store, ID))
    );
    assert_eq!(rejected.last_verified_status.as_deref(), Some("fail"));
    assert_eq!(verification_events(&store, "task.verify.result").len(), 2);
}

#[test]
fn stale_pass_and_fail_cannot_overwrite_changed_task_or_baseline_fields() {
    for outcome in ["pass", "fail"] {
        for mutation in [
            "UPDATE tasks SET revision = revision + 1 WHERE id = ?1",
            "UPDATE tasks SET status = 'open' WHERE id = ?1",
            "UPDATE task_verification SET proof_plan = '[ \"validate_passes\" ]' WHERE todo_id = ?1",
            "UPDATE task_verification SET verification_artifacts = verification_artifacts || ' ' WHERE todo_id = ?1",
            "UPDATE task_verification SET last_verified_status = 'CLAIMED' WHERE todo_id = ?1",
            "UPDATE task_verification SET last_verified_at = 'newer-verification' WHERE todo_id = ?1",
            "UPDATE task_verification SET verification_policy_days = 7 WHERE todo_id = ?1",
            "DELETE FROM task_verification WHERE todo_id = ?1",
        ] {
            let (dir, store) = fixture();
            let stale = capture_done(&store, dir.path(), ID);
            with_conn(&store, |conn| {
                conn.execute(mutation, [ID]).unwrap();
            });
            let before = baseline_rows(&store);
            let current = target_snapshot(&target(&store, ID));
            let error =
                persist_result(&store, &stale, outcome, "late verifier result").unwrap_err();
            assert!(
                error.to_string().contains("STALE_TASK_VERIFICATION"),
                "{outcome}: {mutation}: {error}"
            );
            assert_eq!(baseline_rows(&store), before, "{outcome}: {mutation}");
            assert_eq!(target_snapshot(&target(&store, ID)), current);
            assert!(verification_events(&store, "task.verify.result").is_empty());
        }
    }
}

#[test]
fn fresh_snapshot_cannot_promote_proof_bound_to_old_or_missing_revision() {
    let (dir, store) = fixture();
    capture_done(&store, dir.path(), ID);
    with_conn(&store, |conn| {
        conn.execute(
            "UPDATE tasks SET revision = revision + 1 WHERE id = ?1",
            [ID],
        )
        .unwrap();
    });
    let stale = target(&store, ID);
    let before = baseline_rows(&store);
    assert_eq!(
        verify_target(&stale, &store.root, dir.path(), Some(&passing_snapshot()))
            .unwrap()
            .status,
        "fail"
    );
    assert!(
        persist_result(&store, &stale, "pass", "old proof")
            .unwrap_err()
            .to_string()
            .contains("STALE_TASK_VERIFICATION")
    );
    assert_eq!(baseline_rows(&store), before);

    let mut unbound = artifacts(&stale);
    unbound.as_object_mut().unwrap().remove("task_revision");
    with_conn(&store, |conn| {
        conn.execute(
            "UPDATE task_verification SET verification_artifacts = ?1 WHERE todo_id = ?2",
            crate::core::db::params![unbound.to_string(), ID],
        )
        .unwrap();
    });
    let current = target(&store, ID);
    let before = baseline_rows(&store);
    assert!(
        persist_result(&store, &current, "pass", "legacy proof")
            .unwrap_err()
            .to_string()
            .contains("STALE_TASK_VERIFICATION")
    );
    assert_eq!(baseline_rows(&store), before);
    assert!(verification_events(&store, "task.verify.result").is_empty());
}

#[test]
fn result_event_failure_rolls_back_the_verification_projection() {
    let (dir, store) = fixture();
    let captured = capture_done(&store, dir.path(), ID);
    let before = baseline_rows(&store);
    reject_verification_event(&store, "task.verify.result", ID);
    let error = persist_result(&store, &captured, "fail", "must roll back").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected canonical verification event failure")
    );
    assert_eq!(baseline_rows(&store), before);
    assert!(verification_events(&store, "task.verify.result").is_empty());
    assert_eq!(
        target_snapshot(&target(&store, ID)),
        target_snapshot(&captured)
    );
}

#[test]
fn prune_reopens_and_increments_revision_without_allowing_stale_reinsertion() {
    for outcome in ["pass", "fail"] {
        let (dir, store) = fixture();
        let stale = capture_done(&store, dir.path(), ID);
        prune_verification_for_todo(&store, ID).unwrap();
        let reopened = todo::get_task(&store.root, ID).unwrap().unwrap();
        assert_eq!(reopened.status, "open");
        assert_eq!(reopened.revision, stale.revision + 1);
        assert!(reopened.completed_at.is_none());
        assert!(baseline_rows(&store).is_empty());
        let events = verification_events(&store, "task.verify.prune");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["task_id"], ID);
        assert!(
            persist_result(&store, &stale, outcome, "late result after prune")
                .unwrap_err()
                .to_string()
                .contains("STALE_TASK_VERIFICATION")
        );
        assert!(baseline_rows(&store).is_empty());
        assert!(verification_events(&store, "task.verify.result").is_empty());
    }
}

#[test]
fn prune_event_failure_rolls_back_delete_reopen_and_revision() {
    let (dir, store) = fixture();
    let captured = capture_done(&store, dir.path(), ID);
    let before = baseline_rows(&store);
    reject_verification_event(&store, "task.verify.prune", ID);
    let error = prune_verification_for_todo(&store, ID).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected canonical verification event failure")
    );
    assert_eq!(baseline_rows(&store), before);
    assert_eq!(
        target_snapshot(&target(&store, ID)),
        target_snapshot(&captured)
    );
    assert_eq!(
        todo::get_task(&store.root, ID)
            .unwrap()
            .unwrap()
            .completed_at
            .as_deref(),
        Some("2Z")
    );
    assert!(verification_events(&store, "task.verify.prune").is_empty());
}

#[test]
fn prune_then_recomplete_cannot_consume_the_original_prepared_capture() {
    let (dir, store) = fixture();
    let prepared = prepare(&store, dir.path());
    prune_verification_for_todo(&store, ID).unwrap();
    with_conn(&store, |conn| {
        conn.execute("UPDATE tasks SET status = 'claimed' WHERE id = ?1", [ID])
            .unwrap();
    });
    let result = DbBroker::new(&store.root).with_transaction(
        &todo::todo_db_path(&store.root),
        "test",
        None,
        "test.stale_complete",
        |conn| {
            complete(conn)?;
            prepared.commit_on_connection(conn)
        },
    );
    assert!(result.unwrap_err().to_string().contains("Task changed"));
    assert!(baseline_rows(&store).is_empty());
    let task = todo::get_task(&store.root, ID).unwrap().unwrap();
    assert_eq!(task.status, "claimed");
    assert_eq!(task.revision, 5);
    assert!(verification_events(&store, "task.verify.capture").is_empty());
}

#[test]
fn recovery_binds_current_revision_and_remains_claimed_until_verification() {
    let (dir, store) = fixture();
    capture_done(&store, dir.path(), ID);
    with_conn(&store, |conn| {
        conn.execute(
            "UPDATE tasks SET revision = revision + 1 WHERE id = ?1",
            [ID],
        )
        .unwrap();
    });
    let stale_proof = target(&store, ID);
    let snapshot = ValidationSnapshot {
        passed: true,
        output_hash: "sha256:recovered-output".into(),
    };
    let recovered = refresh_batch_baselines(
        &store,
        &[stale_proof],
        &snapshot,
        &active_validation_epoch(dir.path()).unwrap(),
    )
    .unwrap()
    .remove(0);
    assert_eq!(
        target_snapshot(&recovered),
        target_snapshot(&target(&store, ID))
    );
    assert_eq!(recovered.last_verified_status.as_deref(), Some("CLAIMED"));
    assert_eq!(artifacts(&recovered)["task_id"], ID);
    assert_eq!(artifacts(&recovered)["task_revision"], 6);
    assert_eq!(
        artifacts(&recovered)["proof_plan_results"][0]["output_hash"],
        snapshot.output_hash
    );
    assert_eq!(load_targets(&store, None).unwrap().len(), 1);
    let events = verification_events(&store, "task.verify.recover");
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].payload["payload"]["last_verified_status"],
        "CLAIMED"
    );
    assert_eq!(
        events[0].payload["payload"]["verification_artifacts"],
        artifacts(&recovered)
    );
    let result = verify_target(&recovered, &store.root, dir.path(), Some(&snapshot)).unwrap();
    assert_eq!(result.status, "pass");
    let written = persist_result(
        &store,
        &recovered,
        &result.status,
        "recovered proof verified",
    )
    .unwrap();
    assert_eq!(
        target_snapshot(&written),
        target_snapshot(&target(&store, ID))
    );
    assert!(load_targets(&store, None).unwrap().is_empty());
}

#[test]
fn failed_recovery_snapshot_cannot_replace_any_baseline() {
    let (dir, store, targets) = batch_fixture();
    let before = baseline_rows(&store);
    let snapshot = ValidationSnapshot {
        passed: false,
        output_hash: "sha256:failed-output".into(),
    };
    let error = refresh_batch_baselines(
        &store,
        &targets,
        &snapshot,
        &active_validation_epoch(dir.path()).unwrap(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("failing validation snapshot"));
    assert_eq!(baseline_rows(&store), before);
    assert!(verification_events(&store, "task.verify.recover").is_empty());
}

#[test]
fn stale_second_batch_target_is_checked_before_the_first_write() {
    for mutation in [
        "UPDATE tasks SET revision = revision + 1 WHERE id = ?1",
        "UPDATE task_verification SET last_verified_at = 'newer-batch' WHERE todo_id = ?1",
    ] {
        let (dir, store, targets) = batch_fixture();
        with_conn(&store, |conn| {
            conn.execute(mutation, [&targets[1].todo_id]).unwrap();
            conn.execute(
                &format!(
                    "CREATE TRIGGER reject_early_batch_write BEFORE UPDATE ON task_verification
                 WHEN NEW.todo_id = '{ID}'
                 BEGIN SELECT RAISE(ABORT, 'batch wrote before checking every target'); END;"
                ),
                [],
            )
            .unwrap();
        });
        let before = baseline_rows(&store);
        let error = refresh_batch_baselines(
            &store,
            &targets,
            &passing_snapshot(),
            &active_validation_epoch(dir.path()).unwrap(),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("STALE_TASK_VERIFICATION"),
            "{mutation}: {error}"
        );
        assert_eq!(baseline_rows(&store), before);
        assert!(verification_events(&store, "task.verify.recover").is_empty());
    }
}

#[test]
fn second_recovery_event_failure_rolls_back_both_baselines_and_events() {
    let (dir, store, targets) = batch_fixture();
    let before = baseline_rows(&store);
    reject_verification_event(&store, "task.verify.recover", &targets[1].todo_id);
    let error = refresh_batch_baselines(
        &store,
        &targets,
        &passing_snapshot(),
        &active_validation_epoch(dir.path()).unwrap(),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected canonical verification event failure")
    );
    assert_eq!(baseline_rows(&store), before);
    assert!(verification_events(&store, "task.verify.recover").is_empty());
    for original in &targets {
        assert_eq!(
            target_snapshot(&target(&store, &original.todo_id)),
            target_snapshot(original)
        );
    }
}
