use decapod::core::bounded_process::BoundedCommand;
use decapod::core::broker::DbBroker;
use decapod::core::db::Connection;
use decapod::core::events;
use decapod::core::pool::global_pool;
use decapod::core::schemas;
use decapod::core::store::Store;
use decapod::core::store::StoreKind;
use decapod::core::todo::{
    ClaimMode, TodoCommand, add_task, check_trust_level, claim_task, claim_task_with_lease,
    fleet_health, get_task, get_work_claim, handoff_task_as, initialize_todo_db, list_tasks,
    rebuild_from_events, renew_claim_lease, update_status, yield_claim_lease,
};
use decapod::plugins::policy;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};
use tempfile::tempdir;

fn resolve_decapod_bin() -> PathBuf {
    let cargo_bin = env!("CARGO_BIN_EXE_decapod");
    if let Ok(path) = Path::new(cargo_bin).canonicalize() {
        return path;
    }
    if let Ok(runfiles_dir) = std::env::var("RUNFILES_DIR") {
        let path = Path::new(&runfiles_dir).join("_main").join("decapod");
        if path.exists() {
            return path;
        }
    }
    if let Some(parent) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|path| path.to_path_buf()))
    {
        let path = parent.join("decapod");
        if path.exists() {
            return path;
        }
    }
    PathBuf::from(cargo_bin)
}

fn assert_typed_todo_id(id: &str) {
    let (task_type, body) = id
        .split_once('_')
        .expect("id should contain type separator");
    assert_eq!(task_type.len(), 4, "type prefix must be 4 chars");
    assert_eq!(body.len(), 16, "id body must be 16 chars");
    assert!(
        task_type.chars().all(|c| c.is_ascii_lowercase()),
        "type prefix should be lowercase letters"
    );
    assert!(
        body.chars().all(|c| c.is_ascii_alphanumeric()),
        "id body should be alphanumeric"
    );
}

/// Set trust on the consolidated `agents` table (schema fold #1129).
/// Ensures the row exists so handoff/shared claim policy gates can evaluate.
fn set_agent_trust(db: &Connection, agent_id: &str, trust_level: &str, ts: &str) {
    db.execute(
        "INSERT INTO agents(agent_id, last_seen, status, updated_at, trust_level, expertise_json, category_claims_json)
         VALUES(?1, ?2, 'active', ?2, ?3, '[]', '[]')
         ON CONFLICT(agent_id) DO UPDATE SET
           trust_level = excluded.trust_level,
           updated_at = excluded.updated_at,
           last_seen = excluded.last_seen",
        decapod::core::db::params![agent_id, ts, trust_level],
    )
    .expect("set agent trust");
}

#[test]
fn test_todo_lifecycle() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();

    // 1. Add task
    let add_args = TodoCommand::Add {
        title: "Test task".to_string(),
        description: "".to_string(),
        tags: "tag1".to_string(),
        owner: "arx".to_string(),
        due: None,
        r#ref: "".to_string(),
        scope: "".to_string(),
        dir: Some(tmp.path().to_string_lossy().to_string()),
        priority: "high".to_string(),
        depends_on: "".to_string(),
        blocks: "".to_string(),
        parent: None,
        one_shot: 0,
    };
    let res = add_task(&root, &add_args).unwrap();
    let task_id = res.get("id").unwrap().as_str().unwrap();
    let task_hash = res.get("hash").and_then(|v| v.as_str()).unwrap_or_default();
    assert_typed_todo_id(task_id);
    assert_eq!(task_hash, &task_id.split_once('_').unwrap().1[..6]);

    // 2. Get task
    let task = get_task(&root, task_id).unwrap().expect("Task not found");
    assert_eq!(task.hash, task_hash);
    assert_eq!(task.title, "Test task");
    assert_eq!(task.status, "open");
    assert_eq!(task.owners.len(), 1);
    assert_eq!(task.owners[0].agent_id, "arx");
    assert_eq!(task.owners[0].claim_type, "primary");

    // 3. Mark done
    let store = Store {
        kind: StoreKind::Repo,
        root: root.clone(),
    };
    update_status(
        &store,
        task_id,
        task.revision,
        &task.status,
        "done",
        "task.done",
        serde_json::json!({}),
    )
    .unwrap();
    let task = get_task(&root, task_id).unwrap().unwrap();
    assert_eq!(task.status, "done");

    // 4. List tasks
    let tasks = list_tasks(&root, Some("done".to_string()), None, None, None, None).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, task_id);
}

#[test]
fn claim_returns_bounded_error_when_local_operation_lock_is_busy() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let task_id = add_scoped_task(
        &root,
        "Bounded claim lock regression",
        "architecture",
        root.to_str().unwrap(),
    );
    let db_path = root.join(schemas::LOCAL_DB_NAME);
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let holder_entered = Arc::clone(&entered);
    let holder_release = Arc::clone(&release);
    let holder_path = db_path.clone();
    let holder = std::thread::spawn(move || {
        global_pool().with_write(&holder_path, |_conn| {
            holder_entered.wait();
            holder_release.wait();
            Ok::<_, decapod::core::error::DecapodError>(())
        })
    });

    entered.wait();
    let started = Instant::now();
    let result = claim_task(&root, &task_id, "agent-lock-timeout", ClaimMode::Exclusive);
    let elapsed = started.elapsed();
    assert!(result.is_err(), "claim must report the busy operation lock");
    let message = result.unwrap_err().to_string();
    assert!(message.contains("STORAGE_POOL_LOCK_TIMEOUT"), "{message}");
    assert!(
        elapsed < Duration::from_secs(12),
        "claim exceeded the bounded lock interval: {elapsed:?}"
    );

    release.wait();
    holder
        .join()
        .expect("holder thread")
        .expect("holder operation");
}

#[test]
fn concurrent_status_transition_has_one_winner_and_no_duplicate_events() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let add_args = TodoCommand::Add {
        title: "CAS transition torture task".to_string(),
        description: "Many agents attempt the same completion from one snapshot".to_string(),
        tags: "concurrency,test".to_string(),
        owner: "".to_string(),
        due: None,
        r#ref: "".to_string(),
        scope: "workspace".to_string(),
        dir: Some(root.to_string_lossy().to_string()),
        priority: "high".to_string(),
        depends_on: "".to_string(),
        blocks: "".to_string(),
        parent: None,
        one_shot: 0,
    };
    let added = add_task(&root, &add_args).unwrap();
    let task_id = added["id"].as_str().unwrap().to_string();
    let snapshot = get_task(&root, &task_id).unwrap().unwrap();
    let worker_count = 25;
    let barrier = Arc::new(Barrier::new(worker_count));
    let mut workers = Vec::new();

    for index in 0..worker_count {
        let barrier = Arc::clone(&barrier);
        let root = root.clone();
        let task_id = task_id.clone();
        let expected_revision = snapshot.revision;
        let expected_status = snapshot.status.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            let store = Store {
                kind: StoreKind::Repo,
                root,
            };
            update_status(
                &store,
                &task_id,
                expected_revision,
                &expected_status,
                "done",
                "task.done",
                serde_json::json!({ "actor": format!("agent-{index}") }),
            )
        }));
    }

    let results: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().expect("concurrency worker panicked"))
        .collect();
    let outputs: Vec<_> = results
        .into_iter()
        .map(|result| result.expect("status mutation should return a structured result"))
        .collect();
    assert_eq!(
        outputs
            .iter()
            .filter(|output| output["status"] == "ok")
            .count(),
        1,
        "exactly one stale snapshot may win: {outputs:?}"
    );
    assert_eq!(
        outputs
            .iter()
            .filter(|output| output["status"] == "conflict")
            .count(),
        worker_count - 1,
        "all losers must be explicit conflicts: {outputs:?}"
    );

    let final_task = get_task(&root, &task_id).unwrap().unwrap();
    assert_eq!(final_task.status, "done");
    assert_eq!(final_task.revision, snapshot.revision + 1);

    let conn = Connection::open(root.join(schemas::LOCAL_DB_NAME)).unwrap();
    let done_events: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE stream = 'todo' AND subject_id = ?1 AND event_type = 'task.done'",
            [&task_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(done_events, 1, "losing transitions must not emit events");
}

#[test]
fn broker_transaction_rolls_back_state_when_event_append_fails() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let db_path = root.join(schemas::LOCAL_DB_NAME);
    let broker = DbBroker::new(&root);

    let failed = broker.with_transaction(
        &db_path,
        "test-agent",
        None,
        "todo.atomicity.test",
        |conn| {
            conn.execute(
                "INSERT INTO meta(namespace, key, value) VALUES('test', 'atomicity', 'written')",
                [],
            )
            .map_err(decapod::core::error::DecapodError::StorageError)?;

            let event = serde_json::json!({
                "event_id": "01atomicityfailure000000000000",
                "event_type": "test.failure",
                "payload": {},
                "actor": "test-agent",
            });
            events::append_on_conn(conn, "unknown-stream", &event)?;
            Ok(())
        },
    );
    assert!(
        failed.is_err(),
        "unknown event streams must fail the mutation"
    );

    let conn = Connection::open(&db_path).unwrap();
    let state_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM meta WHERE namespace = 'test' AND key = 'atomicity'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state_count, 0, "state must roll back with the failed event");
}

#[test]
fn event_identity_is_idempotent_and_conflicts_fail_closed() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let conn = Connection::open(root.join(schemas::LOCAL_DB_NAME)).unwrap();
    let event = serde_json::json!({
        "event_id": "01eventidentity0000000000000000",
        "ts": "2026-08-11T00:00:00Z",
        "event_type": "task.test",
        "status": "success",
        "task_id": "feat_01eventidentity",
        "payload": { "value": 1 },
        "actor": "test-agent"
    });
    let first =
        decapod::core::events::append_on_conn(&conn, decapod::core::events::TODO, &event).unwrap();
    let retry =
        decapod::core::events::append_on_conn(&conn, decapod::core::events::TODO, &event).unwrap();
    assert_eq!(first, retry, "replaying an event must be idempotent");

    let mut divergent = event.clone();
    divergent["payload"]["value"] = serde_json::json!(2);
    let error =
        decapod::core::events::append_on_conn(&conn, decapod::core::events::TODO, &divergent)
            .unwrap_err();
    assert!(error.to_string().contains("EVENT_ID_CONFLICT"));
}

#[test]
fn similar_titles_preserve_independent_task_identity() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();

    let active_args = TodoCommand::Add {
        title: "Fix duplicate todo label handoff routing".to_string(),
        description: "Make the active owner see comments from related requests".to_string(),
        tags: "todo,labels,agent-handoff".to_string(),
        owner: "".to_string(),
        due: None,
        r#ref: "".to_string(),
        scope: "".to_string(),
        dir: Some(tmp.path().to_string_lossy().to_string()),
        priority: "high".to_string(),
        depends_on: "".to_string(),
        blocks: "".to_string(),
        parent: None,
        one_shot: 0,
    };
    let active = add_task(&root, &active_args).unwrap();
    let active_id = active["id"].as_str().unwrap();
    claim_task(&root, active_id, "agent-a", ClaimMode::Exclusive).unwrap();

    let distinct_args = TodoCommand::Add {
        title: "Repair similar task tag transfer for owners".to_string(),
        description: "New conversation asks for annotations on claimed work".to_string(),
        tags: "tasks,tags,comments,agent-routing".to_string(),
        owner: "agent-b".to_string(),
        due: None,
        r#ref: "".to_string(),
        scope: "".to_string(),
        dir: Some(tmp.path().to_string_lossy().to_string()),
        priority: "medium".to_string(),
        depends_on: "".to_string(),
        blocks: "".to_string(),
        parent: None,
        one_shot: 0,
    };
    let distinct = add_task(&root, &distinct_args).unwrap();
    assert_ne!(distinct["id"], active_id);
    assert!(distinct.get("consolidated").is_none());

    let similar_args = TodoCommand::Add {
        title: "Fix duplicate todo label handoff routing".to_string(),
        description: "Make the active owner see comments from related requests".to_string(),
        tags: "todo,labels,agent-handoff".to_string(),
        owner: "agent-b".to_string(),
        due: None,
        r#ref: "".to_string(),
        scope: "".to_string(),
        dir: Some(tmp.path().to_string_lossy().to_string()),
        priority: "medium".to_string(),
        depends_on: "".to_string(),
        blocks: "".to_string(),
        parent: None,
        one_shot: 0,
    };
    let duplicate_title = add_task(&root, &similar_args).unwrap();
    assert_ne!(duplicate_title["id"], active_id);
    assert!(duplicate_title.get("consolidated").is_none());

    let open = list_tasks(&root, Some("open".to_string()), None, None, None, None).unwrap();
    assert_eq!(open.len(), 3);
    assert!(
        get_task(&root, active_id)
            .unwrap()
            .unwrap()
            .comments
            .is_empty()
    );
}

