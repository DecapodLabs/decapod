use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn decapod_bin() -> std::path::PathBuf {
    let binary = Path::new(env!("CARGO_BIN_EXE_decapod"));
    if let Ok(resolved) = binary.canonicalize() {
        return resolved;
    }
    if let Some(root) = std::env::var_os("RUNFILES_DIR")
        && let Ok(resolved) = Path::new(&root).join("_main/decapod").canonicalize()
    {
        return resolved;
    }
    if let Some(parent) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        && let Ok(resolved) = parent.join("decapod").canonicalize()
    {
        return resolved;
    }
    binary.to_path_buf()
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .expect("git must execute");
    assert!(status.success(), "git {args:?} failed");
}

fn init_repo(root: &Path, backend: &str, remote: &str) {
    git(root, &["init"]);
    git(root, &["config", "user.name", "Decapod Test"]);
    git(root, &["config", "user.email", "test@example.invalid"]);
    git(root, &["remote", "add", "origin", remote]);
    fs::create_dir_all(root.join(".decapod")).expect("create .decapod");
    fs::write(
        root.join(".decapod/config.toml"),
        format!(
            "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"{backend}\"\n"
        ),
    )
    .expect("write config");
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "initial"]);
}

fn isolated_env(cmd: &mut Command, data_home: &Path, agent: &str) {
    cmd.env("DECAPOD_AGENT_ID", agent)
        .env("DECAPOD_VALIDATE_SKIP_ENV_CHECK", "1")
        .env("DECAPOD_GROUP_BROKER_DISABLE", "1")
        .env("XDG_DATA_HOME", data_home)
        .env("XDG_CONFIG_HOME", data_home)
        .env("HOME", data_home)
        .env_remove("DECAPOD_CLOUD_ORIGIN")
        .env_remove("PROPODUS_URL");
}

#[test]
fn cli_mixed_worktree_cloud_wt_under_local_root_never_reads_root_sqlite() {
    let tmp = TempDir::new().expect("tempdir");
    let repo = tmp.path().join("repo");
    let wt = tmp.path().join("cloud-wt");
    let xdg = tmp.path().join("xdg");
    fs::create_dir_all(&repo).expect("create repo");
    fs::create_dir_all(&xdg).expect("create xdg");

    init_repo(&repo, "local", "https://github.com/DecapodLabs/decapod.git");

    // Seed a distinct sentinel task in canonical local storage via the real CLI.
    let mut acquire = Command::new(decapod_bin());
    isolated_env(&mut acquire, &xdg, "mixed-wt-tester");
    let out = acquire
        .current_dir(&repo)
        .args(["session", "acquire"])
        .output()
        .expect("session acquire");
    assert!(
        out.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mut seed = Command::new(decapod_bin());
    isolated_env(&mut seed, &xdg, "mixed-wt-tester");
    let out = seed
        .current_dir(&repo)
        .args([
            "todo",
            "add",
            "LOCAL_ROOT_CANONICAL_SENTINEL_DO_NOT_LEAK",
            "--format",
            "json",
        ])
        .output()
        .expect("seed local todo");
    assert!(
        out.status.success(),
        "seed todo add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Create a worktree switched to backend = "cloud".
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "cloud-branch",
            wt.to_str().unwrap(),
        ],
    );
    fs::write(
        wt.join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n",
    )
        .expect("write cloud wt config");

    // Running `decapod todo list --format json` in the cloud worktree must NOT return
    // the local root sentinel and must fail closed on missing/unauthenticated cloud session.
    let mut cloud_list = Command::new(decapod_bin());
    isolated_env(&mut cloud_list, &xdg, "mixed-wt-tester");
    let out = cloud_list
        .current_dir(&wt)
        .env("DECAPOD_HEADLESS", "1")
        .args(["todo", "list", "--format", "json"])
        .output()
        .expect("run cloud wt todo list");

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "unauthenticated cloud worktree todo list must fail closed; output={combined}"
    );
    assert!(
        !combined.contains("LOCAL_ROOT_CANONICAL_SENTINEL_DO_NOT_LEAK"),
        "cloud worktree leaked local root tasks! output={combined}"
    );
    assert!(
        !wt.join(".decapod/data/todo.db").exists() && !wt.join(".decapod/data/decapod.db").exists(),
        "cloud worktree created local todo/decapod db"
    );
}

