use decapod::core::store::{Store, StoreKind};
use decapod::core::todo::{
    add_task, claim_task, initialize_todo_db, update_status, ClaimMode, TodoCommand,
};
use decapod::core::workspace;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn git_init(dir: &Path) {
    run_git_cmd(dir, &["init"]);
    run_git_cmd(dir, &["config", "user.email", "test@test.com"]);
    run_git_cmd(dir, &["config", "user.name", "Test"]);
}

fn git_commit(dir: &Path, msg: &str) {
    fs::write(dir.join(".gitkeep"), "").ok();
    run_git_cmd(dir, &["add", "-A"]);
    run_git_cmd(dir, &["commit", "--allow-empty", "-m", msg]);
}

fn run_git_cmd(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to execute git");
    if !output.status.success() {
        panic!(
            "git command failed: git {}\nstdout: {}\nstderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn test_workspace_prune() {
    let tmp = tempdir().expect("tempdir");
    let main_root = tmp.path().join("main_repo");
    fs::create_dir_all(&main_root).expect("create main_repo");

    git_init(&main_root);
    git_commit(&main_root, "init");

    // Initialize todo db in main_root/.decapod/data
    let store_root = main_root.join(".decapod").join("data");
    fs::create_dir_all(&store_root).expect("create data dir");
    initialize_todo_db(&store_root).expect("init todo db");

    let store = Store {
        kind: StoreKind::Repo,
        root: store_root.clone(),
    };

    // 1. Task A (Active / Open & Claimed)
    let task_a_res = add_task(
        &store_root,
        &TodoCommand::Add {
            title: "Task A".to_string(),
            description: "".to_string(),
            tags: "".to_string(),
            owner: "test-agent".to_string(),
            due: None,
            r#ref: "".to_string(),
            scope: "".to_string(),
            dir: Some(main_root.to_string_lossy().to_string()),
            priority: "medium".to_string(),
            depends_on: "".to_string(),
            blocks: "".to_string(),
            parent: None,
            one_shot: 0,
        },
    )
    .expect("add task a");
    let id_a = task_a_res.get("id").unwrap().as_str().unwrap();

    // 2. Task B (Done / Completed)
    let task_b_res = add_task(
        &store_root,
        &TodoCommand::Add {
            title: "Task B".to_string(),
            description: "".to_string(),
            tags: "".to_string(),
            owner: "test-agent".to_string(),
            due: None,
            r#ref: "".to_string(),
            scope: "".to_string(),
            dir: Some(main_root.to_string_lossy().to_string()),
            priority: "medium".to_string(),
            depends_on: "".to_string(),
            blocks: "".to_string(),
            parent: None,
            one_shot: 0,
        },
    )
    .expect("add task b");
    let id_b = task_b_res.get("id").unwrap().as_str().unwrap();

    // 3. Task C (Open but Released/No Claim)
    let task_c_res = add_task(
        &store_root,
        &TodoCommand::Add {
            title: "Task C".to_string(),
            description: "".to_string(),
            tags: "".to_string(),
            owner: "test-agent".to_string(),
            due: None,
            r#ref: "".to_string(),
            scope: "".to_string(),
            dir: Some(main_root.to_string_lossy().to_string()),
            priority: "medium".to_string(),
            depends_on: "".to_string(),
            blocks: "".to_string(),
            parent: None,
            one_shot: 0,
        },
    )
    .expect("add task c");
    let id_c = task_c_res.get("id").unwrap().as_str().unwrap();

    // Manually set distinct hashes in the SQLite db so they don't overlap within the 4.3 minute ULID millisecond threshold
    let db_path = decapod::core::todo::todo_db_path(&store_root);
    let conn = rusqlite::Connection::open(&db_path).expect("open db");
    conn.execute("UPDATE tasks SET hash = 'hashaa' WHERE id = ?", [id_a]).expect("update a");
    conn.execute("UPDATE tasks SET hash = 'hashbb' WHERE id = ?", [id_b]).expect("update b");
    conn.execute("UPDATE tasks SET hash = 'hashcc' WHERE id = ?", [id_c]).expect("update c");

    let hash_a = "hashaa";
    let hash_b = "hashbb";
    let hash_c = "hashcc";

    // Set claims and status
    claim_task(&store_root, id_a, "test-agent", ClaimMode::Exclusive).expect("claim task a");
    claim_task(&store_root, id_b, "test-agent", ClaimMode::Exclusive).expect("claim task b");
    update_status(&store, id_b, "done", "task.done", serde_json::json!({})).expect("done task b");

    // 4. Create actual git worktrees for A, B, C, D
    let workspaces_dir = main_root.join(".decapod").join("workspaces");
    fs::create_dir_all(&workspaces_dir).expect("create workspaces dir");

    // Worktree A (Active branch/task)
    let wt_a_path = workspaces_dir.join(format!("test-agent-todo-{}-todo-a", hash_a));
    let wt_a_branch = format!("agent/test-agent/todo-{}", hash_a);
    run_git_cmd(&main_root, &["worktree", "add", "-b", &wt_a_branch, wt_a_path.to_str().unwrap()]);

    // Worktree B (Task is done)
    let wt_b_path = workspaces_dir.join(format!("test-agent-todo-{}-todo-b", hash_b));
    let wt_b_branch = format!("agent/test-agent/todo-{}", hash_b);
    run_git_cmd(&main_root, &["worktree", "add", "-b", &wt_b_branch, wt_b_path.to_str().unwrap()]);

    // Worktree C (Task has no claim)
    let wt_c_path = workspaces_dir.join(format!("test-agent-todo-{}-todo-c", hash_c));
    let wt_c_branch = format!("agent/test-agent/todo-{}", hash_c);
    run_git_cmd(&main_root, &["worktree", "add", "-b", &wt_c_branch, wt_c_path.to_str().unwrap()]);

    // Worktree D (Branch does not exist - mock this by adding worktree, then deleting its branch)
    let wt_d_path = workspaces_dir.join("test-agent-todo-111111-todo-d");
    let wt_d_branch = "agent/test-agent/todo-111111";
    run_git_cmd(&main_root, &["worktree", "add", "-b", wt_d_branch, wt_d_path.to_str().unwrap()]);

    // Delete branch wt_d_branch using `git branch -D`
    run_git_cmd(&wt_d_path, &["checkout", "--detach"]);
    run_git_cmd(&main_root, &["branch", "-D", wt_d_branch]);

    // Worktree E (Orphaned workspace directory, not registered in git worktrees)
    let wt_e_path = workspaces_dir.join("test-agent-todo-222222-orphaned");
    fs::create_dir_all(&wt_e_path).expect("create wt_e");
    fs::write(wt_e_path.join("some_residual_file.txt"), "hello").expect("write residual");

    // Check directory existence before pruning
    assert!(wt_a_path.exists());
    assert!(wt_b_path.exists());
    assert!(wt_c_path.exists());
    assert!(wt_d_path.exists());
    assert!(wt_e_path.exists());

    // Execute prune!
    let pruned = workspace::prune_workspaces(&main_root, true).expect("prune_workspaces");

    // Verify what was pruned
    // wt_a should NOT be pruned
    assert!(wt_a_path.exists(), "Worktree A (active) must not be pruned");

    // wt_b, wt_c, wt_d, wt_e should be pruned (no longer exist on disk)
    assert!(!wt_b_path.exists(), "Worktree B (completed task) should be pruned");
    assert!(!wt_c_path.exists(), "Worktree C (no active claim) should be pruned");
    assert!(!wt_d_path.exists(), "Worktree D (deleted branch) should be pruned");
    assert!(!wt_e_path.exists(), "Worktree E (unregistered directory) should be pruned");

    // Verify pruned records
    assert!(pruned.iter().any(|p| p.path == wt_b_path.to_string_lossy() && p.reason == "task_completed"));
    assert!(pruned.iter().any(|p| p.path == wt_c_path.to_string_lossy() && p.reason == "no_active_claim"));
    assert!(pruned.iter().any(|p| p.path == wt_d_path.to_string_lossy() && (p.reason == "branch_deleted" || p.reason == "no_matching_task")));
    assert!(pruned.iter().any(|p| p.path == wt_e_path.to_string_lossy() && p.reason == "not_registered"));
}