#[test]
fn same_category_tasks_keep_independent_custody() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let add = |title: &str| {
        add_task(
            &root,
            &TodoCommand::Add {
                title: title.to_string(),
                description: "distinct work".to_string(),
                tags: String::new(),
                owner: String::new(),
                due: None,
                r#ref: String::new(),
                scope: String::new(),
                dir: Some(tmp.path().to_string_lossy().to_string()),
                priority: "normal".to_string(),
                depends_on: String::new(),
                blocks: String::new(),
                parent: None,
                one_shot: 0,
            },
        )
        .unwrap()
    };
    let first = add("Fix the parser boundary");
    let first_id = first["id"].as_str().unwrap();
    let claimed = claim_task(&root, first_id, "agent-a", ClaimMode::Exclusive).unwrap();
    assert_eq!(claimed["result"]["status"], "ok");
    assert_eq!(
        claimed["result"]["authoring_isolation"],
        "custody_acquired_workspace_unproven"
    );
    let second = add("Fix the renderer boundary");
    let second_id = second["id"].as_str().unwrap();
    assert_ne!(first_id, second_id);
    let second_row = get_task(&root, second_id).unwrap().unwrap();
    assert!(
        second_row.assigned_to.is_empty(),
        "category routing must not seize custody"
    );
    let other = claim_task(&root, second_id, "agent-b", ClaimMode::Exclusive).unwrap();
    assert_eq!(other["result"]["status"], "ok");
    assert_eq!(
        other["result"]["authoring_isolation"],
        "custody_acquired_workspace_unproven"
    );
}

#[test]
fn test_similar_active_task_does_not_consolidate_same_owner() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();

    let active_args = TodoCommand::Add {
        title: "Fix duplicate todo label handoff routing".to_string(),
        description: "Make active owner see comments".to_string(),
        tags: "todo,labels,agent-handoff".to_string(),
        owner: "agent-a".to_string(),
        due: None,
        r#ref: "".to_string(),
        scope: "".to_string(),
        dir: Some(tmp.path().to_string_lossy().to_string()),
        priority: "high".to_string(),
        depends_on: "".to_string(),
        blocks: "".to_string(),
        parent: None,
        one_shot: 0,
    };
    let active = add_task(&root, &active_args).unwrap();
    let active_id = active["id"].as_str().unwrap();
    claim_task(&root, active_id, "agent-a", ClaimMode::Exclusive).unwrap();

    let similar_args = TodoCommand::Add {
        title: "Repair similar task tag transfer for owners".to_string(),
        description: "New conversation asks for annotations on claimed work".to_string(),
        tags: "tasks,tags,comments,agent-routing".to_string(),
        owner: "agent-a".to_string(),
        due: None,
        r#ref: "".to_string(),
        scope: "".to_string(),
        dir: Some(tmp.path().to_string_lossy().to_string()),
        priority: "medium".to_string(),
        depends_on: "".to_string(),
        blocks: "".to_string(),
        parent: None,
        one_shot: 0,
    };
    let created = add_task(&root, &similar_args).unwrap();
    assert_ne!(created["id"], active_id);
    assert!(created.get("consolidated").is_none());

    let open = list_tasks(&root, Some("open".to_string()), None, None, None, None).unwrap();
    assert_eq!(
        open.len(),
        2,
        "same-owner similar work should remain independently visible"
    );
}

#[test]
fn test_todo_rebuild() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();

    // Add some tasks
    for i in 0..3 {
        let add_args = TodoCommand::Add {
            title: format!("Task {i}"),
            description: "".to_string(),
            tags: "".to_string(),
            owner: "".to_string(),
            due: None,
            r#ref: "".to_string(),
            scope: "".to_string(),
            dir: Some(tmp.path().to_string_lossy().to_string()),
            priority: "medium".to_string(),
            depends_on: "".to_string(),
            blocks: "".to_string(),
            parent: None,
            one_shot: 0,
        };
        add_task(&root, &add_args).unwrap();
    }

    // Legacy rebuild imports/counts canonical evidence; it does not replay
    // task projections. Each add now appends its domain event and checkpoint.
    let rebuild = rebuild_from_events(&root).unwrap();
    assert_eq!(rebuild["source"], "events");
    assert_eq!(rebuild["events"], 6);
    assert_eq!(rebuild["task_projection_replayed"], false);
    let conn = Connection::open(root.join(schemas::LOCAL_DB_NAME)).unwrap();
    for event_type in ["task.add", "task.journal.checkpoint"] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE stream='todo' AND event_type=?1",
                [event_type],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 3, "{event_type}");
    }
    assert_eq!(
        decapod::core::todo::task_journal_verify(&root).unwrap()["status"],
        "ok"
    );

    // Verify
    let tasks = list_tasks(&root, None, None, None, None, None).unwrap();
    assert_eq!(tasks.len(), 3);
}

#[test]
fn test_trust_level_check() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();

    // Unknown agent defaults to basic
    let has_access = check_trust_level(&root, "unknown_agent", "basic").unwrap();
    assert!(has_access);

    // Unknown agent should NOT have core access (higher than basic)
    let has_access = check_trust_level(&root, "unknown_agent", "core").unwrap();
    assert!(!has_access);

    // Unknown agent should NOT have verified access (higher than basic)
    let has_access = check_trust_level(&root, "unknown_agent", "verified").unwrap();
    assert!(!has_access);
}

#[test]
fn test_trust_level_hierarchy() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();

    // Default is basic, so it should pass basic check
    assert!(check_trust_level(&root, "test_agent", "basic").unwrap());

    // But should fail for higher levels
    assert!(!check_trust_level(&root, "test_agent", "verified").unwrap());
    assert!(!check_trust_level(&root, "test_agent", "core").unwrap());
}

fn run_cmd(repo_root: &Path, args: &[&str]) -> Value {
    run_cmd_as(repo_root, args, "unknown")
}

fn run_cmd_as(repo_root: &Path, args: &[&str], actor: &str) -> Value {
    let output = run_raw_as(repo_root, args, actor);
    assert!(
        output.status.success(),
        "command failed: {:?}\nstderr: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_start = stdout.find('{').expect("json output start");
    serde_json::from_str(&stdout[json_start..]).expect("parse json")
}

fn run_raw(repo_root: &Path, args: &[&str]) -> std::process::Output {
    run_raw_as(repo_root, args, "unknown")
}

fn run_raw_as(repo_root: &Path, args: &[&str], actor: &str) -> std::process::Output {
    Command::new(resolve_decapod_bin())
        .current_dir(repo_root)
        .args(args)
        .env("DECAPOD_AGENT_ID", actor)
        .stdin(std::process::Stdio::null())
        // Fixture commands are serial; retain broker execution without its
        // production batching delay after every CLI call.
        .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1")
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .env("GIT_AUTHOR_NAME", "Decapod Test")
        .env("GIT_AUTHOR_EMAIL", "decapod-test@example.com")
        .env("GIT_COMMITTER_NAME", "Decapod Test")
        .env("GIT_COMMITTER_EMAIL", "decapod-test@example.com")
        .bounded_output(Duration::from_secs(120))
        .unwrap_or_else(|error| {
            panic!("decapod {args:?} did not finish within the plugin command budget: {error}")
        })
}

fn bootstrap_repo(repo: &Path) {
    let init = run_raw(repo, &["init", "--force"]);
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let session = run_raw(repo, &["session", "acquire"]);
    assert!(
        session.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&session.stderr)
    );
}

#[test]
fn test_claim_modes_and_owner_consolidation() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    bootstrap_repo(repo);
    let added = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "add",
            "Claim mode test",
            "--owner",
            "agent-a,agent-b",
        ],
    );
    let task_id = added["id"].as_str().unwrap().to_string();

    let _ = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "claim",
            "--id",
            &task_id,
            "--agent",
            "agent-a",
            "--mode",
            "exclusive",
        ],
    );

    let db = Connection::open(repo.join(".decapod/data/decapod.db")).unwrap();
    let ts = "1771202800Z";
    set_agent_trust(&db, "agent-b", "verified", ts);

    let shared = run_cmd(
        repo,
        &[
            "todo", "--format", "json", "claim", "--id", &task_id, "--agent", "agent-b", "--mode",
            "shared",
        ],
    );
    assert_eq!(shared["status"], "ok");
    assert_eq!(shared["result"]["mode"], "shared");

    let got = run_cmd(repo, &["todo", "--format", "json", "get", "--id", &task_id]);
    assert_eq!(got["item"]["owner"], "agent-a");
    let owners = got["item"]["owners"].as_array().unwrap();
    assert_eq!(owners.len(), 2);
    assert!(
        owners
            .iter()
            .any(|o| o["agent_id"] == "agent-a" && o["claim_type"] == "primary")
    );
    assert!(
        owners
            .iter()
            .any(|o| o["agent_id"] == "agent-b" && o["claim_type"] == "secondary")
    );

    let _ = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "edit",
            "--id",
            &task_id,
            "--owner",
            "agent-c,agent-d",
        ],
    );
    let got_after_edit = run_cmd(repo, &["todo", "--format", "json", "get", "--id", &task_id]);
    assert_eq!(got_after_edit["item"]["owner"], "agent-c");
    let owners_after_edit = got_after_edit["item"]["owners"].as_array().unwrap();
    assert!(
        owners_after_edit
            .iter()
            .any(|o| o["agent_id"] == "agent-c" && o["claim_type"] == "primary")
    );
    assert!(
        owners_after_edit
            .iter()
            .any(|o| o["agent_id"] == "agent-d" && o["claim_type"] == "secondary")
    );
    assert!(
        !owners_after_edit
            .iter()
            .any(|o| o["agent_id"] == "agent-a" || o["agent_id"] == "agent-b")
    );
}

