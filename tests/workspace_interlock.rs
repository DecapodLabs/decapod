use std::path::{Path, PathBuf};
use std::process::Command;
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

#[test]
fn workspace_ensure_preserves_dirty_protected_branch_and_isolates_committed_base() {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path();

    Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir)
        .status()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir)
        .status()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir)
        .status()
        .expect("git config name");

    std::fs::write(dir.join("README.md"), "# test\n").expect("write readme");
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .status()
        .expect("git add");
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(dir)
        .status()
        .expect("git commit");

    let init_out = Command::new(resolve_decapod_bin())
        .args(["init", "--force"])
        .current_dir(dir)
        .output()
        .expect("decapod init");
    assert!(init_out.status.success(), "init failed");

    // The isolated workspace starts from the committed initialized project, never
    // from uncommitted root configuration or source bytes.
    assert!(
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .args(["commit", "-m", "initialize governed project"])
            .current_dir(dir)
            .status()
            .unwrap()
            .success()
    );

    // Dirty the protected branch checkout.
    std::fs::write(dir.join("README.md"), "# changed\n").expect("mutate readme");

    let out = Command::new(resolve_decapod_bin())
        .args(["workspace", "ensure"])
        .current_dir(dir)
        .output()
        .expect("workspace ensure");

    assert!(
        out.status.success(),
        "workspace ensure should return success status with JSON response: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout_str = String::from_utf8_lossy(&out.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout_str).expect("parse JSON");
    assert_eq!(val["status"], "ok");
    assert!(val["can_work"].as_bool().unwrap());
    assert!(val["blockers"].as_array().unwrap().is_empty());
    assert_eq!(
        std::fs::read_to_string(dir.join("README.md")).unwrap(),
        "# changed\n"
    );
    let workspace = PathBuf::from(val["worktree_path"].as_str().unwrap());
    assert_ne!(workspace, dir);
    assert_eq!(
        std::fs::read_to_string(workspace.join("README.md")).unwrap(),
        "# test\n"
    );
    assert_eq!(
        val["root_isolation"]["strategy"],
        "committed_base_only_preserve_source"
    );
}
