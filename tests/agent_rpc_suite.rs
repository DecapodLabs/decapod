use decapod::core::ulid::new_ulid;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

static TEST_REPO_ROOT: OnceLock<PathBuf> = OnceLock::new();
static SESSION_PASSWORD: OnceLock<String> = OnceLock::new();
static RPC_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static CLAIMED_TODO_ID: OnceLock<String> = OnceLock::new();

fn resolve_decapod_bin() -> std::path::PathBuf {
    let cargo_bin = env!("CARGO_BIN_EXE_decapod");
    if let Ok(p) = std::path::Path::new(cargo_bin).canonicalize() {
        return p;
    }
    if let Ok(runfiles_dir) = std::env::var("RUNFILES_DIR") {
        let p = std::path::Path::new(&runfiles_dir)
            .join("_main")
            .join("decapod");
        if p.exists() {
            return p;
        }
    }
    if let Some(parent) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
    {
        let p = parent.join("decapod");
        if p.exists() {
            return p;
        }
    }
    std::path::PathBuf::from(cargo_bin)
}

fn test_repo_root() -> &'static PathBuf {
    TEST_REPO_ROOT.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("decapod_rpc_suite_{}", new_ulid()));
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

        let exe = resolve_decapod_bin();
        let init = Command::new(exe)
            .current_dir(&dir)
            .args(["init", "--force", "--product-summary", "Record Service exposes structured local record operations for client applications."])
            .output()
            .expect("decapod init");
        assert!(
            init.status.success(),
            "decapod init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );

        // The RPC fixture later works on an unaccepted feature checkout.
        // Establish independently accepted policy first, as real workspaces
        // require; its candidate config must not authorize its own operations.
        for args in [
            vec!["add", "."],
            vec!["-c", "user.name=RPC fixture", "-c", "user.email=fixture@example.invalid",
                "commit", "-m", "fixture: accept initial repository policy"],
        ] {
            let accepted = Command::new("git").current_dir(&dir).args(args).output().unwrap();
            assert!(accepted.status.success(), "{}", String::from_utf8_lossy(&accepted.stderr));
        }

        dir
    })
}

fn run_decapod(args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let exe = resolve_decapod_bin();
    let mut cmd = Command::new(exe);
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
    // Bootstrap only the project prerequisites needed by unrelated RPC tests.
    // This prerequisite result is not full publication-readiness proof.

    SESSION_PASSWORD.get_or_init(|| {
        let agent_id = "rpc-suite-agent";

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
                &["govern", "project-check"],
                &[
                    ("DECAPOD_AGENT_ID", agent_id),
                    ("DECAPOD_CLAIM_AUTORUN", "0"),
                    ("DECAPOD_SESSION_PASSWORD", &session_password),
                    ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
                    ("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1"),
                    ("DECAPOD_VALIDATE_TIMEOUT_SECONDS", "30"),
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
            &[
                "rpc",
                "--op",
                "constitution.get",
                "--params",
                r#"{"section":"core/DECAPOD"}"#,
            ],
            &[
                ("DECAPOD_AGENT_ID", agent_id),
                ("DECAPOD_CLAIM_AUTORUN", "0"),
                ("DECAPOD_SESSION_PASSWORD", &session_password),
            ],
        );
        assert!(
            ingest_out.status.success(),
            "constitution.get failed: {}",
            String::from_utf8_lossy(&ingest_out.stderr)
        );

        session_password
    })
}

