use std::process::Command;
use tempfile::TempDir;

#[test]
fn workspace_ensure_blocks_on_protected_branch_with_local_mods() {
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

    let init_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--force"])
        .current_dir(dir)
        .output()
        .expect("decapod init");
    assert!(init_out.status.success(), "init failed");

    // Dirty the protected branch checkout.
    std::fs::write(dir.join("README.md"), "# changed\n").expect("mutate readme");

    let out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["workspace", "ensure"])
        .current_dir(dir)
        .output()
        .expect("workspace ensure");

    assert!(
        !out.status.success(),
        "workspace ensure should block for protected+dirty checkout"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("WORKSPACE_INTERLOCK_DIRTY_PROTECTED"),
        "unexpected stderr: {}",
        stderr
    );
}
