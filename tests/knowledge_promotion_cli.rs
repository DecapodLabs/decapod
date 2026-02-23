use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
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

    let git_init = Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(git_init.status.success(), "git init failed");

    let init = run_decapod(&dir, &["init", "--force"], &[]);
    assert!(
        init.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let acquire = run_decapod(
        &dir,
        &["session", "acquire"],
        &[("DECAPOD_AGENT_ID", "unknown")],
    );
    assert!(
        acquire.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );
    let password = String::from_utf8_lossy(&acquire.stdout)
        .lines()
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .expect("password in session acquire output");

    (tmp, dir, password)
}

#[test]
fn knowledge_promote_writes_append_only_ledger_event() {
    let (_tmp, dir, password) = setup_repo();

    let out = run_decapod(
        &dir,
        &[
            "data",
            "knowledge",
            "promote",
            "--source-entry-id",
            "K_001",
            "--evidence-ref",
            "commit:abc123",
            "--evidence-ref",
            "file:docs/spec.md#L10",
            "--approved-by",
            "human/reviewer-1",
            "--reason",
            "convert episodic finding into procedural norm",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        out.status.success(),
        "knowledge promote failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let payload: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(payload["source_entry_id"], "K_001");
    assert_eq!(payload["target_class"], "procedural");
    assert_eq!(payload["approved_by"], "human/reviewer-1");

    let ledger_path = dir
        .join(".decapod")
        .join("data")
        .join("knowledge.promotions.jsonl");
    assert!(ledger_path.exists(), "ledger should exist");

    let lines = fs::read_to_string(&ledger_path).expect("read ledger");
    let last = lines
        .lines()
        .filter(|l| !l.trim().is_empty())
        .next_back()
        .expect("ledger last line");
    let event: Value = serde_json::from_str(last).expect("valid jsonl line");
    assert_eq!(event["source_entry_id"], "K_001");
    assert_eq!(event["target_class"], "procedural");
}

#[test]
fn knowledge_promote_rejects_missing_evidence_refs() {
    let (_tmp, dir, password) = setup_repo();

    let out = run_decapod(
        &dir,
        &[
            "data",
            "knowledge",
            "promote",
            "--source-entry-id",
            "K_002",
            "--approved-by",
            "human/reviewer-2",
            "--reason",
            "insufficient evidence should fail",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        !out.status.success(),
        "promote should fail without evidence refs"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("at least one --evidence-ref is required"),
        "unexpected error: {}",
        stderr
    );
}
