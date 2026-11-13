//! Real Git/CLI proof: no maintenance exception or skipped workspace gates.
use decapod::core::{todo, workspace};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn resolve_decapod_bin() -> PathBuf {
    let cargo_bin = env!("CARGO_BIN_EXE_decapod");
    if let Ok(path) = Path::new(cargo_bin).canonicalize() {
        return path;
    }
    if let Ok(runfiles) = std::env::var("RUNFILES_DIR") {
        let path = Path::new(&runfiles).join("_main/decapod");
        if path.is_file() {
            return path;
        }
    }
    if let Some(parent) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        let path = parent.join("decapod");
        if path.is_file() {
            return path;
        }
    }
    PathBuf::from(cargo_bin)
}

fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn cli(root: &Path, actor: &str, args: &[&str]) -> Value {
    let out = Command::new(resolve_decapod_bin())
        .args(args)
        .current_dir(root)
        .env("DECAPOD_AGENT_ID", actor)
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or(Value::Null)
}
fn add(root: &Path, actor: &str, title: &str) -> String {
    cli(
        root,
        actor,
        &[
            "todo",
            "add",
            title,
            "--scope",
            "paths:[\"shared.rs\"]",
            "--format",
            "json",
        ],
    )["id"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn ensure(root: &Path, actor: &str, id: &str) -> PathBuf {
    let branch = format!("agent/{actor}/{id}-repair");
    let v = cli(root, actor, &["workspace", "ensure", "--branch", &branch]);
    v["worktree_path"]
        .as_str()
        .map(PathBuf::from)
        .or_else(|| v["git"]["worktree_path"].as_str().map(PathBuf::from))
        .expect("workspace path")
}
#[test]
fn independent_tasks_bootstrap_and_bind_observed_worktrees() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-q", "-b", "master"]);
    git(root, &["config", "user.email", "fixture@example.invalid"]);
    git(root, &["config", "user.name", "Fixture"]);
    fs::write(
        root.join("shared.rs"),
        "pub const LEFT: u8 = 1;\n\n\npub const RIGHT: u8 = 1;\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(
        root,
        &["-c", "commit.gpgsign=false", "commit", "-qm", "base"],
    );
    cli(
        root,
        "alpha",
        &["init", "--backend", "local", "--force", "--proof"],
    );
    let config_path = root.join(".decapod/config.toml");
    let config = fs::read_to_string(&config_path)
        .unwrap()
        .replace(
            "container_workspaces = true",
            "container_workspaces = false",
        )
        .replace("isolation = \"container\"", "isolation = \"worktree\"");
    fs::write(config_path, config).unwrap();
    git(root, &["add", "."]);
    git(
        root,
        &["-c", "commit.gpgsign=false", "commit", "-qm", "initialize"],
    );
    let a = add(root, "alpha", "Fix parser boundary");
    let b = add(root, "beta", "Fix renderer boundary");
    assert_ne!(a, b);
    cli(
        root,
        "alpha",
        &[
            "govern",
            "policy",
            "eval",
            "--command",
            "todo.handoff",
            "--path",
            &a,
        ],
    );
    let forged = Command::new(resolve_decapod_bin())
        .args([
            "govern",
            "policy",
            "approve",
            "--id",
            "synthetic-approval",
            "--actor",
            "operator",
        ])
        .current_dir(root)
        .env("DECAPOD_AGENT_ID", "alpha")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .output()
        .unwrap();
    assert!(!forged.status.success());
    assert!(String::from_utf8_lossy(&forged.stderr).contains("trust"));

    for (id, actor) in [(&a, "alpha"), (&b, "beta")] {
        let claim = cli(
            root,
            actor,
            &["todo", "claim", "--id", id, "--format", "json"],
        );
        assert_eq!(claim["status"], "ok", "{claim}");
    }
    // The source branch belongs to alpha; beta's explicit destination must win.
    git(root, &["checkout", "-b", &format!("source/{a}")]);
    let wa = ensure(root, "alpha", &a);
    let wb = ensure(root, "beta", &b);
    assert_ne!(wa, wb);
    let store = root.join(".decapod/data");
    for (path, actor) in [(&wa, "alpha"), (&wb, "beta")] {
        todo::check_authoring(&store, path, actor, false).unwrap();
        assert!(!todo::check_trust_level(&store, actor, "verified").unwrap());
    }
    let saved_config = fs::read_to_string(root.join(".decapod/config.toml")).unwrap();
    let shared_config = if saved_config.contains("[governance]") {
        saved_config.replace(
            "[governance]",
            "[governance]\nallow_basic_shared_claims = true",
        )
    } else {
        format!("{saved_config}\n[governance]\nallow_basic_shared_claims = true\n")
    };
    fs::write(root.join(".decapod/config.toml"), shared_config).unwrap();
    assert_eq!(
        cli(
            root,
            "beta",
            &[
                "todo", "claim", "--id", &a, "--mode", "shared", "--format", "json"
            ]
        )["status"],
        "ok"
    );
    let secondary = Command::new(resolve_decapod_bin())
        .args([
            "workspace",
            "ensure",
            "--branch",
            &format!("agent/beta/{a}-secondary"),
        ])
        .current_dir(root)
        .env("DECAPOD_AGENT_ID", "beta")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .output()
        .unwrap();
    assert!(!secondary.status.success());
    assert!(String::from_utf8_lossy(&secondary.stderr).contains("WORKSPACE_SECONDARY_CUSTODY"));
    assert_eq!(
        todo::get_task(&store, &a).unwrap().unwrap().assigned_to,
        "alpha"
    );
    fs::write(root.join(".decapod/config.toml"), saved_config).unwrap();
    assert!(todo::admit_authoring(&store, &wa, "beta").is_err());
    assert!(todo::admit_authoring(&store, &root.join("forged"), "alpha").is_err());
    let forged_path = root.join(".decapod/workspaces/unrelated-repository");
    fs::create_dir_all(&forged_path).unwrap();
    git(
        &forged_path,
        &["init", "-q", "-b", &format!("agent/alpha/{a}-repair")],
    );
    assert!(
        todo::admit_authoring(&store, &forged_path, "alpha")
            .unwrap_err()
            .to_string()
            .contains("CHECKOUT_UNVERIFIED")
    );
    let c = add(root, "alpha", "Fix third boundary");
    cli(
        root,
        "alpha",
        &["todo", "claim", "--id", &c, "--format", "json"],
    );
    let wrong_destination = Command::new(resolve_decapod_bin())
        .args([
            "workspace",
            "ensure",
            "--branch",
            &format!("agent/alpha/{c}"),
        ])
        .current_dir(&wa)
        .env("DECAPOD_AGENT_ID", "alpha")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .output()
        .unwrap();
    assert!(!wrong_destination.status.success());
    assert!(
        String::from_utf8_lossy(&wrong_destination.stderr)
            .contains("WORKSPACE_DESTINATION_MISMATCH")
    );
    git(&wa, &["checkout", "-b", &format!("agent/alpha/{c}")]);
    assert!(
        todo::admit_authoring(&store, &wa, "alpha")
            .unwrap_err()
            .to_string()
            .contains("OCCUPIED")
    );
    git(&wa, &["checkout", &format!("agent/alpha/{a}-repair")]);
    let d = add(root, "beta", "Fix fourth boundary");
    cli(
        root,
        "beta",
        &["todo", "claim", "--id", &d, "--format", "json"],
    );
    git(&wa, &["checkout", "-b", &format!("agent/beta/{d}")]);
    assert!(
        todo::admit_authoring(&store, &wa, "beta")
            .unwrap_err()
            .to_string()
            .contains("OCCUPIED")
    );
    git(&wa, &["checkout", &format!("agent/alpha/{a}-repair")]);
    #[cfg(unix)]
    {
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&wa, &alias).unwrap();
        assert_eq!(
            workspace::observe_checkout(&alias),
            workspace::observe_checkout(&wa)
        );
    }
    // Both admitted actors start real overlapping-source edits concurrently.
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let writers: Vec<_> = [
        (
            wa.clone(),
            "alpha",
            "pub const LEFT: u8 = 2;\n\n\npub const RIGHT: u8 = 1;\n",
        ),
        (
            wb.clone(),
            "beta",
            "pub const LEFT: u8 = 1;\n\n\npub const RIGHT: u8 = 3;\n",
        ),
    ]
    .into_iter()
    .map(|(path, actor, text)| {
        let barrier = barrier.clone();
        let store = store.clone();
        std::thread::spawn(move || {
            barrier.wait();
            todo::check_authoring(&store, &path, actor, false).unwrap();
            fs::write(path.join("shared.rs"), text).unwrap();
        })
    })
    .collect();
    for writer in writers {
        writer.join().unwrap();
    }
    for path in [&wa, &wb] {
        git(path, &["add", "shared.rs"]);
        git(
            path,
            &[
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "independent edit",
            ],
        );
    }
    git(
        &wa,
        &["merge", "--no-edit", &format!("agent/beta/{b}-repair")],
    );
    let combined = fs::read_to_string(wa.join("shared.rs")).unwrap();
    assert!(combined.contains("LEFT: u8 = 2") && combined.contains("RIGHT: u8 = 3"));
    // Clean merge alone cannot establish a shared invariant (LEFT == RIGHT).
    fs::write(
        wa.join("combined.rs"),
        "include!(\"shared.rs\"); fn main() { assert_eq!(LEFT, RIGHT); }\n",
    )
    .unwrap();
    let compiled = Command::new("rustc")
        .args(["combined.rs", "-o", "combined"])
        .current_dir(&wa)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(
        !Command::new(wa.join("combined"))
            .current_dir(&wa)
            .output()
            .unwrap()
            .status
            .success(),
        "textual integration must still fail its shared invariant"
    );
    fs::write(
        wa.join("shared.rs"),
        "pub const LEFT: u8 = 2;\npub const RIGHT: u8 = 2;\n",
    )
    .unwrap();
    assert!(
        Command::new("rustc")
            .args(["combined.rs", "-o", "combined"])
            .current_dir(&wa)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        Command::new(wa.join("combined"))
            .output()
            .unwrap()
            .status
            .success()
    );
    // Controlled expiry in this disposable datastore must fence the old checkout.
    let db = decapod::core::db::Connection::open(todo::todo_db_path(&store)).unwrap();
    db.execute("UPDATE tasks SET lease_expires_at='1Z' WHERE id=?1", [&a])
        .unwrap();
    assert!(
        todo::check_authoring(&store, &wa, "alpha", false)
            .unwrap_err()
            .to_string()
            .contains("CUSTODY_STALE")
    );
}

