use dactyl_db::{AccessMode, Connection, DatastoreRoute, Operation, OperationKind};
use decapod::core::backend::{BackendRoute, CloudDatastore, StorageContext};
use decapod::core::dactyl::{DactylBridge, OperationResult};
use decapod::core::dactyl_todo::DactylTodoStore;
use decapod::core::repo_identity::RepositoryIdentity;
use decapod::core::storage::{Task, TodoStore};
use serde_json::{Value, json};
use tempfile::TempDir;
use time::OffsetDateTime;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn read_http_request(stream: &mut TcpStream) -> (String, Value) {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1024];
    let header_end = loop {
        let count = stream.read(&mut chunk).await.expect("read request");
        assert!(count > 0, "request ended before headers");
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = String::from_utf8(bytes[..header_end].to_vec()).expect("request headers");
    let content_length = headers
        .lines()
        .find_map(|line| {
            line.strip_prefix("Content-Length:")
                .or_else(|| line.strip_prefix("content-length:"))
        })
        .expect("content length")
        .trim()
        .parse::<usize>()
        .expect("numeric content length");
    while bytes.len() < header_end + content_length {
        let count = stream.read(&mut chunk).await.expect("read request body");
        assert!(count > 0, "request ended before body");
        bytes.extend_from_slice(&chunk[..count]);
    }
    let body = serde_json::from_slice(&bytes[header_end..header_end + content_length])
        .expect("JSON request body");
    (headers, body)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cloud_dactyl_uses_query_with_opaque_context_not_backend_query_inputs() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake Dactyl service");
    let address = listener.local_addr().expect("fake service address");
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept Dactyl request");
        let (headers, body) = read_http_request(&mut stream).await;
        assert!(
            headers.starts_with("POST /query HTTP/1.1"),
            "unexpected request: {headers}"
        );
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("authorization: bearer test-token")
        );
        assert_eq!(body["context"]["version"], 1);
        assert!(body["context"].is_object());
        assert_eq!(
            body["context"]["payload"]["route"]["Cloud"]["repository"]["canonical_name"],
            "DecapodLabs/decapod"
        );
        assert!(body.get("backend").is_none());
        assert!(body["sql"].as_str().unwrap().contains("SELECT"));

        let response =
            br#"{"columns":["id","title"],"rows":[{"id":"task_1","title":"from dactyl"}]}"#;
        let response_headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response.len()
        );
        stream
            .write_all(response_headers.as_bytes())
            .await
            .expect("write response headers");
        stream
            .write_all(response)
            .await
            .expect("write response body");
    });

    let rows = tokio::task::spawn_blocking(move || {
        let identity = RepositoryIdentity {
            canonical_name: "DecapodLabs/decapod".to_string(),
            owner: "DecapodLabs".to_string(),
            repository: "decapod".to_string(),
            remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
        };
        let route = BackendRoute::cloud(identity, format!("http://{address}"))
            .expect("validated cloud route");
        let context = StorageContext::from_route(route, Some("test-token"))
            .expect("authenticated storage context")
            .with_cloud_datastore(CloudDatastore::Neon)
            .expect("explicit Neon compatibility route");
        let bridge = DactylBridge::from_storage_context(&context, AccessMode::ReadOnly)
            .expect("Dactyl bridge");
        bridge
            .read("SELECT id, title FROM tasks", &[])
            .expect("remote query")
    })
    .await
    .expect("Dactyl client task");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows.as_slice()[0].get_str("id").expect("id"), "task_1");
    assert_eq!(
        rows.as_slice()[0].get_str("title").expect("title"),
        "from dactyl"
    );
    server.await.expect("fake Dactyl service");
}

