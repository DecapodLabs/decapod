use rusqlite::Connection;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(dir).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run decapod")
}

fn setup_repo() -> (TempDir, PathBuf, String) {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path().to_path_buf();

    let init = Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let out = run_decapod(&dir, &["init", "--force"], &[]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let acquire = run_decapod(
        &dir,
        &["session", "acquire"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        acquire.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );
    let acquire_stdout = String::from_utf8_lossy(&acquire.stdout);
    let password = acquire_stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .expect("session password in output");

    let todo_list = run_decapod(
        &dir,
        &["todo", "list"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        todo_list.status.success(),
        "todo list failed: {}",
        String::from_utf8_lossy(&todo_list.stderr)
    );

    (tmp, dir, password)
}

#[test]
fn validate_terminates_with_typed_error_under_db_contention() {
    let (_tmp, dir, password) = setup_repo();
    let db_path = dir.join(".decapod").join("data").join("todo.db");
    assert!(db_path.exists(), "todo db should exist before lock test");

    let conn = Connection::open(&db_path).expect("open todo db");
    conn.execute_batch("BEGIN EXCLUSIVE;")
        .expect("acquire exclusive lock");

    let start = Instant::now();
    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ("DECAPOD_VALIDATE_TIMEOUT_SECS", "2"),
        ],
    );
    let elapsed = start.elapsed();

    assert!(
        !validate.status.success(),
        "validate should fail under forced lock contention"
    );

    let stderr = String::from_utf8_lossy(&validate.stderr);
    assert!(
        stderr.contains("VALIDATE_TIMEOUT_OR_LOCK"),
        "validate stderr should contain typed bounded-time failure marker; got: {}",
        stderr
    );

    assert!(
        elapsed.as_secs() < 10,
        "validate must terminate quickly under contention; elapsed={:?}",
        elapsed
    );

    conn.execute_batch("ROLLBACK;").expect("release lock");
}

#[test]
fn validate_terminates_with_typed_error_under_immediate_lock_contention() {
    let (_tmp, dir, password) = setup_repo();
    let db_path = dir.join(".decapod").join("data").join("todo.db");
    assert!(db_path.exists(), "todo db should exist before lock test");

    let conn = Connection::open(&db_path).expect("open todo db");
    conn.execute_batch("BEGIN IMMEDIATE;")
        .expect("acquire immediate lock");

    let start = Instant::now();
    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ("DECAPOD_VALIDATE_TIMEOUT_SECS", "2"),
        ],
    );
    let elapsed = start.elapsed();

    assert!(
        !validate.status.success(),
        "validate should fail under immediate lock contention"
    );
    let stderr = String::from_utf8_lossy(&validate.stderr);
    assert!(
        stderr.contains("VALIDATE_TIMEOUT_OR_LOCK"),
        "validate stderr should contain typed bounded-time failure marker; got: {}",
        stderr
    );
    assert!(
        elapsed.as_secs() < 10,
        "validate must terminate quickly under contention; elapsed={:?}",
        elapsed
    );

    conn.execute_batch("ROLLBACK;").expect("release lock");
}

#[test]
fn validate_timeout_does_not_strand_db_for_followup_commands() {
    let (_tmp, dir, password) = setup_repo();
    let db_path = dir.join(".decapod").join("data").join("todo.db");
    assert!(db_path.exists(), "todo db should exist before lock test");

    let conn = Connection::open(&db_path).expect("open todo db");
    conn.execute_batch("BEGIN EXCLUSIVE;")
        .expect("acquire exclusive lock");

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ("DECAPOD_VALIDATE_TIMEOUT_SECONDS", "2"),
        ],
    );
    assert!(
        !validate.status.success(),
        "validate should fail under forced lock contention"
    );

    let stderr = String::from_utf8_lossy(&validate.stderr);
    assert!(
        stderr.contains("VALIDATE_TIMEOUT_OR_LOCK"),
        "validate stderr should contain typed bounded-time failure marker; got: {}",
        stderr
    );

    conn.execute_batch("ROLLBACK;").expect("release lock");

    let followup = run_decapod(
        &dir,
        &["todo", "list"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        followup.status.success(),
        "follow-up command should succeed after lock release; stderr:\n{}",
        String::from_utf8_lossy(&followup.stderr)
    );
}

