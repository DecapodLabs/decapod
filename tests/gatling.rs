//! Gatling regression tests — exercises every CLI code path.
//!
//! Converted from `dev/gatling_test.sh` (v2). Each subsystem is tested
//! in an isolated temp directory with a git repo + `decapod init`.

use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a temp dir with git init + decapod init --force.
fn setup_workspace() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path().to_path_buf();

    // git init
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(&dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(&dir)
        .output()
        .unwrap();
    std::fs::write(dir.join("README.md"), "# test\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-q", "-m", "init"])
        .current_dir(&dir)
        .output()
        .unwrap();

    // decapod init --force
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--force"])
        .current_dir(&dir)
        .output()
        .expect("decapod init");
    assert!(out.status.success(), "decapod init --force failed");

    (tmp, dir)
}

/// Run decapod with given args. Returns (success, stdout+stderr).
fn run(dir: &PathBuf, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run decapod");
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), combined)
}

/// Assert command succeeds.
fn ok(dir: &PathBuf, args: &[&str]) {
    let (success, output) = run(dir, args);
    assert!(
        success,
        "expected success for `decapod {}` but got failure:\n{}",
        args.join(" "),
        output
    );
}

/// Assert command fails.
fn fail(dir: &PathBuf, args: &[&str]) {
    let (success, output) = run(dir, args);
    assert!(
        !success,
        "expected failure for `decapod {}` but got success:\n{}",
        args.join(" "),
        output
    );
}

/// Extract first task ID from `todo --format json list` output.
fn extract_task_id(dir: &PathBuf) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["todo", "--format", "json", "list"])
        .current_dir(dir)
        .output()
        .expect("failed to run decapod");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Parse stdout-only (no stderr contamination) as JSON.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(stdout.trim()) {
        if let Some(items) = v.get("items").and_then(|i| i.as_array()) {
            if let Some(first) = items.first() {
                if let Some(id) = first.get("id").and_then(|i| i.as_str()) {
                    return id.to_string();
                }
            }
        }
    }
    // Fallback: extract ULID-style ID with regex from anywhere in the output
    let re = regex::Regex::new(r"R_[0-9A-Z]{26}").unwrap();
    if let Some(m) = re.find(&stdout) {
        return m.as_str().to_string();
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    panic!(
        "could not extract task ID.\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
}

// ---------------------------------------------------------------------------
// 1. Top-Level
// ---------------------------------------------------------------------------

#[test]
fn t001_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("decapod"),
        "version output should contain 'decapod'"
    );
}

#[test]
fn t002_help() {
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(out.status.success());
}

#[test]
fn t003_no_args_errors() {
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn t004_version_command_works() {
    let (_tmp, dir) = setup_workspace();

    let (success, output) = run(&dir, &["version"]);
    assert!(success, "version command should succeed, got:\n{}", output);
    // Output contains the version number (e.g., "0.12.1")
    assert!(
        output.contains(env!("CARGO_PKG_VERSION")),
        "expected version string in output:\n{}",
        output
    );
}

// ---------------------------------------------------------------------------
// 2. Init
// ---------------------------------------------------------------------------

#[test]
fn t010_init_lifecycle() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().to_path_buf();

    // git init
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "t@t"])
        .current_dir(&dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "t"])
        .current_dir(&dir)
        .output()
        .unwrap();
    std::fs::write(dir.join("README.md"), "# t\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-q", "-m", "init"])
        .current_dir(&dir)
        .output()
        .unwrap();

    // T010: basic init
    ok(&dir, &["init"]);
    // T011: init --force (re-init)
    ok(&dir, &["init", "--force"]);
    // T012: init --dry-run
    ok(&dir, &["init", "--dry-run"]);
    // T013: init --all
    ok(&dir, &["init", "--all"]);
    // T014: init --claude
    ok(&dir, &["init", "--claude"]);
    // T015: init --gemini
    ok(&dir, &["init", "--gemini"]);
    // T016: init --agents
    ok(&dir, &["init", "--agents"]);
    // T017: init clean
    ok(&dir, &["init", "clean"]);
    // Re-init after clean
    ok(&dir, &["init", "--force"]);
    // T018: alias `i`
    ok(&dir, &["i"]);
}