#[test]
fn test_risk_zones_and_trust_tiers_enforced() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    // Keep one actual caller throughout this isolated CLI session, including
    // bootstrap; --from is only the expected previous owner.
    let run_raw = |repo: &Path, args: &[&str]| run_raw_as(repo, args, "agent-b");
    let run_cmd = |repo: &Path, args: &[&str]| run_cmd_as(repo, args, "agent-b");
    assert!(run_raw(repo, &["init", "--force"]).status.success());
    assert!(run_raw(repo, &["session", "acquire"]).status.success());

    let added = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "add",
            "Risk/trust test",
            "--owner",
            "agent-a",
        ],
    );
    let task_id = added["id"].as_str().unwrap().to_string();

    // Shared claim requires verified trust (default unknown/basic should fail).
    let shared_fail = run_raw(
        repo,
        &[
            "todo", "--format", "json", "claim", "--id", &task_id, "--agent", "agent-b", "--mode",
            "shared",
        ],
    );
    assert!(
        !shared_fail.status.success(),
        "shared claim should fail without verified trust"
    );
    assert!(String::from_utf8_lossy(&shared_fail.stderr).contains("Policy gate denied"));

    // Grant verified trust to agent-b and retry shared claim.
    let db = Connection::open(repo.join(".decapod/data/decapod.db")).unwrap();
    let ts = "1771203000Z";
    set_agent_trust(&db, "agent-b", "verified", ts);
    drop(db);
    let shared_ok = run_cmd(
        repo,
        &[
            "todo", "--format", "json", "claim", "--id", &task_id, "--agent", "agent-b", "--mode",
            "shared",
        ],
    );
    assert_eq!(shared_ok["status"], "ok");
    assert_eq!(shared_ok["result"]["mode"], "shared");

    // Handoff requires verified trust and explicit approval.
    let db = Connection::open(repo.join(".decapod/data/decapod.db")).unwrap();
    set_agent_trust(&db, "agent-a", "verified", ts);

    let handoff_fail = run_raw(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "handoff",
            "--id",
            &task_id,
            "--to",
            "agent-c",
            "--from",
            "agent-b",
            "--summary",
            "handoff test",
        ],
    );
    assert!(
        !handoff_fail.status.success(),
        "handoff should fail without approval"
    );

    let store = Store {
        kind: StoreKind::Repo,
        root: repo.join(".decapod/data"),
    };
    policy::approve_action(&store, "todo.handoff", None, "operator", "global").unwrap();

    let handoff_ok = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "handoff",
            "--id",
            &task_id,
            "--to",
            "agent-c",
            "--from",
            "agent-b",
            "--summary",
            "handoff test",
        ],
    );
    assert_eq!(handoff_ok["status"], "ok");
    assert_eq!(handoff_ok["result"]["to"], "agent-c");
    assert_eq!(handoff_ok["result"]["lease_lifecycle"], "claimed");
    assert!(
        handoff_ok["result"]["lease_generation"]
            .as_u64()
            .unwrap_or(0)
            >= 1,
        "handoff must publish a lease generation: {handoff_ok}"
    );
    assert!(
        handoff_ok["result"]["intent_anchor"]
            .as_str()
            .is_some_and(|anchor| !anchor.is_empty()),
        "handoff must preserve/bind an intent anchor: {handoff_ok}"
    );
}

#[test]
fn test_done_accepts_positional_id() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    bootstrap_repo(repo);

    let added = run_cmd(
        repo,
        &["todo", "--format", "json", "add", "Positional done test"],
    );
    let task_id = added["id"].as_str().unwrap().to_string();

    let done_out = run_cmd(repo, &["todo", "--format", "json", "done", &task_id]);
    assert_eq!(done_out["status"], "ok");

    let got = run_cmd(repo, &["todo", "--format", "json", "get", "--id", &task_id]);
    assert_eq!(got["item"]["status"], "done");
}

#[test]
fn test_claim_includes_container_result_when_autorun_enabled() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    bootstrap_repo(repo);

    let added = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "add",
            "Claim autorun envelope test",
            "--owner",
            "agent-a",
        ],
    );
    let task_id = added["id"].as_str().unwrap().to_string();

    let claimed = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "claim",
            "--id",
            &task_id,
            "--agent",
            "agent-a",
            "--mode",
            "exclusive",
        ],
    );
    assert_eq!(claimed["status"], "ok");
    assert!(
        claimed.get("container").is_some(),
        "claim response should include container launch result"
    );
    let container_status = claimed["container"]["status"].as_str().unwrap_or("");
    assert!(
        container_status == "ok" || container_status == "error" || container_status == "warning",
        "container status should be ok/error/warning, got '{container_status}'"
    );
}

#[test]
fn test_ownership_rebuild_replay_parity() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    bootstrap_repo(repo);

    let added = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "add",
            "Ownership replay parity",
            "--owner",
            "agent-a,agent-b",
        ],
    );
    let task_id = added["id"].as_str().unwrap().to_string();

    // Prepare trust gates for shared claim.
    let db = Connection::open(repo.join(".decapod/data/decapod.db")).unwrap();
    let ts = "1771203600Z";
    set_agent_trust(&db, "agent-c", "verified", ts);

    let _ = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "claim",
            "--id",
            &task_id,
            "--agent",
            "agent-a",
            "--mode",
            "exclusive",
        ],
    );
    let _ = run_cmd(
        repo,
        &[
            "todo", "--format", "json", "claim", "--id", &task_id, "--agent", "agent-c", "--mode",
            "shared",
        ],
    );
    let _ = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "remove-owner",
            "--id",
            &task_id,
            "--agent",
            "agent-b",
        ],
    );
    let _ = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "add-owner",
            "--id",
            &task_id,
            "--agent",
            "agent-d",
            "--claim-type",
            "secondary",
        ],
    );

    let before = run_cmd(repo, &["todo", "--format", "json", "get", "--id", &task_id]);
    let before_owner = before["item"]["owner"].as_str().unwrap().to_string();
    let before_assigned = before["item"]["assigned_to"].as_str().unwrap().to_string();
    let mut before_owners: Vec<String> = before["item"]["owners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            format!(
                "{}:{}",
                o["agent_id"].as_str().unwrap(),
                o["claim_type"].as_str().unwrap()
            )
        })
        .collect();
    before_owners.sort();

    let _ = run_cmd(repo, &["todo", "--format", "json", "rebuild"]);
    let after = run_cmd(repo, &["todo", "--format", "json", "get", "--id", &task_id]);
    let after_owner = after["item"]["owner"].as_str().unwrap().to_string();
    let after_assigned = after["item"]["assigned_to"].as_str().unwrap().to_string();
    let mut after_owners: Vec<String> = after["item"]["owners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            format!(
                "{}:{}",
                o["agent_id"].as_str().unwrap(),
                o["claim_type"].as_str().unwrap()
            )
        })
        .collect();
    after_owners.sort();

    assert_eq!(
        before_owner, after_owner,
        "owner mirror should survive rebuild"
    );
    assert_eq!(
        before_assigned, after_assigned,
        "assigned_to should survive rebuild"
    );
    assert_eq!(
        before_owners, after_owners,
        "ownership claim/release replay should be deterministic"
    );
}

fn add_scoped_task(root: &Path, title: &str, scope: &str, dir: &str) -> String {
    add_scoped_task_with_dependencies(root, title, scope, dir, "")
}

fn add_scoped_task_with_dependencies(
    root: &Path,
    title: &str,
    scope: &str,
    dir: &str,
    depends_on: &str,
) -> String {
    let args = TodoCommand::Add {
        title: title.to_string(),
        description: format!("houseboat lease test: {title}"),
        tags: "houseboat,fleet".to_string(),
        owner: "".to_string(),
        due: None,
        r#ref: "".to_string(),
        scope: scope.to_string(),
        dir: Some(dir.to_string()),
        priority: "medium".to_string(),
        depends_on: depends_on.to_string(),
        blocks: "".to_string(),
        parent: None,
        one_shot: 0,
    };
    let res = add_task(root, &args).unwrap();
    res["id"].as_str().unwrap().to_string()
}

#[test]
fn exclusive_claim_sets_lease_and_renew_extends_it() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let dir = tmp.path().join("src").join("core");
    std::fs::create_dir_all(&dir).unwrap();

    let task_id = add_scoped_task(
        &root,
        "Lease renewal target alpha",
        "architecture",
        dir.to_str().unwrap(),
    );
    let claim = claim_task_with_lease(
        &root,
        &task_id,
        "agent-lease",
        ClaimMode::Exclusive,
        Some(120),
        Some("intent:houseboat:wave2".to_string()),
    )
    .unwrap();
    assert_eq!(claim["status"], "ok");
    assert_eq!(claim["result"]["lease_seconds"], 120);
    assert_eq!(claim["result"]["lease_generation"], 1);
    assert_eq!(claim["result"]["lease_lifecycle"], "claimed");
    assert_eq!(claim["result"]["intent_anchor"], "intent:houseboat:wave2");
    let first_lease = claim["result"]["lease_expires_at"]
        .as_str()
        .expect("lease_expires_at")
        .to_string();
    assert!(!first_lease.is_empty());

    let renew = renew_claim_lease(&root, &task_id, "agent-lease", Some(600)).unwrap();
    assert_eq!(renew["status"], "ok", "renew should succeed: {renew}");
    assert_eq!(renew["result"]["lease_seconds"], 600);
    assert_eq!(renew["result"]["lease_generation"], 2);
    assert_eq!(renew["result"]["lease_lifecycle"], "extended");
    let second_lease = renew["result"]["lease_expires_at"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(second_lease, first_lease);

    let fleet = fleet_health(&root, Some("agent-lease")).unwrap();
    assert_eq!(fleet["status"], "ok");
    assert!(fleet["fleet"]["claim_count"].as_u64().unwrap() >= 1);
    assert!(
        fleet["fleet"]["capacity"]["reserved_units"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert!(
        fleet["fleet"]["intent_anchors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str() == Some("intent:houseboat:wave2"))
    );
}

#[test]
fn expired_lease_is_reclaimable_by_another_agent() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let dir = tmp.path().join("src").join("workspace");
    std::fs::create_dir_all(&dir).unwrap();

    // Seed a legitimately historical expired claim before journal genesis.
    // Rewriting expiry after a modern claim would be authority corruption,
    // not a simulation of time passing.
    let task_id = "docs_expiredlease0001".to_string();
    let db = root.join(schemas::LOCAL_DB_NAME);
    let conn = Connection::open(&db).unwrap();
    conn.execute(
        "INSERT INTO tasks(id,hash,title,status,created_at,updated_at,dir_path,scope,category,tags,owner,assigned_to,assigned_at,lease_expires_at,lease_generation,lease_lifecycle,intent_anchor,revision)
         VALUES(?1,'expired-fixture','Reclaimable expired lease task','open','1Z','1Z',?2,'documentation','documentation','documentation','agent-old','agent-old','1Z','1Z',1,'claimed','intent:expired-fixture',1)",
        decapod::core::db::params![task_id,dir.to_string_lossy()],
    ).unwrap();
    conn.execute(
        "INSERT INTO task_owners(id,task_id,agent_id,claimed_at,claim_type) VALUES('expired-owner',?1,'agent-old','1Z','primary')",
        [&task_id],
    ).unwrap();
    set_agent_trust(&conn, "agent-old", "basic", "1Z");
    assert_eq!(
        get_task(&root, &task_id).unwrap().unwrap().assigned_to,
        "agent-old"
    );
    assert_eq!(
        decapod::core::todo::task_journal_checkpoint(&root, None).unwrap()["status"],
        "ok"
    );

    let reclaim = claim_task(&root, &task_id, "agent-new", ClaimMode::Exclusive).unwrap();
    assert_eq!(reclaim["status"], "ok", "reclaim should succeed: {reclaim}");
    assert_eq!(reclaim["result"]["reclaimed_from"], "agent-old");
    assert_eq!(reclaim["result"]["assigned_to"], "agent-new");
    assert_eq!(reclaim["result"]["lease_lifecycle"], "reclaimed");
    assert!(reclaim["result"]["lease_generation"].as_u64().unwrap() >= 2);

    let task = get_task(&root, &task_id).unwrap().unwrap();
    assert_eq!(task.assigned_to, "agent-new");
    assert_eq!(
        decapod::core::todo::task_journal_verify(&root).unwrap()["status"],
        "ok"
    );
}

#[test]
fn overlapping_source_claims_are_advisory() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let shared = tmp.path().join("src").join("shared-module");
    std::fs::create_dir_all(&shared).unwrap();
    let nested = shared.join("impl");
    std::fs::create_dir_all(&nested).unwrap();

    let first_id = add_scoped_task(
        &root,
        "Fleet overlap first task unique title alpha",
        "architecture",
        shared.to_str().unwrap(),
    );
    let second_id = add_scoped_task(
        &root,
        "Fleet overlap second task unique title omega",
        "architecture",
        nested.to_str().unwrap(),
    );
    assert_ne!(first_id, second_id);

    let first = claim_task(&root, &first_id, "agent-a", ClaimMode::Exclusive).unwrap();
    assert_eq!(first["status"], "ok", "first claim: {first}");

    let second = claim_task(&root, &second_id, "agent-b", ClaimMode::Exclusive).unwrap();
    assert_eq!(
        second["status"], "ok",
        "logical overlap is not custody: {second}"
    );
    assert_eq!(
        second["result"]["authoring_isolation"],
        "custody_acquired_workspace_unproven"
    );
    let overlaps = second["result"]["advisory_overlaps"].as_array().unwrap();
    assert!(
        overlaps.iter().any(|overlap| {
            (overlap["reason"] == "path_overlap" || overlap["reason"] == "scope_overlap")
                && overlap["enforced"] == false
        }),
        "expected advisory overlap, got {overlaps:?}"
    );
}

#[test]
fn yield_preserves_generation_and_frees_capacity() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let dir = tmp.path().join("src").join("yield");
    std::fs::create_dir_all(&dir).unwrap();

    let task_id = add_scoped_task(
        &root,
        "Yield lease capacity task unique",
        "platform_engineering",
        dir.to_str().unwrap(),
    );
    let claim = claim_task(&root, &task_id, "agent-y", ClaimMode::Exclusive).unwrap();
    assert_eq!(claim["status"], "ok");
    assert_eq!(claim["result"]["lease_generation"], 1);

    let yielded = yield_claim_lease(&root, &task_id, "agent-y", "paused for handoff").unwrap();
    assert_eq!(yielded["status"], "ok", "yield: {yielded}");
    assert_eq!(yielded["result"]["lease_lifecycle"], "yielded");
    assert_eq!(yielded["result"]["lease_generation"], 1);

    let task = get_task(&root, &task_id).unwrap().unwrap();
    assert!(task.assigned_to.is_empty());

    // Same or other agents can reclaim after yield; generation advances on re-issue.
    let reclaim = claim_task(&root, &task_id, "agent-y", ClaimMode::Exclusive).unwrap();
    assert_eq!(reclaim["status"], "ok", "post-yield claim: {reclaim}");
    assert_eq!(reclaim["result"]["assigned_to"], "agent-y");
    assert_eq!(reclaim["result"]["lease_lifecycle"], "claimed");
}