#[test]
fn cross_process_checkout_contention_and_interrupted_ensure_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-q", "-b", "master"]);
    git(root, &["config", "user.email", "fixture@example.invalid"]);
    git(root, &["config", "user.name", "Fixture"]);
    fs::write(root.join("shared.rs"), "pub fn value() -> u8 { 1 }\n").unwrap();
    git(root, &["add", "."]);
    git(
        root,
        &["-c", "commit.gpgsign=false", "commit", "-qm", "base"],
    );
    cli(
        root,
        "alpha",
        &["init", "--backend", "local", "--force", "--proof"],
    );
    let config_path = root.join(".decapod/config.toml");
    let config = fs::read_to_string(&config_path)
        .unwrap()
        .replace(
            "container_workspaces = true",
            "container_workspaces = false",
        )
        .replace("isolation = \"container\"", "isolation = \"worktree\"");
    fs::write(&config_path, config).unwrap();
    git(root, &["add", "."]);
    git(
        root,
        &["-c", "commit.gpgsign=false", "commit", "-qm", "initialize"],
    );

    let bin = resolve_decapod_bin();
    let store = root.join(".decapod/data");

    // 1. Two concurrent OS processes provisioning two overlapping tasks in separate worktrees both succeed.
    let task_a = add(root, "alpha", "Concurrent isolated slice A");
    let task_b = add(root, "beta", "Concurrent isolated slice B");
    assert_eq!(
        cli(
            root,
            "alpha",
            &["todo", "claim", "--id", &task_a, "--format", "json"]
        )["status"],
        "ok"
    );
    assert_eq!(
        cli(
            root,
            "beta",
            &["todo", "claim", "--id", &task_b, "--format", "json"]
        )["status"],
        "ok"
    );

    let branch_a = format!("agent/alpha/{task_a}-par");
    let branch_b = format!("agent/beta/{task_b}-par");
    let child_a = Command::new(&bin)
        .current_dir(root)
        .env("DECAPOD_AGENT_ID", "alpha")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .args(["workspace", "ensure", "--branch", &branch_a])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let child_b = Command::new(&bin)
        .current_dir(root)
        .env("DECAPOD_AGENT_ID", "beta")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .args(["workspace", "ensure", "--branch", &branch_b])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let out_a = child_a.wait_with_output().unwrap();
    let out_b = child_b.wait_with_output().unwrap();
    assert!(
        out_a.status.success(),
        "alpha ensure failed: {}",
        String::from_utf8_lossy(&out_a.stderr)
    );
    assert!(
        out_b.status.success(),
        "beta ensure failed: {}",
        String::from_utf8_lossy(&out_b.stderr)
    );
    let json_a: Value = serde_json::from_slice(&out_a.stdout).unwrap();
    let json_b: Value = serde_json::from_slice(&out_b.stdout).unwrap();
    assert_eq!(json_a["can_work"], true, "{json_a}");
    assert_eq!(json_b["can_work"], true, "{json_b}");
    assert_ne!(json_a["worktree_path"], json_b["worktree_path"]);

    // 2. Two concurrent OS processes racing to bind ONE task across TWO live worktrees:
    //    exactly one wins and one fails with AUTHORING_TASK_ALREADY_BOUND.
    let task_dual = add(root, "alpha", "Single task dual worktree race");
    assert_eq!(
        cli(
            root,
            "alpha",
            &["todo", "claim", "--id", &task_dual, "--format", "json"]
        )["status"],
        "ok"
    );
    let wt_left = root.join(".decapod/workspaces/dual-left");
    let wt_right = root.join(".decapod/workspaces/dual-right");
    fs::create_dir_all(wt_left.parent().unwrap()).unwrap();
    git(
        root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &format!("agent/alpha/{task_dual}-left"),
            wt_left.to_str().unwrap(),
            "master",
        ],
    );
    git(
        root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &format!("agent/alpha/{task_dual}-right"),
            wt_right.to_str().unwrap(),
            "master",
        ],
    );
    let proc_left = Command::new(&bin)
        .current_dir(&wt_left)
        .env("DECAPOD_AGENT_ID", "alpha")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .args(["workspace", "ensure"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let proc_right = Command::new(&bin)
        .current_dir(&wt_right)
        .env("DECAPOD_AGENT_ID", "alpha")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .args(["workspace", "ensure"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let res_left = proc_left.wait_with_output().unwrap();
    let res_right = proc_right.wait_with_output().unwrap();
    let successes = [res_left.status.success(), res_right.status.success()]
        .into_iter()
        .filter(|ok| *ok)
        .count();
    assert_eq!(
        successes,
        1,
        "left=({:?}, {}) right=({:?}, {})",
        res_left.status,
        String::from_utf8_lossy(&res_left.stderr),
        res_right.status,
        String::from_utf8_lossy(&res_right.stderr)
    );
    let loser_stderr = if !res_left.status.success() {
        String::from_utf8_lossy(&res_left.stderr).into_owned()
    } else {
        String::from_utf8_lossy(&res_right.stderr).into_owned()
    };
    assert!(
        loser_stderr.contains("AUTHORING_TASK_ALREADY_BOUND"),
        "{loser_stderr}"
    );
    let db = decapod::core::db::Connection::open(todo::todo_db_path(&store)).unwrap();
    let dual_rows: i64 = db
        .query_row(
            "SELECT count(*) FROM authoring_bindings WHERE task_id=?1",
            [&task_dual],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(dual_rows, 1);

    // 3. Two concurrent OS processes racing on ONE checkout commit exactly one binding & one event.
    let task_same_wt = add(root, "alpha", "Same checkout concurrent ensure");
    assert_eq!(
        cli(
            root,
            "alpha",
            &["todo", "claim", "--id", &task_same_wt, "--format", "json"]
        )["status"],
        "ok"
    );
    let wt_shared = root.join(".decapod/workspaces/shared-race");
    git(
        root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &format!("agent/alpha/{task_same_wt}-shared"),
            wt_shared.to_str().unwrap(),
            "master",
        ],
    );
    let same_1 = Command::new(&bin)
        .current_dir(&wt_shared)
        .env("DECAPOD_AGENT_ID", "alpha")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .args(["workspace", "ensure"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let same_2 = Command::new(&bin)
        .current_dir(&wt_shared)
        .env("DECAPOD_AGENT_ID", "alpha")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .args(["workspace", "ensure"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let same_out_1 = same_1.wait_with_output().unwrap();
    let same_out_2 = same_2.wait_with_output().unwrap();
    assert!(
        same_out_1.status.success() || same_out_2.status.success(),
        "at least one concurrent ensure on the same checkout must succeed"
    );
    let same_rows: i64 = db
        .query_row(
            "SELECT count(*) FROM authoring_bindings WHERE task_id=?1",
            [&task_same_wt],
            |r| r.get(0),
        )
        .unwrap();
    let same_events: i64 = db
        .query_row(
            "SELECT count(*) FROM events WHERE event_type='authoring.bind' AND payload LIKE ?1",
            [format!("%{task_same_wt}%")],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(same_rows, 1);
    assert_eq!(same_events, 1);

    // 4. Interrupted workspace ensure (worktree exists without binding/governance) fails closed
    //    on status, recovers on retry, and stays can_work=true across CLI `todo renew`.
    let task_int = add(root, "alpha", "Interrupted ensure recovery");
    assert_eq!(
        cli(
            root,
            "alpha",
            &["todo", "claim", "--id", &task_int, "--format", "json"]
        )["status"],
        "ok"
    );
    let wt_int = root.join(".decapod/workspaces/interrupted-wt");
    git(
        root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &format!("agent/alpha/{task_int}-int"),
            wt_int.to_str().unwrap(),
            "master",
        ],
    );
    let pre_status = cli(&wt_int, "alpha", &["workspace", "status"]);
    assert_eq!(pre_status["workspace"]["can_work"], false, "{pre_status}");
    assert!(
        pre_status["workspace"]["blockers"]
            .to_string()
            .contains("AUTHORING_BINDING_REQUIRED"),
        "{pre_status}"
    );

    let recovered = cli(&wt_int, "alpha", &["workspace", "ensure"]);
    assert_eq!(recovered["can_work"], true, "{recovered}");
    assert!(wt_int.join(".decapod/governance.json").is_file());

    let renewed = cli(&wt_int, "alpha", &["todo", "renew", "--id", &task_int]);
    assert_eq!(renewed["status"], "ok", "{renewed}");
    let post_renew_status = cli(&wt_int, "alpha", &["workspace", "status"]);
    assert_eq!(
        post_renew_status["workspace"]["can_work"], true,
        "CLI todo renew must advance authoring_bindings without breaking workspace status: {post_renew_status}"
    );
}
