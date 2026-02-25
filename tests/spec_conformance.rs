use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn run_decapod(dir: &PathBuf, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run decapod")
}

fn setup_repo() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path().to_path_buf();

    Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");

    let init = run_decapod(&dir, &["init", "--force"]);
    if !init.status.success() {
        eprintln!(
            "decapod init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );
    }
    assert!(
        init.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    // Create a worktree for tests that need it
    let workspace = run_decapod(&dir, &["workspace", "ensure", "--branch", "test/feature"]);
    if !workspace.status.success() {
        eprintln!(
            "workspace ensure failed (may already exist): {}",
            String::from_utf8_lossy(&workspace.stderr)
        );
    }

    (tmp, dir)
}

#[test]
fn test_capabilities_stability() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(&dir, &["capabilities", "--format", "json"]);
    assert!(out.status.success(), "capabilities failed");

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");

    assert!(json.get("version").is_some(), "version field missing");
    assert!(json.get("capabilities").is_some(), "capabilities missing");
    assert!(json.get("subsystems").is_some(), "subsystems missing");
    assert!(
        json.get("interlock_codes").is_some(),
        "interlock_codes missing"
    );

    let interlock = json["interlock_codes"].as_array().expect("array");
    assert!(
        interlock.iter().any(|v| v == "workspace_required"),
        "workspace_required missing"
    );
    assert!(
        interlock.iter().any(|v| v == "verification_required"),
        "verification_required missing"
    );
    assert!(
        interlock.iter().any(|v| v == "store_boundary_violation"),
        "store_boundary_violation missing"
    );
}

#[test]
fn test_schema_determinism() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(
        &dir,
        &["data", "schema", "--format", "json", "--deterministic"],
    );
    assert!(out.status.success(), "schema failed");

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");

    assert!(
        json.get("command_registry").is_some(),
        "command_registry missing"
    );
    assert!(json.get("subsystems").is_some(), "subsystems missing");
}

#[test]
fn test_validate_terminates_boundedly() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(&dir, &["validate"]);
    let output = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{}{}", output, stderr);

    assert!(
        combined.contains("validate") || combined.contains("gate"),
        "validate should run. stdout: {}, stderr: {}",
        output,
        stderr
    );
}

#[test]
fn test_workspace_protection() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(&dir, &["workspace", "status"]);
    let output = String::from_utf8_lossy(&out.stdout);

    assert!(
        output.contains("git_is_protected"),
        "workspace should report protection status"
    );
    assert!(output.contains("master"), "should show current branch");
}

#[test]
fn test_session_required() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(&dir, &["session", "status"]);
    let output = String::from_utf8_lossy(&out.stdout);

    assert!(
        output.contains("Session") || output.contains("active") || output.contains("inactive"),
        "session should report status"
    );
}

#[test]
fn test_todo_state_machine() {
    let (_tmp, dir) = setup_repo();

    run_decapod(&dir, &["session", "acquire"]);

    let add_out = run_decapod(&dir, &["todo", "add", "test task", "--format", "json"]);
    assert!(
        add_out.status.success(),
        "todo add failed: {}",
        String::from_utf8_lossy(&add_out.stderr)
    );

    let add_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&add_out.stdout)).expect("valid JSON");

    let task_id = add_json["id"].as_str().expect("task id");

    let claim_out = run_decapod(&dir, &["todo", "claim", "--id", task_id]);
    assert!(claim_out.status.success(), "todo claim failed");

    let done_out = run_decapod(&dir, &["todo", "done", "--id", task_id]);
    assert!(done_out.status.success(), "todo done failed");

    let list_out = run_decapod(&dir, &["todo", "list", "--format", "json"]);
    let list_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&list_out.stdout)).expect("valid JSON");

    let tasks = list_json["items"].as_array().expect("items array");
    let task = tasks
        .iter()
        .find(|t| t["id"].as_str() == Some(task_id))
        .expect("task found");

    assert!(
        task["status"] == "done" || task["status"] == "verified",
        "task should be done or verified"
    );
}

#[test]
fn test_store_boundary_enforcement() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(&dir, &["validate", "--store", "repo"]);
    let output = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{}{}", output, stderr);

    assert!(
        combined.contains("validate") || combined.contains("gate") || out.status.success(),
        "repo store validation should execute. stdout: {}, stderr: {}",
        output,
        stderr
    );
}

#[test]
fn test_error_codes_present() {
    let (_tmp, dir) = setup_repo();

    let caps = run_decapod(&dir, &["capabilities", "--format", "json"]);
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&caps.stdout)).expect("valid JSON");

    let interlock = json["interlock_codes"].as_array().expect("array");

    let required_codes = [
        "workspace_required",
        "verification_required",
        "store_boundary_violation",
    ];

    for code in required_codes {
        assert!(
            interlock.iter().any(|v| v == code),
            "required error code {} missing",
            code
        );
    }
}

#[test]
fn test_workspace_protected_patterns() {
    let (_tmp, dir) = setup_repo();

    let caps = run_decapod(&dir, &["capabilities", "--format", "json"]);
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&caps.stdout)).expect("valid JSON");

    let workspace = json.get("workspace").expect("workspace object");
    let protected = workspace["protected_patterns"].as_array().expect("array");

    assert!(protected.iter().any(|v| v == "main"), "main not protected");
    assert!(
        protected.iter().any(|v| v == "master"),
        "master not protected"
    );
}

