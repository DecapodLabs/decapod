use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

static TEST_REPO_ROOT: OnceLock<PathBuf> = OnceLock::new();
static SESSION_PASSWORD: OnceLock<String> = OnceLock::new();
static RPC_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static CLAIMED_TODO_ID: OnceLock<String> = OnceLock::new();

fn test_repo_root() -> &'static PathBuf {
    TEST_REPO_ROOT.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("decapod_rpc_suite_{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&dir).expect("create rpc suite repo dir");

        let git_init = Command::new("git")
            .current_dir(&dir)
            .args(["init", "-b", "master"])
            .output()
            .expect("git init");
        assert!(
            git_init.status.success(),
            "git init failed: {}",
            String::from_utf8_lossy(&git_init.stderr)
        );

        let init = Command::new(env!("CARGO_BIN_EXE_decapod"))
            .current_dir(&dir)
            .args(["init", "--force"])
            .output()
            .expect("decapod init");
        assert!(
            init.status.success(),
            "decapod init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );

        dir
    })
}

fn run_decapod(args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(test_repo_root()).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run decapod")
}

fn run_cmd_with_lock_retry<F>(mut run: F) -> std::process::Output
where
    F: FnMut() -> std::process::Output,
{
    let mut last = None;
    for attempt in 1..=2 {
        let out = run();
        if out.status.success() {
            return out;
        }
        let stderr = String::from_utf8_lossy(&out.stderr).to_ascii_lowercase();
        let transient = stderr.contains("database is locked")
            || stderr.contains("disk i/o error")
            || stderr.contains("validate_timeout_or_lock");
        last = Some(out);
        if transient && attempt < 2 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            continue;
        }
        break;
    }
    last.expect("retry output")
}

fn bootstrap_session() -> &'static str {
    SESSION_PASSWORD.get_or_init(|| {
        let agent_id = "unknown";

        let _ = Command::new("git")
            .current_dir(test_repo_root())
            .args(["checkout", "-b", "feat/test-rpc-suite"])
            .output();

        let session_out = run_decapod(
            &["session", "acquire"],
            &[
                ("DECAPOD_AGENT_ID", agent_id),
                ("DECAPOD_CLAIM_AUTORUN", "0"),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ],
        );
        let session_stdout = String::from_utf8_lossy(&session_out.stdout);
        let session_password = session_stdout
            .lines()
            .find_map(|line| {
                line.strip_prefix("Password: ")
                    .map(|v| v.trim().to_string())
            })
            .unwrap_or_else(|| "test".to_string());

        let validate_out = run_cmd_with_lock_retry(|| {
            run_decapod(
                &["validate"],
                &[
                    ("DECAPOD_AGENT_ID", agent_id),
                    ("DECAPOD_CLAIM_AUTORUN", "0"),
                    ("DECAPOD_SESSION_PASSWORD", &session_password),
                    ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
                    ("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1"),
                    ("DECAPOD_VALIDATE_TIMEOUT_SECONDS", "8"),
                ],
            )
        });
        assert!(
            validate_out.status.success(),
            "validate failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&validate_out.stdout),
            String::from_utf8_lossy(&validate_out.stderr)
        );

        let ingest_out = run_decapod(
            &["docs", "ingest"],
            &[
                ("DECAPOD_AGENT_ID", agent_id),
                ("DECAPOD_CLAIM_AUTORUN", "0"),
                ("DECAPOD_SESSION_PASSWORD", &session_password),
            ],
        );
        assert!(
            ingest_out.status.success(),
            "docs ingest failed: {}",
            String::from_utf8_lossy(&ingest_out.stderr)
        );

        session_password
    })
}

fn ensure_claimed_task(session_password: &str) -> &'static str {
    CLAIMED_TODO_ID.get_or_init(|| {
        let todo_add_out = run_cmd_with_lock_retry(|| {
            run_decapod(
                &["todo", "add", "ordering gate test task", "--format", "json"],
                &[
                    ("DECAPOD_AGENT_ID", "unknown"),
                    ("DECAPOD_CLAIM_AUTORUN", "0"),
                    ("DECAPOD_SESSION_PASSWORD", session_password),
                ],
            )
        });
        assert!(
            todo_add_out.status.success(),
            "todo add failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&todo_add_out.stdout),
            String::from_utf8_lossy(&todo_add_out.stderr)
        );
        let todo_add_json: serde_json::Value =
            serde_json::from_slice(&todo_add_out.stdout).expect("parse todo add");
        let todo_id = todo_add_json["id"].as_str().expect("todo id").to_string();

        let todo_claim_out = run_cmd_with_lock_retry(|| {
            run_decapod(
                &[
                    "todo", "claim", "--id", &todo_id, "--agent", "unknown", "--format", "json",
                ],
                &[
                    ("DECAPOD_AGENT_ID", "unknown"),
                    ("DECAPOD_CLAIM_AUTORUN", "0"),
                    ("DECAPOD_SESSION_PASSWORD", session_password),
                ],
            )
        });
        let todo_claim_json: serde_json::Value =
            serde_json::from_slice(&todo_claim_out.stdout).expect("parse todo claim");
        assert_eq!(todo_claim_json["status"], "ok");

        todo_id
    })
}

