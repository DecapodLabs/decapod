use decapod::core::todo;
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn resolve_decapod_bin() -> PathBuf {
    let cargo_bin = env!("CARGO_BIN_EXE_decapod");
    if let Ok(path) = Path::new(cargo_bin).canonicalize() {
        return path;
    }
    if let Ok(runfiles_dir) = std::env::var("RUNFILES_DIR") {
        let path = Path::new(&runfiles_dir).join("_main").join("decapod");
        if path.exists() {
            return path;
        }
    }
    if let Some(parent) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        let path = parent.join("decapod");
        if path.exists() {
            return path;
        }
    }
    PathBuf::from(cargo_bin)
}

fn contract_fixture() -> tempfile::TempDir {
    let mut builder = tempfile::Builder::new();
    builder.prefix("decapod-cli-contract-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    builder.tempdir().expect("private CLI contract fixture")
}

fn run_decapod(args: &[&str]) -> String {
    // Help/schema are embedded contracts, not mutations of Bazel runfiles.
    let fixture = contract_fixture();
    let initialized = Command::new(resolve_decapod_bin())
        .current_dir(fixture.path())
        .env("XDG_CONFIG_HOME", fixture.path().join(".config"))
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .args(["init", "--force"])
        .output()
        .expect("initialize private contract fixture");
    assert!(
        initialized.status.success(),
        "fixture init failed: {initialized:?}"
    );
    let output = Command::new(resolve_decapod_bin())
        .current_dir(fixture.path())
        .env("XDG_CONFIG_HOME", fixture.path().join(".config"))
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .args(args)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("failed to execute decapod");
    assert!(
        output.status.success(),
        "decapod {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn todo_help_schema_and_docs_stay_in_sync() {
    let expected = [
        "add",
        "list",
        "get",
        "done",
        "archive",
        "comment",
        "edit",
        "claim",
        "release",
        "rebuild",
        "categories",
        "register-agent",
        "ownerships",
        "heartbeat",
        "presence",
        "worker-run",
        "handoff",
        "add-owner",
        "remove-owner",
        "list-owners",
        "register-expertise",
        "expertise",
    ];

    let help = run_decapod(&["todo", "--help"]);
    for command in &expected {
        let re = Regex::new(&format!(r"(?m)^\s+{}\s+", regex::escape(command)))
            .expect("valid help regex");
        assert!(re.is_match(&help), "todo --help missing command: {command}");
    }

    let schema = todo::schema();
    let schema_cmds: HashSet<String> = schema["commands"]
        .as_array()
        .expect("commands array")
        .iter()
        .filter_map(|item| {
            item.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    for command in &expected {
        assert!(
            schema_cmds.contains(*command),
            "todo schema missing command: {command}"
        );
    }
}

#[test]
fn container_help_schema_and_docs_stay_in_sync() {
    let help = run_decapod(&["auto", "container", "run", "--help"]);
    for flag in [
        "--agent",
        "--cmd",
        "--task-id",
        "--push",
        "--pr",
        "--pr-base",
        "--pr-title",
        "--pr-body",
        "--keep-worktree",
        "--inherit-env",
        "--local-only",
    ] {
        assert!(
            help.contains(flag),
            "container run --help missing flag: {flag}"
        );
    }

    let schema_out = run_decapod(&[
        "data",
        "schema",
        "--subsystem",
        "container",
        "--deterministic",
    ]);
    for field in [
        "\"task_id\"",
        "\"pr\"",
        "\"pr_base\"",
        "\"pr_title\"",
        "\"pr_body\"",
        "\"keep_worktree\"",
        "\"inherit_env\"",
        "\"local_only\"",
        "\"dockerfile_template\"",
        "\"extra_packages_env\"",
    ] {
        assert!(
            schema_out.contains(field),
            "container schema missing field: {field}"
        );
    }
}

#[cfg(unix)]
#[test]
fn contract_fixture_is_private_at_creation() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = contract_fixture();
    assert_eq!(
        std::fs::metadata(fixture.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}
// The finite source-CI profile executes this target. Keep consumer readiness
// and initialization regressions in that required suite rather than relying
// on Cargo's discovery of otherwise unscheduled standalone test targets.
mod consumer_readiness;
mod init_ci_scaffolding;
mod init_config_behavior;

// Keep every recovery case in the required selective CLI matrix while balancing
// the two real-process suites within their unchanged 300-second budgets.
#[path = "support/cli_workunit_recovery.rs"]
mod workunit_recovery;
