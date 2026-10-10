use super::*;

fn repo() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".decapod/workspaces")).unwrap();
    temp
}

#[test]
fn interrupted_empty_reservation_can_be_recovered_exactly() {
    let temp = repo();
    let target = temp.path().join(".decapod/workspaces/owned");
    let owned = reserve(temp.path(), &target, false).unwrap();
    let invocation = owned.invocation().to_owned();
    assert!(
        acquire(temp.path(), &target).is_err(),
        "live invocation must exclude prune"
    );
    drop(owned);
    let recovered = acquire(temp.path(), &target).unwrap().unwrap();
    assert_eq!(recovered.invocation(), invocation);
    recovered.remove().unwrap();
    assert!(!target.exists());
    assert!(acquire(temp.path(), &target).unwrap().is_none());
}

#[test]
fn replaced_directory_and_task_looking_name_are_never_adopted() {
    let temp = repo();
    let target = temp.path().join(".decapod/workspaces/todo-completed-task");
    let owned = reserve(temp.path(), &target, false).unwrap();
    drop(owned);
    fs::remove_dir(&target).unwrap();
    fs::create_dir(&target).unwrap();
    fs::write(target.join("user-data"), "keep").unwrap();
    assert!(acquire(temp.path(), &target).unwrap().is_none());
    assert!(reserve(temp.path(), &target, false).is_err());
    assert_eq!(
        fs::read_to_string(target.join("user-data")).unwrap(),
        "keep"
    );
}

#[test]
fn staged_durable_receipt_recovers_before_exposure() {
    let temp = repo();
    let target = temp.path().join(".decapod/workspaces/staged");
    let owned = reserve(temp.path(), &target, true).unwrap();
    let stage = owned.receipt.staging.clone();
    let invocation = owned.invocation().to_owned();
    fs::rename(&target, &stage).unwrap(); // simulate handoff before no-replace rename
    drop(owned);
    assert!(!target.exists());
    let retry = reserve(temp.path(), &target, true).unwrap();
    assert_eq!(retry.invocation(), invocation);
    assert!(retry.container_expected());
    retry.remove().unwrap();
}

#[test]
fn failed_clone_content_remains_owned_for_prune_and_retry() {
    let temp = repo();
    let target = temp.path().join(".decapod/workspaces/failed-clone");
    let owned = reserve(temp.path(), &target, false).unwrap();
    fs::create_dir(target.join(".git")).unwrap();
    fs::write(
        target.join(".git/partial"),
        "interrupted before registration",
    )
    .unwrap();
    drop(owned);
    acquire(temp.path(), &target)
        .unwrap()
        .unwrap()
        .remove()
        .unwrap();
    let retry = reserve(temp.path(), &target, false).unwrap();
    retry.verify().unwrap();
    retry.remove().unwrap();
}

#[test]
fn symlink_replacement_preserves_external_files() {
    use std::os::unix::fs::symlink;
    let temp = repo();
    let target = temp.path().join(".decapod/workspaces/symlink");
    let owned = reserve(temp.path(), &target, false).unwrap();
    drop(owned);
    fs::remove_dir(&target).unwrap();
    let external = temp.path().join("external");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("user"), "keep").unwrap();
    symlink(&external, &target).unwrap();
    assert!(acquire(temp.path(), &target).unwrap().is_none());
    assert!(external.join("user").exists());
}

#[test]
fn missing_persistent_identity_preserves_recovery_without_blocking_live_work() {
    let temp = repo();
    let target = temp.path().join(".decapod/workspaces/no-birthtime");
    let mut owned = reserve(temp.path(), &target, false).unwrap();
    // Simulate a filesystem offering neither birthtime nor nonce xattrs.
    owned.receipt.created_nanos = None;
    owned.persist().unwrap();
    fs::write(target.join("work"), "normal work can continue").unwrap();
    owned.mark_ready().unwrap();
    drop(owned);
    assert!(acquire(temp.path(), &target).unwrap().is_none());
    assert!(target.join("work").exists());
}

#[test]
fn no_replace_publication_preserves_even_empty_raced_user_directory() {
    let temp = repo();
    let source = temp.path().join("source");
    let target = temp.path().join("target");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&target).unwrap();
    let before = identity(&target).unwrap();
    assert!(publish_directory(&source, &target).is_err());
    assert_eq!(identity(&target).unwrap(), before);
    assert!(source.exists());
}

