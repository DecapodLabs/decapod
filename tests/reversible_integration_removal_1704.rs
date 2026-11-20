use decapod::core::assets;
use decapod::core::integration_removal::{
    INTEGRATION_OWNERSHIP_PATH, INTEGRATION_RESTORE_MANIFEST_PATH,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

fn resolve_decapod_bin() -> PathBuf {
    let cargo_bin = env!("CARGO_BIN_EXE_decapod");
    if let Ok(path) = Path::new(cargo_bin).canonicalize() {
        return path;
    }
    PathBuf::from(cargo_bin)
}

fn new_private_tempdir() -> TempDir {
    let mut builder = tempfile::Builder::new();
    builder.prefix("decapod-1704-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    builder.tempdir().expect("private tempdir")
}

fn run_git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {:?} failed:\nstdout: {}\nstderr: {}",
        args,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn run_decapod_in(dir: &Path, xdg_home: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(resolve_decapod_bin());
    cmd.current_dir(dir)
        .env("XDG_CONFIG_HOME", xdg_home)
        .env("DECAPOD_AGENT_ID", "test-agent-1704")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1")
        .env("DECAPOD_STORAGE_OWNER", "1")
        .args(args);
    for key in [
        "DECAPOD_SESSION_PASSWORD",
        "GITHUB_ACTIONS",
        "BUILDKITE",
        "DECAPOD_VALIDATE_SKIP_GIT_GATES",
    ] {
        cmd.env_remove(key);
    }
    cmd.output().expect("execute decapod")
}

fn setup_git_repo() -> (TempDir, PathBuf, PathBuf) {
    let tmp = new_private_tempdir();
    let repo = tmp.path().join("repo");
    let xdg = tmp.path().join("xdg");
    fs::create_dir_all(&repo).unwrap();
    fs::create_dir_all(&xdg).unwrap();
    run_git(&repo, &["init", "-q", "-b", "main"]);
    run_git(&repo, &["config", "user.name", "Test User"]);
    run_git(&repo, &["config", "user.email", "test@example.invalid"]);
    fs::write(repo.join("src_marker.txt"), "project source\n").unwrap();
    run_git(&repo, &["add", "src_marker.txt"]);
    run_git(
        &repo,
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-q",
            "-m",
            "init",
        ],
    );
    (tmp, repo, xdg)
}

fn init_repo(repo: &Path, xdg: &Path, extra: &[&str]) {
    let mut args = vec![
        "init",
        "with",
        "--proof",
        "--no-container-workspaces",
        "--isolation-mode",
        "worktree",
    ];
    args.extend_from_slice(extra);
    let out = run_decapod_in(repo, xdg, &args);
    assert!(
        out.status.success(),
        "decapod init failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[derive(Debug, PartialEq, Eq)]
struct FileSnapshot {
    is_symlink: bool,
    bytes: Vec<u8>,
}

fn snapshot_tree(root: &Path) -> BTreeMap<String, FileSnapshot> {
    let mut map = BTreeMap::new();
    fn walk(base: &Path, cur: &Path, map: &mut BTreeMap<String, FileSnapshot>) {
        let mut entries: Vec<_> = fs::read_dir(cur).unwrap().map(|e| e.unwrap()).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let p = entry.path();
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().to_string();
            if rel == ".git" || rel.starts_with(".git/objects") || rel.starts_with(".git/logs") {
                continue;
            }
            let meta = fs::symlink_metadata(&p).unwrap();
            if meta.file_type().is_symlink() {
                let target = fs::read_link(&p).unwrap();
                map.insert(
                    rel,
                    FileSnapshot {
                        is_symlink: true,
                        bytes: target.to_string_lossy().as_bytes().to_vec(),
                    },
                );
            } else if meta.is_dir() {
                map.insert(
                    format!("{rel}/"),
                    FileSnapshot {
                        is_symlink: false,
                        bytes: Vec::new(),
                    },
                );
                walk(base, &p, map);
            } else if meta.is_file() {
                map.insert(
                    rel,
                    FileSnapshot {
                        is_symlink: false,
                        bytes: fs::read(&p).unwrap(),
                    },
                );
            }
        }
    }
    walk(root, root, &mut map);
    map
}

fn parse_json_stdout(out: &Output) -> serde_json::Value {
    assert!(
        out.status.success(),
        "command failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("valid JSON output")
}

#[test]
fn preview_makes_zero_mutations_and_classifies_all_buckets_separately() {
    let (_tmp, repo, xdg) = setup_git_repo();

    // Pre-existing user gitignore & custom hook & custom file
    fs::write(repo.join(".gitignore"), "target/\n*.log\n").unwrap();
    let hooks_dir = repo.join(".git/hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    fs::write(hooks_dir.join("pre-commit"), "#!/bin/sh\necho user-hook\n").unwrap();
    fs::write(repo.join(".decapod_note.txt"), "keep me\n").unwrap();

    init_repo(&repo, &xdg, &[]);
    // Customize CLAUDE.md and add an authored spec edit and an unknown file in .decapod/
    fs::write(repo.join("CLAUDE.md"), "# User customized CLAUDE.md\n").unwrap();
    fs::write(
        repo.join(".decapod/managed/specs/INTENT.md"),
        "# Intent\n\nAuthored project outcome.\n",
    )
    .unwrap();
    fs::write(repo.join(".decapod/custom_unknown.txt"), "unknown entry\n").unwrap();

    // Add an active todo claim so `active` bucket is populated
    let add_out = run_decapod_in(
        &repo,
        &xdg,
        &["todo", "add", "Active custody item", "--format", "json"],
    );
    let add_json = parse_json_stdout(&add_out);
    let task_id = add_json["id"].as_str().unwrap().to_string();
    let claim_out = run_decapod_in(
        &repo,
        &xdg,
        &["todo", "claim", "--id", &task_id, "--format", "json"],
    );
    assert!(
        claim_out.status.success(),
        "{}",
        String::from_utf8_lossy(&claim_out.stderr)
    );

    let before_tree = snapshot_tree(&repo);
    let before_xdg = snapshot_tree(&xdg);
    let before_git = run_git(&repo, &["status", "--porcelain"]);

    let preview_out = run_decapod_in(&repo, &xdg, &["integration", "preview", "--format", "json"]);
    let preview = parse_json_stdout(&preview_out);

    let after_tree = snapshot_tree(&repo);
    let after_xdg = snapshot_tree(&xdg);
    let after_git = run_git(&repo, &["status", "--porcelain"]);

    assert_eq!(before_tree, after_tree, "preview must not mutate repo tree");
    assert_eq!(
        before_xdg, after_xdg,
        "preview must not mutate XDG config/sessions"
    );
    assert_eq!(before_git, after_git, "preview must not mutate git state");

    assert_eq!(preview["can_apply"], serde_json::json!(false));
    assert!(
        preview["generated"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["path"] == "AGENTS.md")
    );
    assert!(
        preview["edited"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["path"] == "CLAUDE.md")
    );
    assert!(
        preview["edited"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["path"] == ".gitignore")
    );
    assert!(
        preview["authored"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["path"] == ".decapod/managed/specs/INTENT.md")
    );
    assert!(
        preview["authored"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["path"] == ".git/hooks/pre-commit")
    );
    assert!(
        preview["unknown"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["path"] == ".decapod/custom_unknown.txt")
    );
    assert!(
        preview["active"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["kind"] == "active_todo_lease")
    );
}

#[test]
fn byte_identical_authored_files_without_ownership_are_never_deleted() {
    let (_tmp, repo, xdg) = setup_git_repo();

    // Write an authored AGENTS.md whose body is byte-identical to Decapod's canonical AGENTS template
    // (without Decapod release/fingerprint headers), plus an authored pre-commit hook byte-identical
    // to Decapod's pre-commit hook without `setup hook` ownership.
    let canonical_agents_body = assets::canonical_template("AGENTS.md").unwrap();
    fs::write(repo.join("AGENTS.md"), &canonical_agents_body).unwrap();
    let hooks_dir = repo.join(".git/hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    let hook_bytes = "#!/bin/sh\nset -e\ncargo fmt --check\ncargo clippy --all-targets --all-features -- -D warnings\n";
    fs::write(hooks_dir.join("pre-commit"), hook_bytes).unwrap();

    // Case 1: uninitialized repo
    let preview_uninit = parse_json_stdout(&run_decapod_in(
        &repo,
        &xdg,
        &["integration", "preview", "--format", "json"],
    ));
    assert!(
        preview_uninit["authored"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["path"] == "AGENTS.md")
    );
    assert!(preview_uninit["generated"].as_array().unwrap().is_empty());
    let remove_uninit = run_decapod_in(
        &repo,
        &xdg,
        &["integration", "remove", "--authorize", "--format", "json"],
    );
    assert!(remove_uninit.status.success());
    assert_eq!(
        fs::read_to_string(repo.join("AGENTS.md")).unwrap(),
        canonical_agents_body
    );
    assert_eq!(
        fs::read_to_string(hooks_dir.join("pre-commit")).unwrap(),
        hook_bytes
    );

    // Case 2: legacy repo with .decapod/ present (no integration_ownership.json)
    fs::create_dir_all(repo.join(".decapod/data")).unwrap();
    let preview_legacy = parse_json_stdout(&run_decapod_in(
        &repo,
        &xdg,
        &["integration", "preview", "--format", "json"],
    ));
    assert!(
        preview_legacy["authored"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["path"] == "AGENTS.md")
    );
    assert!(
        preview_legacy["authored"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["path"] == ".git/hooks/pre-commit")
    );
    let remove_legacy = run_decapod_in(
        &repo,
        &xdg,
        &["integration", "remove", "--authorize", "--format", "json"],
    );
    assert!(remove_legacy.status.success());
    assert_eq!(
        fs::read_to_string(repo.join("AGENTS.md")).unwrap(),
        canonical_agents_body,
        "byte-identical authored AGENTS.md without ownership headers must never be deleted"
    );
    assert_eq!(
        fs::read_to_string(hooks_dir.join("pre-commit")).unwrap(),
        hook_bytes,
        "byte-identical authored pre-commit hook without ownership evidence must never be deleted"
    );
}

#[test]
fn full_lifecycle_init_use_remove_restore_and_reinit_preserves_bytes_and_state() {
    let (_tmp, repo, xdg) = setup_git_repo();

    // Pre-existing user instructions, hook, local gitignore, and CI workflow
    let user_agents = "# My Project Agent Instructions\nNever delete this.\n";
    let user_gitignore = "# User ignores\n/dist/\n*.tmp\n";
    let user_pre_commit = "#!/bin/sh\necho custom-pre-commit\n";
    fs::write(repo.join("AGENTS.md"), user_agents).unwrap();
    fs::write(repo.join(".gitignore"), user_gitignore).unwrap();
    fs::create_dir_all(repo.join(".git/hooks")).unwrap();
    fs::write(repo.join(".git/hooks/pre-commit"), user_pre_commit).unwrap();

    // 1. Init (backs up AGENTS.md -> AGENTS.md.bak, merges .gitignore)
    init_repo(&repo, &xdg, &[]);
    // Also install Decapod commit-msg hook via `setup hook`
    let hook_out = run_decapod_in(&repo, &xdg, &["setup", "hook", "--commit-msg"]);
    assert!(hook_out.status.success());

    // 2. Use: add custom config section, edit OVERRIDE.md directive, edit GEMINI.md, create & complete a todo
    let cfg_path = repo.join(".decapod/config.toml");
    let mut cfg_text = fs::read_to_string(&cfg_path).unwrap();
    cfg_text.push_str("\n[custom_local]\nflag = \"keep_me\"\n");
    fs::write(&cfg_path, &cfg_text).unwrap();

    let override_path = repo.join(".decapod/OVERRIDE.md");
    let override_text = fs::read_to_string(&override_path).unwrap().replace(
        "Replace this line with this directive's override. Use Markdown or any documentation style you prefer.",
        "Keep project override directive intact.",
    );
    fs::write(&override_path, &override_text).unwrap();

    let custom_gemini = "# Custom GEMINI instructions\n";
    fs::write(repo.join("GEMINI.md"), custom_gemini).unwrap();

    let todo_add = parse_json_stdout(&run_decapod_in(
        &repo,
        &xdg,
        &["todo", "add", "Lifecycle task", "--format", "json"],
    ));
    let task_id = todo_add["id"].as_str().unwrap().to_string();
    assert!(
        run_decapod_in(&repo, &xdg, &["todo", "done", "--id", &task_id])
            .status
            .success()
    );

    let governance_before = fs::read(repo.join(".decapod/governance.json")).unwrap();
    let agents_before_remove = fs::read(repo.join("AGENTS.md")).unwrap();
    let claude_before_remove = fs::read(repo.join("CLAUDE.md")).unwrap();
    let codex_before_remove = fs::read(repo.join("CODEX.md")).unwrap();
    let commit_msg_before_remove = fs::read(repo.join(".git/hooks/commit-msg")).unwrap();

    // Removing without --authorize must fail closed
    let unauth = run_decapod_in(&repo, &xdg, &["integration", "remove"]);
    assert!(!unauth.status.success());
    assert!(String::from_utf8_lossy(&unauth.stderr).contains("INTEGRATION_AUTHORIZATION_REQUIRED"));
    assert!(repo.join("AGENTS.md").exists());

    // Authority boundaries: --purge-state and --remove-protected-governance must be refused
    let purge_denied = run_decapod_in(
        &repo,
        &xdg,
        &["integration", "remove", "--authorize", "--purge-state"],
    );
    assert!(!purge_denied.status.success());
    assert!(
        String::from_utf8_lossy(&purge_denied.stderr).contains("PERMANENT_STATE_DELETION_BOUNDARY")
    );

    let gov_denied = run_decapod_in(
        &repo,
        &xdg,
        &[
            "integration",
            "remove",
            "--authorize",
            "--remove-protected-governance",
        ],
    );
    assert!(!gov_denied.status.success());
    assert!(
        String::from_utf8_lossy(&gov_denied.stderr)
            .contains("PROTECTED_GOVERNANCE_REMOVAL_BLOCKED")
    );

    // 3. Remove (authorized, bounded)
    let remove_json = parse_json_stdout(&run_decapod_in(
        &repo,
        &xdg,
        &["integration", "remove", "--authorize", "--format", "json"],
    ));
    assert_eq!(remove_json["status"], serde_json::json!("removed"));
    assert!(!repo.join("AGENTS.md").exists());
    assert!(!repo.join("CLAUDE.md").exists());
    assert!(!repo.join("CODEX.md").exists());
    assert!(!repo.join(".git/hooks/commit-msg").exists());
    assert!(!repo.join(".github/workflows/decapod-validate.yml").exists());

    // Preserved authored/customized/state artifacts
    assert_eq!(
        fs::read_to_string(repo.join("AGENTS.md.bak")).unwrap(),
        user_agents
    );
    assert_eq!(
        fs::read_to_string(repo.join("GEMINI.md")).unwrap(),
        custom_gemini
    );
    assert_eq!(
        fs::read_to_string(repo.join(".git/hooks/pre-commit")).unwrap(),
        user_pre_commit
    );
    assert_eq!(
        fs::read_to_string(repo.join(".gitignore")).unwrap(),
        user_gitignore
    );
    assert_eq!(fs::read_to_string(&cfg_path).unwrap(), cfg_text);
    assert_eq!(fs::read_to_string(&override_path).unwrap(), override_text);
    assert_eq!(
        fs::read(repo.join(".decapod/governance.json")).unwrap(),
        governance_before
    );
    assert!(repo.join(".decapod/data/decapod.db").exists());
    assert!(repo.join(INTEGRATION_RESTORE_MANIFEST_PATH).exists());

    // Repeated remove is idempotent and preserves manifest
    let remove_again = parse_json_stdout(&run_decapod_in(
        &repo,
        &xdg,
        &["integration", "remove", "--authorize", "--format", "json"],
    ));
    assert_eq!(remove_again["status"], serde_json::json!("already_removed"));
    assert_eq!(remove_again["idempotent_replay"], serde_json::json!(true));

    // 4. Restore from manifest: exact byte-for-byte restoration + idempotent replay
    let restore_json = parse_json_stdout(&run_decapod_in(
        &repo,
        &xdg,
        &["integration", "restore", "--format", "json"],
    ));
    assert_eq!(restore_json["status"], serde_json::json!("restored"));
    assert_eq!(
        fs::read(repo.join("AGENTS.md")).unwrap(),
        agents_before_remove
    );
    assert_eq!(
        fs::read(repo.join("CLAUDE.md")).unwrap(),
        claude_before_remove
    );
    assert_eq!(
        fs::read(repo.join("CODEX.md")).unwrap(),
        codex_before_remove
    );
    assert_eq!(
        fs::read(repo.join(".git/hooks/commit-msg")).unwrap(),
        commit_msg_before_remove
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(repo.join(".git/hooks/commit-msg"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o755,
            "restored commit-msg hook must retain 0755 mode"
        );
    }
    let gi_restored = fs::read_to_string(repo.join(".gitignore")).unwrap();
    assert!(gi_restored.contains("/dist/"));
    assert!(gi_restored.contains(".decapod/data"));

    let restore_again = parse_json_stdout(&run_decapod_in(
        &repo,
        &xdg,
        &["integration", "restore", "--format", "json"],
    ));
    assert_eq!(
        restore_again["status"],
        serde_json::json!("already_restored")
    );
    assert_eq!(restore_again["idempotent_replay"], serde_json::json!(true));

    // 5. Remove again, then re-init via `decapod init` without losing data/custom settings
    assert!(
        run_decapod_in(&repo, &xdg, &["integration", "remove", "--authorize"])
            .status
            .success()
    );
    init_repo(&repo, &xdg, &[]);
    assert!(repo.join("AGENTS.md").exists());
    assert!(repo.join(".decapod/managed/specs/INTENT.md").exists());
    assert_eq!(fs::read_to_string(&override_path).unwrap(), override_text);
    assert!(fs::read_to_string(&cfg_path).unwrap().contains("keep_me"));
    let todo_get = parse_json_stdout(&run_decapod_in(
        &repo,
        &xdg,
        &["todo", "get", "--id", &task_id, "--format", "json"],
    ));
    assert_eq!(todo_get["item"]["status"], serde_json::json!("done"));
}

#[test]
fn refuses_active_worktrees_and_leases_and_symlinks() {
    let (_tmp, repo, xdg) = setup_git_repo();
    init_repo(&repo, &xdg, &[]);
    run_git(&repo, &["add", "-A"]);
    run_git(
        &repo,
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-q",
            "-m",
            "commit decapod init",
        ],
    );

    // 1. Active todo lease + registered worktree under .decapod/workspaces/
    let add = parse_json_stdout(&run_decapod_in(
        &repo,
        &xdg,
        &["todo", "add", "Worktree custody", "--format", "json"],
    ));
    let task_id = add["id"].as_str().unwrap().to_string();
    assert!(
        run_decapod_in(&repo, &xdg, &["todo", "claim", "--id", &task_id])
            .status
            .success()
    );
    let ws_ensure = run_decapod_in(&repo, &xdg, &["workspace", "ensure"]);
    assert!(
        ws_ensure.status.success(),
        "workspace ensure failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&ws_ensure.stdout),
        String::from_utf8_lossy(&ws_ensure.stderr)
    );

    let remove_blocked = run_decapod_in(&repo, &xdg, &["integration", "remove", "--authorize"]);
    assert!(!remove_blocked.status.success());
    let stderr = String::from_utf8_lossy(&remove_blocked.stderr);
    assert!(
        stderr.contains("INTEGRATION_ACTIVE_CUSTODY"),
        "stderr: {stderr}"
    );
    assert!(repo.join("AGENTS.md").exists());

    // Clean up worktree & complete task so custody is clear, then test symlink refusal
    let wt_dir = fs::read_dir(repo.join(".decapod/workspaces"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_dir() && p.file_name().unwrap() != ".staging")
        .expect("worktree dir");
    run_git(
        &repo,
        &["worktree", "remove", "--force", &wt_dir.to_string_lossy()],
    );
    assert!(
        run_decapod_in(&repo, &xdg, &["todo", "done", "--id", &task_id])
            .status
            .success()
    );

    // 2. Symlink replacement on CODEX.md must fail closed
    #[cfg(unix)]
    {
        let external = repo.parent().unwrap().join("outside.txt");
        fs::write(&external, "outside\n").unwrap();
        fs::remove_file(repo.join("CODEX.md")).unwrap();
        std::os::unix::fs::symlink(&external, repo.join("CODEX.md")).unwrap();

        let symlink_blocked =
            run_decapod_in(&repo, &xdg, &["integration", "remove", "--authorize"]);
        assert!(!symlink_blocked.status.success());
        assert!(
            String::from_utf8_lossy(&symlink_blocked.stderr)
                .contains("INTEGRATION_SYMLINK_REFUSED")
        );
        // Nothing else deleted when blocked
        assert!(repo.join("AGENTS.md").exists());
        assert!(repo.join(&external).exists());
    }
}

#[test]
fn interruption_recovery_and_absent_or_older_manifests() {
    let (_tmp, repo, xdg) = setup_git_repo();
    init_repo(&repo, &xdg, &[]);

    // Simulate interrupted removal: remove one file manually after writing an in_progress manifest
    // or test absent/unsupported restore manifest first.
    let restore_absent = run_decapod_in(&repo, &xdg, &["integration", "restore"]);
    assert!(!restore_absent.status.success());
    assert!(String::from_utf8_lossy(&restore_absent.stderr).contains("RESTORE_MANIFEST_MISSING"));

    // Write unsupported restore manifest version -> fail closed
    let manifest_path = repo.join(INTEGRATION_RESTORE_MANIFEST_PATH);
    fs::write(
        &manifest_path,
        r#"{"schema_version":"99.0.0","status":"applied","decapod_release":"9.9.9","created_at":"1Z","updated_at":"1Z","files":[]}"#,
    )
    .unwrap();
    let restore_future = run_decapod_in(&repo, &xdg, &["integration", "restore"]);
    assert!(!restore_future.status.success());
    assert!(
        String::from_utf8_lossy(&restore_future.stderr)
            .contains("RESTORE_MANIFEST_UNSUPPORTED_VERSION")
    );

    // Older compatible manifest (0.9.0) succeeds
    fs::write(
        &manifest_path,
        r#"{"schema_version":"0.9.0","status":"applied","decapod_release":"0.140.0","created_at":"1Z","updated_at":"1Z","files":[]}"#,
    )
    .unwrap();
    let restore_old = parse_json_stdout(&run_decapod_in(
        &repo,
        &xdg,
        &["integration", "restore", "--format", "json"],
    ));
    assert_eq!(restore_old["status"], serde_json::json!("already_restored"));

    // Clean up test manifest, run authorized removal, then simulate interrupted half-applied state
    fs::remove_file(&manifest_path).unwrap();
    assert!(
        run_decapod_in(&repo, &xdg, &["integration", "remove", "--authorize"])
            .status
            .success()
    );
    // Simulate partial/interrupted state: mark manifest status="in_progress" and restore one file manually
    let mut m: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    m["status"] = serde_json::json!("in_progress");
    fs::write(&manifest_path, serde_json::to_vec_pretty(&m).unwrap()).unwrap();

    // Retry removal -> completes idempotently without losing prior manifest entries
    let retry_remove = parse_json_stdout(&run_decapod_in(
        &repo,
        &xdg,
        &["integration", "remove", "--authorize", "--format", "json"],
    ));
    assert_eq!(retry_remove["idempotent_replay"], serde_json::json!(true));
    assert!(
        parse_json_stdout(&run_decapod_in(
            &repo,
            &xdg,
            &["integration", "restore", "--format", "json"]
        ))["restored_paths"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "AGENTS.md")
    );

    // Test legacy repo with absent integration_ownership.json
    fs::remove_file(repo.join(INTEGRATION_OWNERSHIP_PATH)).unwrap();
    fs::remove_file(&manifest_path).unwrap();
    let legacy_remove = parse_json_stdout(&run_decapod_in(
        &repo,
        &xdg,
        &["integration", "remove", "--authorize", "--format", "json"],
    ));
    assert!(
        legacy_remove["removed_paths"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "AGENTS.md")
    );
    // Legacy config.toml without ownership manifest is conservatively preserved
    assert!(repo.join(".decapod/config.toml").exists());
}
