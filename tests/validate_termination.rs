use rusqlite::Connection;
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