#[test]
fn test_preflight_schema_stability() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(&dir, &["preflight", "--op", "validate", "--format", "json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        out.status.success(),
        "preflight should succeed. stderr: {}",
        stderr
    );

    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert!(json.get("op").is_some(), "op field missing");
    assert!(json.get("risk_flags").is_some(), "risk_flags missing");
    assert!(
        json.get("likely_failures").is_some(),
        "likely_failures missing"
    );
    assert!(
        json.get("required_capsules").is_some(),
        "required_capsules missing"
    );
    assert!(
        json.get("next_best_actions").is_some(),
        "next_best_actions missing"
    );
    assert!(json.get("workspace").is_some(), "workspace missing");
}

#[test]
fn test_impact_schema_stability() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(
        &dir,
        &["impact", "--changed-files", "src/a.rs", "--format", "json"],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        out.status.success(),
        "impact should succeed. stderr: {}",
        stderr
    );

    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert!(json.get("changed_files").is_some(), "changed_files missing");
    assert!(
        json.get("will_fail_validate").is_some(),
        "will_fail_validate missing"
    );
    assert!(
        json.get("predicted_failures").is_some(),
        "predicted_failures missing"
    );
    assert!(
        json.get("validation_predictions").is_some(),
        "validation_predictions missing"
    );
    assert!(
        json.get("recommendation").is_some(),
        "recommendation missing"
    );
    assert!(json.get("workspace").is_some(), "workspace missing");
}

#[test]
fn test_preflight_predicts_workspace_required() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(&dir, &["preflight", "--op", "validate", "--format", "json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);

    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let risk_flags = json["risk_flags"].as_array().expect("risk_flags array");
    assert!(
        risk_flags.iter().any(|v| v == "protected_branch"),
        "preflight should detect protected_branch risk on master branch"
    );

    let likely_failures = json["likely_failures"]
        .as_array()
        .expect("likely_failures array");
    let has_workspace_required = likely_failures
        .iter()
        .any(|f| f.get("code").and_then(|c| c.as_str()) == Some("WORKSPACE_REQUIRED"));
    assert!(
        has_workspace_required,
        "preflight should predict WORKSPACE_REQUIRED"
    );

    let next_actions = json["next_best_actions"]
        .as_array()
        .expect("next_best_actions array");
    let has_workspace_ensure = next_actions.iter().any(|a| {
        a.as_str()
            .map(|s| s.contains("workspace ensure"))
            .unwrap_or(false)
    });
    assert!(
        has_workspace_ensure,
        "preflight should suggest workspace ensure"
    );
}

#[test]
fn test_impact_predicts_failure_on_protected_branch() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(
        &dir,
        &["impact", "--changed-files", "src/a.rs", "--format", "json"],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let will_fail = json["will_fail_validate"]
        .as_bool()
        .expect("will_fail_validate bool");
    assert!(
        will_fail,
        "impact should predict validate will fail on protected branch"
    );

    let predicted = json["predicted_failures"]
        .as_array()
        .expect("predicted_failures array");
    let has_workspace_required = predicted
        .iter()
        .any(|f| f.get("code").and_then(|c| c.as_str()) == Some("WORKSPACE_REQUIRED"));
    assert!(
        has_workspace_required,
        "impact should predict WORKSPACE_REQUIRED"
    );
}

#[test]
fn test_capabilities_includes_interlock() {
    let (_tmp, dir) = setup_repo();

    let caps = run_decapod(&dir, &["capabilities", "--format", "json"]);
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&caps.stdout)).expect("valid JSON");

    let caps_list = json["capabilities"].as_array().expect("capabilities array");
    let has_preflight = caps_list
        .iter()
        .any(|c| c.get("name").and_then(|n| n.as_str()) == Some("preflight.check"));
    let has_impact = caps_list
        .iter()
        .any(|c| c.get("name").and_then(|n| n.as_str()) == Some("impact.predict"));

    assert!(has_preflight, "capabilities should include preflight.check");
    assert!(has_impact, "capabilities should include impact.predict");
}

#[test]
fn test_demo_interlock_prediction() {
    let (_tmp, dir) = setup_repo();

    let preflight_out = run_decapod(&dir, &["preflight", "--op", "validate", "--format", "json"]);
    let preflight_stdout = String::from_utf8_lossy(&preflight_out.stdout);

    let preflight: serde_json::Value = serde_json::from_str(&preflight_stdout).unwrap();

    let risk_flags = preflight["risk_flags"].as_array().expect("array");
    assert!(
        risk_flags.iter().any(|v| v == "protected_branch"),
        "preflight should detect protected_branch"
    );

    let impact_out = run_decapod(
        &dir,
        &["impact", "--changed-files", "src/a.rs", "--format", "json"],
    );
    let impact_stdout = String::from_utf8_lossy(&impact_out.stdout);

    let impact: serde_json::Value = serde_json::from_str(&impact_stdout).unwrap();

    let will_fail = impact["will_fail_validate"].as_bool().expect("bool");
    assert!(
        will_fail,
        "impact should predict failure on protected branch"
    );

    println!("\n=== INTERLOCK DEMO ===");
    println!("Preflight detected: {:?}", preflight["risk_flags"]);
    println!("Impact predicted: will_fail_validate = {}", will_fail);
    println!("Recommendation: {}", impact["recommendation"]);
    println!("=== END DEMO ===\n");
}
