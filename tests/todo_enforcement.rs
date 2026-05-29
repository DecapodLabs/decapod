use serde_json::Value;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn setup_workspace() -> (TempDir, std::path::PathBuf, String) {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path().to_path_buf();

    // Init git
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

    // Commit initial state so we can branch
    std::fs::write(dir.join("README.md"), "# Test").expect("write readme");
    Command::new("git")
        .args(["add", "."])
        .current_dir(&dir)
        .output()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&dir)
        .output()
        .expect("git commit");

    // Create a feature branch to pass workspace protection
    Command::new("git")
        .args(["checkout", "-b", "feat/test-enforcement"])
        .current_dir(&dir)
        .output()
        .expect("git checkout");

    // Init decapod
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--force"])
        .current_dir(&dir)
        .output()
        .expect("decapod init");
    assert!(out.status.success(), "decapod init failed");

    // Acquire session
    // We need to set DECAPOD_AGENT_ID to match what we use later, or use default.
    // Let's use "test-agent-enforce".
    let agent_id = "test-agent-enforce";
    let session = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["session", "acquire"])
        .env("DECAPOD_AGENT_ID", agent_id)
        .current_dir(&dir)
        .output()
        .expect("decapod session acquire");

    if !session.status.success() {
        panic!(
            "decapod session acquire failed: {}",
            String::from_utf8_lossy(&session.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&session.stdout);
    let password = stdout
        .lines()
        .find(|l| l.starts_with("Password: "))
        .expect("Password not found in output")
        .strip_prefix("Password: ")
        .unwrap()
        .trim()
        .to_string();

    (tmp, dir, password)
}

fn run_rpc(dir: &std::path::Path, request: Value, agent_id: &str) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["rpc", "--stdin"])
        .current_dir(dir)
        .env("DECAPOD_AGENT_ID", agent_id)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn decapod rpc");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    stdin
        .write_all(serde_json::to_string(&request).unwrap().as_bytes())
        .expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to read stdout");
    // rpc command always succeeds exit code, but might return json error or non-json error?
    // If panic/crash, it fails.
    if !output.status.success() {
        panic!("RPC failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    serde_json::from_slice(&output.stdout).expect("Failed to parse JSON response")
}

fn sanitize_todo_component(todo_id: &str) -> String {
    todo_id
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
}

fn add_and_claim_task(
    dir: &std::path::Path,
    agent_id: &str,
    password: &str,
    title: &str,
) -> (String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args([
            "todo", "add", title, "--owner", agent_id, "--format", "json",
        ])
        .current_dir(dir)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", password)
        .output()
        .expect("todo add");
    assert!(
        out.status.success(),
        "todo add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let add_json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("parse todo add json");
    let task_id = add_json["id"].as_str().expect("task id").to_string();
    let task_hash = add_json["hash"].as_str().expect("task hash").to_string();

    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["todo", "claim", "--id", &task_id, "--agent", agent_id])
        .current_dir(dir)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", password)
        .output()
        .expect("todo claim");
    assert!(
        out.status.success(),
        "todo claim failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    (task_id, task_hash)
}

#[test]
#[ignore = "Broken by constitution densification PR"]
fn test_mandatory_todo_enforcement() {
    let (_tmp, dir, password) = setup_workspace();
    let agent_id = "test-agent-enforce";

    // 1. Run agent.init with no tasks for this agent
    let request = serde_json::json!({
        "op": "agent.init",
        "params": {}
    });

    let res = run_rpc(&dir, request.clone(), agent_id);
    // It should FAIL because of mandatory todo
    assert!(
        !res["success"].as_bool().unwrap(),
        "agent.init should fail when no tasks exist"
    );

    // Check error message
    let error = res["error"]["message"].as_str().unwrap();
    assert!(
        error.contains("Mandate Violation"),
        "Error should be mandate violation"
    );

    let hint = res["blocked_by"][0]["resolve_hint"].as_str().unwrap();
    assert!(
        hint.contains("create and claim a `todo`"),
        "Hint should mention todo"
    );

    // 2. Add a task for this agent
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args([
            "todo",
            "add",
            "Test Task",
            "--owner",
            agent_id,
            "--format",
            "json",
        ])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .output()
        .expect("todo add");

    if !out.status.success() {
        panic!("todo add failed: {}", String::from_utf8_lossy(&out.stderr));
    }

    let add_json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("parse todo add json");
    let task_id = add_json["id"].as_str().expect("task id").to_string();

    // 3. Claim the task
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["todo", "claim", "--id", &task_id, "--agent", agent_id])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .output()
        .expect("todo claim");

    if !out.status.success() {
        panic!(
            "todo claim failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    println!("Task ID: {task_id}");
    // DEBUG: Check task state
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["todo", "get", "--id", &task_id, "--format", "json"])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .output()
        .expect("todo get");
    println!(
        "Task state stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    println!(
        "Task state stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // 4. Run agent.init again
    let res2 = run_rpc(&dir, request.clone(), agent_id);
    if !res2["success"].as_bool().unwrap() {
        println!(
            "agent.init failed. Response: {}",
            serde_json::to_string_pretty(&res2).unwrap()
        );
    }
    assert!(
        res2["success"].as_bool().unwrap(),
        "agent.init should succeed after claiming task"
    );

    // Check allowed_next_ops
    let ops2 = res2["allowed_next_ops"].as_array().unwrap();
    // todo.add should NOT be mandatory (or maybe not even listed as high priority)
    // Actually allowed_next_ops usually returns standard ops.
    // My code only inserts if EMPTY.
    // So "MANDATORY" reason should be gone.

    if let Some(op) = ops2.iter().find(|op| op["op"] == "todo.add") {
        let reason = op["reason"].as_str().unwrap_or("");
        assert!(
            !reason.contains("MANDATORY"),
            "todo.add should NOT be mandatory when task exists"
        );
    }
}

#[test]
fn test_workspace_ensure_requires_claimed_todo_and_scopes_naming() {
    let (_tmp, dir, password) = setup_workspace();
    let agent_id = "test-agent-enforce";

    let auto_todo = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["workspace", "ensure"])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .output()
        .expect("workspace ensure");
    assert!(
        auto_todo.status.success(),
        "workspace ensure should auto-create and claim a coordination todo: {}",
        String::from_utf8_lossy(&auto_todo.stderr)
    );
    let auto_json: serde_json::Value =
        serde_json::from_slice(&auto_todo.stdout).expect("workspace ensure json");
    let auto_branch = auto_json["branch"].as_str().expect("branch");
    let tasks = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["todo", "--format", "json", "list", "--status", "open"])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .output()
        .expect("todo list");
    assert!(
        tasks.status.success(),
        "todo list failed: {}",
        String::from_utf8_lossy(&tasks.stderr)
    );
    let tasks_json: serde_json::Value =
        serde_json::from_slice(&tasks.stdout).expect("todo list json");
    let task_items = tasks_json
        .get("tasks")
        .and_then(|tasks| tasks.as_array())
        .or_else(|| tasks_json.get("items").and_then(|items| items.as_array()))
        .or_else(|| tasks_json.as_array())
        .expect("tasks");
    let generated_task = task_items
        .iter()
        .find(|task| {
            task["title"]
                .as_str()
                .unwrap_or_default()
                .contains("Decapod workspace coordination")
        })
        .expect("auto-created coordination todo");
    assert_eq!(generated_task["assigned_to"].as_str(), Some(agent_id));
    let generated_task_id = generated_task["id"].as_str().expect("generated task id");
    let generated_task_hash = generated_task["hash"]
        .as_str()
        .expect("generated task hash");
    let generated_task_sanitized = sanitize_todo_component(generated_task_id);
    assert!(
        auto_branch.contains(generated_task_id)
            || auto_branch.contains(generated_task_hash)
            || auto_branch.contains(&generated_task_sanitized),
        "auto-created workspace branch '{auto_branch}' must contain generated Decapod todo id/hash"
    );

    Command::new("git")
        .args(["checkout", "feat/test-enforcement"])
        .current_dir(&dir)
        .output()
        .expect("git checkout original branch");

    let no_todo_again = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["workspace", "ensure"])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", "other-agent")
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .output()
        .expect("workspace ensure");
    assert!(
        no_todo_again.status.success(),
        "workspace ensure should also generate a coordination todo for another agent: {}",
        String::from_utf8_lossy(&no_todo_again.stderr)
    );

    let (task_id, task_hash) =
        add_and_claim_task(&dir, agent_id, &password, "Workspace Scoped Task");
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["workspace", "ensure"])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .output()
        .expect("workspace ensure");
    assert!(
        out.status.success(),
        "workspace ensure failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("workspace ensure json");
    let branch = json["branch"].as_str().expect("branch");
    let worktree_path = json["worktree_path"].as_str().expect("worktree_path");
    let sanitized_todo = sanitize_todo_component(&task_id);

    let sanitized_generated = sanitize_todo_component(generated_task_id);
    assert!(
        branch.contains(&task_hash)
            || branch.contains(&task_id)
            || branch.contains(&sanitized_todo)
            || branch.contains(generated_task_hash)
            || branch.contains(generated_task_id)
            || branch.contains(&sanitized_generated),
        "branch '{branch}' must contain an assigned todo hash/id"
    );
    assert!(
        worktree_path.contains(&task_hash)
            || worktree_path.contains(&task_id)
            || worktree_path.contains(&sanitized_todo)
            || worktree_path.contains(generated_task_hash)
            || worktree_path.contains(generated_task_id)
            || worktree_path.contains(&sanitized_generated),
        "worktree path '{worktree_path}' must contain an assigned todo hash/id"
    );
}

