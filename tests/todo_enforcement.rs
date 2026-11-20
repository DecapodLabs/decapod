use serde_json::Value;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

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

    // Init decapod
    let out = Command::new(resolve_decapod_bin())
        .args(["init", "--force"])
        .current_dir(&dir)
        .output()
        .expect("decapod init");
    assert!(out.status.success(), "decapod init failed");

    // Commit the initialized control plane on the protected base before
    // creating the feature branch. Workspace ensure intentionally starts new
    // worktrees from that base, so the fixture must model a repository whose
    // Decapod config and release-bound surfaces are available there.
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(&dir)
        .output()
        .expect("git add decapod init");
    Command::new("git")
        .args(["commit", "-m", "decapod init"])
        .current_dir(&dir)
        .output()
        .expect("git commit decapod init");
    Command::new("git")
        .args(["checkout", "-b", "feat/test-enforcement"])
        .current_dir(&dir)
        .output()
        .expect("git checkout");

    // Acquire session
    // We need to set DECAPOD_AGENT_ID to match what we use later, or use default.
    // Let's use "test-agent-enforce".
    let agent_id = "test-agent-enforce";
    let session = Command::new(resolve_decapod_bin())
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
    let mut child = Command::new(resolve_decapod_bin())
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
    let out = Command::new(resolve_decapod_bin())
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

    let out = Command::new(resolve_decapod_bin())
        .args(["todo", "claim", "--id", &task_id, "--agent", agent_id])
        .current_dir(dir)
        // Host task-binding proof must not depend on a real Docker daemon.
        .env("DECAPOD_CLAIM_AUTORUN", "0")
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
fn test_agent_init_and_todo_claim_lifecycle() {
    let (_tmp, dir, password) = setup_workspace();
    let agent_id = "test-agent-enforce";

    // 1. Run agent.init with no tasks for this agent
    let request = serde_json::json!({
        "op": "agent.init",
        "params": {}
    });

    let res = run_rpc(&dir, request.clone(), agent_id);
    // Orientation is allowed before a task is claimed; workspace mutation is
    // the boundary that requires a claimed task.
    assert!(
        res["success"].as_bool().unwrap(),
        "agent.init should succeed before task claim"
    );

    // 2. Add a task for this agent
    let out = Command::new(resolve_decapod_bin())
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
    let out = Command::new(resolve_decapod_bin())
        .args(["todo", "claim", "--id", &task_id, "--agent", agent_id])
        .current_dir(&dir)
        .env("DECAPOD_CLAIM_AUTORUN", "0")
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
    let out = Command::new(resolve_decapod_bin())
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

    let auto_todo = Command::new(resolve_decapod_bin())
        .args(["workspace", "ensure"])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .env("DECAPOD_TASK_ID", "")
        .env("DECAPOD_EXTERNAL_TASK_ID", "")
        .env("BD_TASK_ID", "")
        .env("BEADS_TASK_ID", "")
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
    let tasks = Command::new(resolve_decapod_bin())
        .args(["todo", "--format", "json", "list", "--status", "open"])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .env("DECAPOD_TASK_ID", "")
        .env("DECAPOD_EXTERNAL_TASK_ID", "")
        .env("BD_TASK_ID", "")
        .env("BEADS_TASK_ID", "")
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

    // Independent tasks may coordinate from the same logical repository dir.
    // Authenticate the second actor independently: the primary actor's session
    // password must not be usable as another agent's credential.
    let other_session = Command::new(resolve_decapod_bin())
        .args(["session", "acquire"])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", "other-agent")
        .output()
        .expect("acquire other-agent session");
    assert!(
        other_session.status.success(),
        "other-agent session acquisition failed: {}",
        String::from_utf8_lossy(&other_session.stderr)
    );
    let other_output = String::from_utf8_lossy(&other_session.stdout);
    let other_password = other_output
        .lines()
        .find_map(|line| line.strip_prefix("Password: "))
        .expect("other-agent session password")
        .trim()
        .to_string();

    // Only actual writable checkouts require exclusive custody.
    let second_agent = Command::new(resolve_decapod_bin())
        .args(["workspace", "ensure"])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", "other-agent")
        .env("DECAPOD_SESSION_PASSWORD", &other_password)
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .output()
        .expect("workspace ensure");
    assert!(
        second_agent.status.success(),
        "independent agent must obtain a distinct managed worktree: {}",
        String::from_utf8_lossy(&second_agent.stderr)
    );
    let second: serde_json::Value =
        serde_json::from_slice(&second_agent.stdout).expect("second workspace JSON");
    assert_ne!(
        second["worktree_path"], auto_json["worktree_path"],
        "independent agents must not bind one writable checkout"
    );

    let (task_id, task_hash) =
        add_and_claim_task(&dir, agent_id, &password, "Workspace Scoped Task");
    let out = Command::new(resolve_decapod_bin())
        .args(["workspace", "ensure"])
        .current_dir(&dir)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .env("DECAPOD_TASK_ID", "")
        .env("DECAPOD_EXTERNAL_TASK_ID", "")
        .env("BD_TASK_ID", "")
        .env("BEADS_TASK_ID", "")
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

    // Recent ULIDs share a six-character prefix. The generated branch must
    // carry the exact primary task, not ask initialization to guess by hash.
    let mut assigned_ids = [generated_task_id, task_id.as_str()];
    assigned_ids.sort_unstable();
    let primary_id = assigned_ids[0];
    assert!(
        branch.contains(primary_id),
        "missing exact task binding: {branch}"
    );
    let workspace = Path::new(worktree_path);
    let plan = decapod::core::governance_document::read_section(workspace, "plan")
        .unwrap()
        .unwrap();
    assert_eq!(plan["todo_ids"], serde_json::json!([primary_id]));
    let trajectory = decapod::core::governance_document::read_section(workspace, "trajectory")
        .unwrap()
        .unwrap();
    assert_eq!(trajectory["task_id"], primary_id);
    let journal = workspace.join(decapod::core::governance_document::GOVERNANCE_PATH);
    let before = std::fs::read(&journal).unwrap();
    add_and_claim_task(
        &dir,
        agent_id,
        &password,
        "Another independently claimed task",
    );
    let repeated = Command::new(resolve_decapod_bin())
        .args(["workspace", "ensure"])
        .current_dir(workspace)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .output()
        .unwrap();
    assert!(
        repeated.status.success(),
        "repeat must retain its exact primary task: {}",
        String::from_utf8_lossy(&repeated.stderr)
    );
    assert_eq!(std::fs::read(&journal).unwrap(), before);

    let renamed = Command::new("git")
        .args(["branch", "-m", "renamed-current-work"])
        .current_dir(workspace)
        .output()
        .unwrap();
    assert!(renamed.status.success());
    let resumed = Command::new(resolve_decapod_bin())
        .args(["rpc", "--op", "agent.init"])
        .current_dir(workspace)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .output()
        .unwrap();
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    assert_eq!(std::fs::read(&journal).unwrap(), before);
}