#[test]
fn handoff_advances_lease_generation_and_preserves_intent() {
    use decapod::core::store::{Store, StoreKind};

    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let dir = tmp.path().join("src").join("handoff-lease");
    std::fs::create_dir_all(&dir).unwrap();

    let task_id = add_scoped_task(
        &root,
        "Houseboat handoff lease transfer unique",
        "handoff_scope",
        dir.to_str().unwrap(),
    );
    let claim = claim_task_with_lease(
        &root,
        &task_id,
        "agent-from",
        ClaimMode::Exclusive,
        Some(900),
        Some("intent:houseboat:wave4".to_string()),
    )
    .unwrap();
    assert_eq!(claim["status"], "ok", "claim: {claim}");
    assert_eq!(claim["result"]["lease_generation"], 1);
    assert_eq!(claim["result"]["intent_anchor"], "intent:houseboat:wave4");

    let store = Store {
        kind: StoreKind::Repo,
        root: root.clone(),
    };
    // Policy gate: handoff requires verified trust + explicit approval.
    // Trust lives on the consolidated agents table after schema fold (#1129).
    let db = Connection::open(root.join(schemas::LOCAL_DB_NAME)).unwrap();
    set_agent_trust(&db, "agent-from", "verified", "100Z");
    policy::approve_action(&store, "todo.handoff", None, "operator", "global").unwrap();

    let handed = handoff_task_as(
        &store,
        &task_id,
        "agent-to",
        Some("agent-from"),
        "wave4 lease transfer",
        Some(600),
        "agent-from",
    )
    .unwrap();
    assert_eq!(handed["status"], "ok", "handoff: {handed}");
    assert_eq!(handed["result"]["status"], "ok");
    assert_eq!(handed["result"]["from"], "agent-from");
    assert_eq!(handed["result"]["to"], "agent-to");
    assert_eq!(handed["result"]["prior_lease_generation"], 1);
    assert_eq!(handed["result"]["lease_generation"], 2);
    assert_eq!(handed["result"]["lease_lifecycle"], "claimed");
    assert_eq!(handed["result"]["intent_anchor"], "intent:houseboat:wave4");
    assert_eq!(handed["result"]["lease_seconds"], 600);
    assert!(
        handed["result"]["lease_expires_at"]
            .as_str()
            .is_some_and(|v| !v.is_empty())
    );

    let task = get_task(&root, &task_id).unwrap().unwrap();
    assert_eq!(task.assigned_to, "agent-to");

    let fleet = fleet_health(&root, Some("agent-to")).unwrap();
    let active = fleet["fleet"]["active_claims"].as_array().unwrap();
    assert!(
        active.iter().any(|c| {
            c["task_id"] == task_id
                && c["agent_id"] == "agent-to"
                && c["lease_generation"] == 2
                && c["intent_anchor"] == "intent:houseboat:wave4"
        }),
        "fleet must show receiver exclusive lease: {fleet}"
    );
}

#[test]
fn exclusive_lease_blocks_unproven_done() {
    use decapod::core::store::{Store, StoreKind};

    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let dir = tmp.path().join("src").join("proof");
    std::fs::create_dir_all(&dir).unwrap();

    let task_id = add_scoped_task(
        &root,
        "Proof gated completion unique task",
        "architecture",
        dir.to_str().unwrap(),
    );
    let claim = claim_task(&root, &task_id, "agent-p", ClaimMode::Exclusive).unwrap();
    assert_eq!(claim["status"], "ok");

    // Simulate todo done dispatch proof gate via exclusive_lease path using CLI handler isn't
    // easy without full Store; exercise the gate through a direct status attempt after checking
    // that fleet still shows exclusive custody.
    let fleet = fleet_health(&root, None).unwrap();
    let active = fleet["fleet"]["active_claims"].as_array().unwrap();
    assert!(
        active
            .iter()
            .any(|c| { c["task_id"] == task_id && c["lease_lifecycle"] == "claimed" }),
        "expected claimed lease in fleet: {fleet}"
    );

    let store = Store {
        kind: StoreKind::Repo,
        root: root.clone(),
    };
    let before = get_task(&root, &task_id).unwrap().unwrap();
    let rejected = update_status(
        &store,
        &task_id,
        before.revision,
        &before.status,
        "done",
        "task.done",
        serde_json::json!({"actor":"agent-p"}),
    )
    .unwrap();
    assert_eq!(
        rejected["status"], "conflict",
        "exclusive completion without proof must fail: {rejected}"
    );
    let after = get_task(&root, &task_id).unwrap().unwrap();
    assert_eq!(after.status, before.status);
    assert_eq!(after.revision, before.revision);

    // Yield removes exclusive custody so unmarked done can proceed for unleased tasks.
    let _ = yield_claim_lease(&root, &task_id, "agent-p", "done without lease").unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: root.clone(),
    };
    let current = get_task(&root, &task_id).unwrap().unwrap();
    let done = update_status(
        &store,
        &task_id,
        current.revision,
        &current.status,
        "done",
        "task.done",
        serde_json::json!({}),
    )
    .unwrap();
    assert_eq!(done["status"], "ok", "yielded task may complete: {done}");
}

#[test]
fn dependency_readiness_gates_claim_and_feeds_work_claim_and_fleet() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let dep_dir = tmp.path().join("src").join("dependency");
    let target_dir = tmp.path().join("src").join("target");
    std::fs::create_dir_all(&dep_dir).unwrap();
    std::fs::create_dir_all(&target_dir).unwrap();

    let dependency_id = add_scoped_task(
        &root,
        "Houseboat dependency prerequisite unique",
        "dependency",
        dep_dir.to_str().unwrap(),
    );
    let target_id = add_scoped_task_with_dependencies(
        &root,
        "Houseboat dependency target unique",
        "target",
        target_dir.to_str().unwrap(),
        &dependency_id,
    );

    let waiting = claim_task(&root, &target_id, "agent-target", ClaimMode::Exclusive).unwrap();
    assert_eq!(waiting["status"], "conflict", "waiting claim: {waiting}");
    assert_eq!(
        waiting["result"]["dependency_readiness"]["state"],
        "waiting"
    );
    assert_eq!(
        waiting["result"]["dependency_readiness"]["blockers"][0]["reason"],
        "dependency_not_complete"
    );

    let db = root.join(schemas::LOCAL_DB_NAME);
    let conn = Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE tasks SET status = 'done' WHERE id = ?1",
        [&dependency_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO task_verification(
            todo_id, proof_plan, verification_artifacts, last_verified_at,
            last_verified_status, last_verified_notes, verification_policy_days, updated_at
         ) VALUES(?1, '[\"validate_passes\"]', ?2, '100Z', 'passed', '', 90, '100Z')",
        decapod::core::db::params![
            dependency_id,
            serde_json::json!({
                "task_id": dependency_id,
                "task_revision": get_task(&root, &dependency_id).unwrap().unwrap().revision,
                "proof_plan_results": [{"proof_gate":"validate_passes","output_hash":"sha256:dep-proof"}]
            }).to_string()
        ],
    )
    .unwrap();

    let claimed = claim_task(&root, &target_id, "agent-target", ClaimMode::Exclusive).unwrap();
    assert_eq!(claimed["status"], "ok", "ready claim: {claimed}");
    assert_eq!(claimed["result"]["dependency_readiness"]["state"], "ready");
    assert_eq!(
        claimed["result"]["dependency_readiness"]["proof_refs"][0],
        format!("proof:{dependency_id}:validate_passes:sha256:dep-proof")
    );
    let work_claim = get_work_claim(&root, &target_id).unwrap().unwrap();
    assert!(work_claim.dependency_readiness.is_ready());

    conn.execute(
        "UPDATE task_verification SET last_verified_status = 'failed' WHERE todo_id = ?1",
        [&dependency_id],
    )
    .unwrap();
    let fleet = fleet_health(&root, Some("agent-target")).unwrap();
    assert_eq!(fleet["fleet"]["dependency_blocked_count"], 1);
    assert_eq!(fleet["fleet"]["proof_blocked_count"], 1);
    assert_eq!(
        fleet["fleet"]["dependency_blocked_claims"][0]["dependency_readiness"]["state"],
        "proof_blocked"
    );
}

#[test]
fn dependency_cycle_fails_closed_before_claim() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let first_dir = tmp.path().join("src").join("cycle-a");
    let second_dir = tmp.path().join("src").join("cycle-b");
    std::fs::create_dir_all(&first_dir).unwrap();
    std::fs::create_dir_all(&second_dir).unwrap();
    let first_id = add_scoped_task(
        &root,
        "Houseboat cycle first unique",
        "cycle-a",
        first_dir.to_str().unwrap(),
    );
    let second_id = add_scoped_task(
        &root,
        "Houseboat cycle second unique",
        "cycle-b",
        second_dir.to_str().unwrap(),
    );

    let db = root.join(schemas::LOCAL_DB_NAME);
    let conn = Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE tasks SET depends_on = ?1 WHERE id = ?2",
        decapod::core::db::params![second_id, first_id],
    )
    .unwrap();
    conn.execute(
        "UPDATE tasks SET depends_on = ?1 WHERE id = ?2",
        decapod::core::db::params![first_id, second_id],
    )
    .unwrap();

    let result = claim_task(&root, &first_id, "agent-cycle", ClaimMode::Exclusive).unwrap();
    assert_eq!(result["status"], "conflict", "cycle claim: {result}");
    assert_eq!(result["result"]["resolution"], "repair_dependency_graph");
    assert_eq!(result["result"]["dependency_readiness"]["state"], "cycle");
    assert_eq!(
        result["result"]["dependency_readiness"]["cycle"],
        serde_json::json!([first_id, second_id, first_id])
    );
}

