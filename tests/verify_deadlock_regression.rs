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
    let db_path = main_repo.join(".decapod/data/decapod.db");
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

    // 2. Prune verification of task 1 (reverts status to 'open' and deletes verification record)
    let prune_1 = run_cmd(&wt_path, &["qa", "verify", "prune", &todo_id_1], &[]);
    assert!(
        prune_1.status.success(),
        "prune 1 failed: {}",
        String::from_utf8_lossy(&prune_1.stderr)
    );

    // 3. Prune verification of task 2
    let prune_2 = run_cmd(&wt_path, &["qa", "verify", "prune", &todo_id_2], &[]);
    assert!(
        prune_2.status.success(),
        "prune 2 failed: {}",
        String::from_utf8_lossy(&prune_2.stderr)
    );

    // 4. Standard validate should now PASS since both tasks are 'open' again!
    let val_post_prune = run_cmd(&wt_path, &["validate"], &[]);
    assert!(
        val_post_prune.status.success(),
        "validate failed post-prune: stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&val_post_prune.stdout),
        String::from_utf8_lossy(&val_post_prune.stderr)
    );

    // 5. Mark task 1 done again
    let done_again_1 = run_cmd(&wt_path, &["todo", "done", "--id", &todo_id_1], &[]);
    assert!(
        done_again_1.status.success(),
        "done again 1 failed: {}",
        String::from_utf8_lossy(&done_again_1.stderr)
    );

    // 6. Regenerate verification baseline for task 1
    let regen_1 = run_cmd(&wt_path, &["qa", "verify", "regen", &todo_id_1], &[]);
    assert!(
        regen_1.status.success(),
        "regen 1 failed: stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&regen_1.stdout),
        String::from_utf8_lossy(&regen_1.stderr)
    );

    // 7. Verify task 1 is indeed in 'pass' verification state now
    let verify_status_1 = run_cmd(&wt_path, &["qa", "verify", "todo", &todo_id_1], &[]);
    assert!(
        verify_status_1.status.success(),
        "verify status 1 check failed: stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify_status_1.stdout),
        String::from_utf8_lossy(&verify_status_1.stderr)
    );
}

#[test]
fn test_verify_replay_excludes_target_from_missing_plan_gate() {
    let tmp = tempdir().unwrap();
    let main_repo = tmp.path().join("main_repo");
    std::fs::create_dir_all(&main_repo).unwrap();

    init_git_repo(&main_repo);

    let init = run_cmd(&main_repo, &["init", "--dir", "."], &[]);
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let config_path = main_repo.join(".decapod").join("config.toml");
    let mut config_content = std::fs::read_to_string(&config_path).unwrap();
    config_content = config_content.replace(
        "container_workspaces = true",
        "container_workspaces = false",
    );
    std::fs::write(&config_path, config_content).unwrap();

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

    let wt_path = main_repo
        .join(".decapod")
        .join("workspaces")
        .join("test-wt-missing-plan");
    std::fs::create_dir_all(wt_path.parent().unwrap()).unwrap();

    let wt_add = Command::new("git")
        .current_dir(&main_repo)
        .args([
            "worktree",
            "add",
            "-b",
            "agent/test-wt-missing-plan",
            wt_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to create git worktree");
    assert!(
        wt_add.status.success(),
        "git worktree add failed: {}",
        String::from_utf8_lossy(&wt_add.stderr)
    );

    let session = run_cmd(&wt_path, &["session", "acquire"], &[]);
    assert!(
        session.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&session.stderr)
    );

    let add = run_cmd(
        &wt_path,
        &[
            "todo",
            "add",
            "Task without governed plan",
            "--dir",
            ".",
            "--format",
            "json",
        ],
        &[],
    );
    assert!(
        add.status.success(),
        "add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let stdout = String::from_utf8_lossy(&add.stdout);
    let start = stdout.find('{').unwrap();
    let add_json: serde_json::Value = serde_json::from_str(&stdout[start..]).unwrap();
    let todo_id = add_json["id"].as_str().unwrap().to_string();

    let done = run_cmd(&wt_path, &["todo", "done", "--id", &todo_id], &[]);
    assert!(
        done.status.success(),
        "done failed: {}",
        String::from_utf8_lossy(&done.stderr)
    );

    let regen = run_cmd(&wt_path, &["qa", "verify", "regen", &todo_id], &[]);
    assert!(
        regen.status.success(),
        "regen failed: stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&regen.stdout),
        String::from_utf8_lossy(&regen.stderr)
    );

    let verify = run_cmd(&wt_path, &["qa", "verify", "todo", &todo_id], &[]);
    assert!(
        verify.status.success(),
        "verify should not fail the target TODO on the missing-plan gate: stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );

    let validate = run_cmd(&wt_path, &["validate"], &[]);
    let validate_combined = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&validate.stdout),
        String::from_utf8_lossy(&validate.stderr)
    );
    assert!(
        !validate.status.success(),
        "regular validation should still require a governed plan after completion"
    );
    assert!(
        validate_combined.contains("NEEDS_PLAN_APPROVAL"),
        "expected regular validation to keep the missing-plan gate: {}",
        validate_combined
    );
}