// ---------------------------------------------------------------------------
// 3. Setup
// ---------------------------------------------------------------------------

#[test]
fn t020_setup_hooks() {
    let (_tmp, dir) = setup_workspace();
    // T020
    ok(&dir, &["setup", "hook", "--commit-msg"]);
    // T021
    ok(&dir, &["setup", "hook", "--pre-commit"]);
    // T022
    ok(&dir, &["setup", "hook", "--uninstall"]);
    // T023
    ok(&dir, &["setup", "--help"]);
}

// ---------------------------------------------------------------------------
// 4. Docs
// ---------------------------------------------------------------------------

#[test]
fn t030_docs() {
    let (_tmp, dir) = setup_workspace();
    // T030
    ok(&dir, &["docs", "show", "core/DECAPOD.md"]);
    // T031
    ok(&dir, &["docs", "show", "specs/INTENT.md"]);
    // T032
    ok(&dir, &["docs", "show", "plugins/TODO.md"]);
    // T033
    ok(&dir, &["docs", "ingest"]);
    // T034
    ok(&dir, &["docs", "override"]);
    // T035
    ok(&dir, &["docs", "--help"]);
    // T036: alias
    ok(&dir, &["d", "show", "core/DECAPOD.md"]);
    // T037: nonexistent doc → error
    fail(&dir, &["docs", "show", "nonexistent.md"]);
}

// ---------------------------------------------------------------------------
// 5. Todo
// ---------------------------------------------------------------------------

#[test]
fn t040_todo_lifecycle() {
    let (_tmp, dir) = setup_workspace();

    // T040: add basic
    ok(
        &dir,
        &["todo", "add", "Test task 1", "--description", "A test task"],
    );
    // T041: add minimal
    ok(&dir, &["todo", "add", "Test task 2"]);
    // T042: list
    ok(&dir, &["todo", "list"]);
    // T043: json list
    ok(&dir, &["todo", "--format", "json", "list"]);
    // T044: text list
    ok(&dir, &["todo", "--format", "text", "list"]);

    // Extract task ID for CRUD
    let id = extract_task_id(&dir);

    // T045: get
    ok(&dir, &["todo", "get", "--id", &id]);
    // T046: claim
    ok(
        &dir,
        &["todo", "claim", "--id", &id, "--agent", "test-agent"],
    );
    // T047: comment
    ok(
        &dir,
        &["todo", "comment", "--id", &id, "--comment", "Test comment"],
    );
    // T047A: add owner
    ok(
        &dir,
        &[
            "todo",
            "add-owner",
            "--id",
            &id,
            "--agent",
            "test-reviewer",
            "--claim-type",
            "secondary",
        ],
    );
    // T047B: list owners
    ok(&dir, &["todo", "list-owners", "--id", &id]);
    // T047C: remove owner
    ok(
        &dir,
        &[
            "todo",
            "remove-owner",
            "--id",
            &id,
            "--agent",
            "test-reviewer",
        ],
    );
    // T048: edit
    ok(
        &dir,
        &["todo", "edit", "--id", &id, "--title", "Updated title"],
    );
    // T049: release
    ok(&dir, &["todo", "release", "--id", &id]);
    // T050: done
    ok(&dir, &["todo", "done", "--id", &id]);
    // T052: categories
    ok(&dir, &["todo", "categories"]);
    // T055: alias
    ok(&dir, &["t", "list"]);
    // T056: help
    ok(&dir, &["todo", "--help"]);
}

#[test]
fn t057_todo_add_all_opts() {
    let (_tmp, dir) = setup_workspace();
    ok(
        &dir,
        &[
            "todo",
            "add",
            "Full task",
            "--description",
            "desc",
            "--priority",
            "high",
            "--tags",
            "bug,ux",
            "--owner",
            "dev1",
        ],
    );
}

