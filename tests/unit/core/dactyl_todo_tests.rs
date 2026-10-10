// Moved from src/decapod/core/dactyl_todo.rs
use super::*;
use crate::core::dactyl::DactylBridge;

const EVENTS_TABLE_SQL: &str = "CREATE TABLE events (event_id TEXT PRIMARY KEY, ts TEXT NOT NULL, seq INTEGER NOT NULL, stream TEXT NOT NULL, subject_kind TEXT, subject_id TEXT, event_type TEXT NOT NULL DEFAULT '', payload TEXT NOT NULL, actor TEXT NOT NULL DEFAULT 'decapod')";

fn local_store() -> Option<(DactylTodoStore, tempfile::TempDir)> {
    let tempdir = tempfile::tempdir().expect("Dactyl tempdir");
    let path = tempdir.path().join("tasks.db");
    std::fs::File::create(&path).expect("Dactyl SQLite file");
    let bridge = match DactylBridge::open_local(&path, dactyl_db::AccessMode::ReadWrite) {
        Ok(bridge) => bridge,
        Err(crate::core::error::DecapodError::DactylError(error))
            if error.adapter_code() == Some("sqlite_runtime_unavailable") =>
        {
            return None;
        }
        Err(error) => panic!("Dactyl file bridge: {error}"),
    };
    bridge
            .write(
                "CREATE TABLE tasks (repo_id TEXT NOT NULL DEFAULT 'DecapodLabs/decapod', id TEXT PRIMARY KEY, hash TEXT NOT NULL, title TEXT NOT NULL, description TEXT, status TEXT NOT NULL, assigned_to TEXT, scope TEXT NOT NULL, dir_path TEXT NOT NULL, priority TEXT NOT NULL, category TEXT NOT NULL, tags TEXT, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, version INTEGER NOT NULL DEFAULT 1, owner TEXT, assigned_at TEXT, completed_at TEXT)",
                &[],
            )
            .expect("task schema");
    bridge.write(EVENTS_TABLE_SQL, &[]).expect("event schema");
    bridge
        .write(
            "CREATE UNIQUE INDEX events_stream_seq ON events(stream, seq)",
            &[],
        )
        .expect("event sequence index");
    drop(bridge);
    let context =
        StorageContext::from_route(crate::core::backend::BackendRoute::Local { path }, None)
            .expect("local context");
    Some((
        DactylTodoStore::new(context, "DecapodLabs/decapod"),
        tempdir,
    ))
}

fn task(title: &str) -> Task {
    let now = Utc::now();
    Task {
        id: String::new(),
        repo_id: "DecapodLabs/decapod".to_string(),
        hash: String::new(),
        title: title.to_string(),
        description: Some("description".to_string()),
        status: "open".to_string(),
        assignee: None,
        scope: "repo".to_string(),
        dir_path: "".to_string(),
        priority: "medium".to_string(),
        category: "bugs".to_string(),
        tags: vec!["dactyl".to_string()],
        created_at: now,
        updated_at: now,
        version: 1,
    }
}

fn event_count(store: &DactylTodoStore) -> i64 {
    store
        .bridge()
        .expect("event bridge")
        .read("SELECT COUNT(*) AS count FROM events", &[])
        .expect("event count")
        .as_slice()
        .first()
        .expect("event count row")
        .get("count")
        .expect("event count value")
}

#[tokio::test]
async fn mutations_use_dactyl_atomic_write_and_observation() {
    let Some((store, _tempdir)) = local_store() else {
        eprintln!("skipping Dactyl todo adapter test: host SQLite runtime unavailable");
        return;
    };
    let added = store
        .add_task(
            task("cloud task"),
            "agent-a".to_string(),
            "intent".to_string(),
        )
        .await
        .expect("add");
    assert_eq!(added.status, "open");
    assert_eq!(event_count(&store), 1);
    assert_eq!(
        store.get_task(&added.id).await.expect("get").unwrap().id,
        added.id
    );
    assert_eq!(store.list_tasks().await.expect("list").len(), 1);

    let claimed = store
        .claim_task(&added.id, "agent-a".to_string())
        .await
        .expect("claim");
    assert_eq!(claimed.status, "in_progress");
    assert_eq!(claimed.assignee.as_deref(), Some("agent-a"));
    assert_eq!(claimed.version, added.version + 1);
    assert_eq!(event_count(&store), 2);

    let released = store
        .release_task(&added.id, "agent-a".to_string())
        .await
        .expect("release");
    assert_eq!(released.status, "open");
    assert_eq!(released.assignee.as_deref(), Some(""));
    assert_eq!(event_count(&store), 3);

    store
        .claim_task(&added.id, "agent-a".to_string())
        .await
        .expect("re-claim");
    assert_eq!(event_count(&store), 4);

    let completed = store
        .complete_task(&added.id, "agent-a".to_string(), String::new())
        .await
        .expect("complete");
    assert_eq!(completed.status, "completed");
    assert_eq!(event_count(&store), 5);
}