#[test]
fn validate_parallel_contention_emits_typed_reasoned_diagnostics() {
    let (_tmp, dir, password) = setup_repo();
    let db_path = dir.join(".decapod").join("data").join("todo.db");
    assert!(db_path.exists(), "todo db should exist before lock test");

    let conn = Connection::open(&db_path).expect("open todo db");
    conn.execute_batch("BEGIN EXCLUSIVE;")
        .expect("acquire exclusive lock");

    let mut outputs = Vec::new();
    for _ in 0..4 {
        outputs.push(run_decapod(
            &dir,
            &["validate"],
            &[
                ("DECAPOD_AGENT_ID", "unknown"),
                ("DECAPOD_SESSION_PASSWORD", &password),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
                ("DECAPOD_VALIDATE_TIMEOUT_SECS", "2"),
                ("DECAPOD_DIAGNOSTICS", "1"),
            ],
        ));
    }

    conn.execute_batch("ROLLBACK;").expect("release lock");

    for output in &outputs {
        assert!(
            !output.status.success(),
            "validate should fail under forced lock contention"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("VALIDATE_TIMEOUT_OR_LOCK"),
            "expected typed failure marker; got: {stderr}"
        );
        assert!(
            stderr.contains("artifacts/diagnostics/validate/"),
            "expected diagnostics artifact path in stderr; got: {stderr}"
        );
    }

    let diagnostics_dir = dir.join("artifacts").join("diagnostics").join("validate");
    assert!(
        diagnostics_dir.exists(),
        "diagnostics directory should be created under artifacts/diagnostics/validate"
    );

    let mut diagnostic_count = 0usize;
    for entry in fs::read_dir(&diagnostics_dir).expect("read diagnostics dir") {
        let entry = entry.expect("diagnostics dir entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        diagnostic_count += 1;
        let raw = fs::read_to_string(&path).expect("read diagnostics artifact");
        let payload: Value = serde_json::from_str(&raw).expect("parse diagnostics artifact");
        assert_eq!(payload["kind"], "validate_diagnostic");
        assert_eq!(payload["op"], "validate");
        assert_eq!(payload["reason_code"], "timeout_acquiring_lock");
        assert!(payload["elapsed_ms"].as_u64().is_some());
        assert!(payload["timeout_secs"].as_u64().unwrap_or(0) > 0);
        assert!(payload["artifact_hash"].as_str().unwrap_or("").len() >= 64);
        assert!(
            !raw.contains(&dir.to_string_lossy().to_string()),
            "diagnostics should not leak absolute worktree path"
        );
        assert!(
            !raw.to_ascii_lowercase().contains("hostname"),
            "diagnostics should not leak hostname"
        );
    }

    assert!(
        diagnostic_count >= 4,
        "expected at least one diagnostics artifact per failed validate run"
    );
}

#[test]
fn validate_diagnostics_disabled_does_not_write_artifacts() {
    let (_tmp, dir, password) = setup_repo();
    let db_path = dir.join(".decapod").join("data").join("todo.db");
    assert!(db_path.exists(), "todo db should exist before lock test");

    let conn = Connection::open(&db_path).expect("open todo db");
    conn.execute_batch("BEGIN EXCLUSIVE;")
        .expect("acquire exclusive lock");

    let output = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ("DECAPOD_VALIDATE_TIMEOUT_SECS", "2"),
        ],
    );
    conn.execute_batch("ROLLBACK;").expect("release lock");

    assert!(
        !output.status.success(),
        "validate should fail under forced lock contention"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("VALIDATE_TIMEOUT_OR_LOCK"),
        "expected typed failure marker; got: {stderr}"
    );
    assert!(
        !stderr.contains("Diagnostics:"),
        "diagnostics path should not be emitted when DECAPOD_DIAGNOSTICS is disabled"
    );
    assert!(
        !dir.join("artifacts").join("diagnostics").exists(),
        "diagnostics artifacts must not be written unless explicitly enabled"
    );
}