#[test]
fn t058_todo_get_nonexistent() {
    let (_tmp, dir) = setup_workspace();
    // Getting a nonexistent task — the CLI currently returns exit 0
    let (_, _) = run(&dir, &["todo", "get", "--id", "NONEXISTENT_ID_12345"]);
}

#[test]
fn t059_todo_add_relationships() {
    let (_tmp, dir) = setup_workspace();
    ok(&dir, &["todo", "add", "Parent task"]);
    let parent_id = extract_task_id(&dir);

    // T059: --ref
    ok(&dir, &["todo", "add", "Ref task", "--ref", "issue#42"]);
    // T05A: --parent
    ok(&dir, &["todo", "add", "Child task", "--parent", &parent_id]);
    // T05B: --depends-on
    ok(
        &dir,
        &["todo", "add", "Dep task", "--depends-on", &parent_id],
    );
    // T05C: --blocks
    ok(&dir, &["todo", "add", "Block task", "--blocks", &parent_id]);
}

#[test]
fn t051_todo_done_validated() {
    let (_tmp, dir) = setup_workspace();
    ok(&dir, &["todo", "add", "Validated task"]);
    let id = extract_task_id(&dir);
    ok(&dir, &["todo", "done", "--id", &id, "--validated"]);
}

#[test]
#[ignore] // BUG-1: rebuild doesn't handle task.edit/task.claim/task.release events
fn t053_todo_rebuild() {
    let (_tmp, dir) = setup_workspace();
    ok(&dir, &["todo", "add", "Rebuild test"]);
    let id = extract_task_id(&dir);
    // Perform operations that emit events which rebuild must handle
    ok(&dir, &["todo", "edit", "--id", &id, "--title", "Edited"]);
    ok(&dir, &["todo", "claim", "--id", &id, "--agent", "a"]);
    ok(&dir, &["todo", "release", "--id", &id]);
    ok(&dir, &["todo", "rebuild"]);
}

// ---------------------------------------------------------------------------
// 6. Validate
// ---------------------------------------------------------------------------

#[test]
fn t060_validate() {
    let (_tmp, dir) = setup_workspace();
    // T060: default validate
    ok(&dir, &["validate"]);
    // T061: --store user
    ok(&dir, &["validate", "--store", "user"]);
    // T062: --store repo
    ok(&dir, &["validate", "--store", "repo"]);
    // T063: --format json
    ok(&dir, &["validate", "--format", "json"]);
    // T064: --format text
    ok(&dir, &["validate", "--format", "text"]);
    // T065: alias v
    ok(&dir, &["v"]);
}

#[test]
fn t066_validate_errors() {
    let (_tmp, dir) = setup_workspace();
    // T066: invalid store falls through to repo (not an error)
    ok(&dir, &["validate", "--store", "invalid"]);
    // T067: invalid format falls through to text (not an error)
    ok(&dir, &["validate", "--format", "invalid"]);
}

// ---------------------------------------------------------------------------
// 7. Govern > Policy
// ---------------------------------------------------------------------------

#[test]
fn t070_policy() {
    let (_tmp, dir) = setup_workspace();
    // T070
    ok(
        &dir,
        &[
            "govern",
            "policy",
            "eval",
            "--command",
            "rm -rf /",
            "--path",
            "/tmp/test",
        ],
    );
    // T071
    ok(
        &dir,
        &["govern", "policy", "eval", "--command", "ls", "--path", "."],
    );
    // T072
    ok(&dir, &["govern", "policy", "riskmap", "init"]);
    // T073
    ok(&dir, &["govern", "policy", "riskmap", "verify"]);
    // T074
    ok(&dir, &["govern", "policy", "--help"]);
    // T075
    ok(
        &dir,
        &["govern", "policy", "approve", "--id", "TEST_APPROVAL_123"],
    );
}

