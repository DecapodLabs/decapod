//! Synthetic public Dactyl wire contract, not hosted authorization/atomicity proof.
// Dedicated fixture threads intentionally use bounded synchronous sockets.
#![allow(clippy::disallowed_types)]
use decapod::core::backend::{BackendRoute, CloudDatastore, StorageContext};
use decapod::core::cloud_todo_operation::{self, FailureKind, Outcome};
use decapod::core::dactyl_todo::DactylTodoStore;
use decapod::core::repo_identity::resolve_repository_identity_from_remote;
use decapod::core::storage::{Task, TodoStore};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

fn request(stream: &mut TcpStream) -> (String, Value) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let end = loop {
        let mut buffer = [0; 4096];
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
    assert!(headers.contains("authorization: bearer synthetic-test-token"));
    let size: usize = headers
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    while bytes.len() < end + size {
        let mut buffer = [0; 4096];
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buffer[..count]);
    }
    (
        headers,
        serde_json::from_slice(&bytes[end..end + size]).unwrap(),
    )
}
fn respond(stream: &mut TcpStream, status: u16, body: Value) {
    let body = body.to_string();
    write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
}
fn accept(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        match listener.accept() {
            Ok((stream, _)) => return stream,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(error) => panic!("missing request: {error}"),
        }
    }
}
fn task() -> Task {
    let now = chrono::Utc::now();
    Task {
        id: "todo_wire_resume".into(),
        repo_id: "example/repo".into(),
        hash: "wire12".into(),
        title: "private-task-title".into(),
        description: Some("private-task-body".into()),
        status: "open".into(),
        assignee: None,
        scope: "repo".into(),
        dir_path: "".into(),
        priority: "medium".into(),
        category: "".into(),
        tags: vec![],
        created_at: now,
        updated_at: now,
        version: 1,
    }
}
fn store(endpoint: &str) -> DactylTodoStore {
    let identity =
        resolve_repository_identity_from_remote("https://github.com/example/repo.git").unwrap();
    let context = StorageContext::from_route(
        BackendRoute::cloud(identity, endpoint).unwrap(),
        Some("synthetic-test-token"),
    )
    .unwrap()
    .with_cloud_datastore(CloudDatastore::Neon)
    .unwrap();
    DactylTodoStore::new(context, "example/repo")
}
fn rows(row: Value) -> Value {
    json!({"columns":row.as_object().unwrap().keys().collect::<Vec<_>>(), "rows":[row], "affected_rows":0})
}

#[tokio::test]
async fn lost_commit_ack_and_failed_reconciliation_keep_unknown_until_exact_retry_receipt() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut event = Value::Null;
        let mut task_row = serde_json::to_value(task()).unwrap();
        for step in 0..5 {
            let mut stream = accept(&listener);
            let (headers, body) = request(&mut stream);
            assert!(!body.to_string().contains("synthetic-test-token"));
            match step {
                0 => {
                    assert!(headers.starts_with("post /query "));
                    respond(
                        &mut stream,
                        200,
                        json!({"columns":["subject_id","payload"],"rows":[]}),
                    );
                }
                1 => {
                    assert!(headers.starts_with("post /batch "));
                    let operations = body["operations"].as_array().unwrap();
                    assert_eq!(operations.len(), 3);
                    let event_params = operations[1]["params"].as_array().unwrap();
                    event = json!({"subject_id":event_params[2],"payload":event_params[4]});
                    // Simulate commit followed by loss of the entire response.
                    task_row["tags"] = "".into();
                    task_row["assignee"] = "".into();
                }
                2 => {
                    assert!(headers.starts_with("post /query ")); /* reconciliation connection also lost */
                }
                3 => {
                    assert!(body["sql"].as_str().unwrap().contains("FROM events"));
                    respond(&mut stream, 200, rows(event.clone()));
                }
                4 => {
                    assert!(body["sql"].as_str().unwrap().contains("FROM tasks"));
                    respond(&mut stream, 200, rows(task_row.clone()));
                }
                _ => unreachable!(),
            }
        }
    });
    let store = store(&endpoint);
    let intent = format!("{}wire-operation", cloud_todo_operation::INTENT_PREFIX);
    let error = store
        .add_task(task(), "actor".into(), intent.clone())
        .await
        .unwrap_err();
    assert_eq!(
        cloud_todo_operation::anyhow_failure(&error),
        (Outcome::OutcomeUnknown, FailureKind::Transport)
    );
    let restored = store
        .add_task(task(), "actor".into(), intent)
        .await
        .unwrap();
    assert_eq!(restored.id, "todo_wire_resume");
    server.join().unwrap();
}

