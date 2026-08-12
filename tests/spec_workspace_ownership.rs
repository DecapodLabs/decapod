//! GitHub #1255: managed spec projections belong to the claimed workspace.
//!
//! A dirty protected root (`main` or `master`) must stay byte-for-byte
//! untouched when status, validate self-heal, or specs.refresh run. The same
//! operations inside `.decapod/workspaces/*` may write projections.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(dir)
        .args(args)
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .env_remove("DECAPOD_WORKSPACE")
        .env("XDG_CONFIG_HOME", dir.join(".xdg-config"));
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run decapod")
}

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git command should start");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn snapshot_specs(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let specs = root.join(".decapod/managed/specs");
    let mut files = BTreeMap::new();
    if !specs.is_dir() {
        return files;
    }
    for entry in fs::read_dir(&specs).expect("read specs") {
        let entry = entry.expect("dirent");
        if entry.file_type().expect("ft").is_file() {
            files.insert(
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).expect("read spec"),
            );
        }
    }
    files
}

fn write_unrelated_root_dirt(root: &Path) {
    fs::write(root.join("NOTES.md"), "pre-existing unrelated root dirt\n")
        .expect("write unrelated dirt");
}

fn setup_protected_repo(default_branch: &str) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path().to_path_buf();

    git(&dir, &["init", "-q", "-b", default_branch]);
    git(&dir, &["config", "user.email", "test@test.com"]);
    git(&dir, &["config", "user.name", "Test"]);

    let init = run_decapod(&dir, &["init", "--force"], &[]);
    assert!(
        init.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let config_path = dir.join(".decapod/config.toml");
    let mut config = fs::read_to_string(&config_path).expect("read config");
    if config.contains("container_workspaces = true") {
        config = config.replace(
            "container_workspaces = true",
            "container_workspaces = false",
        );
    } else if !config.contains("container_workspaces") {
        config.push_str("\ncontainer_workspaces = false\n");
    }
    fs::write(&config_path, config).expect("write config");

    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-m", "init decapod"]);

    write_unrelated_root_dirt(&dir);
    (tmp, dir)
}

fn add_isolated_worktree(main: &Path, name: &str, branch: &str) -> PathBuf {
    let wt = main.join(".decapod").join("workspaces").join(name);
    fs::create_dir_all(wt.parent().unwrap()).expect("workspaces dir");
    git(
        main,
        &["worktree", "add", "-b", branch, wt.to_str().unwrap()],
    );
    wt
}

fn assert_root_untouched(root: &Path, before: &BTreeMap<String, Vec<u8>>, dirt: &str) {
    let after = snapshot_specs(root);
    assert_eq!(
        after, *before,
        "protected root `.decapod/managed/specs/*` must stay byte-for-byte unchanged"
    );
    let notes = fs::read_to_string(root.join("NOTES.md")).expect("read dirt");
    assert_eq!(notes, dirt, "unrelated root dirt must be preserved");
}

fn run_dirty_root_case(default_branch: &str) {
    let (_tmp, root) = setup_protected_repo(default_branch);
    let before = snapshot_specs(&root);
    let dirt = "pre-existing unrelated root dirt\n";
    assert!(!before.is_empty(), "init should scaffold managed specs");

    let status = run_decapod(&root, &["workspace", "status"], &[]);
    assert_root_untouched(&root, &before, dirt);
    let status_out = format!(
        "{}{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(
        !status_out.contains("Project specs re-evaluated against the current codebase")
            || snapshot_specs(&root) == before,
        "workspace status must not refresh specs on {default_branch}: {status_out}"
    );

    let refresh = run_decapod(
        &root,
        &["rpc", "--op", "specs.refresh"],
        &[("DECAPOD_AGENT_ID", "unknown")],
    );
    assert!(
        !refresh.status.success(),
        "specs.refresh from protected {default_branch} must fail closed"
    );
    let refresh_out = format!(
        "{}{}",
        String::from_utf8_lossy(&refresh.stdout),
        String::from_utf8_lossy(&refresh.stderr)
    );
    assert!(
        refresh_out.contains("workspace_required")
            || refresh_out.contains("isolated git worktree")
            || refresh_out.contains(".decapod/workspaces"),
        "expected workspace_required from protected root specs.refresh, got: {refresh_out}"
    );
    assert_root_untouched(&root, &before, dirt);

    let wt = add_isolated_worktree(
        &root,
        "agent-issue-1255",
        &format!("agent/test/{default_branch}-1255"),
    );
    let wt_before = snapshot_specs(&wt);
    assert_eq!(
        wt_before, before,
        "new worktree should start with the same committed specs as root"
    );

    let _ = run_decapod(
        &wt,
        &["session", "acquire"],
        &[("DECAPOD_AGENT_ID", "unknown")],
    );
    let wt_refresh = run_decapod(
        &wt,
        &["rpc", "--op", "specs.refresh"],
        &[("DECAPOD_AGENT_ID", "unknown")],
    );
    assert!(
        wt_refresh.status.success(),
        "specs.refresh inside claimed worktree must succeed: stdout={} stderr={}",
        String::from_utf8_lossy(&wt_refresh.stdout),
        String::from_utf8_lossy(&wt_refresh.stderr)
    );
    assert_root_untouched(&root, &before, dirt);

    let wt_after = snapshot_specs(&wt);
    assert!(
        !wt_after.is_empty(),
        "workspace refresh must keep the spec bundle"
    );
}

#[test]
fn dirty_master_root_stays_untouched_while_workspace_owns_projections() {
    run_dirty_root_case("master");
}

#[test]
fn dirty_main_root_stays_untouched_while_workspace_owns_projections() {
    run_dirty_root_case("main");
}
