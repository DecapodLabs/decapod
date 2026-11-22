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
        "delegate",
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
        "--cgroup-parent",
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
        "\"cgroup_parent\"",
        "\"cgroup_parent_env\"",
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

// Register this acceptance control at the selected CLI target boundary. Changes
// confined to a shared consumer module are not selected by the accepted recipe.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn accepted_local_check_script_cannot_be_replaced_by_candidate_repinning() {
    consumer_readiness::candidate_cannot_repin_a_rewritten_accepted_check_script();
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn accepted_tenant_control_rejects_green_positive_tests_and_requires_fresh_repair_proof() {
    consumer_readiness::accepted_tenant_negative_control_rejects_green_positives_and_proves_repair(
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn selected_roots_require_current_mapping_and_real_combined_behavior() {
    consumer_readiness::selected_roots_reject_omission_and_require_executed_repair();
}

#[test]
fn required_path_cannot_be_supplied_by_unmediated_green_artifact_evidence() {
    consumer_readiness::accepted_required_path_rejects_unmediated_green_and_keeps_optional_unknown(
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn global_triangle_requires_independently_accepted_repair_and_current_proof() {
    consumer_readiness::accepted_global_triangle_requires_policy_repair_and_fresh_mapping();
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn counterfeit_origin_hash_cannot_replace_retained_source_and_repair_stays_possible() {
    consumer_readiness::source_history_refuses_counterfeit_origin_then_accepts_restored_fresh_source();
}

mod init_ci_scaffolding;
mod init_config_behavior;

// The required CLI target must execute these real-terminal setup regressions.
#[cfg(unix)]
mod terminal_onboarding {
    use std::fs::{self, File};
    use std::io::{self, Read, Write};
    use std::os::fd::{AsRawFd, FromRawFd, RawFd};
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

    fn terminal_settings(fd: RawFd) -> io::Result<libc::termios> {
        let mut settings = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut settings) } == 0 {
            Ok(settings)
        } else {
            Err(io::Error::last_os_error())
        }
    }

    impl SetupTerminal {
        fn start(directory: &Path, args: &[&str]) -> Self {
            Self::start_with_env(directory, args, &[])
        }

        fn start_with_env(directory: &Path, args: &[&str], environment: &[(&str, &str)]) -> Self {
            Self::start_in_session(directory, args, environment, true)
        }

        fn start_in_session(
            directory: &Path,
            args: &[&str],
            environment: &[(&str, &str)],
            attach_controlling_terminal: bool,
        ) -> Self {
            let (master, slave) = terminal_pair();
            let termios = terminal_settings(slave.as_raw_fd()).unwrap();
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
                command.pre_exec(move || {
                    if libc::setsid() < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    if attach_controlling_terminal && libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
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
                    let settings = terminal_settings(self.slave.as_raw_fd());
                    #[cfg(target_os = "macos")]
                    let settings = settings.or_else(|error| {
                        // The session leader's exit can revoke the slave on macOS.
                        // The master still exposes the terminal mode it left behind.
                        if error.raw_os_error() == Some(libc::ENOTTY) {
                            terminal_settings(self.master.as_raw_fd())
                        } else {
                            Err(error)
                        }
                    });
                    let termios = settings.expect("read terminal mode after setup exits");
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
        for attach_controlling_terminal in [true, false] {
            let mut terminal = SetupTerminal::start_in_session(
                project.path(),
                &["init", "--dir", "."],
                &[],
                attach_controlling_terminal,
            );
            terminal.expect("What are you building?");
            terminal.send("\n");
            // The question is printed before the selector switches to raw input.
            terminal.expect("1. Recommended");
            let mode = terminal_settings(terminal.slave.as_raw_fd()).unwrap();
            assert_eq!(
                mode.c_lflag & (libc::ICANON | libc::ECHO),
                0,
                "selector did not enter raw mode (controlling terminal attached: {attach_controlling_terminal})"
            );
            terminal.send("\x1b[B");
            terminal.expect("› Customize");
            terminal.send("\x1b");
            assert!(!terminal.finish().success());
            assert!(terminal.output.contains("Setup cancelled"));
            assert!(!project.path().join(".decapod").exists());
            assert!(!project.path().join(".git").exists());
        }
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
        let (event_id, ts, actor, subject_kind, subject_id, raw): (
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
        ) = db
            .query_row(
                "SELECT event_id, ts, actor, subject_kind, subject_id, payload
                 FROM events
                 WHERE stream='todo' AND subject_id=?1 AND event_type='policy.basic_shared_claim.used'
                 ORDER BY seq DESC LIMIT 1",
                [&task],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert!(raw.contains("\"_ce_v1\""));
        let event = decapod::core::events::reconstruct_stored_payload(
            "todo",
            &event_id,
            &ts,
            "policy.basic_shared_claim.used",
            &actor,
            subject_kind.as_deref(),
            subject_id.as_deref(),
            &raw,
        )
        .unwrap();
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

// The required CLI target must execute the real governed maintenance cases.
// The standalone target remains available for focused reproduction.
#[path = "sqlite_maintenance.rs"]
mod sqlite_maintenance;

mod effective_grant_cli_contracts {
    use serde_json::Value;
    use std::path::Path;
    use std::process::{Command, Output};
    use tempfile::TempDir;

    fn decapod_bin() -> std::path::PathBuf {
        super::resolve_decapod_bin()
    }

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Decapod Test")
            .env("GIT_AUTHOR_EMAIL", "test@decapod.local")
            .env("GIT_COMMITTER_NAME", "Decapod Test")
            .env("GIT_COMMITTER_EMAIL", "test@decapod.local")
            .args(args)
            .output()
            .expect("run git");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn run_cli(dir: &Path, agent: &str, session_pw: Option<&str>, args: &[&str]) -> Output {
        let mut cmd = Command::new(decapod_bin());
        cmd.current_dir(dir)
            .env("HOME", dir)
            .env("XDG_CONFIG_HOME", dir.join(".config"))
            .env("DECAPOD_AGENT_ID", agent)
            .env("DECAPOD_STORAGE_OWNER", "1")
            .env("DECAPOD_CLAIM_AUTORUN", "0")
            .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1");
        if let Some(pw) = session_pw {
            cmd.env("DECAPOD_SESSION_PASSWORD", pw);
        } else {
            cmd.env_remove("DECAPOD_SESSION_PASSWORD");
        }
        cmd.args(args).output().expect("run decapod")
    }

    fn setup_repo_with_session() -> (TempDir, String) {
        let fixture = super::contract_fixture();
        let root = fixture.path();
        git(root, &["init", "-b", "master"]);
        let init = run_cli(root, "coordinator", None, &["init", "--force"]);
        assert!(
            init.status.success(),
            "init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );
        std::fs::create_dir_all(root.join("src/api")).unwrap();
        std::fs::write(root.join("src/api/routes.rs"), "pub fn ok() {}\n").unwrap();
        std::fs::create_dir_all(root.join(".github/workflows")).unwrap();
        std::fs::write(root.join(".github/workflows/ci.yml"), "name: ci\n").unwrap();

        let cfg = r#"schema_version = "1.0.0"

[init]
specs = true
ci = false
diagram_style = "ascii"
entrypoints = ["AGENTS.md"]

[repo]
name = "cli-grant-test"
summary = "Effective grant CLI test"
base_branch = "master"

[governance]
protected_paths = [".github/workflows/**"]
approval_categories = ["destructive_operations"]
"#;
        std::fs::write(root.join(".decapod/config.toml"), cfg).unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-m", "init"]);
        git(root, &["checkout", "-b", "agent/coordinator/grant-test"]);

        let sess = run_cli(root, "coordinator", None, &["session", "acquire"]);
        assert!(
            sess.status.success(),
            "session acquire failed: {}",
            String::from_utf8_lossy(&sess.stderr)
        );
        let sess_stdout = String::from_utf8_lossy(&sess.stdout);
        let pw = sess_stdout
            .lines()
            .find_map(|line| {
                let t = line.trim();
                t.strip_prefix("Password:")
                    .or_else(|| t.strip_prefix("DECAPOD_SESSION_PASSWORD="))
                    .or_else(|| t.strip_prefix("export DECAPOD_SESSION_PASSWORD="))
            })
            .map(|s| s.trim().trim_matches('\'').trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| panic!("session password line missing in:\n{sess_stdout}"));
        (fixture, pw)
    }

    #[test]
    fn red_green_5_cli_rpc_grant_refusal_lawful_repair_and_fresh_authorization() {
        let (fixture, pw) = setup_repo_with_session();
        let root = fixture.path();

        let parent_out = run_cli(
            root,
            "coordinator",
            Some(&pw),
            &[
                "todo",
                "add",
                "Parent grant test",
                "--scope",
                "allow:[src/**];deny:[.github/workflows/**]",
                "--dir",
                root.to_str().unwrap(),
                "--format",
                "json",
            ],
        );
        assert!(parent_out.status.success());
        let parent_id = serde_json::from_slice::<Value>(&parent_out.stdout).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        let claim_p = run_cli(
            root,
            "coordinator",
            Some(&pw),
            &["todo", "claim", "--id", &parent_id, "--format", "json"],
        );
        assert!(claim_p.status.success());

        let child_out = run_cli(
            root,
            "coordinator",
            Some(&pw),
            &[
                "todo",
                "add",
                "Child API task",
                "--parent",
                &parent_id,
                "--scope",
                "allow:[src/api/**]",
                "--dir",
                root.to_str().unwrap(),
                "--format",
                "json",
            ],
        );
        assert!(child_out.status.success());
        let child_id = serde_json::from_slice::<Value>(&child_out.stdout).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        let claim_c = run_cli(
            root,
            "coordinator",
            Some(&pw),
            &["todo", "claim", "--id", &child_id, "--format", "json"],
        );
        assert!(claim_c.status.success());

        // 1) RPC policy.grant.explain denies .github/workflows/ci.yml via parent denial + narrow scope.
        let rpc_params = serde_json::json!({
            "command": "workspace.publish",
            "path": ".github/workflows/ci.yml",
            "task_id": child_id,
        })
        .to_string();
        let rpc_out = run_cli(
            root,
            "coordinator",
            Some(&pw),
            &[
                "rpc",
                "--op",
                "policy.grant.explain",
                "--params",
                &rpc_params,
            ],
        );
        assert!(
            rpc_out.status.success(),
            "rpc policy.grant.explain failed: {}",
            String::from_utf8_lossy(&rpc_out.stderr)
        );
        let rpc_json: Value = serde_json::from_slice(&rpc_out.stdout).unwrap();
        assert_eq!(
            rpc_json["success"], true,
            "rpc policy.grant.explain failed: {rpc_json:#}"
        );
        assert_eq!(rpc_json["result"]["decision"], "denied");
        assert!(rpc_json["result"]["prestate"]["policy_revision"].is_string());

        // 2) Operator acquires fixture session; basic agent sees denied trust floor and cannot self-approve,
        // while operator sees unproven before approval and todo.archive refuses until approved.
        let basic_grant = run_cli(
            root,
            "coordinator",
            Some(&pw),
            &[
                "govern",
                "policy",
                "grant",
                "--command",
                "task.archive",
                "--path",
                &child_id,
                "--task-id",
                &child_id,
                "--format",
                "json",
            ],
        );
        assert!(basic_grant.status.success());
        let g_basic: Value = serde_json::from_slice(&basic_grant.stdout).unwrap();
        assert_eq!(g_basic["decision"], "denied");

        let op_sess = run_cli(root, "operator", None, &["session", "acquire"]);
        assert!(op_sess.status.success());
        let op_stdout = String::from_utf8_lossy(&op_sess.stdout);
        let op_pw = op_stdout
            .lines()
            .find_map(|line| {
                let t = line.trim();
                t.strip_prefix("Password:")
                    .or_else(|| t.strip_prefix("DECAPOD_SESSION_PASSWORD="))
                    .or_else(|| t.strip_prefix("export DECAPOD_SESSION_PASSWORD="))
            })
            .map(|s| s.trim().trim_matches('\'').trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| panic!("operator session password missing in:\n{op_stdout}"));

        let op_child_out = run_cli(
            root,
            "operator",
            Some(&op_pw),
            &[
                "todo",
                "add",
                "Operator child task",
                "--parent",
                &parent_id,
                "--scope",
                "allow:[src/api/**]",
                "--dir",
                root.to_str().unwrap(),
                "--format",
                "json",
            ],
        );
        assert!(op_child_out.status.success());
        let op_child_id = serde_json::from_slice::<Value>(&op_child_out.stdout).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            run_cli(
                root,
                "operator",
                Some(&op_pw),
                &["todo", "claim", "--id", &op_child_id, "--format", "json"],
            )
            .status
            .success()
        );

        let grant_before = run_cli(
            root,
            "operator",
            Some(&op_pw),
            &[
                "govern",
                "policy",
                "grant",
                "--command",
                "task.archive",
                "--path",
                &op_child_id,
                "--task-id",
                &op_child_id,
                "--format",
                "json",
            ],
        );
        assert!(grant_before.status.success());
        let g_before: Value = serde_json::from_slice(&grant_before.stdout).unwrap();
        assert_eq!(g_before["decision"], "unproven");
        assert_eq!(g_before["approval_required"], true);
        let fp = g_before["target_fingerprint"].as_str().unwrap().to_string();

        let archive_refused = run_cli(
            root,
            "operator",
            Some(&op_pw),
            &["todo", "archive", "--id", &op_child_id],
        );
        assert!(
            !archive_refused.status.success(),
            "unapproved archive must fail"
        );

        // 3) Agent cannot self-approve; lawful repair requires operator session in the fixture.
        let self_approve = run_cli(
            root,
            "coordinator",
            Some(&pw),
            &[
                "govern", "policy", "approve", "--id", &fp, "--scope", "global",
            ],
        );
        assert!(
            !self_approve.status.success(),
            "basic agent must not self-approve policy actions"
        );

        let approve = run_cli(
            root,
            "operator",
            Some(&op_pw),
            &[
                "govern", "policy", "approve", "--id", &fp, "--scope", "global",
            ],
        );
        assert!(
            approve.status.success(),
            "operator policy approve failed: {}",
            String::from_utf8_lossy(&approve.stderr)
        );

        let grant_after = run_cli(
            root,
            "operator",
            Some(&op_pw),
            &[
                "govern",
                "policy",
                "grant",
                "--command",
                "task.archive",
                "--path",
                &op_child_id,
                "--task-id",
                &op_child_id,
                "--format",
                "json",
            ],
        );
        assert!(grant_after.status.success());
        let g_after: Value = serde_json::from_slice(&grant_after.stdout).unwrap();
        assert_eq!(g_after["decision"], "allowed", "{g_after:?}");
        assert!(
            !g_after["approval_ids_matched"]
                .as_array()
                .unwrap()
                .is_empty()
        );

        let archive_ok = run_cli(
            root,
            "operator",
            Some(&op_pw),
            &["todo", "archive", "--id", &op_child_id, "--format", "json"],
        );
        assert!(
            archive_ok.status.success(),
            "archive after approval failed: {}",
            String::from_utf8_lossy(&archive_ok.stderr)
        );
    }

    #[test]
    fn cli_cumulative_delegation_budget_rejects_sibling_overflow() {
        let (fixture, pw) = setup_repo_with_session();
        let root = fixture.path();

        let p_out = run_cli(
            root,
            "coordinator",
            Some(&pw),
            &[
                "todo",
                "add",
                "Shared parent budget",
                "--scope",
                "allow:[src/**]",
                "--dir",
                root.to_str().unwrap(),
                "--format",
                "json",
            ],
        );
        let parent_id = serde_json::from_slice::<Value>(&p_out.stdout).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            run_cli(
                root,
                "coordinator",
                Some(&pw),
                &["todo", "claim", "--id", &parent_id, "--format", "json"],
            )
            .status
            .success()
        );

        let mut children = Vec::new();
        for (label, cap) in [("c1", "2"), ("c2", "2"), ("c3", "1")] {
            let out = run_cli(
                root,
                "coordinator",
                Some(&pw),
                &[
                    "todo",
                    "add",
                    label,
                    "--parent",
                    &parent_id,
                    "--scope",
                    "allow:[src/api/**]",
                    "--dir",
                    root.to_str().unwrap(),
                    "--format",
                    "json",
                ],
            );
            let cid = serde_json::from_slice::<Value>(&out.stdout).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string();
            let dlg = run_cli(
                root,
                "coordinator",
                Some(&pw),
                &[
                    "todo",
                    "delegate",
                    "--parent",
                    &parent_id,
                    "--child",
                    &cid,
                    "--allow",
                    "src/api/**",
                    "--budget-total",
                    "3",
                    "--budget-child",
                    cap,
                    "--depth",
                    "1",
                    "--format",
                    "json",
                ],
            );
            assert!(
                dlg.status.success(),
                "delegate {label} failed: {}",
                String::from_utf8_lossy(&dlg.stderr)
            );
            children.push(cid);
        }

        // First claim reserves 2/3 -> succeeds.
        assert!(
            run_cli(
                root,
                "worker-1",
                Some(&pw),
                &["todo", "claim", "--id", &children[0], "--format", "json"],
            )
            .status
            .success()
        );
        // Second claim wants 2 more -> 4 > 3 -> fails atomically.
        let c2_claim = run_cli(
            root,
            "worker-2",
            Some(&pw),
            &["todo", "claim", "--id", &children[1], "--format", "json"],
        );
        assert!(!c2_claim.status.success());
        assert!(
            String::from_utf8_lossy(&c2_claim.stderr)
                .contains("CUMULATIVE_DELEGATION_BUDGET_EXCEEDED")
        );
        // Third claim wants 1 -> 2 + 1 = 3 <= 3 -> succeeds.
        assert!(
            run_cli(
                root,
                "worker-3",
                Some(&pw),
                &["todo", "claim", "--id", &children[2], "--format", "json"],
            )
            .status
            .success()
        );
    }

    #[test]
    fn guidance_surfaces_exclude_maintainer_entrypoint_drift_setting() {
        let (fixture, pw) = setup_repo_with_session();
        let root = fixture.path();
        let needle = "_allow_entrypoint_version_drift";

        for args in [
            vec!["--help"],
            vec!["validate", "--help"],
            vec!["init", "--help"],
            vec!["govern", "--help"],
            vec!["docs", "ingest"],
            vec!["rpc", "--op", "context.resolve", "--params", "{}"],
        ] {
            let out = run_cli(root, "coordinator", Some(&pw), &args);
            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(
                !combined.contains(needle),
                "decapod {:?} leaked maintainer setting {needle}",
                args
            );
        }
    }
}

#[test]
fn cloud_admin_help_secrets_and_rpc_capabilities_stay_in_sync() {
    let cloud_help = run_decapod(&["cloud", "--help"]);
    for sub in [
        "orgs", "members", "invites", "identity", "billing", "admins", "recovery", "policies",
        "upgrades",
    ] {
        let re = Regex::new(&format!(r"(?m)^\s+{}\s+", regex::escape(sub))).unwrap();
        assert!(re.is_match(&cloud_help), "cloud --help missing `{sub}`");
    }

    let project_help = run_decapod(&["cloud", "project", "--help"]);
    for sub in ["use", "create", "enroll"] {
        assert!(
            project_help.contains(sub),
            "cloud project --help missing `{sub}`"
        );
    }

    let members_help = run_decapod(&["cloud", "members", "--help"]);
    for sub in [
        "list",
        "assign",
        "revoke-seat",
        "revoke-member",
        "transfer-ownership",
        "project-settings",
    ] {
        assert!(
            members_help.contains(sub),
            "cloud members --help missing `{sub}`"
        );
    }

    let accept_help = run_decapod(&["cloud", "invites", "accept", "--help"]);
    assert!(accept_help.contains("--secret-stdin"));
    assert!(!accept_help.contains("--invitation-secret"));
    assert!(!accept_help.contains("--secret "));

    let confirm_help = run_decapod(&["cloud", "identity", "confirm-link", "--help"]);
    assert!(confirm_help.contains("--secret-stdin"));
    assert!(!confirm_help.contains("--stepup-secret"));
    assert!(!confirm_help.contains("--secret "));

    let billing_help = run_decapod(&["cloud", "billing", "--help"]);
    assert!(billing_help.contains("portal"));
    assert!(billing_help.contains("limits"));

    let caps_out = run_decapod(&["capabilities", "--format", "json"]);
    let caps: serde_json::Value = serde_json::from_str(&caps_out).unwrap();
    let cloud_sub = caps["subsystems"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "cloud")
        .expect("cloud subsystem in capabilities");
    let ops = cloud_sub["ops"].as_array().unwrap();
    for expected in [
        "project.enroll",
        "orgs",
        "members",
        "invites",
        "identity",
        "billing",
        "enroll",
        "admins",
        "recovery",
        "policies",
        "upgrades",
    ] {
        assert!(
            ops.iter().any(|v| v == expected),
            "cloud subsystem missing op `{expected}`"
        );
    }
}