fn run_rpc(request: serde_json::Value) -> serde_json::Value {
    let session_password = bootstrap_session();
    let _todo_id = ensure_claimed_task(session_password);

    let lock = RPC_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    let run_rpc_once = |req: &serde_json::Value| -> serde_json::Value {
        for attempt in 1..=2 {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
            cmd.current_dir(test_repo_root())
                .args(["rpc", "--stdin"])
                .env("DECAPOD_AGENT_ID", "unknown")
                .env("DECAPOD_CLAIM_AUTORUN", "0")
                .env("DECAPOD_SESSION_PASSWORD", session_password)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let mut child = cmd.spawn().expect("spawn decapod rpc");
            let mut stdin = child.stdin.take().expect("open stdin");
            stdin
                .write_all(serde_json::to_string(req).unwrap().as_bytes())
                .expect("write stdin");
            drop(stdin);

            let output = {
                let started = std::time::Instant::now();
                let timeout = std::time::Duration::from_secs(8);
                loop {
                    if child.try_wait().expect("poll child").is_some() {
                        break child.wait_with_output().expect("read stdout");
                    }
                    if started.elapsed() >= timeout {
                        let _ = child.kill();
                        break child.wait_with_output().expect("read stdout after kill");
                    }
                    std::thread::sleep(std::time::Duration::from_millis(30));
                }
            };

            if output.status.success()
                && let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                    return json;
                }

            let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
            let transient = stderr.contains("database is locked")
                || stderr.contains("disk i/o error")
                || stderr.contains("validate_timeout_or_lock")
                || !output.status.success();
            if transient && attempt < 2 {
                std::thread::sleep(std::time::Duration::from_millis(200));
                continue;
            }

            panic!(
                "rpc call failed:\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        unreachable!("retry loop should return or panic")
    };

    let init_res = run_rpc_once(&serde_json::json!({ "op": "agent.init", "params": {} }));
    assert!(
        init_res["success"].as_bool().unwrap(),
        "agent.init failed: {}",
        init_res
    );

    let ctx_res = run_rpc_once(&serde_json::json!({ "op": "context.resolve", "params": {} }));
    assert!(
        ctx_res["success"].as_bool().unwrap(),
        "context.resolve failed: {}",
        ctx_res
    );

    let response = run_rpc_once(&request);
    if response["success"] == false {
        eprintln!(
            "RPC Error: {}",
            serde_json::to_string_pretty(&response).unwrap()
        );
    }
    response
}

#[test]
fn test_rpc_context_resolve_determinism() {
    let request = serde_json::json!({
        "op": "context.resolve",
        "params": {
            "op": "workspace.ensure",
            "touched_paths": ["src/core/rpc.rs"],
            "intent_tags": ["security"],
            "limit": 5
        }
    });

    let res1 = run_rpc(request.clone());
    let res2 = run_rpc(request.clone());

    assert_eq!(res1["result"], res2["result"]);
    assert!(res1["success"].as_bool().unwrap());

    let fragments = res1["result"]["fragments"].as_array().unwrap();
    assert!(!fragments.is_empty());
}

#[test]
fn test_rpc_schema_get() {
    let request = serde_json::json!({
        "op": "schema.get",
        "params": {
            "entity": "todo"
        }
    });

    let res = run_rpc(request);
    assert!(res["success"].as_bool().unwrap());
    assert_eq!(res["result"]["schema_version"], "v1");
}

#[test]
fn test_rpc_store_upsert_knowledge() {
    let id = format!("K_TEST_{}", ulid::Ulid::new());
    let request = serde_json::json!({
        "op": "store.upsert",
        "params": {
            "entity": "knowledge",
            "payload": {
                "id": id,
                "title": "RPC Test Knowledge",
                "text": "This is a test entry from RPC",
                "provenance": "cmd:cargo-test"
            }
        }
    });

    let res = run_rpc(request);
    assert!(res["success"].as_bool().unwrap());
    assert_eq!(res["result"]["stored"], true);
    assert_eq!(res["result"]["id"], id);
}

#[test]
fn test_rpc_context_bindings() {
    let request = serde_json::json!({
        "op": "context.bindings",
        "params": {}
    });

    let res = run_rpc(request);
    assert!(res["success"].as_bool().unwrap());
    assert!(res["result"]["ops"].get("workspace.ensure").is_some());
}

#[test]
fn test_rpc_trace_and_redaction() {
    let secret_id = format!("SECRET_{}", ulid::Ulid::new());
    let request = serde_json::json!({
        "op": "schema.get",
        "params": {
            "entity": "todo",
            "my_password": "supersecretpassword",
            "id": secret_id
        }
    });

    let _res = run_rpc(request);

    let lock = RPC_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    let output = run_cmd_with_lock_retry(|| {
        run_decapod(
            &["trace", "export", "--last", "50"],
            &[
                ("DECAPOD_SESSION_PASSWORD", "test"),
                ("DECAPOD_AGENT_ID", "test"),
                ("DECAPOD_CLAIM_AUTORUN", "0"),
            ],
        )
    });

    assert!(
        output.status.success(),
        "trace export failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let trace_line = String::from_utf8_lossy(&output.stdout);
    assert!(trace_line.contains(&secret_id));
    assert!(trace_line.contains("[REDACTED]"));
    assert!(!trace_line.contains("supersecretpassword"));
}