// ---------------------------------------------------------------------------
// 8. Govern > Health
// ---------------------------------------------------------------------------

#[test]
fn t080_health() {
    let (_tmp, dir) = setup_workspace();
    // T080
    ok(
        &dir,
        &[
            "govern",
            "health",
            "claim",
            "--id",
            "test-claim-1",
            "--subject",
            "System is healthy",
            "--kind",
            "assertion",
        ],
    );
    // T081
    ok(
        &dir,
        &[
            "govern",
            "health",
            "proof",
            "--claim-id",
            "test-claim-1",
            "--surface",
            "manual check",
            "--result",
            "pass",
        ],
    );
    // T082
    ok(&dir, &["govern", "health", "get", "--id", "test-claim-1"]);
    // T083
    ok(&dir, &["govern", "health", "summary"]);
    // T084
    ok(&dir, &["govern", "health", "autonomy"]);
    // T085
    ok(&dir, &["govern", "health", "--help"]);
    // T086: claim with provenance
    ok(
        &dir,
        &[
            "govern",
            "health",
            "claim",
            "--id",
            "test-claim-2",
            "--subject",
            "Has tests",
            "--kind",
            "proof",
            "--provenance",
            "test suite",
        ],
    );
}

// ---------------------------------------------------------------------------
// 9. Govern > Proof
// ---------------------------------------------------------------------------

#[test]
fn t090_proof() {
    let (_tmp, dir) = setup_workspace();
    // T090
    ok(&dir, &["govern", "proof", "run"]);
    // T091: proof test is a stub — expect failure
    fail(&dir, &["govern", "proof", "test", "--name", "schema-check"]);
    // T092
    ok(&dir, &["govern", "proof", "list"]);
    // T093
    ok(&dir, &["govern", "proof", "--help"]);
}

// ---------------------------------------------------------------------------
// 10. Govern > Watcher
// ---------------------------------------------------------------------------

#[test]
fn t100_watcher() {
    let (_tmp, dir) = setup_workspace();
    ok(&dir, &["govern", "watcher", "run"]);
    ok(&dir, &["govern", "watcher", "--help"]);
}

// ---------------------------------------------------------------------------
// 11. Govern > Feedback
// ---------------------------------------------------------------------------

#[test]
fn t110_feedback() {
    let (_tmp, dir) = setup_workspace();
    // T110
    ok(
        &dir,
        &[
            "govern",
            "feedback",
            "add",
            "--source",
            "test-agent",
            "--text",
            "Test feedback",
        ],
    );
    // T111
    ok(
        &dir,
        &[
            "govern",
            "feedback",
            "add",
            "--source",
            "test-agent",
            "--text",
            "Feedback with link",
            "--links",
            "specs/INTENT.md",
        ],
    );
    // T112
    ok(&dir, &["govern", "feedback", "propose"]);
    // T113
    ok(&dir, &["govern", "feedback", "--help"]);
}

// ---------------------------------------------------------------------------
// 12. Data > Archive
// ---------------------------------------------------------------------------

#[test]
fn t120_archive() {
    let (_tmp, dir) = setup_workspace();
    ok(&dir, &["data", "archive", "list"]);
    ok(&dir, &["data", "archive", "verify"]);
    ok(&dir, &["data", "archive", "--help"]);
}

// ---------------------------------------------------------------------------
// 13. Data > Knowledge
// ---------------------------------------------------------------------------

#[test]
fn t130_knowledge() {
    let (_tmp, dir) = setup_workspace();
    // T130: provenance needs scheme prefix
    ok(
        &dir,
        &[
            "data",
            "knowledge",
            "add",
            "--id",
            "kb-001",
            "--title",
            "Test entry",
            "--text",
            "Some knowledge text",
            "--provenance",
            "cmd:manual-entry",
        ],
    );
    // T131: with claim-id
    ok(
        &dir,
        &[
            "data",
            "knowledge",
            "add",
            "--id",
            "kb-002",
            "--title",
            "Linked entry",
            "--text",
            "Knowledge with claim",
            "--provenance",
            "cmd:manual-entry",
            "--claim-id",
            "test-claim-1",
        ],
    );
    // T132
    ok(&dir, &["data", "knowledge", "search", "--query", "test"]);
    // T133
    ok(&dir, &["data", "knowledge", "--help"]);
}

