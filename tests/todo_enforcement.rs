use std::process::{Command, Stdio};
use std::io::Write;
use serde_json::Value;
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
        panic!("decapod session acquire failed: {}", String::from_utf8_lossy(&session.stderr));
    }
    
    let stdout = String::from_utf8_lossy(&session.stdout);
    let password = stdout.lines()
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
    stdin.write_all(serde_json::to_string(&request).unwrap().as_bytes()).expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to read stdout");
    // rpc command always succeeds exit code, but might return json error or non-json error?
    // If panic/crash, it fails.
    if !output.status.success() {
        panic!("RPC failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    serde_json::from_slice(&output.stdout).expect("Failed to parse JSON response")
}

#[test]
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
    assert!(!res["success"].as_bool().unwrap(), "agent.init should fail when no tasks exist");
    
    // Check error message
    let error = res["error"]["message"].as_str().unwrap();
    assert!(error.contains("Mandate Violation"), "Error should be mandate violation");
    
    let hint = res["blocked_by"][0]["resolve_hint"].as_str().unwrap();
    assert!(hint.contains("create and claim a `todo`"), "Hint should mention todo");

    // 2. Add a task for this agent
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["todo", "add", "Test Task", "--owner", agent_id, "--format", "json"])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .output()
        .expect("todo add");
    
    if !out.status.success() {
        panic!("todo add failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    
    let add_json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse todo add json");
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
        panic!("todo claim failed: {}", String::from_utf8_lossy(&out.stderr));
    }

    println!("Task ID: {}", task_id);
    // DEBUG: Check task state
    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["todo", "get", "--id", &task_id, "--format", "json"])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .output()
        .expect("todo get");
    println!("Task state stdout: {}", String::from_utf8_lossy(&out.stdout));
    println!("Task state stderr: {}", String::from_utf8_lossy(&out.stderr));

    // 4. Run agent.init again
    let res2 = run_rpc(&dir, request.clone(), agent_id);
    if !res2["success"].as_bool().unwrap() {
        println!("agent.init failed. Response: {}", serde_json::to_string_pretty(&res2).unwrap());
    }
    assert!(res2["success"].as_bool().unwrap(), "agent.init should succeed after claiming task");
    
    // Check allowed_next_ops
    let ops2 = res2["allowed_next_ops"].as_array().unwrap();
    // todo.add should NOT be mandatory (or maybe not even listed as high priority)
    // Actually allowed_next_ops usually returns standard ops.
    // My code only inserts if EMPTY.
    // So "MANDATORY" reason should be gone.
    
    if let Some(op) = ops2.iter().find(|op| op["op"] == "todo.add") {
        let reason = op["reason"].as_str().unwrap_or("");
        assert!(!reason.contains("MANDATORY"), "todo.add should NOT be mandatory when task exists");
    }
}
