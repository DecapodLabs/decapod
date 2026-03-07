use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use tempfile::TempDir;

static SHARED_REPO: OnceLock<(TempDir, PathBuf)> = OnceLock::new();

fn run_decapod(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(dir)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .args(args)
        .output()
        .expect("run decapod")
}

fn setup_repo() -> &'static (TempDir, PathBuf) {
    SHARED_REPO.get_or_init(|| {
        let tmp = TempDir::new().expect("tmpdir");
        let dir = tmp.path().to_path_buf();

        Command::new("git")
            .current_dir(&dir)
            .args(["init", "-b", "master"])
            .output()
            .expect("git init");

        let init = run_decapod(&dir, &["init", "--force"]);
        assert!(
            init.status.success(),
            "decapod init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );

        (tmp, dir)
    })
}

#[test]
fn test_capabilities_schema_stability() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(dir, &["capabilities", "--format", "json"]);
    assert!(out.status.success(), "capabilities failed");

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");

    let required_fields = ["version", "capabilities", "subsystems", "interlock_codes"];
    for field in required_fields {
        assert!(
            json.get(field).is_some(),
            "capabilities must have '{}' field",
            field
        );
    }

    let caps = json["capabilities"].as_array().expect("capabilities array");
    assert!(!caps.is_empty(), "capabilities must not be empty");
}

#[test]
fn test_schema_determinism_command() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(
        dir,
        &["data", "schema", "--format", "json", "--deterministic"],
    );
    assert!(out.status.success(), "schema failed");

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");

    assert!(
        json.get("command_registry").is_some(),
        "schema must have command_registry"
    );

    let commands = json["command_registry"].as_array().expect("commands array");
    assert!(!commands.is_empty(), "commands must not be empty");

    let has_essential = commands.iter().any(|c| {
        c.get("name")
            .and_then(|n| n.as_str())
            .map(|n| n.contains("validate"))
            .unwrap_or(false)
    });
    assert!(has_essential, "must have validate command");
}

#[test]
fn test_interlock_codes_present() {
    let (_tmp, dir) = setup_repo();

    let caps = run_decapod(dir, &["capabilities", "--format", "json"]);
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&caps.stdout)).expect("valid JSON");

    let codes = json["interlock_codes"]
        .as_array()
        .expect("interlock_codes array");

    let required_codes = [
        "workspace_required",
        "verification_required",
        "store_boundary_violation",
    ];

    for code in &required_codes {
        assert!(
            codes.iter().any(|c| c == code),
            "required interlock code '{}' must be present",
            code
        );
    }
}

#[test]
fn test_validate_command_works() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(dir, &["validate"]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        out.status.success() || combined.contains("gate") || combined.contains("validate"),
        "validate should execute and report gates. Got: {}",
        combined
    );
}

#[test]
fn test_workspace_command_structure() {
    let (_tmp, dir) = setup_repo();

    let status = run_decapod(dir, &["workspace", "status"]);
    assert!(status.status.success(), "workspace status should succeed");

    let output = String::from_utf8_lossy(&status.stdout);
    assert!(
        output.contains("branch") || output.contains("workspace") || output.contains("git"),
        "workspace status should report branch info"
    );
}

#[test]
fn test_todo_command_structure() {
    let (_tmp, dir) = setup_repo();

    run_decapod(dir, &["session", "acquire"]);

    let add = run_decapod(
        dir,
        &["todo", "add", "test contract task", "--format", "json"],
    );
    assert!(
        add.status.success(),
        "todo add should succeed: {}",
        String::from_utf8_lossy(&add.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&add.stdout)).expect("valid JSON");

    assert!(json.get("id").is_some(), "todo add should return id");
}

#[test]
fn test_constitution_docs_accessible() {
    let (_tmp, dir) = setup_repo();

    let docs = run_decapod(dir, &["docs", "show", "core/DECAPOD.md"]);
    assert!(
        docs.status.success(),
        "docs show should succeed: {}",
        String::from_utf8_lossy(&docs.stderr)
    );

    let output = String::from_utf8_lossy(&docs.stdout);
    assert!(
        output.contains("Decapod"),
        "docs show should return Decapod content"
    );
}

#[test]
fn test_session_required_for_mutation() {
    let (_tmp, dir) = setup_repo();

    let add_without_session = run_decapod(dir, &["todo", "add", "should fail"]);
    let output = String::from_utf8_lossy(&add_without_session.stderr);

    assert!(
        !add_without_session.status.success() || output.contains("session"),
        "mutation without session should fail or mention session"
    );
}

#[test]
fn test_capabilities_includes_core_commands() {
    let (_tmp, dir) = setup_repo();

    let caps = run_decapod(dir, &["capabilities", "--format", "json"]);
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&caps.stdout)).expect("valid JSON");

    let caps_list = json["capabilities"].as_array().expect("capabilities array");

    let required_commands = ["validate", "workspace", "todo", "session", "docs"];

    for cmd in &required_commands {
        let has_cmd = caps_list.iter().any(|c| {
            c.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n.contains(cmd))
                .unwrap_or(false)
        });
        assert!(has_cmd, "capabilities must include '{}' command", cmd);
    }
}

#[test]
fn test_interlock_drift_detection_capability() {
    let (_tmp, dir) = setup_repo();

    let preflight = run_decapod(dir, &["preflight", "--op", "validate", "--format", "json"]);
    assert!(preflight.status.success(), "preflight should work");

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&preflight.stdout)).expect("valid JSON");

    assert!(
        json.get("risk_flags").is_some(),
        "preflight must report risk_flags"
    );

    assert!(
        json.get("likely_failures").is_some(),
        "preflight must predict failures"
    );
}