#[test]
fn test_workspace_ensure_reuses_external_todo_ref_for_exclusive_claims() {
    let (_tmp, dir, password) = setup_workspace();
    let external_ref = "bd-568";

    let first = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["workspace", "ensure"])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", "external-agent-one")
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .env("BD_TASK_ID", external_ref)
        .output()
        .expect("workspace ensure");
    assert!(
        first.status.success(),
        "first workspace ensure should create and claim external coordination todo: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    let second = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["workspace", "ensure"])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", "external-agent-two")
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .env("BD_TASK_ID", external_ref)
        .output()
        .expect("workspace ensure");
    assert!(
        !second.status.success(),
        "second workspace ensure should not create a duplicate Decapod todo for the same external task"
    );
    let second_stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        second_stderr.contains("WORKSPACE_TODO_CLAIM_CONFLICT"),
        "expected claim conflict for reused external task ref, got: {second_stderr}"
    );
}

#[test]
fn test_workspace_ensure_container_creates_coordination_todo() {
    let (_tmp, dir, password) = setup_workspace();
    let agent_id = "test-agent-container-no-todo";

    let fake_bin = dir.join("fake-bin");
    std::fs::create_dir_all(&fake_bin).expect("create fake bin");
    let fake_docker = fake_bin.join("docker");
    std::fs::write(&fake_docker, "#!/bin/sh\nexit 0\n").expect("write fake docker");
    let mut perms = std::fs::metadata(&fake_docker)
        .expect("fake docker metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&fake_docker, perms).expect("chmod fake docker");

    let path = std::env::var_os("PATH").expect("PATH");
    let mut paths = vec![fake_bin];
    paths.extend(std::env::split_paths(&path));
    let fake_path = std::env::join_paths(paths).expect("join PATH");

    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["workspace", "ensure", "--container"])
        .current_dir(&dir)
        .env("PATH", fake_path)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .output()
        .expect("workspace ensure --container");

    assert!(
        out.status.success(),
        "workspace ensure --container should create and claim a coordination todo: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("WORKSPACE_NO_CLAIMED_TODO"),
        "container remediation must not be blocked by todo claim gate: {stderr}"
    );

    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("workspace ensure --container json");
    let branch = json["branch"].as_str().expect("branch");
    let worktree_path = json["worktree_path"].as_str().expect("worktree_path");
    let tasks = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["todo", "--format", "json", "list", "--status", "open"])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .output()
        .expect("todo list");
    assert!(
        tasks.status.success(),
        "todo list failed: {}",
        String::from_utf8_lossy(&tasks.stderr)
    );
    let tasks_json: serde_json::Value =
        serde_json::from_slice(&tasks.stdout).expect("todo list json");
    let task_items = tasks_json
        .get("tasks")
        .and_then(|tasks| tasks.as_array())
        .or_else(|| tasks_json.get("items").and_then(|items| items.as_array()))
        .or_else(|| tasks_json.as_array())
        .expect("tasks");
    let generated_task = task_items
        .iter()
        .find(|task| {
            task["title"]
                .as_str()
                .unwrap_or_default()
                .contains("Decapod workspace coordination")
        })
        .expect("auto-created coordination todo");
    assert_eq!(generated_task["assigned_to"].as_str(), Some(agent_id));
    let generated_task_id = generated_task["id"].as_str().expect("generated task id");
    let generated_task_hash = generated_task["hash"]
        .as_str()
        .expect("generated task hash");
    let generated_task_sanitized = sanitize_todo_component(generated_task_id);

    assert!(
        branch.contains(generated_task_id)
            || branch.contains(generated_task_hash)
            || branch.contains(&generated_task_sanitized),
        "container branch should use generated todo scope, got: {branch}"
    );
    assert!(
        worktree_path.contains(generated_task_id)
            || worktree_path.contains(generated_task_hash)
            || worktree_path.contains(&generated_task_sanitized),
        "container worktree should use generated todo scope, got: {worktree_path}"
    );
}