#[test]
fn cli_mixed_worktree_unaccepted_local_downgrade_under_accepted_cloud_fails_closed() {
    let tmp = TempDir::new().expect("tempdir");
    let repo = tmp.path().join("repo");
    let wt = tmp.path().join("local-wt");
    let xdg = tmp.path().join("xdg");
    fs::create_dir_all(&repo).expect("create repo");
    fs::create_dir_all(&xdg).expect("create xdg");

    init_repo(&repo, "cloud", "https://github.com/DecapodLabs/decapod.git");

    git(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "local-branch",
            wt.to_str().unwrap(),
        ],
    );
    fs::write(
        wt.join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"local\"\n",
    )
    .expect("write local wt config");

    // #1776 security invariant: when accepted `main` requires `backend = "cloud"`,
    // an unmerged worktree switching to `backend = "local"` MUST fail closed and
    // never initialize local SQLite.
    let mut add = Command::new(decapod_bin());
    isolated_env(&mut add, &xdg, "mixed-wt-local");
    let out = add
        .current_dir(&wt)
        .args([
            "todo",
            "add",
            "LOCAL_WT_UNDER_CLOUD_ROOT",
            "--format",
            "json",
        ])
        .output()
        .expect("local wt todo add");
    assert!(
        !out.status.success(),
        "unaccepted local downgrade under accepted cloud root must fail closed"
    );
    assert!(!wt.join(".decapod/data/decapod.db").exists());
    assert!(!repo.join(".decapod/data/decapod.db").exists());
}

#[test]
fn cli_mixed_worktree_local_wt_under_dirty_cloud_root_uses_local_sqlite() {
    let tmp = TempDir::new().expect("tempdir");
    let repo = tmp.path().join("repo");
    let wt = tmp.path().join("local-wt");
    let xdg = tmp.path().join("xdg");
    fs::create_dir_all(&repo).expect("create repo");
    fs::create_dir_all(&xdg).expect("create xdg");

    init_repo(&repo, "local", "https://github.com/DecapodLabs/decapod.git");
    git(&repo, &["update-ref", "refs/remotes/origin/main", "HEAD"]);

    git(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "local-branch",
            wt.to_str().unwrap(),
        ],
    );
    // Switch the root checkout onto an unaccepted cloud feature branch while
    // accepted `main` and the invoking worktree stay `local`.
    git(&repo, &["checkout", "-b", "root-cloud-feature"]);
    fs::write(
        repo.join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n",
    )
    .expect("write cloud config on root feature branch");
    git(&repo, &["add", ".decapod/config.toml"]);
    git(&repo, &["commit", "-m", "root cloud feature branch"]);

    let mut acquire = Command::new(decapod_bin());
    isolated_env(&mut acquire, &xdg, "mixed-wt-local");
    let out = acquire
        .current_dir(&wt)
        .args(["session", "acquire"])
        .output()
        .expect("session acquire");
    assert!(
        out.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mut add = Command::new(decapod_bin());
    isolated_env(&mut add, &xdg, "mixed-wt-local");
    let out = add
        .current_dir(&wt)
        .args([
            "todo",
            "add",
            "LOCAL_WT_UNDER_DIRTY_CLOUD_ROOT",
            "--format",
            "json",
        ])
        .output()
        .expect("local wt todo add");
    assert!(
        out.status.success(),
        "local wt todo add failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let mut list = Command::new(decapod_bin());
    isolated_env(&mut list, &xdg, "mixed-wt-local");
    let out = list
        .current_dir(&wt)
        .args(["todo", "list", "--format", "json"])
        .output()
        .expect("local wt todo list");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("LOCAL_WT_UNDER_DIRTY_CLOUD_ROOT"),
        "expected local task in output={stdout}"
    );
}