fn ensure_claimed_task(session_password: &str) -> &'static str {
    CLAIMED_TODO_ID.get_or_init(|| {
        let agent_id = "rpc-suite-agent";
        let todo_add_out = run_cmd_with_lock_retry(|| {
            run_decapod(
                &["todo", "add", "ordering gate test task", "--format", "json"],
                &[
                    ("DECAPOD_AGENT_ID", agent_id),
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
                    "todo", "claim", "--id", &todo_id, "--agent", agent_id, "--format", "json",
                ],
                &[
                    ("DECAPOD_AGENT_ID", agent_id),
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
            let exe = resolve_decapod_bin();
            let mut cmd = Command::new(exe);
            cmd.current_dir(test_repo_root())
                .args(["rpc", "--stdin"])
                .env("DECAPOD_AGENT_ID", "rpc-suite-agent")
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
                && let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout)
            {
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
        "agent.init failed: {init_res}"
    );

    let ctx_res = run_rpc_once(&serde_json::json!({ "op": "context.resolve", "params": {} }));
    assert!(
        ctx_res["success"].as_bool().unwrap(),
        "context.resolve failed: {ctx_res}"
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
            "touched_paths": ["src/decapod/core/rpc.rs"],
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
    let id = format!("K_TEST_{}", new_ulid());
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
    let secret_id = format!("SECRET_{}", new_ulid());
    let request = serde_json::json!({
        // Schema bootstrap is deliberately read-only and sessionless. Exercise
        // redaction on a real traced mutation rather than requiring a repair
        // reference to initialize or append to operational storage.
        "op": "store.upsert",
        "params": {
            "entity": "knowledge",
            "payload": {
                "id": secret_id,
                "title": "Trace redaction fixture",
                "text": "Non-secret tracing regression payload",
                "provenance": "cmd:cargo-test"
            },
            "my_password": "supersecretpassword",
            "id": secret_id
        }
    });

    let response = run_rpc(request);
    assert_eq!(response["success"], true);

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

#[test]
fn test_rpc_failures_return_accurate_recovery_context_and_lawful_repair() {
    let exe = resolve_decapod_bin();
    let repo = tempfile::tempdir().expect("tempdir");

    let run_in = |dir: &std::path::Path, args: &[&str], envs: &[(&str, &str)]| {
        let mut cmd = Command::new(&exe);
        cmd.current_dir(dir).args(args);
        for (k, v) in envs {
            cmd.env(k, v);
        }
        cmd.output().expect("run decapod")
    };

    assert!(
        run_in(repo.path(), &["git", "init", "-b", "master"], &[])
            .status
            .success()
            || Command::new("git")
                .current_dir(repo.path())
                .args(["init", "-b", "master"])
                .output()
                .unwrap()
                .status
                .success()
    );
    let init_out = run_in(
        repo.path(),
        &[
            "init",
            "--force",
            "--product-summary",
            "Recovery test fixture",
        ],
        &[],
    );
    assert!(
        init_out.status.success(),
        "{}",
        String::from_utf8_lossy(&init_out.stderr)
    );
    for args in [
        vec!["add", "."],
        vec![
            "-c",
            "user.name=Recovery fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "init",
        ],
    ] {
        let out = Command::new("git")
            .current_dir(repo.path())
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // 1. Unsupported command returns unknown_op with empty allowed_next_ops (no agent.init).
    let unknown_out = run_in(
        repo.path(),
        &["rpc", "--op", "does.not.exist", "--params", "{}"],
        &[],
    );
    let unknown_json: serde_json::Value =
        serde_json::from_slice(&unknown_out.stdout).expect("unknown_op JSON");
    assert_eq!(unknown_json["success"], false);
    assert_eq!(unknown_json["error"]["code"], "unknown_op");
    assert_eq!(unknown_json["allowed_next_ops"], serde_json::json!([]));
    assert_eq!(
        unknown_json["error"]["details"]["failure_class"],
        serde_json::json!("unsupported_no_recovery")
    );
    assert!(unknown_json["context_capsule"]["spec"].is_string());

    // 2. Protected-branch mutating RPC returns structured JSON with workspace.ensure next_op, never agent.init.
    let agent_id = "recovery-agent-1948";
    let sess_out = run_in(
        repo.path(),
        &["session", "acquire"],
        &[("DECAPOD_AGENT_ID", agent_id)],
    );
    assert!(sess_out.status.success());
    let password = String::from_utf8_lossy(&sess_out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("Password: ").map(|s| s.trim().to_string()))
        .expect("session password");
    let envs = [
        ("DECAPOD_AGENT_ID", agent_id),
        ("DECAPOD_SESSION_PASSWORD", password.as_str()),
        ("DECAPOD_CLAIM_AUTORUN", "0"),
    ];

    let prot_out = run_in(
        repo.path(),
        &["rpc", "--op", "specs.refresh", "--params", "{}"],
        &envs,
    );
    assert!(!prot_out.status.success());
    let prot_json: serde_json::Value =
        serde_json::from_slice(&prot_out.stdout).expect("protected_branch JSON");
    assert_eq!(prot_json["success"], false);
    assert_eq!(prot_json["error"]["code"], "protected_branch");
    assert_eq!(
        prot_json["blocked_by"][0]["kind"],
        serde_json::json!("protected_branch")
    );
    let prot_ops: Vec<&str> = prot_json["allowed_next_ops"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v["op"].as_str())
        .collect();
    assert_eq!(prot_ops, vec!["workspace.status", "workspace.ensure"]);

    // 3. Create & claim a todo and enter isolated worktree, then test invalid entity -> schema.get -> lawful repair.
    let add_out = run_in(
        repo.path(),
        &["todo", "add", "Test recovery path", "--format", "json"],
        &envs,
    );
    assert!(add_out.status.success());
    let todo_id = serde_json::from_slice::<serde_json::Value>(&add_out.stdout).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        run_in(
            repo.path(),
            &["todo", "claim", "--id", &todo_id, "--format", "json"],
            &envs,
        )
        .status
        .success()
    );
    let branch = format!("agent/{agent_id}/{todo_id}-recovery");
    let wt_out = run_in(
        repo.path(),
        &["workspace", "ensure", "--branch", &branch],
        &envs,
    );
    assert!(wt_out.status.success());
    let wt_path = std::path::PathBuf::from(
        serde_json::from_slice::<serde_json::Value>(&wt_out.stdout).unwrap()["worktree_path"]
            .as_str()
            .unwrap(),
    );

    // Before constitutional awareness warm-up, store.upsert fails with constitutional_awareness_required (not agent.init session error).
    let aware_out = run_in(
        &wt_path,
        &[
            "rpc",
            "--op",
            "store.upsert",
            "--params",
            r#"{"entity":"todo","payload":{"title":"needs-awareness"}}"#,
        ],
        &envs,
    );
    assert!(!aware_out.status.success());
    let aware_json: serde_json::Value =
        serde_json::from_slice(&aware_out.stdout).expect("awareness JSON");
    assert_eq!(
        aware_json["error"]["code"],
        "constitutional_awareness_required"
    );
    assert_eq!(
        aware_json["error"]["details"]["failure_class"],
        serde_json::json!("session_custody_required")
    );

    // Complete prerequisites, then trigger invalid_entity -> schema.get -> valid store.upsert.
    let mut pr_envs = envs.to_vec();
    pr_envs.push(("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"));
    pr_envs.push(("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1"));
    assert!(
        run_in(&wt_path, &["govern", "project-check"], &pr_envs)
            .status
            .success()
    );
    assert!(
        run_in(
            &wt_path,
            &["rpc", "--op", "agent.init", "--params", "{}"],
            &envs
        )
        .status
        .success()
    );
    assert!(
        run_in(
            &wt_path,
            &[
                "rpc",
                "--op",
                "constitution.get",
                "--params",
                r#"{"section":"core/DECAPOD"}"#,
            ],
            &envs,
        )
        .status
        .success()
    );
    assert!(
        run_in(
            &wt_path,
            &["rpc", "--op", "context.resolve", "--params", "{}"],
            &envs
        )
        .status
        .success()
    );

    let bad_ent_out = run_in(
        &wt_path,
        &[
            "rpc",
            "--op",
            "store.upsert",
            "--params",
            r#"{"entity":"bogus","payload":{}}"#,
        ],
        &envs,
    );
    assert!(bad_ent_out.status.success());
    let bad_ent_json: serde_json::Value =
        serde_json::from_slice(&bad_ent_out.stdout).expect("invalid_entity JSON");
    assert_eq!(bad_ent_json["success"], false);
    assert_eq!(bad_ent_json["error"]["code"], "invalid_entity");
    assert_eq!(
        bad_ent_json["allowed_next_ops"][0]["op"],
        serde_json::json!("schema.get")
    );
    assert_eq!(
        bad_ent_json["allowed_next_ops"][0]["required_params"],
        serde_json::json!(["entity"])
    );

    // Independent consumer projection preserves refusal details, blockers, allowed_next_ops, and context_capsule.
    let proj_params = serde_json::json!({
        "kind": "rpc_response",
        "response": bad_ent_json,
    });
    let proj_out = run_in(
        &wt_path,
        &[
            "rpc",
            "--op",
            "interop.project",
            "--params",
            &proj_params.to_string(),
        ],
        &envs,
    );
    assert!(proj_out.status.success());
    let proj_json: serde_json::Value = serde_json::from_slice(&proj_out.stdout).unwrap();
    let projection = &proj_json["result"]["projection"];
    assert_eq!(projection["status"], serde_json::json!("blocked"));
    assert_eq!(
        projection["error"]["code"],
        serde_json::json!("invalid_entity")
    );
    assert_eq!(
        projection["allowed_next_ops"],
        serde_json::json!(["schema.get"])
    );
    assert!(projection["context_capsule"].is_object());

    // Follow recommended schema.get, then lawfully complete store.upsert while keeping failed trace visible.
    assert!(
        run_in(
            &wt_path,
            &[
                "rpc",
                "--op",
                "schema.get",
                "--params",
                r#"{"entity":"todo"}"#
            ],
            &envs,
        )
        .status
        .success()
    );
    let good_out = run_in(
        &wt_path,
        &[
            "rpc",
            "--op",
            "store.upsert",
            "--params",
            r#"{"entity":"todo","payload":{"title":"Recovered todo"}}"#,
        ],
        &envs,
    );
    assert!(good_out.status.success());
    let good_json: serde_json::Value = serde_json::from_slice(&good_out.stdout).unwrap();
    assert_eq!(good_json["success"], true);

    let trace_out = run_in(&wt_path, &["trace", "export", "--last", "20"], &envs);
    assert!(trace_out.status.success());
    let trace_text = String::from_utf8_lossy(&trace_out.stdout);
    assert!(
        trace_text.contains("invalid_entity") && trace_text.contains("Recovered todo"),
        "trace must retain original failed observation alongside repaired success"
    );
}
