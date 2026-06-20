use rusqlite::Connection;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

fn run_cmd(repo_root: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let exe = env!("CARGO_BIN_EXE_decapod");
    let mut cmd = Command::new(exe);
    cmd.current_dir(repo_root).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to run decapod {args:?}: {e}"))
}

fn init_git_repo(path: &Path) {
    Command::new("git")
        .current_dir(path)
        .args(["init"])
        .output()
        .expect("failed to init git repo");
    Command::new("git")
        .current_dir(path)
        .args(["config", "user.email", "test@test.com"])
        .output()
        .expect("failed to config git email");
    Command::new("git")
        .current_dir(path)
        .args(["config", "user.name", "test"])
        .output()
        .expect("failed to config git name");
    std::fs::write(path.join(".gitkeep"), "").ok();
    Command::new("git")
        .current_dir(path)
        .args(["add", "."])
        .output()
        .expect("failed to git add");
    Command::new("git")
        .current_dir(path)
        .args(["commit", "-m", "initial"])
        .output()
        .expect("failed to git commit");
}

#[test]
fn test_verify_deadlock_prevention() {
    let tmp = tempdir().unwrap();
    let main_repo = tmp.path().join("main_repo");
    std::fs::create_dir_all(&main_repo).unwrap();

    init_git_repo(&main_repo);

    // Initialize decapod in main repo
    let init = run_cmd(&main_repo, &["init", "--dir", "."], &[]);
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    // Disable container workspaces in config.toml for the test to avoid container_workspace_required blockers
    let config_path = main_repo.join(".decapod").join("config.toml");
    let mut config_content = std::fs::read_to_string(&config_path).unwrap();
    config_content = config_content.replace(
        "container_workspaces = true",
        "container_workspaces = false",
    );
    std::fs::write(&config_path, config_content).unwrap();

    // Commit the initialized files (including .decapod, AGENTS.md, etc.) so that they exist in worktrees
    Command::new("git")
        .current_dir(&main_repo)
        .args(["add", "-A"])
        .output()
        .expect("failed to git add all files");
    Command::new("git")
        .current_dir(&main_repo)
        .args(["commit", "-m", "add decapod init and entrypoints metadata"])
        .output()
        .expect("failed to git commit");

    // Create a worktree under .decapod/workspaces/test-wt
    let wt_path = main_repo
        .join(".decapod")
        .join("workspaces")
        .join("test-wt");
    std::fs::create_dir_all(wt_path.parent().unwrap()).unwrap();

    let wt_add = Command::new("git")
        .current_dir(&main_repo)
        .args([
            "worktree",
            "add",
            "-b",
            "agent/test-wt",
            wt_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to create git worktree");
    assert!(
        wt_add.status.success(),
        "git worktree add failed: {}",
        String::from_utf8_lossy(&wt_add.stderr)
    );

    // Acquire session in worktree repo
    let session = run_cmd(&wt_path, &["session", "acquire"], &[]);
    assert!(
        session.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&session.stderr)
    );

    // Add task 1
    let add_1 = run_cmd(
        &wt_path,
        &["todo", "add", "Task 1", "--dir", ".", "--format", "json"],
        &[],
    );
    assert!(
        add_1.status.success(),
        "add 1 failed: {}",
        String::from_utf8_lossy(&add_1.stderr)
    );
    let stdout_1 = String::from_utf8_lossy(&add_1.stdout);
    let start_1 = stdout_1.find('{').unwrap();
    let add_1_json: serde_json::Value = serde_json::from_str(&stdout_1[start_1..]).unwrap();
    let todo_id_1 = add_1_json["id"].as_str().unwrap().to_string();

    // Add task 2
    let add_2 = run_cmd(
        &wt_path,
        &["todo", "add", "Task 2", "--dir", ".", "--format", "json"],
        &[],
    );
    assert!(
        add_2.status.success(),
        "add 2 failed: {}",
        String::from_utf8_lossy(&add_2.stderr)
    );
    let stdout_2 = String::from_utf8_lossy(&add_2.stdout);
    let start_2 = stdout_2.find('{').unwrap();
    let add_2_json: serde_json::Value = serde_json::from_str(&stdout_2[start_2..]).unwrap();
    let todo_id_2 = add_2_json["id"].as_str().unwrap().to_string();

    // Mark both tasks as done so their status is 'done'
    let done_1 = run_cmd(&wt_path, &["todo", "done", "--id", &todo_id_1], &[]);
    assert!(
        done_1.status.success(),
        "done 1 failed: {}",
        String::from_utf8_lossy(&done_1.stderr)
    );
    let done_2 = run_cmd(&wt_path, &["todo", "done", "--id", &todo_id_2], &[]);
    assert!(
        done_2.status.success(),
        "done 2 failed: {}",
        String::from_utf8_lossy(&done_2.stderr)
    );

    // Create a dummy plan.json so NEEDS_PLAN_APPROVAL check passes
    let plan = serde_json::json!({
        "schema_version": "1.0.0",
        "title": "Test Plan",
        "intent": "Test Intent",
        "state": "APPROVED",
        "todo_ids": [todo_id_1.clone(), todo_id_2.clone()],
        "proof_hooks": [],
        "unknowns": [],
        "human_questions": [],
        "stop_conditions": [],
        "unresolved_contradictions": [],
        "deferred_questions": [],
        "constraints": {
            "forbidden_paths": []
        },
        "updated_at": "2026-06-20T00:00:00Z"
    });
    let plan_str = serde_json::to_string_pretty(&plan).unwrap();

    let plan_dir_wt = wt_path.join(".decapod").join("governance");
    std::fs::create_dir_all(&plan_dir_wt).unwrap();
    std::fs::write(plan_dir_wt.join("plan.json"), &plan_str).unwrap();

    let plan_dir_main = main_repo.join(".decapod").join("governance");
    std::fs::create_dir_all(&plan_dir_main).unwrap();
    std::fs::write(plan_dir_main.join("plan.json"), &plan_str).unwrap();

    // Manually force both tasks to have failed verification status in the SQLite DB
    let db_path = main_repo.join(".decapod/data/todo.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO task_verification(todo_id, proof_plan, verification_artifacts, last_verified_at, last_verified_status, last_verified_notes, verification_policy_days, updated_at)
         VALUES(?1, '[\"validate_passes\"]', '{}', '2026-06-20T00:00:00Z', 'fail', 'Stale failure', 90, '2026-06-20T00:00:00Z')
         ON CONFLICT(todo_id) DO UPDATE SET last_verified_status = 'fail'",
        [&todo_id_1]
    ).unwrap();
    conn.execute(
        "INSERT INTO task_verification(todo_id, proof_plan, verification_artifacts, last_verified_at, last_verified_status, last_verified_notes, verification_policy_days, updated_at)
         VALUES(?1, '[\"validate_passes\"]', '{}', '2026-06-20T00:00:00Z', 'fail', 'Stale failure', 90, '2026-06-20T00:00:00Z')
         ON CONFLICT(todo_id) DO UPDATE SET last_verified_status = 'fail'",
        [&todo_id_2]
    ).unwrap();

    // 1. Standard validate should FAIL due to unverified done tasks
    let val_normal = run_cmd(&wt_path, &["validate"], &[]);
    let val_normal_combined = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&val_normal.stdout),
        String::from_utf8_lossy(&val_normal.stderr)
    );
    assert!(
        !val_normal.status.success(),
        "validate should fail when there are failed task verifications: {}",
        val_normal_combined
    );
    assert!(
        val_normal_combined.contains("PROOF_HOOK_FAILED"),
        "Expected PROOF_HOOK_FAILED error in: {}",
        val_normal_combined
    );

    // 2. Validate with skip flag should PASS!
    let val_skip = run_cmd(&wt_path, &["validate", "--skip-todo-verification"], &[]);
    assert!(
        val_skip.status.success(),
        "validate --skip-todo-verification failed: stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&val_skip.stdout),
        String::from_utf8_lossy(&val_skip.stderr)
    );
}