fn rows_to_dactyl_json(rows: dactyl_db::Rows) -> Value {
    let columns = rows
        .as_slice()
        .first()
        .map(|r| r.columns.clone())
        .unwrap_or_default();
    let encoded_rows: Vec<Value> = rows
        .as_slice()
        .iter()
        .map(|row| {
            let mut obj = serde_json::Map::new();
            for (col, val) in row.columns.iter().zip(row.values.iter()) {
                obj.insert(col.clone(), val.clone());
            }
            Value::Object(obj)
        })
        .collect();
    json!({
        "columns": columns,
        "rows": encoded_rows,
        "affected_rows": 0,
        "generated_keys": []
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dactyl_todo_store_neon_atomic_cas_and_event_ordering_without_local_sqlite() {
    let server_dir = TempDir::new().expect("server dir");
    let server_db = server_dir.path().join("remote_neon_store.sqlite");
    let init_conn = Connection::open(DatastoreRoute::sqlite(
        server_db.to_string_lossy().to_string(),
    ))
    .expect("open remote sqlite fixture");
    init_conn
        .atomic(&[
            Operation::write(
                "CREATE TABLE IF NOT EXISTS tasks (
                    repo_id TEXT NOT NULL,
                    id TEXT PRIMARY KEY,
                    hash TEXT NOT NULL,
                    title TEXT NOT NULL,
                    description TEXT DEFAULT '',
                    tags TEXT DEFAULT '',
                    owner TEXT DEFAULT '',
                    status TEXT NOT NULL DEFAULT 'open',
                    dir_path TEXT NOT NULL,
                    scope TEXT NOT NULL,
                    priority TEXT DEFAULT 'medium',
                    category TEXT DEFAULT '',
                    assigned_to TEXT DEFAULT '',
                    assigned_at TEXT,
                    completed_at TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    version INTEGER NOT NULL DEFAULT 1
                )",
                vec![],
            ),
            Operation::write(
                "CREATE TABLE IF NOT EXISTS events (
                    event_id TEXT PRIMARY KEY,
                    ts TEXT NOT NULL,
                    seq INTEGER NOT NULL,
                    stream TEXT NOT NULL,
                    subject_kind TEXT,
                    subject_id TEXT,
                    event_type TEXT NOT NULL DEFAULT '',
                    payload TEXT NOT NULL,
                    actor TEXT NOT NULL
                )",
                vec![],
            ),
        ])
        .expect("init remote neon fixture schema");
    drop(init_conn);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake Neon Dactyl service");
    let address = listener.local_addr().expect("service addr");
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let db_for_server = server_db.clone();
    let server = tokio::spawn(async move {
        loop {
            let (mut stream, _) = tokio::select! {
                accepted = listener.accept() => accepted.expect("accept"),
                _ = &mut shutdown_rx => break,
            };
            let db_path = db_for_server.clone();
            let (headers, body) = read_http_request(&mut stream).await;
            assert!(
                headers
                    .to_ascii_lowercase()
                    .contains("authorization: bearer neon-token-1"),
                "missing bearer in headers: {headers}"
            );
            assert!(body.get("backend").is_none());
            assert_eq!(body["context"]["version"], 1);

            let first_line = headers.lines().next().unwrap_or("");
            let (status, status_text, reply_body) = if first_line.starts_with("POST /query ") {
                let op: Operation = serde_json::from_value(json!({
                    "kind": "read",
                    "sql": body["sql"],
                    "params": body["params"],
                }))
                .expect("decode query Operation");
                let conn = Connection::open(DatastoreRoute::sqlite(
                    db_path.to_string_lossy().to_string(),
                ))
                .expect("open fixture db");
                match conn.read(op.sql(), op.params()) {
                    Ok(rows) => (200, "OK", rows_to_dactyl_json(rows)),
                    Err(err) => (
                        500,
                        "Internal Server Error",
                        json!({"code": "storage_failure", "message": err.to_string()}),
                    ),
                }
            } else if first_line.starts_with("POST /batch ") {
                let ops: Vec<Operation> =
                    serde_json::from_value(body["operations"].clone()).expect("decode atomic ops");
                let conn = Connection::open(DatastoreRoute::sqlite(
                    db_path.to_string_lossy().to_string(),
                ))
                .expect("open fixture db");
                match conn.atomic(&ops) {
                    Ok(res) => {
                        let mut encoded = Vec::with_capacity(res.results.len());
                        for (op, item) in ops.iter().zip(res.results) {
                            match (op.kind(), item) {
                                (OperationKind::Read, OperationResult::Rows(rows)) => {
                                    encoded.push(rows_to_dactyl_json(rows));
                                }
                                (_, OperationResult::Write(w)) => {
                                    encoded.push(json!({
                                        "columns": [],
                                        "rows": [],
                                        "affected_rows": w.affected_rows,
                                        "generated_keys": []
                                    }));
                                }
                                _ => panic!("unexpected atomic operation result shape"),
                            }
                        }
                        (200, "OK", json!({ "results": encoded }))
                    }
                    Err(err) => (
                        409,
                        "Conflict",
                        json!({"code": "row_conflict", "message": err.to_string()}),
                    ),
                }
            } else {
                (
                    404,
                    "Not Found",
                    json!({"code": "not_found", "message": first_line}),
                )
            };

            let encoded = serde_json::to_string(&reply_body).unwrap();
            let resp = format!(
                "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{encoded}",
                encoded.len()
            );
            stream.write_all(resp.as_bytes()).await.unwrap();
        }
    });

    let repo_dir = TempDir::new().expect("repo dir");
    let local_sqlite = repo_dir.path().join(".decapod/data/decapod.db");
    let identity = RepositoryIdentity {
        canonical_name: "DecapodLabs/decapod".to_string(),
        owner: "DecapodLabs".to_string(),
        repository: "decapod".to_string(),
        remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
    };
    let route =
        BackendRoute::cloud(identity, format!("http://{address}")).expect("validated cloud route");
    let context = StorageContext::from_route(route, Some("neon-token-1"))
        .expect("storage context")
        .with_cloud_datastore(CloudDatastore::Neon)
        .expect("neon route");
    let store = DactylTodoStore::new(context, "DecapodLabs/decapod");

    let now = OffsetDateTime::now_utc();
    let cross_repo_err = store
        .add_task(
            Task {
                id: "todo_cross_repo".to_string(),
                repo_id: "OtherOrg/other-repo".to_string(),
                hash: "01cross".to_string(),
                title: "Cross repo denied".to_string(),
                description: None,
                status: "open".to_string(),
                assignee: None,
                scope: "core".to_string(),
                dir_path: ".".to_string(),
                priority: "high".to_string(),
                category: "features".to_string(),
                tags: vec!["dactyl".to_string()],
                created_at: now,
                updated_at: now,
                version: 1,
            },
            "agent-alpha".to_string(),
            "intent:cross-repo".to_string(),
        )
        .await
        .expect_err("cross-repo task must fail closed");
    assert!(cross_repo_err.to_string().contains("BlockedBeforeSubmit"));

    let added = store
        .add_task(
            Task {
                id: "todo_neon_parity_1".to_string(),
                repo_id: "DecapodLabs/decapod".to_string(),
                hash: "01neon".to_string(),
                title: "Dactyl Neon atomic parity".to_string(),
                description: Some("Verify event+state atomicity".to_string()),
                status: "open".to_string(),
                assignee: None,
                scope: "core".to_string(),
                dir_path: ".".to_string(),
                priority: "high".to_string(),
                category: "features".to_string(),
                tags: vec!["dactyl".to_string(), "cloud".to_string()],
                created_at: now,
                updated_at: now,
                version: 1,
            },
            "agent-alpha".to_string(),
            "intent:neon-add".to_string(),
        )
        .await
        .expect("add task via Neon Dactyl");
    assert_eq!(added.id, "todo_neon_parity_1");
    assert_eq!(added.version, 1);

    let listed = store.list_tasks().await.expect("list tasks");
    assert_eq!(listed.len(), 1);
    assert_eq!(
        store
            .get_task("todo_neon_parity_1")
            .await
            .expect("get task")
            .expect("exists")
            .title,
        "Dactyl Neon atomic parity"
    );

    // Concurrent claim CAS race: exactly one winner, loser fails closed.
    let (res_a, res_b) = tokio::join!(
        store.claim_task("todo_neon_parity_1", "agent-alpha".to_string()),
        store.claim_task("todo_neon_parity_1", "agent-beta".to_string())
    );
    let (winner, loser_err) = match (res_a, res_b) {
        (Ok(w), Err(e)) => (w, e),
        (Err(e), Ok(w)) => (w, e),
        (Ok(_), Ok(_)) => panic!("both concurrent claims succeeded"),
        (Err(e1), Err(e2)) => panic!("both concurrent claims failed: {e1} / {e2}"),
    };
    assert_eq!(winner.status, "in_progress");
    assert_eq!(winner.version, 2);
    assert!(!loser_err.to_string().is_empty());
    let winner_actor = winner.assignee.clone().expect("winner assignee");

    // Non-holder release fails; holder release succeeds (version 3); second agent claims (version 4) & completes (version 5).
    let other_actor = if winner_actor == "agent-alpha" {
        "agent-beta"
    } else {
        "agent-alpha"
    };
    assert!(
        store
            .release_task("todo_neon_parity_1", other_actor.to_string())
            .await
            .is_err()
    );
    let released = store
        .release_task("todo_neon_parity_1", winner_actor.clone())
        .await
        .expect("holder release");
    assert_eq!(released.status, "open");
    assert_eq!(released.version, 3);

    let reclaimed = store
        .claim_task("todo_neon_parity_1", other_actor.to_string())
        .await
        .expect("second agent claim");
    assert_eq!(reclaimed.version, 4);
    assert_eq!(reclaimed.assignee.as_deref(), Some(other_actor));

    let completed = store
        .complete_task(
            "todo_neon_parity_1",
            other_actor.to_string(),
            "verified".to_string(),
        )
        .await
        .expect("holder complete");
    assert_eq!(completed.status, "completed");
    assert_eq!(completed.version, 5);

    // Verify ordered event stream committed atomically in the remote Neon fixture and zero local SQLite DB created.
    let inspect = Connection::open(DatastoreRoute::sqlite(
        server_db.to_string_lossy().to_string(),
    ))
    .expect("inspect remote db");
    let event_rows = inspect
        .read(
            "SELECT seq, event_type, actor FROM events WHERE subject_id = $1 ORDER BY seq ASC",
            &["todo_neon_parity_1".into()],
        )
        .expect("query events");
    let types: Vec<String> = event_rows
        .as_slice()
        .iter()
        .map(|r| r.get_str("event_type").unwrap().to_string())
        .collect();
    assert_eq!(
        types,
        vec![
            "task.add",
            "task.claim",
            "task.release",
            "task.claim",
            "task.done"
        ]
    );
    assert!(
        !local_sqlite.exists(),
        "Cloud Dactyl operations must never create .decapod/data/decapod.db"
    );

    let _ = shutdown_tx.send(());
    server.await.expect("server shutdown");
}