#[test]
fn archive_eval_fingerprint_authorizes_only_the_named_todo() {
    let tmp = tempdir().unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(tmp.path())
            .status()
            .unwrap()
            .success()
    );
    let data_root = tmp.path().join(".decapod/data");
    std::fs::create_dir_all(&data_root).unwrap();
    std::fs::write(
        tmp.path().join(".decapod/config.toml"),
        r#"schema_version = "1.0.0"
[init]
specs = true
ci = true
diagram_style = "mermaid"
entrypoints = []
[repo]
product_name = "approval-test"
[governance]
approval_categories = ["destructive_operations"]
"#,
    )
    .unwrap();
    let root = data_root.as_path();
    initialize_todo_db(root).unwrap();
    policy::initialize_policy_db(root).unwrap();
    let alpha = add_scoped_task(
        root,
        "Approval target alpha",
        "root",
        root.to_str().unwrap(),
    );
    let beta = add_scoped_task(
        root,
        "Separate archive target beta",
        "root",
        root.to_str().unwrap(),
    );
    let store = Store {
        kind: StoreKind::Repo,
        root: root.to_path_buf(),
    };
    let archive = |id: &str| {
        let task = get_task(root, id).unwrap().unwrap();
        update_status(
            &store,
            id,
            task.revision,
            &task.status,
            "archived",
            "task.archive",
            serde_json::json!({}),
        )
    };
    assert!(archive(&alpha).is_err());
    let fingerprint = policy::derive_fingerprint("task.archive", Some(&alpha), "global");
    let grant = policy::approve_action(&store, &fingerprint, None, "operator", "global").unwrap();
    assert!(archive(&beta).is_err());
    assert_eq!(get_task(root, &beta).unwrap().unwrap().status, "open");
    assert_eq!(archive(&alpha).unwrap()["status"], "ok");
    assert_eq!(get_task(root, &alpha).unwrap().unwrap().status, "archived");
    policy::revoke_approval(&store, &grant).unwrap();
    assert!(archive(&alpha).is_err());

    let expired = policy::approve_action(&store, &fingerprint, None, "operator", "global").unwrap();
    let db = Connection::open(policy::policy_db_path(root)).unwrap();
    db.execute(
        "UPDATE approvals SET expires_at = '1Z' WHERE approval_id = ?1",
        [expired.as_str()],
    )
    .unwrap();
    assert!(
        archive(&alpha).is_err(),
        "expired resource approval must stay denied"
    );
    assert_eq!(get_task(root, &beta).unwrap().unwrap().status, "open");

    // An intentional action-wide approval retains compatibility at both gates.
    let general =
        policy::approve_action(&store, "task.archive", None, "operator", "global").unwrap();
    assert_eq!(archive(&beta).unwrap()["status"], "ok");
    policy::revoke_approval(&store, &general).unwrap();
    assert!(archive(&beta).is_err());
}

#[test]
fn handoff_checks_real_actor_and_preserves_owner_until_approved() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    initialize_todo_db(root).unwrap();
    let dir = root.join("handoff-recovery");
    std::fs::create_dir_all(&dir).unwrap();
    let id = add_scoped_task(
        root,
        "Handoff recovery",
        "handoff-recovery",
        dir.to_str().unwrap(),
    );
    claim_task(root, &id, "missing-owner", ClaimMode::Exclusive).unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: root.to_path_buf(),
    };
    let db = Connection::open(root.join(schemas::LOCAL_DB_NAME)).unwrap();
    set_agent_trust(&db, "missing-owner", "verified", "100Z");
    set_agent_trust(&db, "recovery-caller", "verified", "100Z");

    let absent =
        handoff_task_as(&store, &id, "receiver", None, "recover", None, "unknown").unwrap_err();
    assert!(absent.to_string().contains("HANDOFF_CALLER_MISSING"));
    let impersonation = handoff_task_as(
        &store,
        &id,
        "receiver",
        Some("missing-owner"),
        "recover",
        None,
        "basic-caller",
    )
    .unwrap_err();
    assert!(impersonation.to_string().contains("trust"));
    let unapproved = handoff_task_as(
        &store,
        &id,
        "receiver",
        None,
        "recover",
        None,
        "recovery-caller",
    )
    .unwrap_err();
    assert!(unapproved.to_string().contains("approval"));
    assert_eq!(
        get_task(root, &id).unwrap().unwrap().assigned_to,
        "missing-owner"
    );

    policy::approve_action(&store, "todo.handoff", Some(&id), "operator", "global").unwrap();
    let stale = handoff_task_as(
        &store,
        &id,
        "receiver",
        Some("wrong-owner"),
        "recover",
        None,
        "recovery-caller",
    )
    .unwrap_err();
    assert!(stale.to_string().contains("approval"));
    assert_eq!(
        get_task(root, &id).unwrap().unwrap().assigned_to,
        "missing-owner"
    );
    let exact =
        decapod::core::todo::handoff_approval_target(&id, "missing-owner", "recovery-caller", 1);
    policy::approve_action_with_expiry(
        &store,
        "todo.handoff",
        Some(&exact),
        "operator",
        "global",
        Some(&format!(
            "{}Z",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 900
        )),
    )
    .unwrap();
    let handed = handoff_task_as(
        &store,
        &id,
        "recovery-caller",
        Some("missing-owner"),
        "operator approved ownership recovery",
        None,
        "recovery-caller",
    )
    .unwrap();
    assert_eq!(handed["status"], "ok");
    assert_eq!(handed["result"]["actor"], "recovery-caller");
    assert_eq!(handed["result"]["from"], "missing-owner");
    assert_eq!(handed["result"]["to"], "recovery-caller");
    assert_eq!(
        get_task(root, &id).unwrap().unwrap().assigned_to,
        "recovery-caller"
    );
    let (event_id, ts, actor, subject_kind, subject_id, raw): (
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
    ) = db
        .query_row(
            "SELECT event_id, ts, actor, subject_kind, subject_id, payload
             FROM events
             WHERE stream='todo' AND subject_id=?1 AND event_type='task.handoff'
             ORDER BY seq DESC LIMIT 1",
            [&id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    assert!(raw.contains("\"_ce_v1\""));
    let event = decapod::core::events::reconstruct_stored_payload(
        "todo",
        &event_id,
        &ts,
        "task.handoff",
        &actor,
        subject_kind.as_deref(),
        subject_id.as_deref(),
        &raw,
    )
    .unwrap();
    assert_eq!(event["actor"], "recovery-caller");
    assert_eq!(event["payload"]["from"], "missing-owner");
}

#[test]
fn operator_recovery_requires_exact_one_use_authority() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    initialize_todo_db(root).unwrap();
    let dir = root.join("operator-recovery");
    std::fs::create_dir_all(&dir).unwrap();
    let id = add_scoped_task(
        root,
        "Operator recovery",
        "operator-recovery",
        dir.to_str().unwrap(),
    );
    claim_task(root, &id, "unavailable-owner", ClaimMode::Exclusive).unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: root.to_path_buf(),
    };

    let spoof = handoff_task_as(
        &store,
        &id,
        "receiver",
        Some("operator"),
        "recover",
        None,
        "basic-caller",
    )
    .unwrap_err();
    assert!(spoof.to_string().contains("trust"));
    let receiver_spoof = handoff_task_as(
        &store,
        &id,
        "operator",
        Some("unavailable-owner"),
        "recover",
        None,
        "basic-caller",
    )
    .unwrap_err();
    assert!(receiver_spoof.to_string().contains("trust"));
    assert_eq!(
        get_task(root, &id).unwrap().unwrap().assigned_to,
        "unavailable-owner"
    );
    assert!(
        handoff_task_as(
            &store,
            &id,
            "receiver",
            Some("unavailable-owner"),
            "recover",
            None,
            "operator"
        )
        .is_err()
    );
    policy::approve_action(
        &store,
        "todo.handoff",
        Some("another-task"),
        "operator",
        "global",
    )
    .unwrap();
    assert!(
        handoff_task_as(
            &store,
            &id,
            "receiver",
            Some("unavailable-owner"),
            "recover",
            None,
            "operator"
        )
        .is_err()
    );
    policy::approve_action(&store, "todo.handoff", None, "operator", "global").unwrap();
    let general = handoff_task_as(
        &store,
        &id,
        "receiver",
        Some("unavailable-owner"),
        "recover",
        None,
        "operator",
    )
    .unwrap_err();
    assert!(
        general
            .to_string()
            .contains("HANDOFF_RECOVERY_APPROVAL_REQUIRED")
    );

    let expired =
        policy::approve_action(&store, "todo.handoff", Some(&id), "operator", "global").unwrap();
    let db = Connection::open(root.join(schemas::LOCAL_DB_NAME)).unwrap();
    db.execute(
        "UPDATE approvals SET expires_at='1Z' WHERE approval_id=?1",
        [&expired],
    )
    .unwrap();
    let expired_result = handoff_task_as(
        &store,
        &id,
        "receiver",
        Some("unavailable-owner"),
        "expired recovery",
        None,
        "operator",
    )
    .unwrap_err();
    assert!(
        expired_result
            .to_string()
            .contains("HANDOFF_RECOVERY_APPROVAL_REQUIRED")
    );
    assert_eq!(
        get_task(root, &id).unwrap().unwrap().assigned_to,
        "unavailable-owner"
    );

    for (receiver, generation) in [("receiver", 0), ("another-receiver", 1)] {
        let stale_target = decapod::core::todo::handoff_approval_target(
            &id,
            "unavailable-owner",
            receiver,
            generation,
        );
        let expiry = format!(
            "{}Z",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 900
        );
        policy::approve_action_with_expiry(
            &store,
            "todo.handoff",
            Some(&stale_target),
            "operator",
            "global",
            Some(&expiry),
        )
        .unwrap();
        assert!(
            handoff_task_as(
                &store,
                &id,
                "receiver",
                Some("unavailable-owner"),
                "stale authority",
                None,
                "operator"
            )
            .is_err()
        );
        assert_eq!(
            get_task(root, &id).unwrap().unwrap().assigned_to,
            "unavailable-owner"
        );
    }
    let exact =
        decapod::core::todo::handoff_approval_target(&id, "unavailable-owner", "receiver", 1);
    let approval = policy::approve_action_with_expiry(
        &store,
        "todo.handoff",
        Some(&exact),
        "operator",
        "global",
        Some(&format!(
            "{}Z",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 900
        )),
    )
    .unwrap();
    let missing_owner =
        handoff_task_as(&store, &id, "receiver", None, "recover", None, "operator").unwrap_err();
    assert!(
        missing_owner
            .to_string()
            .contains("HANDOFF_RECOVERY_OWNER_REQUIRED")
    );
    assert!(
        policy::list_approvals(&store)
            .unwrap()
            .iter()
            .any(|a| a.approval_id == approval)
    );
    let handed = handoff_task_as(
        &store,
        &id,
        "receiver",
        Some("unavailable-owner"),
        "approved operator recovery",
        None,
        "operator",
    )
    .unwrap();
    assert_eq!(handed["status"], "ok");
    assert_eq!(handed["result"]["actor"], "operator");
    assert_eq!(handed["result"]["recovery"], true);
    assert_eq!(handed["result"]["consumed_approval_ids"][0], approval);
    assert!(
        !policy::list_approvals(&store)
            .unwrap()
            .iter()
            .any(|a| a.approval_id == approval)
    );
    let replay = handoff_task_as(
        &store,
        &id,
        "next-receiver",
        Some("receiver"),
        "reuse old approval",
        None,
        "operator",
    )
    .unwrap_err();
    assert!(
        replay
            .to_string()
            .contains("HANDOFF_RECOVERY_APPROVAL_REQUIRED")
    );
    assert_eq!(
        get_task(root, &id).unwrap().unwrap().assigned_to,
        "receiver"
    );
}