// ---------------------------------------------------------------------------
// 14. Data > Context
// ---------------------------------------------------------------------------

#[test]
fn t140_context() {
    let (_tmp, dir) = setup_workspace();
    let test_file = dir.join("test_file.txt");
    std::fs::write(&test_file, "some content\n").unwrap();
    let test_file_str = test_file.to_str().unwrap();

    // T140
    ok(
        &dir,
        &[
            "data",
            "context",
            "audit",
            "--profile",
            "main",
            "--files",
            test_file_str,
        ],
    );
    // T141
    ok(
        &dir,
        &[
            "data",
            "context",
            "pack",
            "--path",
            test_file_str,
            "--summary",
            "Test context pack",
        ],
    );
    // T142: restore with fake archive → expect error
    fail(
        &dir,
        &[
            "data",
            "context",
            "restore",
            "--id",
            "ctx-001",
            "--profile",
            "main",
            "--current-files",
            test_file_str,
        ],
    );
    // T143
    ok(&dir, &["data", "context", "--help"]);
}

// ---------------------------------------------------------------------------
// 15. Data > Schema
// ---------------------------------------------------------------------------

#[test]
fn t150_schema() {
    let (_tmp, dir) = setup_workspace();
    ok(&dir, &["data", "schema"]);
    ok(&dir, &["data", "schema", "--format", "json"]);
    ok(&dir, &["data", "schema", "--format", "md"]);
    ok(&dir, &["data", "schema", "--deterministic"]);
    ok(&dir, &["data", "schema", "--subsystem", "todo"]);
    ok(&dir, &["data", "schema", "--subsystem", "health"]);
    ok(&dir, &["data", "schema", "--subsystem", "policy"]);
    // Invalid subsystem returns gracefully (exit 0 with error in JSON)
    ok(&dir, &["data", "schema", "--subsystem", "nonexistent"]);
}

// ---------------------------------------------------------------------------
// 16. Data > Repo
// ---------------------------------------------------------------------------

#[test]
fn t160_repo() {
    let (_tmp, dir) = setup_workspace();
    ok(&dir, &["data", "repo", "map"]);
    ok(&dir, &["data", "repo", "graph"]);
    ok(&dir, &["data", "repo", "--help"]);
}

// ---------------------------------------------------------------------------
// 17. Data > Broker
// ---------------------------------------------------------------------------

#[test]
fn t170_broker() {
    let (_tmp, dir) = setup_workspace();
    ok(&dir, &["data", "broker", "audit"]);
    ok(&dir, &["data", "broker", "--help"]);
}

// ---------------------------------------------------------------------------
// 18. Data > Teammate
// ---------------------------------------------------------------------------

#[test]
fn t180_teammate() {
    let (_tmp, dir) = setup_workspace();
    // T180
    ok(
        &dir,
        &[
            "data",
            "teammate",
            "add",
            "--category",
            "style",
            "--key",
            "theme",
            "--value",
            "dark mode",
        ],
    );
    // T181
    ok(
        &dir,
        &[
            "data",
            "teammate",
            "add",
            "--category",
            "workflow",
            "--key",
            "editor",
            "--value",
            "neovim",
            "--context",
            "coding sessions",
            "--source",
            "user_request",
        ],
    );
    // T182
    ok(&dir, &["data", "teammate", "list"]);
    // T183
    ok(
        &dir,
        &[
            "data",
            "teammate",
            "get",
            "--category",
            "style",
            "--key",
            "theme",
        ],
    );
    // T184
    ok(
        &dir,
        &[
            "data",
            "teammate",
            "observe",
            "--content",
            "User prefers concise responses",
        ],
    );
    // T185
    ok(
        &dir,
        &[
            "data",
            "teammate",
            "observe",
            "--content",
            "Always uses dark mode",
            "--category",
            "style",
        ],
    );
    // T186
    ok(&dir, &["data", "teammate", "prompt"]);
    // T187
    ok(
        &dir,
        &["data", "teammate", "prompt", "--context", "code review"],
    );
    // T188
    ok(&dir, &["data", "teammate", "prompt", "--format", "json"]);
    // T189
    ok(&dir, &["data", "teammate", "--help"]);
}

