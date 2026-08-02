use decapod::core::store::{Store, StoreKind};
use decapod::core::todo::{
    ClaimMode, TodoCommand, add_task, claim_task, initialize_todo_db, update_status,
};
use decapod::core::workspace;
use decapod::plugins::federation::initialize_federation_db;
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
    conn.execute("UPDATE tasks SET hash = 'hashaa' WHERE id = ?", [id_a])
        .expect("update a");
    conn.execute("UPDATE tasks SET hash = 'hashbb' WHERE id = ?", [id_b])
        .expect("update b");
    conn.execute("UPDATE tasks SET hash = 'hashcc' WHERE id = ?", [id_c])
        .expect("update c");

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
    run_git_cmd(
        &main_root,
        &[
            "worktree",
            "add",
            "-b",
            &wt_a_branch,
            wt_a_path.to_str().unwrap(),
        ],
    );

    // Worktree B (Task is done)
    let wt_b_path = workspaces_dir.join(format!("test-agent-todo-{}-todo-b", hash_b));
    let wt_b_branch = format!("agent/test-agent/todo-{}", hash_b);
    run_git_cmd(
        &main_root,
        &[
            "worktree",
            "add",
            "-b",
            &wt_b_branch,
            wt_b_path.to_str().unwrap(),
        ],
    );

    // Worktree C (Task has no claim)
    let wt_c_path = workspaces_dir.join(format!("test-agent-todo-{}-todo-c", hash_c));
    let wt_c_branch = format!("agent/test-agent/todo-{}", hash_c);
    run_git_cmd(
        &main_root,
        &[
            "worktree",
            "add",
            "-b",
            &wt_c_branch,
            wt_c_path.to_str().unwrap(),
        ],
    );

    // Worktree D (Branch does not exist - mock this by adding worktree, then deleting its branch)
    let wt_d_path = workspaces_dir.join("test-agent-todo-111111-todo-d");
    let wt_d_branch = "agent/test-agent/todo-111111";
    run_git_cmd(
        &main_root,
        &[
            "worktree",
            "add",
            "-b",
            wt_d_branch,
            wt_d_path.to_str().unwrap(),
        ],
    );

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
    assert!(
        !wt_b_path.exists(),
        "Worktree B (completed task) should be pruned"
    );
    assert!(
        !wt_c_path.exists(),
        "Worktree C (no active claim) should be pruned"
    );
    assert!(
        !wt_d_path.exists(),
        "Worktree D (deleted branch) should be pruned"
    );
    assert!(
        !wt_e_path.exists(),
        "Worktree E (unregistered directory) should be pruned"
    );

    // Verify pruned records
    assert!(
        pruned
            .iter()
            .any(|p| p.path == wt_b_path.to_string_lossy() && p.reason == "task_completed")
    );
    assert!(
        pruned
            .iter()
            .any(|p| p.path == wt_c_path.to_string_lossy() && p.reason == "no_active_claim")
    );
    assert!(pruned.iter().any(|p| p.path == wt_d_path.to_string_lossy()
        && (p.reason == "branch_deleted" || p.reason == "no_matching_task")));
    assert!(
        pruned
            .iter()
            .any(|p| p.path == wt_e_path.to_string_lossy() && p.reason == "not_registered")
    );
}