#[test]
fn cli_handoff_without_from_uses_active_caller() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    let run = |args: &[&str]| {
        Command::new(resolve_decapod_bin())
            .current_dir(repo)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Decapod Test")
            .env("GIT_AUTHOR_EMAIL", "decapod-test@example.com")
            .env("GIT_COMMITTER_NAME", "Decapod Test")
            .env("GIT_COMMITTER_EMAIL", "decapod-test@example.com")
            .env("DECAPOD_AGENT_ID", "active-owner")
            .env("DECAPOD_CLAIM_AUTORUN", "0")
            .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };
    let init = run(&["init", "--force"]);
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let root = repo.join(".decapod/data");
    initialize_todo_db(&root).unwrap();
    let id = add_scoped_task(
        &root,
        "CLI handoff identity",
        "cli-handoff",
        repo.to_str().unwrap(),
    );
    claim_task(&root, &id, "active-owner", ClaimMode::Exclusive).unwrap();
    let db = Connection::open(root.join(schemas::LOCAL_DB_NAME)).unwrap();
    set_agent_trust(&db, "active-owner", "verified", "100Z");
    let store = Store {
        kind: StoreKind::Repo,
        root: root.clone(),
    };
    policy::approve_action(&store, "todo.handoff", Some(&id), "operator", "global").unwrap();
    let output = run(&[
        "todo",
        "--format",
        "json",
        "handoff",
        "--id",
        &id,
        "--to",
        "receiver",
        "--summary",
        "ordinary handoff",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout[stdout.find('{').unwrap()..]).unwrap();
    assert_eq!(json["result"]["actor"], "active-owner");
    assert_eq!(json["result"]["from"], "active-owner");
    assert_eq!(json["result"]["recovery"], false);
    assert_eq!(
        get_task(&root, &id).unwrap().unwrap().assigned_to,
        "receiver"
    );
}

#[test]
fn handoff_failure_returns_agent_recovery_without_authorizing_or_mutating() {
    let tmp = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: tmp.path().to_path_buf(),
    };
    let error = handoff_task_as(
        &store,
        "task-identity",
        "receiver",
        Some("recorded-owner"),
        "recover",
        None,
        "unknown",
    )
    .unwrap_err();
    let message = error.to_string();
    assert!(message.contains("HANDOFF_CALLER_MISSING"));
    let recovery: Value =
        serde_json::from_str(message.split("\nHANDOFF_RECOVERY: ").nth(1).unwrap()).unwrap();
    assert_eq!(
        recovery["schema_version"],
        "decapod.todo.handoff-recovery.v1"
    );
    assert_eq!(recovery["task_id"], "task-identity");
    assert_eq!(recovery["actor"], "unknown");
    assert_eq!(recovery["expected_owner"], "recorded-owner");
    assert_eq!(recovery["executor"], "agent_or_harness");
    assert_eq!(
        recovery["inspect_argv"],
        serde_json::json!(["decapod", "todo", "get", "--id", "task-identity"])
    );
    assert_eq!(
        recovery["evaluate_argv"],
        serde_json::json!([
            "decapod",
            "govern",
            "policy",
            "eval",
            "--command",
            "todo.handoff",
            "--path",
            "task-identity"
        ])
    );
    let next = recovery["next_action"].as_str().unwrap();
    assert!(next.contains("obtain the human decision through conversation"));
    assert!(next.contains("never impersonate the owner or self-promote trust"));
    assert!(next.contains("before any retry"));
    assert!(recovery.get("approve_argv").is_none());
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);

    let invalid = handoff_task_as(
        &store,
        "task-identity",
        "",
        None,
        "recover",
        None,
        "actual-caller",
    )
    .unwrap();
    assert_eq!(invalid["status"], "error");
    assert_eq!(invalid["recovery"]["actor"], "actual-caller");
    assert_eq!(
        invalid["recovery"]["reason"],
        "handoff requires a non-empty --to agent id"
    );
    assert!(
        invalid["recovery"]["reason"]
            .as_str()
            .unwrap()
            .contains("non-empty")
    );
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
}