#[tokio::test]
async fn stale_claim_is_a_conflict_and_does_not_change_the_task() {
    let Some((store, _tempdir)) = local_store() else {
        eprintln!("skipping Dactyl todo adapter test: host SQLite runtime unavailable");
        return;
    };
    let added = store
        .add_task(
            task("only one winner"),
            "agent-a".to_string(),
            "intent".to_string(),
        )
        .await
        .expect("add");
    store
        .claim_task(&added.id, "agent-a".to_string())
        .await
        .expect("first claim");
    let events_before_conflict = event_count(&store);
    let error = store
        .claim_task(&added.id, "agent-b".to_string())
        .await
        .expect_err("second claim must conflict");
    assert!(error.to_string().contains("state conflict"));
    assert_eq!(event_count(&store), events_before_conflict);
    assert_eq!(
        store.list_tasks().await.expect("list")[0]
            .assignee
            .as_deref(),
        Some("agent-a")
    );
}

#[tokio::test]
async fn retry_reconciles_one_immutable_creation_event_after_task_changes() {
    let Some((store, _tempdir)) = local_store() else {
        panic!("SQLite required for retry regression");
    };
    let mut original = task("resumable task");
    original.id = "todo_resumable_original".into();
    original.hash = "resume".into();
    let intent = format!("{}operation-one", cloud_todo_operation::INTENT_PREFIX);
    // Dropping the successful response models the client's lost acknowledgement.
    let _ = store
        .add_task(original.clone(), "agent-a".into(), intent.clone())
        .await
        .unwrap();
    store
        .claim_task(&original.id, "agent-a".into())
        .await
        .unwrap();
    let retried = store
        .add_task(original.clone(), "agent-a".into(), intent.clone())
        .await
        .unwrap();
    assert_eq!(retried.id, original.id);
    assert_eq!(retried.status, "in_progress");
    assert_eq!(
        event_count(&store),
        2,
        "one creation and one claim, no replay event"
    );
    assert_eq!(store.list_tasks().await.unwrap().len(), 1);
    let mut changed = original.clone();
    changed.description = Some("different input".into());
    let error = store
        .add_task(changed, "agent-a".into(), intent.clone())
        .await
        .unwrap_err();
    assert_eq!(
        cloud_todo_operation::anyhow_failure(&error).1,
        FailureKind::Conflict
    );
    assert_eq!(event_count(&store), 2);
    let resumed = store
        .add_task(original, "other-agent".into(), intent)
        .await
        .unwrap();
    assert_eq!(resumed.status, "in_progress");
    assert_eq!(event_count(&store), 2);
    let actor: String = store
        .bridge()
        .unwrap()
        .read(
            "SELECT actor FROM events WHERE event_type = 'task.add'",
            &[],
        )
        .unwrap()
        .as_slice()[0]
        .get("actor")
        .unwrap();
    assert_eq!(
        actor, "agent-a",
        "retry never overwrites original audit attribution"
    );
}

#[tokio::test]
async fn failed_atomic_creation_can_retry_without_partial_task_or_event() {
    let Some((store, _tempdir)) = local_store() else {
        panic!("SQLite required for retry regression");
    };
    let mut original = task("atomic retry");
    original.id = "todo_atomic_retry".into();
    original.hash = "atomic".into();
    let intent = format!("{}atomic-operation", cloud_todo_operation::INTENT_PREFIX);
    store.bridge().unwrap().write("CREATE TRIGGER reject_add BEFORE INSERT ON events BEGIN SELECT RAISE(ABORT, 'fixture failure'); END", &[]).unwrap();
    let error = store
        .add_task(original.clone(), "agent-a".into(), intent.clone())
        .await
        .unwrap_err();
    assert_eq!(
        cloud_todo_operation::anyhow_failure(&error).0,
        Outcome::OutcomeUnknown
    );
    assert!(store.list_tasks().await.unwrap().is_empty());
    assert_eq!(event_count(&store), 0);
    store
        .bridge()
        .unwrap()
        .write("DROP TRIGGER reject_add", &[])
        .unwrap();
    store
        .add_task(original, "agent-a".into(), intent)
        .await
        .unwrap();
    assert_eq!(event_count(&store), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_same_operation_retries_never_duplicate_task_or_event() {
    let Some((store, _tempdir)) = local_store() else {
        panic!("SQLite required for concurrent retry regression");
    };
    let mut original = task("concurrent retry");
    original.id = "todo_concurrent_retry".into();
    original.hash = "concur".into();
    let intent = format!(
        "{}concurrent-operation",
        cloud_todo_operation::INTENT_PREFIX
    );
    let first = store.clone();
    let second = store.clone();
    let (a, b) = tokio::join!(
        first.add_task(original.clone(), "agent-a".into(), intent.clone()),
        second.add_task(original.clone(), "agent-a".into(), intent.clone())
    );
    assert!(
        a.is_ok() || b.is_ok(),
        "at least one attempt must commit: {a:?} {b:?}"
    );
    store
        .add_task(original, "agent-a".into(), intent)
        .await
        .unwrap();
    assert_eq!(store.list_tasks().await.unwrap().len(), 1);
    assert_eq!(event_count(&store), 1);
}