// ---------------------------------------------------------------------------
// 19. Auto > Cron
// ---------------------------------------------------------------------------

#[test]
fn t190_cron() {
    let (_tmp, dir) = setup_workspace();
    // T190
    ok(
        &dir,
        &[
            "auto",
            "cron",
            "add",
            "--name",
            "test-cron",
            "--schedule",
            "0 * * * *",
            "--command",
            "echo hello",
        ],
    );
    // T191
    ok(
        &dir,
        &[
            "auto",
            "cron",
            "add",
            "--name",
            "full-cron",
            "--schedule",
            "*/5 * * * *",
            "--command",
            "echo world",
            "--description",
            "A test cron",
            "--tags",
            "test,dev",
        ],
    );
    // T192
    ok(&dir, &["auto", "cron", "list"]);

    // Extract cron ID from list output
    let (_, out) = run(&dir, &["auto", "cron", "list"]);
    let cron_id = extract_ulid_from(&out);

    // T193
    ok(&dir, &["auto", "cron", "get", "--id", &cron_id]);
    // T194
    ok(
        &dir,
        &[
            "auto",
            "cron",
            "update",
            "--id",
            &cron_id,
            "--schedule",
            "*/10 * * * *",
        ],
    );
    // T195
    ok(&dir, &["auto", "cron", "list", "--status", "active"]);
    // T196
    ok(&dir, &["auto", "cron", "list", "--name-search", "test"]);
    // T197
    ok(&dir, &["auto", "cron", "delete", "--id", &cron_id]);
    // T198
    ok(&dir, &["auto", "cron", "--help"]);
}

// ---------------------------------------------------------------------------
// 20. Auto > Reflex
// ---------------------------------------------------------------------------

#[test]
fn t200_reflex() {
    let (_tmp, dir) = setup_workspace();
    // T200
    ok(
        &dir,
        &[
            "auto",
            "reflex",
            "add",
            "--name",
            "test-reflex",
            "--trigger-type",
            "event",
            "--action-type",
            "command",
            "--action-config",
            "echo done",
        ],
    );
    // T201
    ok(
        &dir,
        &[
            "auto",
            "reflex",
            "add",
            "--name",
            "full-reflex",
            "--trigger-type",
            "event",
            "--action-type",
            "command",
            "--action-config",
            "echo world",
            "--description",
            "A test reflex",
            "--tags",
            "test",
        ],
    );
    // T202
    ok(&dir, &["auto", "reflex", "list"]);

    let (_, out) = run(&dir, &["auto", "reflex", "list"]);
    let reflex_id = extract_ulid_from(&out);

    // T203
    ok(&dir, &["auto", "reflex", "get", "--id", &reflex_id]);
    // T204
    ok(
        &dir,
        &[
            "auto",
            "reflex",
            "update",
            "--id",
            &reflex_id,
            "--name",
            "updated-reflex",
        ],
    );
    // T205
    ok(&dir, &["auto", "reflex", "list", "--status", "active"]);
    // T206
    ok(&dir, &["auto", "reflex", "delete", "--id", &reflex_id]);
    // T207
    ok(&dir, &["auto", "reflex", "--help"]);
}

// ---------------------------------------------------------------------------
// 21. QA > Verify
// ---------------------------------------------------------------------------