#[tokio::test]
async fn retry_token_does_not_bypass_authorized_receipt_reads() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        let (headers, body) = request(&mut stream);
        assert!(headers.starts_with("post /query "));
        assert!(body["sql"].as_str().unwrap().contains("FROM events"));
        respond(
            &mut stream,
            403,
            json!({"error":{"code":"repository_not_authorized","message":"private backend"}}),
        );
        // No mutation or secondary route is accepted by this fixture.
    });
    let error = store(&endpoint)
        .add_task(
            task(),
            "actor".into(),
            format!("{}wire-operation", cloud_todo_operation::INTENT_PREFIX),
        )
        .await
        .unwrap_err();
    assert_eq!(
        cloud_todo_operation::anyhow_failure(&error),
        (Outcome::BlockedBeforeSubmit, FailureKind::Authorization)
    );
    assert!(!error.to_string().contains("private backend"));
    server.join().unwrap();
}

#[test]
fn production_auth_failure_json_and_operation_inspection_do_not_mutate_protected_checkout() {
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::process::Command;
    fn snapshot(root: &Path, path: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                snapshot(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    std::fs::read(path).unwrap(),
                );
            }
        }
    }
    let root = tempfile::tempdir().unwrap();
    let machine = tempfile::tempdir().unwrap();
    for args in [
        vec!["init", "-q"],
        vec![
            "remote",
            "add",
            "origin",
            "https://github.com/example/repo.git",
        ],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(root.path())
                .status()
                .unwrap()
                .success()
        );
    }
    std::fs::create_dir_all(root.path().join(".decapod/data")).unwrap();
    std::fs::write(root.path().join(".decapod/config.toml"), "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n").unwrap();
    let mut before = BTreeMap::new();
    snapshot(root.path(), root.path(), &mut before);
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_decapod"))
            .arg("todo")
            .args(args)
            .args(["--format", "json"])
            .current_dir(root.path())
            .env("HOME", machine.path())
            .env("XDG_CONFIG_HOME", machine.path().join("config"))
            .env("XDG_DATA_HOME", machine.path().join("data"))
            .env("DECAPOD_HEADLESS", "1")
            .env("DECAPOD_AGENT_ID", "retry-proof")
            .env("DECAPOD_CLOUD_DATASTORE", "neon")
            .env("DECAPOD_PROPODUS_API_URL", "http://127.0.0.1:1")
            .env_remove("DECAPOD_ACCESS_TOKEN")
            .env_remove("DECAPOD_CLOUD_AUTH_MODE")
            .output()
            .unwrap()
    };
    let output = invoke(&["add", "private-title", "--description", "private-payload"]);
    assert!(!output.status.success());
    let diagnostic: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "invalid diagnostic: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(diagnostic["status"], "blocked_before_submit");
    assert!(diagnostic["operation_id"].is_string());
    for secret in ["private-title", "private-payload"] {
        assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
        assert!(!String::from_utf8_lossy(&output.stderr).contains(secret));
    }
    let operations = invoke(&["operations"]);
    assert!(
        operations.status.success(),
        "{}",
        String::from_utf8_lossy(&operations.stderr)
    );
    let records: Value = serde_json::from_slice(&operations.stdout).unwrap();
    assert_eq!(
        records["operations"][0]["operation_id"],
        diagnostic["operation_id"]
    );
    let mut after = BTreeMap::new();
    snapshot(root.path(), root.path(), &mut after);
    assert_eq!(before, after);
    assert!(!root.path().join(".decapod/data/decapod.db").exists());
}