#[test]
fn test_workspace_ensure_reuses_external_todo_ref_for_exclusive_claims() {
    let (_tmp, dir, password) = setup_workspace();
    let external_ref = "bd-568";

    let first = Command::new(resolve_decapod_bin())
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

    let second = Command::new(resolve_decapod_bin())
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
    let runtime_log = dir.join("runtime-args.log");
    std::fs::create_dir_all(&fake_bin).expect("create fake bin");
    for runtime in ["docker", "podman"] {
        let fake_runtime = fake_bin.join(runtime);
        std::fs::write(
            &fake_runtime,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 0\n",
                runtime_log.display()
            ),
        )
        .expect("write fake runtime");
        let mut perms = std::fs::metadata(&fake_runtime)
            .expect("fake runtime metadata")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_runtime, perms).expect("chmod fake runtime");
    }

    let path = std::env::var_os("PATH").expect("PATH");
    let mut paths = vec![fake_bin];
    paths.extend(std::env::split_paths(&path));
    let fake_path = std::env::join_paths(paths).expect("join PATH");

    let out = Command::new(resolve_decapod_bin())
        .args(["workspace", "ensure", "--container"])
        .current_dir(&dir)
        .env("PATH", fake_path)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .env("DECAPOD_CONTAINER_LOCAL_BINARY_FALLBACK", "1")
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
    let tasks = Command::new(resolve_decapod_bin())
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
    let runtime_args = std::fs::read_to_string(&runtime_log).expect("runtime args log");
    assert!(
        runtime_args.contains("--build-arg DECAPOD_IMAGE=debian:bookworm-slim")
            && runtime_args.contains("--build-arg DECAPOD_USE_LOCAL_BINARY=1"),
        "workspace container proof should pass local binary fallback build args, got: {runtime_args}"
    );
}