#[test]
fn validate_diagnostics_payload_is_sanitized() {
    let (_tmp, dir, password) = setup_repo();
    let db_path = dir.join(".decapod").join("data").join("todo.db");
    assert!(db_path.exists(), "todo db should exist before lock test");

    let conn = Connection::open(&db_path).expect("open todo db");
    conn.execute_batch("BEGIN EXCLUSIVE;")
        .expect("acquire exclusive lock");

    let output = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ("DECAPOD_VALIDATE_TIMEOUT_SECS", "2"),
            ("DECAPOD_DIAGNOSTICS", "1"),
        ],
    );
    conn.execute_batch("ROLLBACK;").expect("release lock");
    assert!(
        !output.status.success(),
        "validate should fail under forced lock contention"
    );

    let diagnostics_dir = dir.join("artifacts").join("diagnostics").join("validate");
    let artifact_path = fs::read_dir(&diagnostics_dir)
        .expect("read diagnostics dir")
        .find_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension().and_then(|s| s.to_str()) == Some("json")).then_some(path)
        })
        .expect("at least one diagnostics artifact");

    let raw = fs::read_to_string(&artifact_path).expect("read diagnostics artifact");
    let payload: Value = serde_json::from_str(&raw).expect("parse diagnostics artifact");
    assert_eq!(payload["kind"], "validate_diagnostic");
    assert_eq!(payload["op"], "validate");
    assert_eq!(payload["reason_code"], "timeout_acquiring_lock");

    let object = payload.as_object().expect("diagnostics payload object");
    let forbidden_keys = [
        "hostname", "username", "env", "cwd", "path", "pid", "command",
    ];
    for key in forbidden_keys {
        assert!(
            !object.contains_key(key),
            "diagnostics payload must not contain forbidden key '{key}'"
        );
    }

    let forbidden_patterns = [
        "/home/",
        "C:\\",
        "USER=",
        "HOSTNAME=",
        "PATH=",
        "DECAPOD_SESSION_PASSWORD",
    ];
    for pat in forbidden_patterns {
        assert!(
            !raw.contains(pat),
            "diagnostics payload must not contain forbidden pattern '{pat}'"
        );
    }

    let run_id = payload["run_id"].as_str().expect("run_id string");
    assert_eq!(run_id.len(), 32, "run_id must be 128-bit hex");
    assert!(
        run_id.chars().all(|c| c.is_ascii_hexdigit()),
        "run_id must be lower/upper hex only"
    );
    assert!(
        !run_id.contains('-'),
        "run_id must be non-ULID/non-hyphenated to avoid inferential timestamp encoding"
    );
}

#[test]
fn validate_smoke_runtime_is_bounded_without_contention() {
    let (_tmp, dir, password) = setup_repo();
    let mut durations_ms = Vec::new();

    for _ in 0..3 {
        let start = Instant::now();
        let output = run_decapod(
            &dir,
            &["validate"],
            &[
                ("DECAPOD_AGENT_ID", "unknown"),
                ("DECAPOD_SESSION_PASSWORD", &password),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
                ("DECAPOD_VALIDATE_TIMEOUT_SECS", "10"),
            ],
        );
        let elapsed = start.elapsed().as_millis() as u64;
        durations_ms.push(elapsed);
        assert!(
            output.status.success(),
            "validate should pass without forced contention; stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            elapsed < 10_000,
            "validate runtime exceeded bounded smoke threshold: {}ms",
            elapsed
        );
    }

    let total: u64 = durations_ms.iter().sum();
    assert!(
        total < 20_000,
        "three sequential validates should stay within bounded aggregate runtime; got {}ms",
        total
    );
}
