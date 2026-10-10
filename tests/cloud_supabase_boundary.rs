//! Local HTTP fixtures. These prove client composition, never PostgreSQL or
//! service authorization. The real service profile lives in supabase_service.rs.
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

fn project() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec!["init", "-q"],
        vec![
            "remote",
            "add",
            "origin",
            "https://github.com/example/supabase-proof.git",
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
    std::fs::write(root.path().join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n"
    ).unwrap();
    root
}

fn command(root: &Path, selector: &str, endpoint: &str, token: Option<&str>) -> Output {
    command_with_args(root, selector, endpoint, token, &["list"])
}

fn command_with_args(
    root: &Path,
    selector: &str,
    endpoint: &str,
    token: Option<&str>,
    args: &[&str],
) -> Output {
    command_with_optional_datastore(root, Some(selector), endpoint, token, args)
}

fn command_with_optional_datastore(
    root: &Path,
    selector: Option<&str>,
    endpoint: &str,
    token: Option<&str>,
    args: &[&str],
) -> Output {
    let mut command = Command::new(
        std::fs::canonicalize(env!("CARGO_BIN_EXE_decapod"))
            .expect("resolve decapod binary before changing working directory"),
    );
    command
        .arg("todo")
        .args(args)
        .args(["--format", "json"])
        .current_dir(root)
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("DECAPOD_HEADLESS", "1")
        .env("DECAPOD_AGENT_ID", "supabase-proof")
        .env_remove("DECAPOD_CLOUD_AUTH_MODE")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .env_remove("DECAPOD_CLOUD_DATASTORE")
        .env("DECAPOD_PROPODUS_API_URL", endpoint)
        .env_remove("DECAPOD_ACCESS_TOKEN");
    if let Some(selector) = selector {
        command.env("DECAPOD_CLOUD_DATASTORE", selector);
    }
    if let Some(token) = token {
        command.env("DECAPOD_ACCESS_TOKEN", token);
    }
    command.output().unwrap()
}

fn assert_no_fallback(root: &Path) {
    assert!(!root.join(".decapod/data/decapod.db").exists());
    assert!(!root.join(".decapod/data/todo.db").exists());
}