#[test]
fn test_issue_1737_unknown_claim_rejected_and_legacy_unknown_recovery_preserves_custody() {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path().join("repo");
    let home = tmp.path().join("home");
    let xdg_cfg = tmp.path().join("xdg-config");
    let xdg_data = tmp.path().join("xdg-data");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&xdg_cfg).unwrap();
    std::fs::create_dir_all(&xdg_data).unwrap();

    let git = |cwd: &Path, args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("HOME", &home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .expect("git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };

    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.name", "Fixture"]);
    git(&dir, &["config", "user.email", "fixture@example.invalid"]);
    git(&dir, &["commit", "--allow-empty", "-m", "Fixture baseline"]);

    fn copy_dir_recursive(src: &Path, dst: &Path) {
        std::fs::create_dir_all(dst).unwrap();
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let ty = entry.file_type().unwrap();
            let target = dst.join(entry.file_name());
            if ty.is_dir() {
                copy_dir_recursive(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    let run_isolated =
        |cwd: &Path, args: &[&str], agent: Option<&str>, stdin_data: Option<&str>| {
            let mut cmd = Command::new(resolve_decapod_bin());
            cmd.current_dir(cwd)
                .args(args)
                .env("HOME", &home)
                .env("XDG_CONFIG_HOME", &xdg_cfg)
                .env("XDG_DATA_HOME", &xdg_data)
                .env("DECAPOD_CLAIM_AUTORUN", "0")
                .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1")
                .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
                .env_remove("DECAPOD_AGENT_ID")
                .env_remove("DECAPOD_SESSION_PASSWORD")
                .env_remove("DECAPOD_TASK_ID")
                .env_remove("DECAPOD_EXTERNAL_TASK_ID")
                .env_remove("BD_TASK_ID")
                .env_remove("BEADS_TASK_ID");
            if let Some(a) = agent {
                cmd.env("DECAPOD_AGENT_ID", a);
            }
            if let Some(input) = stdin_data {
                cmd.stdin(Stdio::piped());
                cmd.stdout(Stdio::piped());
                cmd.stderr(Stdio::piped());
                let mut child = cmd.spawn().expect("spawn decapod");
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(input.as_bytes())
                    .unwrap();
                child.wait_with_output().unwrap()
            } else {
                cmd.stdin(Stdio::null()).output().unwrap()
            }
        };

    // Eval, init --proof --no-ci --isolation-mode worktree, and docs ingest succeed without DECAPOD_AGENT_ID.
    let eval_out = run_isolated(
        &dir,
        &["eval", "--stdin", "--format", "json"],
        None,
        Some("Reproduce missing authoring identity"),
    );
    assert!(
        eval_out.status.success(),
        "eval failed: {}",
        String::from_utf8_lossy(&eval_out.stderr)
    );
    let init_out = run_isolated(
        &dir,
        &["init", "--proof", "--no-ci", "--isolation-mode", "worktree"],
        None,
        None,
    );
    assert!(
        init_out.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init_out.stderr)
    );
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-m", "Accept Decapod init baseline"]);
    let docs_out = run_isolated(&dir, &["docs", "ingest"], None, None);
    assert!(
        docs_out.status.success(),
        "docs ingest failed: {}",
        String::from_utf8_lossy(&docs_out.stderr)
    );
    let list_out = run_isolated(&dir, &["todo", "list", "--format", "json"], None, None);
    assert!(list_out.status.success());

    // Clone the initialized pre-journal baseline before `todo add` so legacy
    // 'unknown' recovery checks do not pay for a second `decapod init --proof`.
    let legacy_tmp = tempfile::tempdir().unwrap();
    let legacy_dir = legacy_tmp.path().to_path_buf();
    copy_dir_recursive(&dir, &legacy_dir);

    // todo add works when DECAPOD_AGENT_ID is unset, as do read-only todo list/get.
    let add_out = run_isolated(
        &dir,
        &["todo", "add", "--format", "json", "Reproduce #1737"],
        None,
        None,
    );
    assert!(
        add_out.status.success(),
        "todo add failed: {}",
        String::from_utf8_lossy(&add_out.stderr)
    );
    let add_json: Value = serde_json::from_slice(&add_out.stdout).expect("parse todo add");
    let task_id = add_json["id"].as_str().expect("task id").to_string();

    // Claiming as unset / "--agent unknown" must fail closed with AUTHORING_ACTOR_REQUIRED.
    for (args, agent_env) in [
        (
            vec!["todo", "claim", "--id", &task_id, "--format", "json"],
            None,
        ),
        (
            vec![
                "todo", "claim", "--id", &task_id, "--agent", "unknown", "--format", "json",
            ],
            Some("actor-a"),
        ),
    ] {
        let out = run_isolated(&dir, &args, agent_env, None);
        assert!(
            !out.status.success(),
            "todo claim must fail closed for {args:?} env={agent_env:?}: stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            combined.contains("AUTHORING_ACTOR_REQUIRED")
                && combined.contains("DECAPOD_AGENT_ID=<actor>"),
            "expected actionable AUTHORING_ACTOR_REQUIRED error, got: {combined}"
        );
    }

    let get_after_denied: Value = serde_json::from_slice(
        &run_isolated(
            &dir,
            &["todo", "get", "--id", &task_id, "--format", "json"],
            None,
            None,
        )
        .stdout,
    )
    .unwrap();
    assert_eq!(get_after_denied["item"]["assigned_to"], "");

    // Branching/workspace flows (CLI and RPC) must also fail closed without mutating todo/worktree state.
    let unknown_branch = format!("agent/unknown/{task_id}");
    for (args, agent_env, stdin_payload) in [
        (
            vec!["workspace", "ensure", "--branch", &unknown_branch],
            None,
            None,
        ),
        (
            vec!["rpc", "--stdin"],
            None,
            Some(format!(
                r#"{{"op":"workspace.ensure","params":{{"branch":"{unknown_branch}"}}}}"#
            )),
        ),
    ] {
        let out = run_isolated(&dir, &args, agent_env, stdin_payload.as_deref());
        assert!(
            !out.status.success(),
            "workspace ensure must fail closed for {args:?} env={agent_env:?}"
        );
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            combined.contains("AUTHORING_ACTOR_REQUIRED")
                && combined.contains("DECAPOD_AGENT_ID=<actor>"),
            "expected AUTHORING_ACTOR_REQUIRED on workspace ensure, got: {combined}"
        );
    }

    let list_after_ws: Value = serde_json::from_slice(
        &run_isolated(&dir, &["todo", "list", "--format", "json"], None, None).stdout,
    )
    .unwrap();
    assert_eq!(
        list_after_ws["items"].as_array().unwrap().len(),
        1,
        "rejected workspace ensure must not create coordination todos"
    );
    assert!(
        !dir.join(".decapod/workspaces").exists()
            || std::fs::read_dir(dir.join(".decapod/workspaces"))
                .unwrap()
                .next()
                .is_none(),
        "rejected workspace ensure must not create worktrees"
    );

    // Seed pre-existing 'unknown' claims into the cloned pre-journal workspace
    // (simulating tasks created by older binaries) and verify narrow, fail-closed
    // recovery by a real actor, custody preservation when real authoring_bindings
    // exist, and real-actor cross-owner isolation.
    let legacy_task1 = "code_legacy_unknown_1";
    let legacy_bound = "code_legacy_unknown_bound";
    {
        let db = decapod::core::db::Connection::open(legacy_dir.join(".decapod/data/decapod.db"))
            .expect("open legacy decapod.db");
        for (id, title) in [
            (legacy_task1, "Legacy unknown task 1"),
            (legacy_bound, "Legacy unknown with real binding"),
        ] {
            db.execute(
                "INSERT INTO tasks(
                    id, hash, title, status, priority, category, dir_path, scope, assigned_to,
                    assigned_at, lease_expires_at, lease_generation, lease_lifecycle,
                    owner, revision, created_at, updated_at
                 ) VALUES(
                    ?1, ?2, ?3, 'open', 'medium', 'code', ?4, 'core', 'unknown',
                    '100Z', '4102444800Z', 1, 'claimed', 'unknown', 1, '100Z', '100Z'
                 )",
                decapod::core::db::params![
                    id,
                    format!("hash_{id}"),
                    title,
                    legacy_dir.to_string_lossy().to_string()
                ],
            )
            .unwrap();
            db.execute(
                "INSERT INTO task_owners(id, task_id, agent_id, claimed_at, claim_type)
                 VALUES(?1, ?2, 'unknown', '100Z', 'primary')",
                decapod::core::db::params![format!("own_{id}"), id],
            )
            .unwrap();
        }
        db.execute(
            "INSERT INTO authoring_bindings(
                worktree_id, task_id, agent_id, lease_generation, branch, repository_id, updated_at
             ) VALUES(
                'wt_real_1', ?1, 'other-real-actor', 1,
                'agent/other-real-actor/legacy_bound', 'repo-legacy', '100Z'
             )",
            [legacy_bound],
        )
        .unwrap();
    }
    let ckpt_out = run_isolated(
        &legacy_dir,
        &["todo", "journal", "checkpoint"],
        Some("actor-a"),
        None,
    );
    assert!(
        ckpt_out.status.success(),
        "todo journal checkpoint failed: {}",
        String::from_utf8_lossy(&ckpt_out.stderr)
    );

    // Real actor-a recovers legacy_task1 via todo claim, then obtains an authoring workspace.
    let adopt_claim = run_isolated(
        &legacy_dir,
        &["todo", "claim", "--id", legacy_task1, "--format", "json"],
        Some("actor-a"),
        None,
    );
    assert!(
        adopt_claim.status.success(),
        "actor-a recovery claim failed: {}",
        String::from_utf8_lossy(&adopt_claim.stderr)
    );
    let adopt_json: Value = serde_json::from_slice(&adopt_claim.stdout).unwrap();
    assert_eq!(adopt_json["status"], "ok");
    assert_eq!(adopt_json["result"]["assigned_to"], "actor-a");
    assert_eq!(adopt_json["result"]["reclaimed_from"], "unknown");

    let branch_a = format!("agent/actor-a/{legacy_task1}");
    let ws_a = run_isolated(
        &legacy_dir,
        &["workspace", "ensure", "--branch", &branch_a],
        Some("actor-a"),
        None,
    );
    assert!(
        ws_a.status.success(),
        "actor-a workspace ensure failed: {}",
        String::from_utf8_lossy(&ws_a.stderr)
    );
    let ws_a_json: Value = serde_json::from_slice(&ws_a.stdout).unwrap();
    assert_eq!(ws_a_json["can_work"], true);

    // Real actor-b is STILL blocked from claiming or branching actor-a's task.
    let claim_b = run_isolated(
        &legacy_dir,
        &["todo", "claim", "--id", legacy_task1, "--format", "json"],
        Some("actor-b"),
        None,
    );
    assert!(claim_b.status.success());
    let claim_b_json: Value = serde_json::from_slice(&claim_b.stdout).unwrap();
    assert_eq!(claim_b_json["status"], "conflict");
    assert_eq!(claim_b_json["result"]["assigned_to"], "actor-a");

    let branch_b = format!("agent/actor-b/{legacy_task1}");
    let ws_b = run_isolated(
        &legacy_dir,
        &["workspace", "ensure", "--branch", &branch_b],
        Some("actor-b"),
        None,
    );
    assert!(
        !ws_b.status.success(),
        "actor-b workspace ensure on actor-a task must fail closed"
    );
    assert!(String::from_utf8_lossy(&ws_b.stderr).contains("WORKSPACE_BRANCH_TODO_CLAIM_CONFLICT"));

    // Pre-existing 'unknown' task WITH active real-actor authoring custody stays blocked.
    let bound_claim = run_isolated(
        &legacy_dir,
        &["todo", "claim", "--id", legacy_bound, "--format", "json"],
        Some("actor-a"),
        None,
    );
    assert!(bound_claim.status.success());
    let bound_json: Value = serde_json::from_slice(&bound_claim.stdout).unwrap();
    assert_eq!(bound_json["status"], "conflict");
}
