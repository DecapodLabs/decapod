use decapod::core::todo;
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

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

fn contract_fixture() -> tempfile::TempDir {
    let mut builder = tempfile::Builder::new();
    builder.prefix("decapod-cli-contract-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    builder.tempdir().expect("private CLI contract fixture")
}

fn run_decapod(args: &[&str]) -> String {
    // Help/schema are embedded contracts, not mutations of Bazel runfiles.
    let fixture = contract_fixture();
    let initialized = Command::new(resolve_decapod_bin())
        .current_dir(fixture.path())
        .env("XDG_CONFIG_HOME", fixture.path().join(".config"))
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .args(["init", "--force"])
        .output()
        .expect("initialize private contract fixture");
    assert!(
        initialized.status.success(),
        "fixture init failed: {initialized:?}"
    );
    let output = Command::new(resolve_decapod_bin())
        .current_dir(fixture.path())
        .env("XDG_CONFIG_HOME", fixture.path().join(".config"))
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .args(args)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("failed to execute decapod");
    assert!(
        output.status.success(),
        "decapod {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn todo_help_schema_and_docs_stay_in_sync() {
    let expected = [
        "add",
        "list",
        "get",
        "done",
        "archive",
        "comment",
        "edit",
        "claim",
        "release",
        "rebuild",
        "categories",
        "register-agent",
        "ownerships",
        "heartbeat",
        "presence",
        "worker-run",
        "handoff",
        "add-owner",
        "remove-owner",
        "list-owners",
        "register-expertise",
        "expertise",
    ];

    let help = run_decapod(&["todo", "--help"]);
    for command in &expected {
        let re = Regex::new(&format!(r"(?m)^\s+{}\s+", regex::escape(command)))
            .expect("valid help regex");
        assert!(re.is_match(&help), "todo --help missing command: {command}");
    }

    let schema = todo::schema();
    let schema_cmds: HashSet<String> = schema["commands"]
        .as_array()
        .expect("commands array")
        .iter()
        .filter_map(|item| {
            item.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    for command in &expected {
        assert!(
            schema_cmds.contains(*command),
            "todo schema missing command: {command}"
        );
    }
}

#[test]
fn container_help_schema_and_docs_stay_in_sync() {
    let help = run_decapod(&["auto", "container", "run", "--help"]);
    for flag in [
        "--agent",
        "--cmd",
        "--task-id",
        "--push",
        "--pr",
        "--pr-base",
        "--pr-title",
        "--pr-body",
        "--keep-worktree",
        "--inherit-env",
        "--local-only",
    ] {
        assert!(
            help.contains(flag),
            "container run --help missing flag: {flag}"
        );
    }

    let schema_out = run_decapod(&[
        "data",
        "schema",
        "--subsystem",
        "container",
        "--deterministic",
    ]);
    for field in [
        "\"task_id\"",
        "\"pr\"",
        "\"pr_base\"",
        "\"pr_title\"",
        "\"pr_body\"",
        "\"keep_worktree\"",
        "\"inherit_env\"",
        "\"local_only\"",
        "\"dockerfile_template\"",
        "\"extra_packages_env\"",
    ] {
        assert!(
            schema_out.contains(field),
            "container schema missing field: {field}"
        );
    }
}

#[cfg(unix)]
#[test]
fn contract_fixture_is_private_at_creation() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = contract_fixture();
    assert_eq!(
        std::fs::metadata(fixture.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}
// The finite source-CI profile executes this target. Keep consumer readiness
// and initialization regressions in that required suite rather than relying
// on Cargo's discovery of otherwise unscheduled standalone test targets.
mod consumer_readiness;
mod init_ci_scaffolding;
mod init_config_behavior;

#[path = "support/justfile_inventory.rs"]
mod justfile_inventory;

// Keep every recovery case in the required selective CLI matrix while balancing
// the two real-process suites within their unchanged 300-second budgets.
#[path = "support/cli_workunit_recovery.rs"]
mod workunit_recovery;

// Real command processes exercise caller capture, policy lookup and persisted
// ownership. Core-only claim tests cannot detect a CLI routing regression.
mod claim_boundaries {
    use decapod::core::bounded_process::BoundedCommand;
    use decapod::core::db::{OptionalExtension, db_connect_for_validate};
    use serde_json::Value;
    use std::fs;
    use std::path::Path;
    use std::process::{Command, Output};
    use std::sync::{Arc, Barrier};
    use std::time::Duration;

    fn run(root: &Path, cwd: &Path, actor: &str, args: &[&str]) -> Output {
        Command::new(super::resolve_decapod_bin())
            .current_dir(cwd)
            .args(args)
            .env("DECAPOD_AGENT_ID", actor)
            .env("DECAPOD_CLAIM_AUTORUN", "0")
            .env("XDG_CONFIG_HOME", root.join("machine-config"))
            .env("XDG_DATA_HOME", root.join("machine-data"))
            .env_remove("DECAPOD_SESSION_PASSWORD")
            .env_remove("DECAPOD_GROUP_BROKER_REQUEST_ID")
            .bounded_output(Duration::from_secs(45))
            .expect("bounded claim CLI")
    }

    fn json(output: Output) -> Value {
        assert!(
            output.status.success(),
            "CLI failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("real CLI JSON")
    }

    fn git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(root)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .bounded_output(Duration::from_secs(15))
            .unwrap();
        assert!(
            output.status.success(),
            "Git: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn fixture() -> tempfile::TempDir {
        let fixture = super::contract_fixture();
        let root = fixture.path();
        git(root, &["init", "-b", "main"]);
        let init = run(
            root,
            root,
            "setup",
            &["init", "--no-ci", "--no-specs", "--no-container-workspaces"],
        );
        assert!(
            init.status.success(),
            "fixture init: {}",
            String::from_utf8_lossy(&init.stderr)
        );
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/a.rs"), "// alpha\n").unwrap();
        fs::write(root.join("src/b.rs"), "// beta\n").unwrap();
        git(root, &["add", "src", ".decapod/config.toml"]);
        git(
            root,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-m",
                "fixture",
            ],
        );
        fixture
    }

    fn add(root: &Path, title: &str, scope: &str) -> String {
        json(run(
            root,
            root,
            "setup",
            &[
                "todo",
                "add",
                title,
                "--scope",
                scope,
                "--dir",
                root.to_str().unwrap(),
                "--format",
                "json",
            ],
        ))["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn claim(root: &Path, cwd: &Path, actor: &str, id: &str, mode: &str) -> Output {
        run(
            root,
            cwd,
            actor,
            &[
                "todo", "claim", "--id", id, "--mode", mode, "--format", "json",
            ],
        )
    }

    fn get(root: &Path, cwd: &Path, id: &str) -> Value {
        json(run(
            root,
            cwd,
            "observer",
            &["todo", "get", "--id", id, "--format", "json"],
        ))
    }

    fn policy(repo: &Path, enabled: bool) {
        let path = repo.join(".decapod/config.toml");
        let text = fs::read_to_string(&path).unwrap();
        let mut config: toml::Value = toml::from_str(&text).unwrap();
        config
            .get_mut("governance")
            .unwrap()
            .as_table_mut()
            .unwrap()
            .insert(
                "allow_basic_shared_claims".into(),
                toml::Value::Boolean(enabled),
            );
        fs::write(path, toml::to_string_pretty(&config).unwrap()).unwrap();
    }

    #[test]
    fn concurrent_disjoint_file_claims_preserve_callers_and_reject_overlap() {
        let fixture = fixture();
        let root = fixture.path();
        let a = add(root, "Amber parcel", r#"paths:["src/a.rs"]"#);
        let b = add(root, "Blue parcel", r#"paths:["src/b.rs"]"#);
        let overlap = add(root, "Copper parcel", r#"paths:["src/a.rs"]"#);
        assert_ne!(a, b);
        assert_ne!(a, overlap);
        let worktree = root.join("beta-worktree");
        git(
            root,
            &[
                "worktree",
                "add",
                "-b",
                "agent/beta",
                worktree.to_str().unwrap(),
            ],
        );
        let barrier = Arc::new(Barrier::new(2));
        let results = std::thread::scope(|threads| {
            let alpha = threads.spawn(|| {
                barrier.wait();
                claim(root, root, "alpha", &a, "exclusive")
            });
            let beta = threads.spawn(|| {
                barrier.wait();
                claim(root, &worktree, "beta", &b, "exclusive")
            });
            // Join both callers before assertions or fixture teardown.
            (alpha.join(), beta.join())
        });
        assert_eq!(json(results.0.unwrap())["result"]["assigned_to"], "alpha");
        assert_eq!(json(results.1.unwrap())["result"]["assigned_to"], "beta");
        let alpha = get(root, &worktree, &a);
        let beta = get(root, root, &b);
        assert_eq!(
            alpha["root"], beta["root"],
            "worktrees must share the canonical store"
        );
        assert_eq!(alpha["item"]["assigned_to"], "alpha");
        assert_eq!(beta["item"]["assigned_to"], "beta");
        let denied = claim(root, &worktree, "gamma", &overlap, "exclusive");
        let denied: Value =
            serde_json::from_slice(&denied.stdout).expect("typed overlap rejection");
        assert_eq!(denied["status"], "conflict");
        assert_eq!(denied["result"]["overlap"], true);
        assert_eq!(get(root, root, &overlap)["item"]["assigned_to"], "");
    }

    #[test]
    fn shared_claim_requires_canonical_opt_in_and_never_elevates_trust() {
        let fixture = fixture();
        let root = fixture.path();
        let task = add(root, "Owned amber parcel", r#"paths:["src/a.rs"]"#);
        let unowned = add(root, "Unowned blue parcel", r#"paths:["src/b.rs"]"#);
        assert_ne!(task, unowned);
        assert_eq!(
            json(claim(root, root, "primary", &task, "exclusive"))["status"],
            "ok"
        );
        let worktree = root.join("member-worktree");
        git(
            root,
            &[
                "worktree",
                "add",
                "-b",
                "agent/member",
                worktree.to_str().unwrap(),
            ],
        );
        policy(&worktree, true);
        let denied = claim(root, &worktree, "member", &task, "shared");
        assert!(
            !denied.status.success(),
            "candidate policy must not grant shared membership"
        );
        assert!(
            String::from_utf8_lossy(&denied.stderr).contains("trust 'basic' < required 'verified'")
        );
        policy(root, true);
        let joined = json(claim(root, &worktree, "member", &task, "shared"));
        assert_eq!(joined["status"], "ok");
        assert_eq!(joined["result"]["mode"], "shared");
        assert_eq!(joined["result"]["assigned_to"], "primary");
        let observed = get(root, &worktree, &task);
        assert_eq!(observed["item"]["assigned_to"], "primary");
        let owners = observed["item"]["owners"].as_array().unwrap();
        assert!(
            owners
                .iter()
                .any(|o| o["agent_id"] == "primary" && o["claim_type"] == "primary")
        );
        assert!(
            owners
                .iter()
                .any(|o| o["agent_id"] == "member" && o["claim_type"] == "secondary")
        );
        let db_path = Path::new(observed["root"].as_str().unwrap()).join("decapod.db");
        let db = db_connect_for_validate(db_path.to_str().unwrap()).unwrap();
        let trust: Option<String> = db
            .query_row(
                "SELECT trust_level FROM agents WHERE agent_id=?1",
                ["member"],
                |row| row.get(0),
            )
            .optional()
            .unwrap();
        assert_eq!(trust.as_deref().unwrap_or("basic"), "basic");
        let raw: String = db.query_row("SELECT payload FROM events WHERE stream='todo' AND subject_id=?1 AND event_type='policy.basic_shared_claim.used' ORDER BY seq DESC LIMIT 1",
            [&task], |row| row.get(0)).unwrap();
        let event: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(event["actor"], "member");
        assert_eq!(event["payload"]["primary_owner_label"], "primary");
        assert_eq!(
            event["payload"]["identity_assurance"],
            "cooperative_not_authenticated"
        );
        assert!(
            event["payload"]["config_digest"]
                .as_str()
                .unwrap()
                .starts_with("sha256:")
        );
        drop(db);
        assert!(
            !claim(root, &worktree, "member", &unowned, "shared")
                .status
                .success()
        );
        policy(root, false);
        assert!(
            !claim(root, &worktree, "later", &task, "shared")
                .status
                .success()
        );
        assert_eq!(get(root, root, &task)["item"]["assigned_to"], "primary");
    }
}
