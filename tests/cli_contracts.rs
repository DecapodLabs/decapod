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
        .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1")
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
        .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1")
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

// The required CLI target must execute these real-terminal setup regressions.
#[cfg(unix)]
mod terminal_onboarding {
    use std::fs::{self, File};
    use std::io::{self, Read, Write};
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::time::{Duration, Instant};

    fn binary() -> PathBuf {
        let cargo = PathBuf::from(env!("CARGO_BIN_EXE_decapod"));
        if let Ok(path) = cargo.canonicalize() {
            return path;
        }
        if let Ok(runfiles) = std::env::var("RUNFILES_DIR") {
            let path = Path::new(&runfiles).join("_main/decapod");
            if path.exists() {
                return path;
            }
        }
        std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("decapod")
    }

    fn terminal_pair() -> (File, File) {
        let (mut master, mut slave) = (-1, -1);
        let mut size = libc::winsize {
            ws_row: 30,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::addr_of_mut!(size),
                )
            },
            0
        );
        let master = unsafe { File::from_raw_fd(master) };
        let slave = unsafe { File::from_raw_fd(slave) };
        (master, slave)
    }

    struct SetupTerminal {
        child: Child,
        master: File,
        slave: File,
        original_flags: libc::tcflag_t,
        output: String,
        _home: tempfile::TempDir,
    }

    impl SetupTerminal {
        fn start(directory: &Path, args: &[&str]) -> Self {
            Self::start_with_env(directory, args, &[])
        }

        fn start_with_env(directory: &Path, args: &[&str], environment: &[(&str, &str)]) -> Self {
            let (master, slave) = terminal_pair();
            let mut termios = unsafe { std::mem::zeroed() };
            assert_eq!(
                unsafe { libc::tcgetattr(slave.as_raw_fd(), &mut termios) },
                0
            );
            let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
            assert_ne!(
                unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
                -1
            );
            let home = tempfile::tempdir().unwrap();
            let mut command = Command::new(binary());
            command
                .args(args)
                .current_dir(directory)
                .env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap_or_default())
                .env("HOME", home.path())
                .env("TERM", "xterm-256color")
                .env("NO_COLOR", "1")
                .envs(environment.iter().copied())
                .stdin(Stdio::from(slave.try_clone().unwrap()))
                .stdout(Stdio::from(slave.try_clone().unwrap()))
                .stderr(Stdio::from(slave.try_clone().unwrap()));
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            Self {
                child: command.spawn().unwrap(),
                master,
                slave,
                original_flags: termios.c_lflag,
                output: String::new(),
                _home: home,
            }
        }

        fn pump(&mut self) {
            let mut bytes = [0; 16384];
            loop {
                match self.master.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(count) => self
                        .output
                        .push_str(&String::from_utf8_lossy(&bytes[..count])),
                    Err(error)
                        if error.kind() == io::ErrorKind::WouldBlock
                            || error.raw_os_error() == Some(libc::EIO) =>
                    {
                        break;
                    }
                    Err(error) => panic!("PTY read: {error}"),
                }
            }
        }

        fn expect(&mut self, text: &str) {
            let start = self.output.len();
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                self.pump();
                if self.output[start..].contains(text) {
                    return;
                }
                assert!(
                    self.child.try_wait().unwrap().is_none(),
                    "setup exited waiting for {text:?}: {}",
                    self.output
                );
                assert!(
                    Instant::now() < deadline,
                    "setup stalled waiting for {text:?}: {}",
                    self.output
                );
                std::thread::sleep(Duration::from_millis(15));
            }
        }

        fn send(&mut self, input: &str) {
            self.master.write_all(input.as_bytes()).unwrap();
        }

        fn finish(&mut self) -> ExitStatus {
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                self.pump();
                if let Some(status) = self.child.try_wait().unwrap() {
                    self.pump();
                    let mut termios = unsafe { std::mem::zeroed() };
                    assert_eq!(
                        unsafe { libc::tcgetattr(self.slave.as_raw_fd(), &mut termios) },
                        0
                    );
                    assert_eq!(
                        termios.c_lflag & (libc::ICANON | libc::ECHO),
                        self.original_flags & (libc::ICANON | libc::ECHO),
                        "terminal mode leaked"
                    );
                    return status;
                }
                assert!(
                    Instant::now() < deadline,
                    "setup failed to terminate: {}",
                    self.output
                );
                std::thread::sleep(Duration::from_millis(15));
            }
        }

        fn recommended(&mut self) {
            self.expect("What are you building?");
            self.send("A quiet place to plan good work.\n");
            self.expect("How would you like to set up?");
            self.send("\r");
            self.expect("Ready when you are");
        }
    }

    impl Drop for SetupTerminal {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[test]
    fn recommended_setup_has_three_decisions_and_an_agent_handoff() {
        let project = tempfile::tempdir().unwrap();
        let mut terminal = SetupTerminal::start(project.path(), &["init", "--dir", "."]);
        terminal.recommended();
        assert!(!terminal.output.contains("Define the user-visible outcome"));
        assert!(
            !terminal
                .output
                .contains("Architecture is not established by repository shape")
        );
        assert!(
            !project.path().join(".git").exists(),
            "Git changed before review"
        );
        assert!(
            !project.path().join(".decapod").exists(),
            "governance changed before review"
        );
        terminal.send("\r");
        assert!(terminal.finish().success(), "{}", terminal.output);
        assert!(
            terminal
                .output
                .contains("Your project is ready for your agent.")
        );
        assert!(
            terminal.output.contains("Required validation has not run")
                || terminal.output.contains("does not run validation")
        );
        assert!(
            !terminal.output.contains("\x1b[1;36m"),
            "NO_COLOR was ignored"
        );
        assert!(!terminal.output.contains("DEBUG:"));
        let config = fs::read_to_string(project.path().join(".decapod/config.toml")).unwrap();
        assert!(config.contains("A quiet place to plan good work."));
        assert!(config.contains("backend = \"local\""));
        assert!(project.path().join("AGENTS.md").exists());
    }

    #[test]
    fn edit_then_cancel_leaves_a_new_project_uncreated() {
        let root = tempfile::tempdir().unwrap();
        let mut terminal = SetupTerminal::start(root.path(), &["init"]);
        terminal.expect("Where would you like to begin?");
        terminal.send("Create a project\r");
        terminal.expect("Project directory");
        terminal.send("new-project\n");
        terminal.recommended();
        terminal.send("Edit choices\r");
        terminal.expect("Make it yours");
        terminal.send("Workspace\r");
        terminal.expect("Where should agent work run?");
        terminal.send("Git worktrees\r");
        terminal.expect("Make it yours");
        terminal.send("\r");
        terminal.expect("Ready when you are");
        terminal.send("Cancel\r");
        assert!(terminal.finish().success(), "{}", terminal.output);
        assert!(!root.path().join("new-project").exists());
        assert!(!root.path().join(".git").exists());
        assert!(terminal.output.contains("No setup changes were applied."));
    }

    #[test]
    fn cloud_review_can_cancel_without_creating_state_or_contacting_the_service() {
        let project = tempfile::tempdir().unwrap();
        let service = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        service.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", service.local_addr().unwrap());
        let mut terminal = SetupTerminal::start_with_env(
            project.path(),
            &["init", "--dir", ".", "--backend", "cloud"],
            &[
                ("DECAPOD_TEST_MODE", "1"),
                ("DECAPOD_CLOUD_TEST_ORIGIN", &origin),
            ],
        );
        terminal.recommended();
        assert!(
            terminal
                .output
                .contains("service confirms repository access")
        );
        assert!(!terminal.output.contains("Cloud seats"));
        assert!(!terminal.output.contains("USD"));
        terminal.send("Cancel\r");
        assert!(terminal.finish().success(), "{}", terminal.output);
        assert!(!project.path().join(".git").exists());
        assert!(!project.path().join(".decapod").exists());
        assert!(
            matches!(service.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock)
        );
    }

    #[test]
    fn lone_escape_cancels_without_hanging_or_leaking_raw_mode() {
        let project = tempfile::tempdir().unwrap();
        let mut terminal = SetupTerminal::start(project.path(), &["init", "--dir", "."]);
        terminal.expect("What are you building?");
        terminal.send("\n");
        terminal.expect("How would you like to set up?");
        terminal.send("\x1b");
        assert!(!terminal.finish().success());
        assert!(terminal.output.contains("Setup cancelled"));
        assert!(!project.path().join(".decapod").exists());
        assert!(!project.path().join(".git").exists());
    }

    #[test]
    fn preview_creates_neither_git_nor_governance_state() {
        let project = tempfile::tempdir().unwrap();
        let mut terminal =
            SetupTerminal::start(project.path(), &["init", "--dir", ".", "--dry-run"]);
        terminal.recommended();
        terminal.send("\r");
        assert!(terminal.finish().success(), "{}", terminal.output);
        assert!(terminal.output.contains("Preview complete."));
        assert_eq!(
            fs::read_dir(project.path()).unwrap().count(),
            0,
            "preview wrote project state"
        );
    }

    #[test]
    fn redirected_output_never_waits_for_terminal_setup_input() {
        let project = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let (_master, slave) = terminal_pair();
        let mut child = Command::new(binary())
            .args(["init", "--dir", ".", "--dry-run"])
            .current_dir(project.path())
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", home.path())
            .env("TERM", "xterm-256color")
            .stdin(Stdio::from(slave))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                let _ = child.kill();
                let output = child.wait_with_output().unwrap();
                panic!(
                    "redirected output opened an interactive prompt: {}",
                    String::from_utf8_lossy(&output.stdout)
                );
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!stdout.contains("What are you building?"));
        assert!(!stdout.contains("How would you like to set up?"));
        assert_eq!(fs::read_dir(project.path()).unwrap().count(), 0);
    }
}

#[path = "support/justfile_inventory.rs"]
mod justfile_inventory;

#[path = "support/storage_inventory.rs"]
mod storage_inventory;

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
            .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1")
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
    fn concurrent_disjoint_file_claims_preserve_callers_and_allow_logical_overlap() {
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
        // Logical file overlap is advisory, not exclusive custody of a
        // shared writable checkout. Both independent tasks can be claimed.
        let admitted = claim(root, &worktree, "gamma", &overlap, "exclusive");
        let admitted: Value =
            serde_json::from_slice(&admitted.stdout).expect("typed claim outcome");
        assert_eq!(admitted["status"], "ok");
        assert_eq!(get(root, root, &overlap)["item"]["assigned_to"], "gamma");
        assert_eq!(get(root, root, &a)["item"]["assigned_to"], "alpha");
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