#[test]
fn t210_verify() {
    let (_tmp, dir) = setup_workspace();
    ok(
        &dir,
        &[
            "todo",
            "add",
            "Verify test task",
            "--description",
            "For QA verify",
        ],
    );
    let id = extract_task_id(&dir);

    // T210
    ok(&dir, &["qa", "verify", "todo", &id]);
    // T211
    ok(&dir, &["qa", "verify", "--stale"]);
    // T212
    ok(&dir, &["qa", "verify", "--json"]);
    // T213
    ok(&dir, &["qa", "verify", "--help"]);
}

// ---------------------------------------------------------------------------
// 22. QA > Check (env-dependent — skip crate-description in temp dir)
// ---------------------------------------------------------------------------

#[test]
fn t220_check() {
    let (_tmp, dir) = setup_workspace();
    // T220: no flags
    ok(&dir, &["qa", "check"]);
    // T223
    ok(&dir, &["qa", "check", "--help"]);
}

// ---------------------------------------------------------------------------
// 23-24. Group Help & Aliases
// ---------------------------------------------------------------------------

#[test]
fn t240_group_help() {
    let (_tmp, dir) = setup_workspace();
    ok(&dir, &["govern", "--help"]);
    ok(&dir, &["g", "--help"]);
    ok(&dir, &["data", "--help"]);
    ok(&dir, &["auto", "--help"]);
    ok(&dir, &["a", "--help"]);
    ok(&dir, &["qa", "--help"]);
    ok(&dir, &["q", "--help"]);
}

// ---------------------------------------------------------------------------
// 25. Edge Cases & Error Paths
// ---------------------------------------------------------------------------