#[test]
fn unsupported_selector_fails_through_production_cli_without_sqlite() {
    let root = project();
    let output = command(
        root.path(),
        "unsupported-secret-selector",
        "http://127.0.0.1:1",
        Some("synthetic-bearer-secret"),
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported cloud datastore"), "{stderr}");
    assert!(!stderr.contains("unsupported-secret-selector"));
    assert!(!stderr.contains("synthetic-bearer-secret"));
    assert_no_fallback(root.path());
}

#[cfg(not(feature = "supabase-cloud"))]
#[test]
fn feature_disabled_supabase_fails_before_remote_or_local_io() {
    let root = project();
    let output = command(
        root.path(),
        "supabase",
        "http://127.0.0.1:1",
        Some("synthetic-bearer-secret"),
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("supabase-cloud"));
    assert_no_fallback(root.path());
}

#[test]
fn cloud_runtime_deserialization_uses_its_selected_datastore() {
    const CHILD: &str = "DECAPOD_TEST_CLOUD_CONFIG_CHILD";
    if std::env::var_os(CHILD).is_some() {
        use decapod::CloudRuntimeConfig;
        const PROPODUS_VERCEL_NEON_ENTRYPOINT: &str = "https://project-oqn7i.vercel.app";
        let neon: CloudRuntimeConfig = serde_json::from_str(r#"{"datastore":"neon"}"#).unwrap();
        assert_eq!(neon.datastore, "neon");
        assert_eq!(neon.api_url, PROPODUS_VERCEL_NEON_ENTRYPOINT);
        assert!(neon.validate_datastore().is_ok());
        let supabase: CloudRuntimeConfig =
            serde_json::from_str(r#"{"datastore":"supabase"}"#).unwrap();
        assert_eq!(supabase.datastore, "supabase");
        assert!(supabase.api_url.is_empty());
        assert!(supabase.validate_datastore().is_err());
        let implicit: CloudRuntimeConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(implicit, CloudRuntimeConfig::default());
        assert_eq!(
            implicit.datastore,
            std::env::var("DECAPOD_CLOUD_DATASTORE").unwrap_or_else(|_| "supabase".into())
        );
        return;
    }
    for ambient in [None, Some("neon"), Some("supabase")] {
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "cloud_runtime_deserialization_uses_its_selected_datastore",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env_remove("DECAPOD_PROPODUS_API_URL")
            .env_remove("DECAPOD_CLOUD_DATASTORE");
        if let Some(value) = ambient {
            child.env("DECAPOD_CLOUD_DATASTORE", value);
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn default_cloud_requires_service_endpoint_without_local_fallback() {
    let root = project();
    let output = command_with_optional_datastore(
        root.path(),
        None,
        "",
        Some("synthetic-bearer-secret"),
        &["list"],
    );
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    #[cfg(feature = "supabase-cloud")]
    assert!(
        error.contains("explicit authenticated service endpoint"),
        "{error}"
    );
    #[cfg(not(feature = "supabase-cloud"))]
    assert!(error.contains("supabase-cloud"), "{error}");
    assert!(!error.contains("synthetic-bearer-secret"));
    assert_no_fallback(root.path());
}

#[cfg(feature = "supabase-cloud")]
mod enabled {
    use super::*;
    use decapod::core::backend::{BackendRoute, CloudDatastore, StorageContext};
    use decapod::core::dactyl::DactylBridge;
    use decapod::core::repo_identity::resolve_repository_identity_from_remote;
    use serde_json::{Value, json};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::JoinHandle;
    use std::time::Duration;

    fn server(status: u16, body: Value) -> (String, JoinHandle<Value>) {
        request_server("/query", status, move |_| body)
    }

    fn request_server(
        path: &'static str,
        status: u16,
        response: impl FnOnce(&Value) -> Value + Send + 'static,
    ) -> (String, JoinHandle<Value>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(stream) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(error) => panic!("fixture received no request within its budget: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let body_start = loop {
                let mut buffer = [0; 4096];
                let read = stream.read(&mut buffer).unwrap();
                assert_ne!(read, 0, "request ended before headers");
                bytes.extend_from_slice(&buffer[..read]);
                if let Some(offset) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
                    break offset + 4;
                }
            };
            let headers = String::from_utf8_lossy(&bytes[..body_start]).to_lowercase();
            assert!(headers.starts_with(&format!("post {path} ")));
            assert!(headers.contains("authorization: bearer synthetic-bearer-secret"));
            let length = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .unwrap()
                .trim()
                .parse::<usize>()
                .unwrap();
            while bytes.len() < body_start + length {
                let mut buffer = [0; 4096];
                let read = stream.read(&mut buffer).unwrap();
                assert_ne!(read, 0);
                bytes.extend_from_slice(&buffer[..read]);
            }
            let request = serde_json::from_slice(&bytes[body_start..body_start + length]).unwrap();
            let body = response(&request).to_string();
            write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            request
        });
        (endpoint, handle)
    }

    fn task_row(id: &str, status: &str, version: i32) -> Value {
        json!({
            "repo_id": "example/supabase-proof", "id": id, "hash": "proof1",
            "title": "Supabase command fixture", "description": null,
            "status": status,
            "assignee": if status == "open" { "" } else { "supabase-proof" },
            "scope": "root",
            "dir_path": "", "priority": "medium", "category": "",
            "tags": "cloud, supabase", "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:01Z", "version": version
        })
    }

    fn task_rows(row: Value) -> Value {
        let columns: Vec<_> = row.as_object().unwrap().keys().cloned().collect();
        json!({"columns": columns, "rows": [row], "affected_rows": 0})
    }

    fn batch_result(row: Value, task_count: u64, event_count: u64) -> Value {
        json!({"results": [
            {"columns": [], "rows": [], "affected_rows": task_count},
            {"columns": [], "rows": [], "affected_rows": event_count},
            task_rows(row)
        ]})
    }

    fn assert_context(request: &Value) {
        assert_eq!(request["context"]["version"], 1);
        assert_eq!(
            request["context"]["payload"]["route"]["Cloud"]["repository"]["canonical_name"],
            "example/supabase-proof"
        );
        assert!(!request.to_string().contains("synthetic-bearer-secret"));
        assert!(
            request["context"]["payload"]
                .get("cloud_datastore")
                .is_none()
        );
    }

    fn assert_safe_failure(output: &Output, root: &Path) {
        assert!(!output.status.success());
        let diagnostic = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!diagnostic.contains("synthetic-bearer-secret"));
        assert!(!diagnostic.contains("private database details"));
        assert!(!diagnostic.contains("Supabase command fixture"));
        assert_no_fallback(root);
    }

    #[test]
    fn cloud_status_uses_the_selected_endpoint_session_without_network_io() {
        use sha2::{Digest, Sha256};
        const CHILD: &str = "DECAPOD_TEST_CLOUD_STATUS_CHILD";
        if let Ok(expected) = std::env::var(CHILD) {
            assert_eq!(
                decapod::core::auth::is_token_valid(Path::new(".")),
                expected == "configured"
            );
            return;
        }
        let root = project();
        let data = root.path().join("data/decapod");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(
            data.join("session_token.json"),
            r#"{"token":"synthetic-neon-status-token"}"#,
        )
        .unwrap();
        let endpoint = "https://selected.example.test";
        let status = |selector: Option<&str>, endpoint: &str| {
            let mut command =
                Command::new(std::fs::canonicalize(env!("CARGO_BIN_EXE_decapod")).unwrap());
            command
                .args(["cloud", "status"])
                .current_dir(root.path())
                .env("HOME", root.path())
                .env("XDG_DATA_HOME", root.path().join("data"))
                .env("XDG_CONFIG_HOME", root.path().join("config"))
                .env_remove("DECAPOD_ACCESS_TOKEN")
                .env_remove("DECAPOD_CLOUD_DATASTORE")
                .env("DECAPOD_PROPODUS_API_URL", endpoint);
            if let Some(selector) = selector {
                command.env("DECAPOD_CLOUD_DATASTORE", selector);
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let text = String::from_utf8(output.stdout).unwrap();
            let mut gate = Command::new(std::env::current_exe().unwrap());
            gate.args([
                "--exact",
                "enabled::cloud_status_uses_the_selected_endpoint_session_without_network_io",
                "--nocapture",
            ])
            .current_dir(root.path())
            .env(
                CHILD,
                if text.contains("bearer configured") {
                    "configured"
                } else {
                    "unavailable"
                },
            )
            .env("HOME", root.path())
            .env("XDG_DATA_HOME", root.path().join("data"))
            .env_remove("DECAPOD_ACCESS_TOKEN")
            .env_remove("DECAPOD_CLOUD_DATASTORE")
            .env("DECAPOD_PROPODUS_API_URL", endpoint);
            if let Some(selector) = selector {
                gate.env("DECAPOD_CLOUD_DATASTORE", selector);
            }
            let gate_output = gate.output().unwrap();
            assert!(
                gate_output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&gate_output.stdout),
                String::from_utf8_lossy(&gate_output.stderr)
            );
            assert!(!text.contains("synthetic-") && !text.contains("selected.example.test"));
            text
        };
        assert!(
            status(None, endpoint).contains("unavailable"),
            "Supabase must ignore the legacy Neon session"
        );
        let scoped = data
            .join("services")
            .join(format!("{:x}", Sha256::digest(endpoint.as_bytes())));
        std::fs::create_dir_all(&scoped).unwrap();
        std::fs::write(
            scoped.join("session_token.json"),
            r#"{"token":"synthetic-supabase-status-token"}"#,
        )
        .unwrap();
        assert!(status(None, endpoint).contains("bearer configured"));
        assert!(status(None, "https://other.example.test").contains("unavailable"));
        assert!(status(Some("neon"), "").contains("bearer configured"));
        assert_no_fallback(root.path());
    }

    #[test]
    fn backend_cloud_without_datastore_override_uses_supabase() {
        let root = project();
        let (endpoint, server) =
            server(200, json!({"columns": [], "rows": [], "affected_rows": 0}));
        let output = command_with_optional_datastore(
            root.path(),
            None,
            &endpoint,
            Some("synthetic-bearer-secret"),
            &["list"],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let request = server.join().unwrap();
        assert_context(&request);
        assert_no_fallback(root.path());
    }

    #[test]
    fn production_cli_mutations_send_one_ordered_task_event_observation_batch() {
        for (operation, event, status, version) in [
            ("add", "task.add", "open", 1),
            ("claim", "task.claim", "in_progress", 2),
            ("release", "task.release", "open", 3),
            ("done", "task.done", "completed", 5),
        ] {
            let root = project();
            let (endpoint, server) = request_server("/batch", 200, move |request| {
                assert_context(request);
                assert_eq!(request["access_mode"], "read_write");
                let operations = request["operations"].as_array().unwrap();
                assert_eq!(operations.len(), 3);
                assert_eq!(operations[0]["kind"], "write");
                assert_eq!(operations[1]["kind"], "write");
                assert_eq!(operations[2]["kind"], "read");
                let expected_sql = match operation {
                    "add" => {
                        "INSERT INTO tasks (repo_id, id, hash, title, description, tags, owner, status, dir_path, scope, priority, category, assigned_to, created_at, updated_at, version) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $14, 1)"
                    }
                    "claim" => {
                        "UPDATE tasks SET status = 'in_progress', assigned_to = $1, assigned_at = $2, updated_at = $2, version = COALESCE(version, 1) + 1 WHERE id = $3 AND status IN ('open', 'pending') AND (assigned_to = '' OR assigned_to IS NULL)"
                    }
                    "release" => {
                        "UPDATE tasks SET status = 'open', assigned_to = '', assigned_at = NULL, updated_at = $1, version = COALESCE(version, 1) + 1 WHERE id = $2 AND status = 'in_progress' AND assigned_to = $3"
                    }
                    "done" => {
                        "UPDATE tasks SET status = 'completed', completed_at = $1, updated_at = $1, version = COALESCE(version, 1) + 1 WHERE id = $2 AND status = 'in_progress' AND assigned_to = $3"
                    }
                    _ => unreachable!(),
                };
                assert_eq!(operations[0]["sql"], expected_sql);
                let event_sql = operations[1]["sql"]
                    .as_str()
                    .unwrap()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                assert_eq!(
                    event_sql,
                    "WITH event_params(event_id, event_ts, task_id, event_type, event_payload, event_actor) AS ( VALUES ($1, $2, $3, $4, $5, $6) ) INSERT INTO events (event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor) SELECT event_params.event_id, event_params.event_ts, COALESCE((SELECT MAX(seq) FROM events WHERE stream = 'todo'), 0) + 1, 'todo', 'task', event_params.task_id, event_params.event_type, event_params.event_payload, event_params.event_actor FROM event_params WHERE EXISTS ( SELECT 1 FROM tasks WHERE tasks.id = event_params.task_id AND tasks.updated_at = event_params.event_ts )"
                );
                assert_eq!(
                    operations[2]["sql"],
                    "SELECT repo_id, id, hash, title, description, status, assigned_to AS assignee, scope, dir_path, priority, category, tags, created_at, updated_at, version FROM tasks WHERE id = $1"
                );
                let id = operations[2]["params"][0].as_str().unwrap();
                assert!(!id.is_empty());
                assert_eq!(operations[2]["params"], json!([id]));
                let event_params = operations[1]["params"].as_array().unwrap();
                assert_eq!(event_params.len(), 6);
                assert!(!event_params[0].as_str().unwrap().is_empty());
                assert_eq!(event_params[2], id);
                assert_eq!(event_params[3], event);
                assert_eq!(event_params[5], "supabase-proof");
                let timestamp = &event_params[1];
                chrono::DateTime::parse_from_rfc3339(timestamp.as_str().unwrap()).unwrap();
                let payload: Value =
                    serde_json::from_str(event_params[4].as_str().unwrap()).unwrap();
                let params = operations[0]["params"].as_array().unwrap();
                match operation {
                    "add" => {
                        assert_eq!(params.len(), 14);
                        assert!(id.starts_with("todo_"));
                        assert_eq!(params[0], "example/supabase-proof");
                        assert_eq!(params[1], id);
                        assert_eq!(params[3], "Supabase command fixture");
                        assert_eq!(params[7], "open");
                        assert_eq!(&params[13], timestamp);
                        assert_eq!(
                            payload,
                            json!({"title": "Supabase command fixture", "status": "open"})
                        );
                    }
                    "claim" => {
                        assert_eq!(id, "todo_proof1");
                        assert_eq!(
                            operations[0]["params"],
                            json!(["supabase-proof", timestamp, id])
                        );
                        assert_eq!(payload, json!({"assigned_to": "supabase-proof"}));
                    }
                    "release" | "done" => {
                        assert_eq!(id, "todo_proof1");
                        assert_eq!(
                            operations[0]["params"],
                            json!([timestamp, id, "supabase-proof"])
                        );
                        let expected_payload = if operation == "release" {
                            json!({"released_by": "supabase-proof"})
                        } else {
                            json!({"resolution": ""})
                        };
                        assert_eq!(payload, expected_payload);
                    }
                    _ => unreachable!(),
                }
                batch_result(task_row(id, status, version), 1, 1)
            });
            let args = if operation == "add" {
                vec!["add", "Supabase command fixture"]
            } else {
                vec![operation, "--id", "todo_proof1"]
            };
            let output = command_with_args(
                root.path(),
                "supabase",
                &endpoint,
                Some("synthetic-bearer-secret"),
                &args,
            );
            let request = server.join().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let result: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(result["cmd"], format!("todo.{operation}"));
            assert_eq!(result["item"]["id"], request["operations"][2]["params"][0]);
            assert_eq!(result["item"]["status"], status);
            assert_eq!(
                result["item"]["assignee"],
                if status == "open" {
                    ""
                } else {
                    "supabase-proof"
                }
            );
            assert_eq!(result["item"]["version"], version);
            assert!(result["item"]["description"].is_null());
            assert_eq!(result["item"]["tags"], json!(["cloud", "supabase"]));
            assert_no_fallback(root.path());
        }
    }

    #[test]
    fn production_cli_keyed_reads_preserve_rows_and_missing_results() {
        for args in [
            vec!["get", "--id", "todo_proof1"],
            vec!["show", "--id", "todo_proof1"],
            vec!["show", "todo_proof1"],
        ] {
            for present in [true, false] {
                let root = project();
                let row = task_row("todo_proof1", "open", 1);
                let body = if present {
                    task_rows(row)
                } else {
                    json!({"columns": [], "rows": [], "affected_rows": 0})
                };
                let (endpoint, server) = server(200, body);
                let output = command_with_args(
                    root.path(),
                    "supabase",
                    &endpoint,
                    Some("synthetic-bearer-secret"),
                    &args,
                );
                let request = server.join().unwrap();
                assert_context(&request);
                assert_eq!(request["kind"], "read");
                assert_eq!(request["access_mode"], "read_only");
                assert_eq!(request["params"], json!(["todo_proof1"]));
                assert!(request["sql"].as_str().unwrap().contains("WHERE id = $1"));
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let result: Value = serde_json::from_slice(&output.stdout).unwrap();
                if present {
                    assert_eq!(result["status"], "ok");
                    assert_eq!(result["item"]["id"], "todo_proof1");
                    assert_eq!(result["item"]["version"], 1);
                } else {
                    assert_eq!(result["status"], "not_found");
                    assert!(result["item"].is_null());
                }
                assert_no_fallback(root.path());
            }
        }
    }

    #[test]
    fn production_cli_rejects_zero_or_non_singleton_state_and_event_counts() {
        for operation in ["add", "claim", "release", "done"] {
            for (task_count, event_count) in [(0, 0), (2, 1), (1, 0), (1, 2)] {
                let root = project();
                let (endpoint, server) = request_server("/batch", 200, move |_| {
                    batch_result(task_row("todo_proof1", "open", 1), task_count, event_count)
                });
                let args = if operation == "add" {
                    vec!["add", "Supabase command fixture"]
                } else {
                    vec![operation, "--id", "todo_proof1"]
                };
                let output = command_with_args(
                    root.path(),
                    "supabase",
                    &endpoint,
                    Some("synthetic-bearer-secret"),
                    &args,
                );
                server.join().unwrap();
                assert_safe_failure(&output, root.path());
                assert!(String::from_utf8_lossy(&output.stderr).contains("expected exactly one"));
            }
        }
    }

    #[test]
    fn production_cli_mutation_service_failures_are_safe_without_local_fallback() {
        for operation in ["add", "claim", "release", "done"] {
            for (status, code) in [
                (401, "session_expired"),
                (403, "permission_denied"),
                (500, "storage_failure"),
                (504, "timeout"),
            ] {
                let root = project();
                let (endpoint, server) = request_server(
                    "/batch",
                    status,
                    move |_| json!({"error": {"code": code, "message": "synthetic-bearer-secret private database details"}}),
                );
                let args = if operation == "add" {
                    vec!["add", "Supabase command fixture"]
                } else {
                    vec![operation, "--id", "todo_proof1"]
                };
                let output = command_with_args(
                    root.path(),
                    "supabase",
                    &endpoint,
                    Some("synthetic-bearer-secret"),
                    &args,
                );
                server.join().unwrap();
                assert_safe_failure(&output, root.path());
            }
        }
    }

    #[test]
    fn production_cli_validated_completion_never_sends_a_mutation() {
        let root = project();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let output = command_with_args(
            root.path(),
            "supabase",
            &endpoint,
            Some("synthetic-bearer-secret"),
            &["done", "--id", "todo_proof1", "--validated"],
        );
        assert_safe_failure(&output, root.path());
        assert!(String::from_utf8_lossy(&output.stderr).contains("proof-capture"));
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn production_cli_sends_context_and_bearer_and_normalizes_empty_rows() {
        let root = project();
        let (endpoint, server) =
            server(200, json!({"columns": [], "rows": [], "affected_rows": 0}));
        let output = command(
            root.path(),
            "supabase",
            &endpoint,
            Some("synthetic-bearer-secret"),
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let request = server.join().unwrap();
        assert_eq!(request["kind"], "read");
        assert_eq!(request["access_mode"], "read_only");
        assert_eq!(request["context"]["version"], 1);
        assert_eq!(
            request["context"]["payload"]["route"]["Cloud"]["repository"]["canonical_name"],
            "example/supabase-proof"
        );
        assert!(!request.to_string().contains("synthetic-bearer-secret"));
        assert!(
            request["context"]["payload"]
                .get("cloud_datastore")
                .is_none()
        );
        assert_no_fallback(root.path());
    }

    #[test]
    fn rejected_expired_denied_storage_and_timeout_responses_are_safe() {
        for (status, code) in [
            (401, "authentication_required"),
            (401, "session_expired"),
            (403, "permission_denied"),
            (500, "storage_failure"),
            (504, "timeout"),
        ] {
            let root = project();
            let (endpoint, server) = server(
                status,
                json!({"error": {"code": code, "message": "synthetic-bearer-secret private database details"}}),
            );
            let output = command(
                root.path(),
                "supabase",
                &endpoint,
                Some("synthetic-bearer-secret"),
            );
            assert!(!output.status.success(), "status {status} must fail");
            let diagnostics = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(!diagnostics.contains("synthetic-bearer-secret"));
            assert!(!diagnostics.contains("private database details"));
            server.join().unwrap();
            assert_no_fallback(root.path());
        }
    }

    #[test]
    fn missing_bearer_and_unreachable_service_never_fall_back() {
        let root = project();
        let missing = command(root.path(), "supabase", "http://127.0.0.1:1", None);
        assert!(!missing.status.success());
        assert_no_fallback(root.path());
        let unreachable = command(
            root.path(),
            "supabase",
            "http://127.0.0.1:1",
            Some("synthetic-bearer-secret"),
        );
        assert!(!unreachable.status.success());
        assert!(!String::from_utf8_lossy(&unreachable.stderr).contains("synthetic-bearer-secret"));
        assert_no_fallback(root.path());
    }

    #[test]
    fn supabase_never_sends_legacy_machine_credentials_to_the_preview_endpoint() {
        let root = project();
        let credential_dir = root.path().join("data/decapod");
        std::fs::create_dir_all(&credential_dir).unwrap();
        let legacy = json!({"access_token": "synthetic-old-access", "refresh_token": "synthetic-old-refresh", "session_id": "synthetic-old-session", "expires_at": "2000-01-01T00:00:00Z"}).to_string();
        std::fs::write(credential_dir.join("session_token.json"), &legacy).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(stream) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(error) => panic!("onboarding fixture received no request: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut buffer = [0; 4096];
                let count = stream.read(&mut buffer).unwrap();
                assert_ne!(count, 0);
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(start) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..start]).to_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .unwrap()
                        .trim()
                        .parse()
                        .unwrap();
                    if bytes.len() >= start + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            assert!(
                !request.contains("synthetic-old-"),
                "legacy credential left machine scope"
            );
            assert!(!request.starts_with("POST /query"));
            assert!(!request.lines().next().unwrap().contains("refresh"));
            assert!(!request.to_lowercase().contains("authorization: bearer"));
            let body = r#"{"error":{"code":"invalid_token","message":"fixture denial"}}"#;
            write!(stream, "HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        });
        let output = command(root.path(), "supabase", &endpoint, None);
        assert!(!output.status.success());
        server.join().unwrap();
        assert_eq!(
            std::fs::read_to_string(credential_dir.join("session_token.json")).unwrap(),
            legacy
        );
        assert!(!String::from_utf8_lossy(&output.stderr).contains("synthetic-old-"));
        assert_no_fallback(root.path());
    }

    #[test]
    fn stalled_service_has_a_bounded_timeout_without_local_fallback() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(stream) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(error) => panic!("timeout fixture received no request: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).unwrap() > 0);
            std::thread::sleep(Duration::from_secs(17));
        });
        let root = project();
        let start = std::time::Instant::now();
        let output = command(
            root.path(),
            "supabase",
            &endpoint,
            Some("synthetic-bearer-secret"),
        );
        assert!(!output.status.success());
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "request was not bounded"
        );
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(diagnostic.contains("transport failed"), "{diagnostic}");
        assert!(!diagnostic.contains("synthetic-bearer-secret"));
        assert_no_fallback(root.path());
        server.join().unwrap();
    }

    #[test]
    fn explicit_supabase_route_preserves_typed_integer_boolean_and_null_results() {
        let (endpoint, server) = server(
            200,
            json!({"columns": ["id", "version", "active", "optional"], "rows": [{"id": "explicit-id", "version": 7, "active": true, "optional": null}], "affected_rows": 0}),
        );
        let identity = resolve_repository_identity_from_remote(
            "https://github.com/example/supabase-proof.git",
        )
        .unwrap();
        let context = StorageContext::from_route(
            BackendRoute::cloud(identity, endpoint).unwrap(),
            Some("synthetic-bearer-secret"),
        )
        .unwrap()
        .with_cloud_datastore(CloudDatastore::Supabase)
        .unwrap();
        let bridge =
            DactylBridge::from_storage_context(&context, dactyl_db::AccessMode::ReadOnly).unwrap();
        let rows = bridge
            .read("SELECT id, version, active, optional FROM fixture", &[])
            .unwrap();
        assert_eq!(rows.as_slice()[0].get_str("id").unwrap(), "explicit-id");
        assert_eq!(rows.as_slice()[0].get_int("version").unwrap(), 7);
        assert!(rows.as_slice()[0].get::<_, bool>("active").unwrap());
        assert!(
            rows.as_slice()[0]
                .get::<_, Option<String>>("optional")
                .unwrap()
                .is_none()
        );
        server.join().unwrap();
    }
}

#[test]
fn supabase_auth_transport_does_not_forward_refresh_body_or_bearer_on_redirect() {
    use decapod::core::backend::CloudDatastore;
    use decapod::core::propodus::{CurlTransport, PropodusTransport};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;
    assert!(CurlTransport::for_datastore(CloudDatastore::Neon).follow_redirects);
    for status in [307, 308] {
        let destination = TcpListener::bind("127.0.0.1:0").unwrap();
        destination.set_nonblocking(true).unwrap();
        let origin = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/session/refresh", origin.local_addr().unwrap());
        let redirect = format!("http://{}/capture", destination.local_addr().unwrap());
        let origin = std::thread::spawn(move || {
            let (mut stream, _) = origin.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut buffer = [0; 4096];
                let count = stream.read(&mut buffer).unwrap();
                assert_ne!(count, 0);
                bytes.extend_from_slice(&buffer[..count]);
                if String::from_utf8_lossy(&bytes).contains("synthetic-refresh-secret") {
                    break;
                }
            }
            write!(stream, "HTTP/1.1 {status} Redirect\r\nLocation: {redirect}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let response = CurlTransport::for_datastore(CloudDatastore::Supabase)
            .request(
                "POST",
                &endpoint,
                "synthetic-user-bearer",
                Some(br#"{"refresh_token":"synthetic-refresh-secret"}"#),
            )
            .unwrap();
        assert_eq!(response.status, status);
        origin.join().unwrap();
        assert_eq!(
            destination.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock,
            "redirect destination received credentials"
        );
    }
}