#[test]
fn explicit_file_claims_keep_basic_agents_disjoint_without_shared_grants() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    initialize_todo_db(root).unwrap();
    let dir = root.to_str().unwrap();
    let a = add_scoped_task(root, "Amber parcel", r#"paths:["src/a.rs"]"#, dir);
    let b = add_scoped_task(root, "Blue parcel", r#"paths:["src/b.rs"]"#, dir);
    assert_ne!(a, b);
    assert_eq!(
        claim_task(root, &a, "alpha", ClaimMode::Exclusive).unwrap()["status"],
        "ok"
    );
    assert_eq!(
        claim_task(root, &b, "beta", ClaimMode::Exclusive).unwrap()["status"],
        "ok"
    );
    assert!(!check_trust_level(root, "alpha", "verified").unwrap());
    assert!(claim_task(root, &a, "beta", ClaimMode::Shared).is_err());
    let overlap = add_scoped_task(root, "Copper parcel", r#"paths:["src/a.rs"]"#, dir);
    let overlap_claim = claim_task(root, &overlap, "gamma", ClaimMode::Exclusive).unwrap();
    assert_eq!(overlap_claim["status"], "ok", "{overlap_claim}");
    assert_eq!(
        overlap_claim["result"]["authoring_isolation"],
        "custody_acquired_workspace_unproven"
    );
    let expected = std::fs::canonicalize(root).unwrap().join("src/a.rs");
    assert_eq!(
        get_work_claim(root, &a).unwrap().unwrap().paths,
        vec![expected.display().to_string()]
    );
    rebuild_from_events(root).unwrap();
    assert_eq!(
        get_task(root, &a).unwrap().unwrap().scope,
        r#"paths:["src/a.rs"]"#
    );
}

#[test]
fn explicit_scope_add_rejects_invalid_paths_and_existing_symlinks() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    initialize_todo_db(root).unwrap();
    let args = |scope: &str| TodoCommand::Add {
        title: "Literal footprint".into(),
        description: String::new(),
        tags: String::new(),
        owner: String::new(),
        due: None,
        r#ref: String::new(),
        scope: scope.into(),
        dir: Some(root.to_string_lossy().into_owned()),
        priority: "medium".into(),
        depends_on: String::new(),
        blocks: String::new(),
        parent: None,
        one_shot: 0,
    };
    for scope in [
        "paths:[]",
        r#"paths:["../outside"]"#,
        r#"paths:["/absolute"]"#,
        "paths:invalid",
    ] {
        assert!(add_task(root, &args(scope)).is_err(), "{scope}");
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(tmp.path(), root.join("alias")).unwrap();
        let error = add_task(root, &args(r#"paths:["alias/file.rs"]"#)).unwrap_err();
        assert!(error.to_string().contains("symlink"));
    }
}

#[test]
fn explicit_scope_handoff_treats_source_overlap_as_advisory() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    initialize_todo_db(root).unwrap();
    let dir = root.to_str().unwrap();
    let a = add_scoped_task(root, "Amber parcel", r#"paths:["a.rs"]"#, dir);
    let same = add_scoped_task(root, "Copper parcel", r#"paths:["a.rs"]"#, dir);
    let different = add_scoped_task(root, "Blue parcel", r#"paths:["b.rs"]"#, dir);
    for id in [&a, &same, &different] {
        assert_eq!(
            claim_task(root, id, "alpha", ClaimMode::Exclusive).unwrap()["status"],
            "ok"
        );
    }
    let store = Store {
        kind: StoreKind::Repo,
        root: root.to_path_buf(),
    };
    let db = Connection::open(root.join(schemas::LOCAL_DB_NAME)).unwrap();
    set_agent_trust(&db, "alpha", "verified", "100Z");
    policy::approve_action(&store, "todo.handoff", None, "operator", "global").unwrap();
    assert_eq!(
        handoff_task_as(
            &store,
            &same,
            "beta",
            Some("alpha"),
            "overlap must remain",
            None,
            "alpha"
        )
        .unwrap()["status"],
        "ok"
    );
    assert_eq!(
        handoff_task_as(
            &store,
            &different,
            "beta",
            Some("alpha"),
            "disjoint footprint",
            None,
            "alpha"
        )
        .unwrap()["status"],
        "ok"
    );
    assert_eq!(
        get_task(root, &different).unwrap().unwrap().scope,
        r#"paths:["b.rs"]"#
    );
}

fn write_basic_shared_config(repo: &Path, enabled: bool) {
    let config = format!(
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"ascii\"\nentrypoints = []\n[repo]\n[governance]\nallow_basic_shared_claims = {enabled}\n"
    );
    std::fs::write(repo.join(".decapod/config.toml"), config).unwrap();
}

#[test]
fn basic_shared_claim_opt_in_is_bounded_and_audited() {
    let temp = tempdir().unwrap();
    let repo = temp.path();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success()
    );
    let root = repo.join(".decapod/data");
    std::fs::create_dir_all(&root).unwrap();
    initialize_todo_db(&root).unwrap();
    let id = add_scoped_task(&root, "Owned shared work", "", repo.to_str().unwrap());
    // Create both fixtures before claiming: active-task deduplication can reuse
    // a similar already-owned task instead of creating an unowned task.
    let unowned = add_scoped_task(&root, "Unowned shared work", "", repo.to_str().unwrap());
    assert_ne!(id, unowned);
    assert!(
        get_task(&root, &unowned)
            .unwrap()
            .unwrap()
            .assigned_to
            .is_empty()
    );
    assert_eq!(
        claim_task(&root, &id, "owner", ClaimMode::Exclusive).unwrap()["status"],
        "ok"
    );
    write_basic_shared_config(repo, false);
    assert!(claim_task(&root, &id, "member", ClaimMode::Shared).is_err());
    write_basic_shared_config(repo, true);
    assert_eq!(
        claim_task(&root, &id, "member", ClaimMode::Shared).unwrap()["status"],
        "ok"
    );
    assert!(!check_trust_level(&root, "member", "verified").unwrap());
    let task = get_task(&root, &id).unwrap().unwrap();
    assert!(
        task.owners
            .iter()
            .any(|owner| owner.agent_id == "member" && owner.claim_type == "secondary")
    );
    let db = Connection::open(root.join(schemas::LOCAL_DB_NAME)).unwrap();
    let (event_id, ts, actor, subject_kind, subject_id, raw): (
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
    ) = db
        .query_row(
            "SELECT event_id, ts, actor, subject_kind, subject_id, payload
             FROM events
             WHERE stream='todo' AND subject_id=?1 AND event_type='policy.basic_shared_claim.used'
             ORDER BY seq DESC LIMIT 1",
            [&id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    assert!(raw.contains("\"_ce_v1\""));
    let event = decapod::core::events::reconstruct_stored_payload(
        "todo",
        &event_id,
        &ts,
        "policy.basic_shared_claim.used",
        &actor,
        subject_kind.as_deref(),
        subject_id.as_deref(),
        &raw,
    )
    .unwrap();
    assert_eq!(event["actor"], "member");
    assert_eq!(event["payload"]["primary_owner_label"], "owner");
    assert_eq!(
        event["payload"]["identity_assurance"],
        "cooperative_not_authenticated"
    );
    assert!(
        event["payload"]["config_digest"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    set_agent_trust(&db, "untrusted", "untrusted", "1793750000Z");
    drop(db);
    assert!(claim_task(&root, &id, "untrusted", ClaimMode::Shared).is_err());
    assert!(claim_task(&root, &id, "owner", ClaimMode::Shared).is_err());
    assert!(claim_task(&root, &unowned, "other", ClaimMode::Shared).is_err());
    write_basic_shared_config(repo, false);
    assert!(claim_task(&root, &id, "later", ClaimMode::Shared).is_err());
    assert!(
        get_task(&root, &id)
            .unwrap()
            .unwrap()
            .owners
            .iter()
            .any(|owner| owner.agent_id == "member")
    );
}

#[test]
fn basic_shared_claim_does_not_override_stronger_or_approval_gates() {
    let temp = tempdir().unwrap();
    let repo = temp.path();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success()
    );
    let root = repo.join(".decapod/data");
    std::fs::create_dir_all(&root).unwrap();
    initialize_todo_db(&root).unwrap();
    let id = add_scoped_task(&root, "Guarded shared work", "", repo.to_str().unwrap());
    claim_task(&root, &id, "owner", ClaimMode::Exclusive).unwrap();
    write_basic_shared_config(repo, true);
    let db = Connection::open(root.join(schemas::LOCAL_DB_NAME)).unwrap();
    db.execute(
        "UPDATE risk_zones SET required_trust_level='core' WHERE zone_name='todo.claim.shared'",
        [],
    )
    .unwrap();
    drop(db);
    assert!(claim_task(&root, &id, "member", ClaimMode::Shared).is_err());
    let db = Connection::open(root.join(schemas::LOCAL_DB_NAME)).unwrap();
    db.execute("UPDATE risk_zones SET required_trust_level='verified', requires_approval=1 WHERE zone_name='todo.claim.shared'", []).unwrap();
    drop(db);
    assert!(claim_task(&root, &id, "member", ClaimMode::Shared).is_err());
    let db = Connection::open(root.join(schemas::LOCAL_DB_NAME)).unwrap();
    let count: i64 = db.query_row("SELECT count(*) FROM events WHERE stream='todo' AND event_type='policy.basic_shared_claim.used'", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 0);
}

#[test]
fn basic_shared_claim_rolls_back_if_audit_append_fails() {
    let temp = tempdir().unwrap();
    let repo = temp.path();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success()
    );
    let root = repo.join(".decapod/data");
    std::fs::create_dir_all(&root).unwrap();
    initialize_todo_db(&root).unwrap();
    let id = add_scoped_task(&root, "Audit rollback", "", repo.to_str().unwrap());
    claim_task(&root, &id, "owner", ClaimMode::Exclusive).unwrap();
    write_basic_shared_config(repo, true);
    let db = Connection::open(root.join(schemas::LOCAL_DB_NAME)).unwrap();
    // A trigger is one statement; execute_batch splits its body semicolon.
    db.execute("CREATE TRIGGER reject_shared_audit BEFORE INSERT ON events WHEN NEW.event_type='policy.basic_shared_claim.used' BEGIN SELECT RAISE(ABORT, 'audit unavailable'); END;", []).unwrap();
    drop(db);
    assert!(claim_task(&root, &id, "member", ClaimMode::Shared).is_err());
    let task = get_task(&root, &id).unwrap().unwrap();
    assert!(!task.owners.iter().any(|owner| owner.agent_id == "member"));
}

#[test]
fn concurrent_same_task_claims_commit_one_owner_and_one_success_event() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    initialize_todo_db(root).unwrap();
    let id = add_scoped_task(root, "Copper parcel", "root", root.to_str().unwrap());
    let barrier = Arc::new(Barrier::new(2));
    let workers: Vec<_> = ["alpha", "beta"]
        .into_iter()
        .map(|actor| {
            let root = root.to_owned();
            let id = id.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                claim_task(&root, &id, actor, ClaimMode::Exclusive).unwrap()
            })
        })
        .collect();
    let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(
        results.iter().filter(|v| v["status"] == "ok").count(),
        1,
        "{results:?}"
    );
    let db = Connection::open(root.join(schemas::LOCAL_DB_NAME)).unwrap();
    let count:i64=db.query_row("SELECT count(*) FROM events WHERE stream='todo' AND event_type='task.claim' AND subject_id=?1",[&id],|r|r.get(0)).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn named_resource_custody_excludes_even_the_same_agent() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();
    initialize_todo_db(root).unwrap();
    let a = add_scoped_task(
        root,
        "Copper parcel",
        "resource:release-production",
        root.to_str().unwrap(),
    );
    let b = add_scoped_task(
        root,
        "Silver parcel",
        "resource:release-production",
        root.to_str().unwrap(),
    );
    let c = add_scoped_task(
        root,
        "Golden parcel",
        "resource:integration-staging",
        root.to_str().unwrap(),
    );
    assert_eq!(
        claim_task(root, &a, "alpha", ClaimMode::Exclusive).unwrap()["status"],
        "ok"
    );
    for actor in ["alpha", "beta"] {
        let blocked = claim_task(root, &b, actor, ClaimMode::Exclusive).unwrap();
        assert_eq!(blocked["status"], "conflict");
        assert_eq!(blocked["result"]["conflict_class"], "shared_resource");
    }
    assert_eq!(
        claim_task(root, &c, "beta", ClaimMode::Exclusive).unwrap()["status"],
        "ok"
    );
}

fn init_git_repo_with_store(repo: &Path) -> PathBuf {
    for args in [
        &["init", "-q", "-b", "master"][..],
        &["config", "user.email", "test@example.invalid"],
        &["config", "user.name", "Test"],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(repo)
                .status()
                .unwrap()
                .success()
        );
    }
    std::fs::write(
        repo.join(".gitignore"),
        ".decapod/data/\n.decapod/managed/\n.decapod/workspaces/\n",
    )
    .unwrap();
    std::fs::create_dir_all(repo.join(".decapod")).unwrap();
    write_basic_shared_config(repo, false);
    std::fs::write(repo.join("README.md"), "# fixture\n").unwrap();
    for args in [
        &["add", ".gitignore", ".decapod/config.toml", "README.md"][..],
        &["-c", "commit.gpgsign=false", "commit", "-qm", "base"],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(repo)
                .status()
                .unwrap()
                .success()
        );
    }
    let store = repo.join(".decapod/data");
    std::fs::create_dir_all(&store).unwrap();
    initialize_todo_db(&store).unwrap();
    store
}

fn add_canonical_worktree(repo: &Path, name: &str, branch: &str) -> PathBuf {
    let wt = repo.join(".decapod/workspaces").join(name);
    std::fs::create_dir_all(wt.parent().unwrap()).unwrap();
    let out = Command::new("git")
        .args([
            "worktree",
            "add",
            "-q",
            "-b",
            branch,
            wt.to_str().unwrap(),
            "master",
        ])
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git worktree add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    wt
}

#[test]
fn authoring_bind_forced_event_failure_rolls_back_with_zero_orphan_rows() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    let store = init_git_repo_with_store(repo);
    let task_id = add_scoped_task(
        &store,
        "Transactional bind rollback",
        "root",
        repo.to_str().unwrap(),
    );
    assert_eq!(
        claim_task(&store, &task_id, "alpha", ClaimMode::Exclusive).unwrap()["status"],
        "ok"
    );
    let wt = add_canonical_worktree(
        repo,
        "alpha-wt-rollback",
        &format!("agent/alpha/{task_id}-rb"),
    );

    let db = Connection::open(store.join(schemas::LOCAL_DB_NAME)).unwrap();
    let zone_trust: String = db
        .query_row(
            "SELECT required_trust_level FROM risk_zones WHERE zone_name='workspace.authoring'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(zone_trust, "basic");
    db.execute(
        "CREATE TRIGGER reject_authoring_bind_audit BEFORE INSERT ON events WHEN NEW.event_type='authoring.bind' BEGIN SELECT RAISE(ABORT, 'forced_authoring_bind_event_failure'); END;",
        [],
    )
    .unwrap();
    drop(db);

    let err = decapod::core::todo::admit_authoring(&store, &wt, "alpha")
        .unwrap_err()
        .to_string();
    assert!(err.contains("forced_authoring_bind_event_failure"), "{err}");

    let db = Connection::open(store.join(schemas::LOCAL_DB_NAME)).unwrap();
    let orphan_rows: i64 = db
        .query_row("SELECT count(*) FROM authoring_bindings", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        orphan_rows, 0,
        "failed event append must roll back authoring_bindings"
    );
    let bind_events: i64 = db
        .query_row(
            "SELECT count(*) FROM events WHERE event_type='authoring.bind'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(bind_events, 0);
    let broker_ops: i64 = db
        .query_row(
            "SELECT count(*) FROM events WHERE stream='broker' AND payload LIKE '%workspace.authoring.bind%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        broker_ops >= 1,
        "admit_authoring must use dedicated workspace.authoring.bind op"
    );

    db.execute("DROP TRIGGER reject_authoring_bind_audit", [])
        .unwrap();
    drop(db);

    decapod::core::todo::admit_authoring(&store, &wt, "alpha").unwrap();
    decapod::core::todo::check_authoring(&store, &wt, "alpha", false).unwrap();
    let db = Connection::open(store.join(schemas::LOCAL_DB_NAME)).unwrap();
    let committed_rows: i64 = db
        .query_row("SELECT count(*) FROM authoring_bindings", [], |r| r.get(0))
        .unwrap();
    assert_eq!(committed_rows, 1);
}

#[test]
fn authoring_binding_one_to_one_uniqueness_renew_and_full_lifecycle_retirement() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    let store = init_git_repo_with_store(repo);
    let task_id = add_scoped_task(&store, "Lifecycle custody", "root", repo.to_str().unwrap());
    assert_eq!(
        claim_task(&store, &task_id, "alpha", ClaimMode::Exclusive).unwrap()["status"],
        "ok"
    );

    let wt1 = add_canonical_worktree(repo, "alpha-wt-1", &format!("agent/alpha/{task_id}-one"));
    let wt2 = add_canonical_worktree(repo, "alpha-wt-2", &format!("agent/alpha/{task_id}-two"));

    // Unbound checkout fails closed in check_authoring.
    assert!(
        decapod::core::todo::check_authoring(&store, &wt1, "alpha", false)
            .unwrap_err()
            .to_string()
            .contains("AUTHORING_BINDING_REQUIRED")
    );
    assert!(
        decapod::core::todo::check_authoring(&store, &wt1, "unknown", false)
            .unwrap_err()
            .to_string()
            .contains("AUTHORING_ACTOR_REQUIRED")
    );

    // Bind wt1 -> second live worktree wt2 for the same task must be rejected.
    decapod::core::todo::admit_authoring(&store, &wt1, "alpha").unwrap();
    let dup_err = decapod::core::todo::admit_authoring(&store, &wt2, "alpha")
        .unwrap_err()
        .to_string();
    assert!(
        dup_err.contains("AUTHORING_TASK_ALREADY_BOUND"),
        "{dup_err}"
    );

    // Same-owner todo renew advances the already-admitted binding atomically,
    // preserving wt1 admissibility while still rejecting dual live worktree wt2.
    let renewed = renew_claim_lease(&store, &task_id, "alpha", Some(1800)).unwrap();
    assert_eq!(renewed["status"], "ok");
    decapod::core::todo::check_authoring(&store, &wt1, "alpha", false)
        .expect("renewed lease must advance admitted authoring binding");
    assert!(
        decapod::core::todo::admit_authoring(&store, &wt2, "alpha")
            .unwrap_err()
            .to_string()
            .contains("AUTHORING_TASK_ALREADY_BOUND")
    );

    // Removed/deleted worktree recovery: removing wt1 allows wt2 to replace the dead binding.
    assert!(
        Command::new("git")
            .args(["worktree", "remove", "--force", wt1.to_str().unwrap()])
            .current_dir(repo)
            .status()
            .unwrap()
            .success()
    );
    decapod::core::todo::admit_authoring(&store, &wt2, "alpha")
        .expect("removed worktree binding must be replaced cleanly");
    decapod::core::todo::check_authoring(&store, &wt2, "alpha", false).unwrap();

    let count_bindings = |store: &Path| -> i64 {
        let db = Connection::open(store.join(schemas::LOCAL_DB_NAME)).unwrap();
        db.query_row("SELECT count(*) FROM authoring_bindings", [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(count_bindings(&store), 1);

    // 1. yield retires binding.
    assert_eq!(
        yield_claim_lease(&store, &task_id, "alpha", "pause").unwrap()["status"],
        "ok"
    );
    assert_eq!(count_bindings(&store), 0);
    assert!(decapod::core::todo::check_authoring(&store, &wt2, "alpha", false).is_err());

    // 2. re-claim + bind, then release_task_as_actor retires binding.
    claim_task(&store, &task_id, "alpha", ClaimMode::Exclusive).unwrap();
    decapod::core::todo::admit_authoring(&store, &wt2, "alpha").unwrap();
    assert_eq!(count_bindings(&store), 1);
    assert_eq!(
        decapod::core::todo::release_task_as_actor(&store, &task_id, "alpha").unwrap()["status"],
        "ok"
    );
    assert_eq!(count_bindings(&store), 0);

    // 3. re-claim with 3s lease + bind, then expired reclaim by beta retires alpha's binding and requires fresh bind.
    assert_eq!(
        claim_task_with_lease(
            &store,
            &task_id,
            "alpha",
            ClaimMode::Exclusive,
            Some(3),
            None
        )
        .unwrap()["status"],
        "ok"
    );
    decapod::core::todo::admit_authoring(&store, &wt2, "alpha").unwrap();
    std::thread::sleep(Duration::from_millis(3200));
    let reclaimed = claim_task(&store, &task_id, "beta", ClaimMode::Exclusive).unwrap();
    assert_eq!(reclaimed["status"], "ok");
    assert_eq!(count_bindings(&store), 0);
    assert!(decapod::core::todo::check_authoring(&store, &wt2, "beta", false).is_err());
    decapod::core::todo::admit_authoring(&store, &wt2, "beta").unwrap();
    assert_eq!(count_bindings(&store), 1);

    // 4. handoff from beta to gamma retires beta's binding and requires fresh bind for gamma.
    {
        let db = Connection::open(store.join(schemas::LOCAL_DB_NAME)).unwrap();
        set_agent_trust(&db, "beta", "verified", "1793750000Z");
        set_agent_trust(&db, "gamma", "verified", "1793750000Z");
    }
    let repo_store = Store {
        kind: StoreKind::Repo,
        root: store.clone(),
    };
    policy::approve_action(&repo_store, "todo.handoff", None, "operator", "global").unwrap();
    let handed = handoff_task_as(
        &repo_store,
        &task_id,
        "gamma",
        Some("beta"),
        "transfer",
        Some(1800),
        "beta",
    )
    .unwrap();
    assert_eq!(handed["status"], "ok");
    assert_eq!(count_bindings(&store), 0);
    assert!(decapod::core::todo::check_authoring(&store, &wt2, "gamma", false).is_err());
    decapod::core::todo::admit_authoring(&store, &wt2, "gamma").unwrap();
    assert_eq!(count_bindings(&store), 1);

    // 5. archive (and done) retires binding via update_status.
    assert_eq!(
        decapod::core::todo::release_task_as_actor(&store, &task_id, "gamma").unwrap()["status"],
        "ok"
    );

    let archive_task = add_scoped_task(&store, "Archive lifecycle", "root", repo.to_str().unwrap());
    claim_task(&store, &archive_task, "alpha", ClaimMode::Exclusive).unwrap();
    let wt_arch = add_canonical_worktree(
        repo,
        "alpha-wt-arch",
        &format!("agent/alpha/{archive_task}-arch"),
    );
    decapod::core::todo::admit_authoring(&store, &wt_arch, "alpha").unwrap();
    assert_eq!(count_bindings(&store), 1);
    policy::approve_action(&repo_store, "task.archive", None, "operator", "global").unwrap();
    let arch_snapshot = get_task(&store, &archive_task).unwrap().unwrap();
    update_status(
        &repo_store,
        &archive_task,
        arch_snapshot.revision,
        &arch_snapshot.status,
        "archived",
        "task.archive",
        serde_json::json!({}),
    )
    .unwrap();
    assert_eq!(count_bindings(&store), 0);

    // Also verify `done` transition retires binding via update_status.
    let done_task = add_scoped_task(&store, "Done lifecycle", "root", repo.to_str().unwrap());
    claim_task(&store, &done_task, "alpha", ClaimMode::Exclusive).unwrap();
    let wt_done = add_canonical_worktree(
        repo,
        "alpha-wt-done",
        &format!("agent/alpha/{done_task}-done"),
    );
    decapod::core::todo::admit_authoring(&store, &wt_done, "alpha").unwrap();
    assert_eq!(count_bindings(&store), 1);
    decapod::core::todo::release_task_as_actor(&store, &done_task, "alpha").unwrap();
    // Seed a residual pre-done binding row so update_status("done") exercises retirement directly.
    let wt_done_obs = decapod::core::workspace::observe_checkout(&wt_done);
    {
        let db = Connection::open(store.join(schemas::LOCAL_DB_NAME)).unwrap();
        db.execute(
            "INSERT INTO authoring_bindings(worktree_id,task_id,agent_id,lease_generation,branch,repository_id,updated_at) VALUES(?1,?2,'alpha',1,?3,?4,'200Z')",
            decapod::core::db::params![
                wt_done_obs.worktree_id,
                done_task,
                wt_done_obs.branch,
                wt_done_obs.repository_id
            ],
        )
        .unwrap();
    }
    assert_eq!(count_bindings(&store), 1);
    let done_snapshot = get_task(&store, &done_task).unwrap().unwrap();
    update_status(
        &repo_store,
        &done_task,
        done_snapshot.revision,
        &done_snapshot.status,
        "done",
        "task.done",
        serde_json::json!({}),
    )
    .unwrap();
    assert_eq!(count_bindings(&store), 0);

    // 6. workspace prune retires any residual binding on a pruned Decapod-owned worktree.
    let prune_tmp = tempdir().unwrap();
    let prune_repo = prune_tmp.path();
    let prune_store = init_git_repo_with_store(prune_repo);
    let prune_repo_store = Store {
        kind: StoreKind::Repo,
        root: prune_store.clone(),
    };
    let prune_task = add_scoped_task(
        &prune_store,
        "Prune lifecycle",
        "root",
        prune_repo.to_str().unwrap(),
    );
    claim_task(&prune_store, &prune_task, "alpha", ClaimMode::Exclusive).unwrap();
    let ensured_prune = decapod::core::workspace::ensure_workspace(
        prune_repo,
        Some(decapod::core::workspace::WorkspaceConfig {
            branch: Some(format!("agent/alpha/{prune_task}-prune")),
            use_container: false,
            base_image: None,
            base_branch: Some("master".to_string()),
        }),
        "alpha",
    )
    .unwrap();
    let wt_prune = ensured_prune.git.worktree_path.unwrap();
    decapod::core::todo::release_task_as_actor(&prune_store, &prune_task, "alpha").unwrap();
    let prune_snap = get_task(&prune_store, &prune_task).unwrap().unwrap();
    update_status(
        &prune_repo_store,
        &prune_task,
        prune_snap.revision,
        &prune_snap.status,
        "done",
        "task.done",
        serde_json::json!({}),
    )
    .unwrap();
    assert_eq!(count_bindings(&prune_store), 0);
    let wt_prune_obs = decapod::core::workspace::observe_checkout(&wt_prune);
    {
        let db = Connection::open(prune_store.join(schemas::LOCAL_DB_NAME)).unwrap();
        db.execute(
            "INSERT INTO authoring_bindings(worktree_id,task_id,agent_id,lease_generation,branch,repository_id,updated_at) VALUES(?1,?2,'alpha',1,?3,?4,'200Z')",
            decapod::core::db::params![
                wt_prune_obs.worktree_id,
                prune_task,
                wt_prune_obs.branch,
                wt_prune_obs.repository_id
            ],
        )
        .unwrap();
    }
    assert_eq!(count_bindings(&prune_store), 1);
    let prune_report = decapod::core::workspace::prune_workspaces_report(prune_repo, true).unwrap();
    assert!(
        !prune_report.pruned.is_empty(),
        "expected wt_prune to be pruned, got {prune_report:?}"
    );
    assert_eq!(count_bindings(&prune_store), 0);

    // 7. Missing datastore fails closed in get_workspace_status.
    let missing_repo = tempdir().unwrap();
    let missing_store = init_git_repo_with_store(missing_repo.path());
    let missing_wt = add_canonical_worktree(
        missing_repo.path(),
        "alpha-wt-missing-db",
        "agent/alpha/bugs_0123456789abcdef-missing",
    );
    std::fs::remove_dir_all(&missing_store).unwrap();
    let ws_status = decapod::core::workspace::get_workspace_status(&missing_wt).unwrap();
    assert!(!ws_status.can_work, "{ws_status:?}");
    assert!(!ws_status.blockers.is_empty());
}

#[test]
fn shared_resource_reservations_gate_release_and_publish_without_blocking_source_authoring() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    let store = init_git_repo_with_store(repo);

    let rel_task = add_scoped_task(
        &store,
        "Release shepherd",
        "resource:release",
        repo.to_str().unwrap(),
    );
    let pub_task = add_scoped_task(
        &store,
        "Integration shepherd",
        "resource:integration:master",
        repo.to_str().unwrap(),
    );
    let src_task = add_scoped_task(
        &store,
        "Isolated feature work",
        "root",
        repo.to_str().unwrap(),
    );

    assert_eq!(
        claim_task(&store, &rel_task, "shepherd", ClaimMode::Exclusive).unwrap()["status"],
        "ok"
    );
    assert_eq!(
        claim_task(&store, &pub_task, "shepherd", ClaimMode::Exclusive).unwrap()["status"],
        "ok"
    );

    // Source task claim in the same repo succeeds without bogus path_overlap against resource tasks.
    let src_claimed = claim_task(&store, &src_task, "author", ClaimMode::Exclusive).unwrap();
    assert_eq!(src_claimed["status"], "ok");
    assert!(
        src_claimed["result"].get("advisory_overlaps").is_none(),
        "resource reservations must not create advisory dir_path overlaps: {src_claimed}"
    );
    assert_eq!(
        src_claimed["result"]["authoring_permission"],
        "permitted_after_isolated_workspace_bind"
    );

    // Isolated authoring admission for `author` succeeds while `shepherd` holds release/integration.
    let author_wt = add_canonical_worktree(
        repo,
        "author-wt",
        &format!("agent/author/{src_task}-feature"),
    );
    decapod::core::todo::admit_authoring(&store, &author_wt, "author").unwrap();
    decapod::core::todo::check_authoring(&store, &author_wt, "author", false).unwrap();

    // Shared mutation boundaries reject `author` with SHARED_RESOURCE_OCCUPIED while allowing `shepherd`.
    let rel_err = decapod::core::todo::enforce_shared_resource_available(
        &store,
        "author",
        &["resource:release"],
        "release.lineage_sync",
    )
    .unwrap_err()
    .to_string();
    assert!(rel_err.contains("SHARED_RESOURCE_OCCUPIED"), "{rel_err}");
    assert!(rel_err.contains("resource:release"), "{rel_err}");
    assert!(rel_err.contains(&rel_task), "{rel_err}");
    assert!(rel_err.contains("shepherd"), "{rel_err}");

    let pub_err = decapod::core::todo::enforce_shared_resource_available(
        &store,
        "author",
        &["resource:integration", "resource:integration:master"],
        "workspace.publish",
    )
    .unwrap_err()
    .to_string();
    assert!(pub_err.contains("SHARED_RESOURCE_OCCUPIED"), "{pub_err}");
    assert!(pub_err.contains("resource:integration:master"), "{pub_err}");

    // CLI boundary check: `decapod release lineage-sync` rejects `author` and admits `shepherd`.
    let bin = resolve_decapod_bin();
    let denied_cli = Command::new(&bin)
        .current_dir(&author_wt)
        .env("DECAPOD_AGENT_ID", "author")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .args(["release", "lineage-sync"])
        .output()
        .unwrap();
    assert!(
        !denied_cli.status.success(),
        "non-owner release lineage-sync must fail"
    );
    let combined_denied = format!(
        "{}{}",
        String::from_utf8_lossy(&denied_cli.stdout),
        String::from_utf8_lossy(&denied_cli.stderr)
    );
    assert!(
        combined_denied.contains("SHARED_RESOURCE_OCCUPIED"),
        "{combined_denied}"
    );

    decapod::core::todo::enforce_shared_resource_available(
        &store,
        "shepherd",
        &["resource:release"],
        "release.lineage_sync",
    )
    .expect("resource reservation owner may execute shared mutation");
    decapod::core::todo::enforce_shared_resource_available(
        &store,
        "shepherd",
        &["resource:integration", "resource:integration:master"],
        "workspace.publish",
    )
    .expect("integration reservation owner may publish");

    // Releasing the reservations unblocks `author`.
    yield_claim_lease(&store, &rel_task, "shepherd", "done").unwrap();
    yield_claim_lease(&store, &pub_task, "shepherd", "done").unwrap();
    decapod::core::todo::enforce_shared_resource_available(
        &store,
        "author",
        &["resource:release"],
        "release.lineage_sync",
    )
    .unwrap();
    decapod::core::todo::enforce_shared_resource_available(
        &store,
        "author",
        &["resource:integration", "resource:integration:master"],
        "workspace.publish",
    )
    .unwrap();
}