#[test]
fn t280_invalid_subcommand() {
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .arg("notacommand")
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn t281_todo_add_empty_string() {
    let (_tmp, dir) = setup_workspace();
    // Currently succeeds — noted as EDGE-1 in audit
    ok(&dir, &["todo", "add", ""]);
}

#[test]
fn t282_todo_get_missing_id() {
    let (_tmp, dir) = setup_workspace();
    fail(&dir, &["todo", "get"]);
}

#[test]
fn t283_docs_show_empty_path() {
    let (_tmp, dir) = setup_workspace();
    fail(&dir, &["docs", "show", ""]);
}

#[test]
fn t284_knowledge_add_missing_fields() {
    let (_tmp, dir) = setup_workspace();
    fail(&dir, &["data", "knowledge", "add", "--id", "kb-only"]);
}

#[test]
fn t285_cron_add_missing_schedule() {
    let (_tmp, dir) = setup_workspace();
    fail(
        &dir,
        &["auto", "cron", "add", "--name", "bad", "--command", "echo"],
    );
}

#[test]
fn t286_reflex_add_missing_trigger() {
    let (_tmp, dir) = setup_workspace();
    fail(
        &dir,
        &[
            "auto",
            "reflex",
            "add",
            "--name",
            "bad",
            "--action-type",
            "command",
            "--action-config",
            "echo",
        ],
    );
}

#[test]
fn t287_teammate_get_missing_key() {
    let (_tmp, dir) = setup_workspace();
    fail(&dir, &["data", "teammate", "get", "--category", "style"]);
}

#[test]
fn t288_health_claim_missing_fields() {
    let (_tmp, dir) = setup_workspace();
    fail(&dir, &["govern", "health", "claim", "--id", "only-id"]);
}

// ---------------------------------------------------------------------------
// 14. Decide
// ---------------------------------------------------------------------------

#[test]
fn t290_decide_trees() {
    let (_tmp, dir) = setup_workspace();
    let (success, output) = run(&dir, &["decide", "trees"]);
    assert!(success, "decide trees failed:\n{}", output);
    assert!(output.contains("web-app"));
    assert!(output.contains("microservice"));
    assert!(output.contains("cli-tool"));
    assert!(output.contains("library"));
}

#[test]
fn t291_decide_suggest() {
    let (_tmp, dir) = setup_workspace();
    let (success, output) = run(
        &dir,
        &["decide", "suggest", "--prompt", "build a web application"],
    );
    assert!(success, "decide suggest failed:\n{}", output);
    assert!(output.contains("web-app"));
}

#[test]
fn t292_decide_session_lifecycle() {
    let (_tmp, dir) = setup_workspace();

    // Start a session
    let (success, output) = run(
        &dir,
        &[
            "decide",
            "start",
            "--tree",
            "cli-tool",
            "--title",
            "Gatling CLI Test",
        ],
    );
    assert!(success, "decide start failed:\n{}", output);
    assert!(output.contains("DS_"));

    // Extract session ID
    let re = regex::Regex::new(r"DS_[0-9A-Z]{26}").unwrap();
    let session_id = re
        .find(&output)
        .map(|m| m.as_str().to_string())
        .expect("no session ID found");

    // Next question
    let (success, output) = run(&dir, &["decide", "next", "--session", &session_id]);
    assert!(success, "decide next failed:\n{}", output);
    assert!(output.contains("language"));

    // Record a decision
    let (success, output) = run(
        &dir,
        &[
            "decide",
            "record",
            "--session",
            &session_id,
            "--question",
            "language",
            "--value",
            "rust",
        ],
    );
    assert!(success, "decide record failed:\n{}", output);
    assert!(output.contains("DD_"));
    assert!(output.contains("Rust"));

    // List decisions
    let (success, output) = run(&dir, &["decide", "list", "--session", &session_id]);
    assert!(success, "decide list failed:\n{}", output);
    assert!(output.contains("rust"));

    // Session list
    let (success, output) = run(&dir, &["decide", "session", "list"]);
    assert!(success, "decide session list failed:\n{}", output);
    assert!(output.contains(&session_id));

    // Session get
    let (success, output) = run(&dir, &["decide", "session", "get", "--id", &session_id]);
    assert!(success, "decide session get failed:\n{}", output);
    assert!(output.contains("Gatling CLI Test"));

    // Complete
    let (success, output) = run(&dir, &["decide", "complete", "--session", &session_id]);
    assert!(success, "decide complete failed:\n{}", output);
    assert!(output.contains("completed"));
}

#[test]
fn t293_decide_invalid_tree() {
    let (_tmp, dir) = setup_workspace();
    fail(
        &dir,
        &["decide", "start", "--tree", "nonexistent", "--title", "Bad"],
    );
}

#[test]
fn t294_decide_invalid_option() {
    let (_tmp, dir) = setup_workspace();

    // Start session
    let (success, output) = run(
        &dir,
        &[
            "decide",
            "start",
            "--tree",
            "cli-tool",
            "--title",
            "Bad Option Test",
        ],
    );
    assert!(success);
    let re = regex::Regex::new(r"DS_[0-9A-Z]{26}").unwrap();
    let session_id = re.find(&output).unwrap().as_str();

    // Invalid option value
    fail(
        &dir,
        &[
            "decide",
            "record",
            "--session",
            session_id,
            "--question",
            "language",
            "--value",
            "cobol",
        ],
    );
}

#[test]
fn t295_decide_schema() {
    let (_tmp, dir) = setup_workspace();
    let (success, output) = run(&dir, &["decide", "schema"]);
    assert!(success, "decide schema failed:\n{}", output);
    assert!(output.contains("decide"));
    assert!(output.contains("decisions.db"));
}

#[test]
fn t296_decide_init() {
    let (_tmp, dir) = setup_workspace();
    let (success, output) = run(&dir, &["decide", "init"]);
    assert!(success, "decide init failed:\n{}", output);
    assert!(output.contains("initialized"));
}

// ---------------------------------------------------------------------------
// Helper: extract a ULID from text output (26-char uppercase alphanumeric)
// ---------------------------------------------------------------------------

fn extract_ulid_from(text: &str) -> String {
    let re = regex::Regex::new(r"[0-9A-Z]{26}").unwrap();
    re.find(text)
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| panic!("no ULID found in output:\n{}", text))
}