fn git(repo: &Path, args: &[&str]) {
    use crate::core::bounded_process::{BoundedCommand, CONTROL_TIMEOUT};
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .bounded_output(CONTROL_TIMEOUT)
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_repo() -> tempfile::TempDir {
    let temp = repo();
    git(temp.path(), &["init", "-b", "master"]);
    git(temp.path(), &["config", "user.name", "Test"]);
    git(
        temp.path(),
        &["config", "user.email", "test@example.invalid"],
    );
    fs::write(temp.path().join("README"), "base").unwrap();
    git(temp.path(), &["add", "README"]);
    git(temp.path(), &["commit", "-m", "base"]);
    crate::core::todo::initialize_todo_db(&temp.path().join(".decapod/data")).unwrap();
    temp
}

#[test]
fn interruption_child() {
    use crate::core::bounded_process::{BoundedCommand, CONTROL_TIMEOUT};
    let Some(root) = std::env::var_os("DECAPOD_LIFECYCLE_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let phase = std::env::var("DECAPOD_LIFECYCLE_TEST_PHASE").unwrap();
    if phase == "recover" {
        let report = crate::core::workspace::prune_workspaces_report(&root, true).unwrap();
        fs::write(root.join("prune-report"), format!("{report:?}")).unwrap();
        return;
    }
    let target = test_target(&root);
    let owned = reserve(
        &root,
        &target,
        matches!(phase.as_str(), "docker" | "failed_startup"),
    )
    .unwrap();
    if phase == "failed_startup" {
        fs::write(target.join("user-work"), "keep").unwrap();
    }
    if phase == "after_clone" {
        let out = std::process::Command::new("git")
            .arg("clone")
            .arg("--local")
            .arg(&root)
            .arg(&target)
            .bounded_output(CONTROL_TIMEOUT)
            .unwrap();
        assert!(out.status.success());
    }
    fs::write(root.join("handoff-ready"), owned.invocation()).unwrap();
    if phase == "docker" {
        let image = std::env::var("DECAPOD_TEST_DOCKER_IMAGE").expect("explicit Docker test image");
        let name = format!("decapod-lifecycle-{}", owned.invocation().to_lowercase());
        let _ = std::process::Command::new("docker")
            .args([
                "run",
                "--rm",
                "--network",
                "none",
                "--pull",
                "never",
                "--name",
                &name,
                "--label",
                "org.decapod.managed=workspace",
                "--label",
                &format!("org.decapod.workspace.path={}", target.display()),
                "--label",
                &format!("org.decapod.invocation={}", owned.invocation()),
                &image,
                "sleep",
                "120",
            ])
            .bounded_output(std::time::Duration::from_secs(180));
    } else {
        std::thread::sleep(std::time::Duration::from_secs(120));
    }
}

fn test_target(root: &Path) -> PathBuf {
    root.join(".decapod/workspaces")
        .join(fs::read_to_string(root.join("target-name")).unwrap_or_else(|_| "interrupted".into()))
}

fn active_task(root: &Path) {
    use crate::core::todo::{self, ClaimMode, TodoCommand};
    let data = root.join(".decapod/data");
    let task = todo::add_task(
        &data,
        &TodoCommand::Add {
            title: "Interrupted task".into(),
            description: String::new(),
            tags: String::new(),
            owner: "test-agent".into(),
            due: None,
            r#ref: String::new(),
            scope: String::new(),
            dir: Some(root.to_string_lossy().into_owned()),
            priority: "medium".into(),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .unwrap();
    let id = task["id"].as_str().unwrap();
    todo::claim_task(&data, id, "test-agent", ClaimMode::Exclusive).unwrap();
    fs::write(root.join("target-name"), format!("agent-test-{id}")).unwrap();
}

fn start_interrupted_child(root: &Path, phase: &str) -> std::process::Child {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "core::workspace_lifecycle::tests::interruption_child",
            "--nocapture",
        ])
        .env("DECAPOD_LIFECYCLE_TEST_ROOT", root)
        .env("DECAPOD_LIFECYCLE_TEST_PHASE", phase)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap()
}

fn wait_ready(root: &Path) -> bool {
    let start = std::time::Instant::now();
    while !root.join("handoff-ready").exists()
        && start.elapsed() < std::time::Duration::from_secs(10)
    {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    root.join("handoff-ready").exists()
}

#[test]
fn killed_handoffs_prune_only_owned_residue_and_allow_retry() {
    for phase in ["before_clone", "after_clone"] {
        let temp = git_repo();
        let unrelated = temp.path().join(".decapod/workspaces/notes-todo-completed");
        fs::create_dir(&unrelated).unwrap();
        fs::write(unrelated.join("user"), "keep").unwrap();
        let mut child = start_interrupted_child(temp.path(), phase);
        let ready = wait_ready(temp.path());
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(ready, "child did not reach {phase}");
        let target = temp.path().join(".decapod/workspaces/interrupted");
        let report = crate::core::workspace::prune_workspaces_report(temp.path(), true).unwrap();
        assert!(!target.exists(), "owned residue survived: {report:?}");
        assert!(unrelated.join("user").exists());
        reserve(temp.path(), &target, false)
            .unwrap()
            .remove()
            .unwrap();
    }
}

/// Opt in only on a runner with Docker and an explicitly chosen pre-pulled image.
/// Default unit runs do not claim real-container interruption coverage.
#[test]
#[ignore = "requires DECAPOD_RUN_DOCKER_LIFECYCLE_TEST=1 and DECAPOD_TEST_DOCKER_IMAGE"]
fn docker_parent_interrupt_recovers_owned_container() {
    use crate::core::bounded_process::{BoundedCommand, CONTROL_TIMEOUT};
    assert_eq!(
        std::env::var("DECAPOD_RUN_DOCKER_LIFECYCLE_TEST").as_deref(),
        Ok("1")
    );
    let temp = git_repo();
    active_task(temp.path());
    let mut child = start_interrupted_child(temp.path(), "docker");
    let ready = wait_ready(temp.path());
    let invocation = fs::read_to_string(temp.path().join("handoff-ready")).unwrap_or_default();
    let name = format!("decapod-lifecycle-{}", invocation.to_lowercase());
    let started = std::time::Instant::now();
    let mut running = false;
    while ready && started.elapsed() < std::time::Duration::from_secs(30) {
        let out = std::process::Command::new("docker")
            .args(["inspect", "--format", "{{.State.Running}}", &name])
            .bounded_output(CONTROL_TIMEOUT);
        if out.is_ok_and(|output| output.status.success() && output.stdout.starts_with(b"true")) {
            running = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let _ = child.kill();
    child.wait().unwrap();
    let target = test_target(temp.path());
    let report = crate::core::workspace::prune_workspaces_report(temp.path(), true);
    // Observe recovery BEFORE any fallback cleanup; cleanup itself is not proof.
    let observed = std::process::Command::new("docker")
        .args(["container", "inspect", &name])
        .bounded_output(CONTROL_TIMEOUT);
    let absent_after_prune = observed.as_ref().is_ok_and(|output| {
        let error = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
        !output.status.success()
            && (error.contains("no such container") || error.contains("no such object"))
    });
    let cleanup = crate::core::container_runtime::remove_container_for_invocation(
        "docker",
        &name,
        &target,
        Some(&invocation),
    );
    assert!(cleanup.is_ok(), "test cleanup failed: {cleanup:?}");
    assert!(
        ready && running,
        "Docker helper did not reach running handoff"
    );
    let report = report.unwrap();
    assert!(
        report
            .skipped
            .iter()
            .any(|item| item.reason == "active_claim"
                && item
                    .detail
                    .contains("abandoned invocation container reconciled")),
        "missing recovery result: {report:?}"
    );
    assert!(
        target.exists(),
        "active task workspace files must be retained: {report:?}"
    );
    assert!(
        absent_after_prune,
        "container survived prune before fallback cleanup: {observed:?}"
    );
}

#[test]
fn active_claim_failed_startup_recovery_preserves_user_work() {
    use crate::core::bounded_process::{BoundedCommand, CONTROL_TIMEOUT};
    use std::os::unix::fs::PermissionsExt;
    let temp = git_repo();
    active_task(temp.path());
    let mut child = start_interrupted_child(temp.path(), "failed_startup");
    let ready = wait_ready(temp.path());
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(ready);
    let bin = temp.path().join("mock-bin");
    fs::create_dir(&bin).unwrap();
    let runtime = bin.join("docker");
    fs::write(
        &runtime,
        "#!/bin/sh\nif [ \"$1 $2\" = \"container ls\" ]; then exit 0; fi\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
    fs::copy(&runtime, bin.join("podman")).unwrap();
    fs::set_permissions(bin.join("podman"), fs::Permissions::from_mode(0o700)).unwrap();
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    for _ in 0..2 {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "core::workspace_lifecycle::tests::interruption_child",
            ])
            .env("DECAPOD_LIFECYCLE_TEST_ROOT", temp.path())
            .env("DECAPOD_LIFECYCLE_TEST_PHASE", "recover")
            .env("PATH", &path)
            .bounded_output(CONTROL_TIMEOUT)
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            fs::read_to_string(temp.path().join("prune-report"))
                .unwrap()
                .contains("abandoned invocation container reconciled")
        );
        assert_eq!(
            fs::read_to_string(test_target(temp.path()).join("user-work")).unwrap(),
            "keep"
        );
    }
}
