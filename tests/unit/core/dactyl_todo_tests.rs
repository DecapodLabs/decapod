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