#[test]
fn test_workspace_prune_unmerged_prevention() {
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

    // Task F (Done / Completed, but has unmerged commit)
    let task_f_res = add_task(
        &store_root,
        &TodoCommand::Add {
            title: "Task F".to_string(),
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
    .expect("add task f");
    let id_f = task_f_res.get("id").unwrap().as_str().unwrap();

    let db_path = decapod::core::todo::todo_db_path(&store_root);
    let conn = rusqlite::Connection::open(&db_path).expect("open db");
    conn.execute("UPDATE tasks SET hash = 'hashff' WHERE id = ?", [id_f])
        .expect("update f");

    let hash_f = "hashff";
    claim_task(&store_root, id_f, "test-agent", ClaimMode::Exclusive).expect("claim task f");
    update_status(&store, id_f, "done", "task.done", serde_json::json!({})).expect("done task f");

    let workspaces_dir = main_root.join(".decapod").join("workspaces");
    fs::create_dir_all(&workspaces_dir).expect("create workspaces dir");

    let wt_f_path = workspaces_dir.join(format!("test-agent-todo-{}-todo-f", hash_f));
    let wt_f_branch = format!("agent/test-agent/todo-{}", hash_f);
    run_git_cmd(
        &main_root,
        &[
            "worktree",
            "add",
            "-b",
            &wt_f_branch,
            wt_f_path.to_str().unwrap(),
        ],
    );

    // Commit a change inside wt_f to diverge it from master
    fs::write(wt_f_path.join("unmerged_file.txt"), "divergent content")
        .expect("write divergent file");
    run_git_cmd(&wt_f_path, &["add", "unmerged_file.txt"]);
    run_git_cmd(&wt_f_path, &["commit", "-m", "unmerged work"]);

    // Execute prune!
    let pruned = workspace::prune_workspaces(&main_root, true).expect("prune_workspaces");

    // Since the commit on wt_f_branch is NOT in master/main, it should NOT be pruned!
    assert!(
        wt_f_path.exists(),
        "Worktree F must not be pruned since its HEAD commit is unmerged"
    );
    assert!(!pruned.iter().any(|p| p.path == wt_f_path.to_string_lossy()));

    // Now, merge wt_f_branch into master
    run_git_cmd(&main_root, &["merge", &wt_f_branch]);

    // Execute prune again!
    let pruned_after_merge =
        workspace::prune_workspaces(&main_root, true).expect("prune_workspaces");

    // Now it should be pruned!
    assert!(
        !wt_f_path.exists(),
        "Worktree F should be pruned after its branch is merged"
    );
    assert!(
        pruned_after_merge
            .iter()
            .any(|p| p.path == wt_f_path.to_string_lossy() && p.reason == "task_completed")
    );
}

#[test]
fn test_workspace_prune_non_force_preserves_dirty_and_unregistered_data() {
    let tmp = tempdir().expect("tempdir");
    let main_root = tmp.path().join("main_repo");
    fs::create_dir_all(&main_root).expect("create main_repo");

    git_init(&main_root);
    git_commit(&main_root, "init");

    let store_root = main_root.join(".decapod").join("data");
    fs::create_dir_all(&store_root).expect("create data dir");
    initialize_todo_db(&store_root).expect("init todo db");
    let store = Store {
        kind: StoreKind::Repo,
        root: store_root.clone(),
    };

    let task = add_task(
        &store_root,
        &TodoCommand::Add {
            title: "Dirty stale workspace".to_string(),
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
    .expect("add task");
    let task_id = task.get("id").unwrap().as_str().unwrap();
    let db_path = decapod::core::todo::todo_db_path(&store_root);
    let conn = rusqlite::Connection::open(&db_path).expect("open todo db");
    conn.execute(
        "UPDATE tasks SET hash = 'hashdirty' WHERE id = ?",
        [task_id],
    )
    .expect("update task hash");
    claim_task(&store_root, task_id, "test-agent", ClaimMode::Exclusive).expect("claim task");
    update_status(&store, task_id, "done", "task.done", serde_json::json!({}))
        .expect("complete task");

    let workspaces_dir = main_root.join(".decapod").join("workspaces");
    fs::create_dir_all(&workspaces_dir).expect("create workspaces dir");
    let dirty_path = workspaces_dir.join("test-agent-todo-hashdirty-dirty");
    run_git_cmd(
        &main_root,
        &[
            "worktree",
            "add",
            "-b",
            "agent/test-agent/todo-hashdirty",
            dirty_path.to_str().unwrap(),
        ],
    );
    fs::write(dirty_path.join("uncommitted.txt"), "preserve me").expect("write uncommitted change");

    let orphan_path = workspaces_dir.join("orphaned-houseboat-workspace");
    fs::create_dir_all(&orphan_path).expect("create orphan workspace");
    fs::write(orphan_path.join("evidence.txt"), "inspect me").expect("write orphan evidence");

    let report =
        workspace::prune_workspaces_report(&main_root, false).expect("non-force prune report");
    assert!(dirty_path.exists(), "dirty workspace must be preserved");
    assert!(orphan_path.exists(), "orphan workspace must be preserved");
    assert!(report.pruned.is_empty());
    assert!(
        report
            .skipped
            .iter()
            .any(|workspace| workspace.path == dirty_path.to_string_lossy()
                && workspace.reason == "dirty_workspace")
    );
    assert!(
        report
            .skipped
            .iter()
            .any(|workspace| workspace.path == orphan_path.to_string_lossy()
                && workspace.reason == "unregistered_workspace")
    );

    let forced = workspace::prune_workspaces_report(&main_root, true).expect("force prune report");
    assert!(!dirty_path.exists(), "force must remove the stale worktree");
    assert!(
        !orphan_path.exists(),
        "force must remove the orphan workspace"
    );
    assert!(
        forced
            .pruned
            .iter()
            .any(|workspace| workspace.path == dirty_path.to_string_lossy())
    );
    assert!(
        forced
            .pruned
            .iter()
            .any(|workspace| workspace.path == orphan_path.to_string_lossy())
    );
}

#[test]
fn test_validate_stale_workspaces_cleanup() {
    let tmp = tempdir().expect("tempdir");
    let main_root = tmp.path().join("main_repo");
    fs::create_dir_all(&main_root).expect("create main_repo");

    git_init(&main_root);
    git_commit(&main_root, "init");

    let store_root = main_root.join(".decapod").join("data");
    fs::create_dir_all(&store_root).expect("create data dir");
    initialize_todo_db(&store_root).expect("init todo db");
    initialize_federation_db(&store_root).expect("init federation schema for validation");

    let store = Store {
        kind: StoreKind::Repo,
        root: store_root.clone(),
    };

    let task_res = add_task(
        &store_root,
        &TodoCommand::Add {
            title: "Task Prunable".to_string(),
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
    .expect("add task");
    let id = task_res.get("id").unwrap().as_str().unwrap();

    let db_path = decapod::core::todo::todo_db_path(&store_root);
    let conn = rusqlite::Connection::open(&db_path).expect("open db");
    conn.execute("UPDATE tasks SET hash = 'hashprune' WHERE id = ?", [id])
        .expect("update hash");

    claim_task(&store_root, id, "test-agent", ClaimMode::Exclusive).expect("claim task");
    update_status(&store, id, "done", "task.done", serde_json::json!({})).expect("done task");

    let workspaces_dir = main_root.join(".decapod").join("workspaces");
    fs::create_dir_all(&workspaces_dir).expect("create workspaces dir");

    let wt_path = workspaces_dir.join("test-agent-todo-hashprune-todo");
    let wt_branch = "agent/test-agent/todo-hashprune";
    run_git_cmd(
        &main_root,
        &[
            "worktree",
            "add",
            "-b",
            wt_branch,
            wt_path.to_str().unwrap(),
        ],
    );
    // Unique unmerged tip so the branch is not yet on a protected branch.
    fs::write(wt_path.join("agent-work.txt"), "in progress").expect("write worktree file");
    run_git_cmd(&wt_path, &["add", "agent-work.txt"]);
    run_git_cmd(&wt_path, &["commit", "-m", "unmerged agent work"]);
    assert!(wt_path.exists());

    // Unmerged completed work remains fail-closed (not pruned).
    let unmerged = workspace::prune_workspaces_report(&main_root, false).expect("unmerged report");
    assert!(
        wt_path.exists(),
        "unmerged completed worktree must be preserved without --force"
    );
    assert!(
        unmerged
            .pruned
            .iter()
            .all(|p| p.path != wt_path.to_string_lossy()),
        "unmerged workspace must not appear in pruned list"
    );

    // Make the tip reachable from the protected default branch so cleanup is eligible.
    let default_branch = {
        let out = std::process::Command::new("git")
            .args([
                "-C",
                main_root.to_str().unwrap(),
                "symbolic-ref",
                "--short",
                "HEAD",
            ])
            .output()
            .expect("default branch");
        // HEAD may be on worktree; resolve main_repo default via master/main show-ref
        let candidates = ["master", "main"];
        let mut found = String::new();
        for c in candidates {
            let o = std::process::Command::new("git")
                .args([
                    "-C",
                    main_root.to_str().unwrap(),
                    "show-ref",
                    "--verify",
                    &format!("refs/heads/{c}"),
                ])
                .output()
                .expect("show-ref");
            if o.status.success() {
                found = c.to_string();
                break;
            }
        }
        if found.is_empty() {
            found = String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
        found
    };
    run_git_cmd(&main_root, &["checkout", &default_branch]);
    run_git_cmd(&main_root, &["merge", "--ff-only", wt_branch]);
    // Keep the candidate worktree clean after merge so non-force prune is allowed.
    run_git_cmd(&wt_path, &["reset", "--hard", "HEAD"]);
    run_git_cmd(&wt_path, &["clean", "-fd"]);

    // Eligible clean stale workspaces are pruned by the same path validation uses.
    let report = workspace::prune_workspaces_report(&main_root, false).expect("prune report");
    assert!(
        report
            .pruned
            .iter()
            .any(|workspace| workspace.path == wt_path.to_string_lossy()),
        "eligible clean stale workspace should be pruned: {report:?}"
    );
    assert!(
        !wt_path.exists(),
        "eligible clean stale workspace must be removed from disk"
    );
}
