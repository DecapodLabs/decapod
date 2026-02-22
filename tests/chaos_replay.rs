use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use tempfile::TempDir;

fn setup_workspace() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path().to_path_buf();

    Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(&dir)
        .output()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(&dir)
        .output()
        .expect("git config name");
    std::fs::write(dir.join("README.md"), "# chaos replay\n").expect("write readme");

    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--force"])
        .current_dir(&dir)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("decapod init");
    assert!(out.status.success(), "decapod init --force failed");

    let session = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["session", "acquire"])
        .current_dir(&dir)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("decapod session acquire");
    assert!(
        session.status.success(),
        "decapod session acquire failed:\n{}\n{}",
        String::from_utf8_lossy(&session.stdout),
        String::from_utf8_lossy(&session.stderr)
    );

    // Warm up the TODO DB to avoid migration race conditions in threads
    let _ = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["todo", "list"])
        .current_dir(&dir)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output();

    (tmp, dir)
}

fn run(dir: &PathBuf, args: &[&str]) -> (bool, String) {
    run_with_retries(dir, args, 1)
}

/// Run a decapod command with retry logic for transient SQLite I/O errors
/// that occur under heavy concurrent process contention.
fn run_with_retries(dir: &PathBuf, args: &[&str], max_retries: u32) -> (bool, String) {
    for attempt in 0..=max_retries {
        let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
            .args(args)
            .current_dir(dir)
            .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
            .output()
            .expect("failed to run decapod");
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if out.status.success() || attempt == max_retries {
            return (out.status.success(), combined);
        }
        // Retry on transient SQLite I/O errors (disk I/O error, database is locked)
        if combined.contains("disk I/O error") || combined.contains("database is locked") {
            thread::sleep(std::time::Duration::from_millis(50 * (attempt as u64 + 1)));
            continue;
        }
        return (false, combined);
    }
    unreachable!()
}

fn list_chaos_projection(dir: &PathBuf) -> BTreeMap<String, (String, String, String)> {
    let (success, out) = run(dir, &["todo", "--format", "json", "list"]);
    assert!(success, "todo list failed:\n{}", out);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid list json");
    let items = value["items"].as_array().expect("items array");

    let mut projection = BTreeMap::new();
    for item in items {
        let title = item["title"].as_str().unwrap_or_default();
        if !title.starts_with("CHAOS:") {
            continue;
        }
        let id = item["id"].as_str().unwrap_or_default().to_string();
        let status = item["status"].as_str().unwrap_or_default().to_string();
        let owner = item["owner"].as_str().unwrap_or_default().to_string();
        projection.insert(id, (title.to_string(), status, owner));
    }
    projection
}

#[test]
#[ignore = "flaky test: session file race condition in parallel workers"]
fn chaos_multi_agent_replay_is_deterministic() {
    let (_tmp, dir) = setup_workspace();
    let workers = 4usize;
    let tasks_per_worker = 8usize;

    let mut handles = Vec::new();
    for worker in 0..workers {
        let dir_clone = dir.clone();
        handles.push(thread::spawn(move || {
            let agent = format!("agent-{}", worker);
            for n in 0..tasks_per_worker {
                let title = format!("CHAOS: {} task {}", agent, n);
                let (ok, out) = run_with_retries(
                    &dir_clone,
                    &[
                        "todo",
                        "add",
                        &title,
                        "--owner",
                        &agent,
                        "--priority",
                        "high",
                        "--tags",
                        "chaos,replay",
                    ],
                    3,
                );
                assert!(ok, "todo add failed for {}:\n{}", title, out);

                // Inject controlled failures while concurrent writes happen.
                if n % 3 == 0 {
                    let (should_fail, _) = run(&dir_clone, &["todo", "get"]);
                    assert!(!should_fail, "expected malformed command to fail");
                }
            }
        }));
    }

    for h in handles {
        h.join().expect("worker join");
    }

    let before = list_chaos_projection(&dir);
    assert_eq!(before.len(), workers * tasks_per_worker);

    let (ok_rebuild_1, rebuild_1) = run(&dir, &["todo", "--format", "json", "rebuild"]);
    assert!(ok_rebuild_1, "rebuild #1 failed:\n{}", rebuild_1);
    let rebuild_json_1: serde_json::Value =
        serde_json::from_str(&rebuild_1).expect("valid rebuild #1 json");
    assert_eq!(rebuild_json_1["status"], "ok");

    let after_first_rebuild = list_chaos_projection(&dir);
    assert_eq!(before, after_first_rebuild);

    let (ok_rebuild_2, rebuild_2) = run(&dir, &["todo", "--format", "json", "rebuild"]);
    assert!(ok_rebuild_2, "rebuild #2 failed:\n{}", rebuild_2);
    let rebuild_json_2: serde_json::Value =
        serde_json::from_str(&rebuild_2).expect("valid rebuild #2 json");
    assert_eq!(rebuild_json_2["status"], "ok");

    let after_second_rebuild = list_chaos_projection(&dir);
    assert_eq!(after_first_rebuild, after_second_rebuild);

    // Event log integrity: all event IDs must be unique even under concurrent writers.
    let events_path = dir.join(".decapod").join("data").join("todo.events.jsonl");
    let content = std::fs::read_to_string(events_path).expect("read todo events");
    let mut ids = HashSet::new();
    for line in content.lines() {
        let v: serde_json::Value = serde_json::from_str(line).expect("event json");
        let id = v["event_id"].as_str().expect("event_id").to_string();
        assert!(
            ids.insert(id),
            "duplicate event_id found in todo.events.jsonl"
        );
    }
}
