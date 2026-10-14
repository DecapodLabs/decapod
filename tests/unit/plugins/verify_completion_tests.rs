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
