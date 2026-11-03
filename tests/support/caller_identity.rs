//! Shared socket-backed caller ownership scenario. The daemonless acceptance
//! entrypoint cannot skip; the broker-suite entrypoint retains local opt-out.

#[allow(dead_code)]
pub(crate) fn required() {
    #[cfg(unix)]
    unix::required();
    #[cfg(not(unix))]
    panic!("required caller socket acceptance needs a Unix socket executor");
}

#[allow(dead_code)]
pub(crate) fn optional() {
    #[cfg(unix)]
    unix::optional();
    #[cfg(not(unix))]
    eprintln!("caller socket acceptance: SKIPPED (optional Unix socket executor unavailable)");
}

fn require_socket(supported: bool) {
    assert!(
        supported,
        "required caller socket acceptance blocked: broker probe did not queue over its Unix socket"
    );
}

#[test]
fn required_socket_probe_cannot_skip() {
    require_socket(true);
    assert!(std::panic::catch_unwind(|| require_socket(false)).is_err());
}

#[cfg(unix)]
mod unix {
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command};
    use std::time::{Duration, Instant};
    use tempfile::TempDir;
    type DecapodRunner = fn(&Path, &[&str], &[(&str, &str)]) -> std::process::Output;

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

    fn setup_repo_with(run: DecapodRunner) -> (TempDir, PathBuf, String) {
        let tmp = TempDir::new().expect("tmpdir");
        let dir = tmp.path().to_path_buf();

        let init = Command::new("git")
            .current_dir(&dir)
            .args(["init", "-b", "master"])
            .output()
            .expect("git init");
        assert!(init.status.success(), "git init failed");

        let out = run(&dir, &["init", "--force"], &[]);
        assert!(
            out.status.success(),
            "decapod init failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        let acquire = run(
            &dir,
            &["session", "acquire"],
            &[
                ("DECAPOD_AGENT_ID", "unknown"),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ],
        );
        assert!(
            acquire.status.success(),
            "session acquire failed: {}",
            String::from_utf8_lossy(&acquire.stderr)
        );
        let stdout = String::from_utf8_lossy(&acquire.stdout);
        let password = stdout
            .lines()
            .find_map(|line| {
                line.strip_prefix("Password: ")
                    .map(|s| s.trim().to_string())
            })
            .expect("session password");

        (tmp, dir, password)
    }

    fn acquire_session_password_with(dir: &Path, agent_id: &str, run: DecapodRunner) -> String {
        let acquire = run(
            dir,
            &["session", "acquire"],
            &[
                ("DECAPOD_AGENT_ID", agent_id),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ],
        );
        assert!(
            acquire.status.success(),
            "session acquire failed for {agent_id}: {}",
            String::from_utf8_lossy(&acquire.stderr)
        );
        let stdout = String::from_utf8_lossy(&acquire.stdout);
        stdout
            .lines()
            .find_map(|line| {
                line.strip_prefix("Password: ")
                    .map(|s| s.trim().to_string())
            })
            .expect("session password")
    }

    fn broker_socket_supported_with(dir: &Path, password: &str, run: DecapodRunner) -> bool {
        let hook = dir.join("broker-socket-probe.log");
        let probe = run(
            dir,
            &["todo", "add", "broker-socket-probe"],
            &[
                ("DECAPOD_AGENT_ID", "unknown"),
                ("DECAPOD_SESSION_PASSWORD", password),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
                ("DECAPOD_GROUP_BROKER_REQUEST_ID", "BROKER_SOCKET_PROBE"),
                (
                    "DECAPOD_GROUP_BROKER_TEST_HOOK_FILE",
                    hook.to_string_lossy().as_ref(),
                ),
            ],
        );
        if !probe.status.success() {
            return false;
        }
        wait_for_hook_line(
            &hook,
            "queued|BROKER_SOCKET_PROBE",
            Duration::from_millis(300),
        )
    }

    fn wait_for_hook_line(hook_file: &Path, needle: &str, timeout: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Ok(raw) = std::fs::read_to_string(hook_file)
                && raw.lines().any(|line| line.contains(needle))
            {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }

    #[cfg(unix)]
    struct ClaimProbeChild(Option<Child>);
    #[cfg(unix)]
    impl ClaimProbeChild {
        fn terminate(&mut self) {
            if let Some(child) = self.0.as_mut() {
                // This child has not been reaped, so its owned group cannot be reused.
                unsafe {
                    libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
                }
                let _ = child.kill();
            }
        }
        fn output(
            &mut self,
            stdout: &Path,
            stderr: &Path,
            hooks: &Path,
            stage: &str,
        ) -> std::process::Output {
            self.output_timeout(stdout, stderr, hooks, stage, Duration::from_secs(30))
        }
        fn output_timeout(
            &mut self,
            stdout: &Path,
            stderr: &Path,
            hooks: &Path,
            stage: &str,
            timeout: Duration,
        ) -> std::process::Output {
            let start = Instant::now();
            loop {
                if let Some(status) = self.0.as_mut().unwrap().try_wait().unwrap() {
                    self.0.take();
                    return std::process::Output {
                        status,
                        stdout: std::fs::read(stdout).unwrap(),
                        stderr: std::fs::read(stderr).unwrap(),
                    };
                }
                if start.elapsed() >= timeout {
                    self.terminate();
                    let _ = self.0.take().unwrap().wait();
                    panic!(
                        "{stage} timed out; stdout={} stderr={} hooks={}",
                        std::fs::read_to_string(stdout).unwrap_or_default(),
                        std::fs::read_to_string(stderr).unwrap_or_default(),
                        std::fs::read_to_string(hooks).unwrap_or_default()
                    );
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    #[cfg(unix)]
    impl Drop for ClaimProbeChild {
        fn drop(&mut self) {
            self.terminate();
            if let Some(mut child) = self.0.take() {
                let _ = child.wait();
            }
        }
    }

    fn run_contention_setup(
        dir: &Path,
        args: &[&str],
        envs: &[(&str, &str)],
    ) -> std::process::Output {
        run_contention_command(
            dir,
            args,
            envs,
            Duration::from_secs(30),
            "caller fixture setup",
        )
    }

    fn run_contention_command(
        dir: &Path,
        args: &[&str],
        envs: &[(&str, &str)],
        timeout: Duration,
        stage: &str,
    ) -> std::process::Output {
        use decapod::core::bounded_process::BoundedCommand;
        Command::new(resolve_decapod_bin())
            .current_dir(dir)
            .args(args)
            .envs(envs.iter().copied())
            .bounded_output(timeout)
            .unwrap_or_else(|error| panic!("{stage}: {error}"))
    }

    pub(super) fn required() {
        let (_tmp, dir, password) = setup_repo_with(run_contention_setup);
        super::require_socket(broker_socket_supported_with(
            &dir,
            &password,
            run_contention_setup,
        ));
        claims(&dir, &password);
    }

    pub(super) fn optional() {
        match std::env::var("DECAPOD_REQUIRE_CALLER_SOCKET") {
            Ok(value) if value == "1" => return required(),
            Ok(value) if value == "0" => (),
            Err(std::env::VarError::NotPresent) => (),
            _ => panic!("DECAPOD_REQUIRE_CALLER_SOCKET must be unset, 0 or 1"),
        }
        let (_tmp, dir, password) = setup_repo_with(run_contention_setup);
        if !broker_socket_supported_with(&dir, &password, run_contention_setup) {
            eprintln!("caller socket acceptance: SKIPPED (optional Unix socket probe unavailable)");
            return;
        }
        claims(&dir, &password);
    }

    fn claims(dir: &Path, password: &str) {
        eprintln!(
            "caller socket acceptance: probe passed; executing concurrent ownership assertions"
        );
        let mut tasks = Vec::new();
        // Keep category ownership disjoint using categories seeded by normal init.
        // Caller identities are not category names and must not bypass validation.
        for (agent, category) in [("caller-a", "features"), ("caller-b", "docs")] {
            let task_dir = dir.join(agent);
            std::fs::create_dir(&task_dir).unwrap();
            let added = run_contention_setup(
                dir,
                &[
                    "todo",
                    "add",
                    agent,
                    "--scope",
                    agent,
                    "--dir",
                    task_dir.to_str().unwrap(),
                ],
                &[
                    ("DECAPOD_AGENT_ID", "unknown"),
                    ("DECAPOD_SESSION_PASSWORD", password),
                ],
            );
            assert!(
                added.status.success(),
                "{}",
                String::from_utf8_lossy(&added.stderr)
            );
            let value: serde_json::Value = serde_json::from_slice(&added.stdout).unwrap();
            let id = value["id"].as_str().unwrap().to_owned();
            let edited = run_contention_setup(
                dir,
                &["todo", "edit", "--id", &id, "--category", category],
                &[
                    ("DECAPOD_AGENT_ID", "unknown"),
                    ("DECAPOD_SESSION_PASSWORD", password),
                ],
            );
            assert!(
                edited.status.success(),
                "{}",
                String::from_utf8_lossy(&edited.stderr)
            );
            let credential = acquire_session_password_with(&dir, agent, run_contention_setup);
            tasks.push((agent.to_owned(), credential, id));
        }

        // Elect a third caller, then submit both claims through its live socket.
        // The leader remains ephemeral; the bounded guard cleans up on test failure.
        let hooks = dir.join("identity-leader-hooks");
        let stdout = dir.join("identity-leader-out");
        let stderr = dir.join("identity-leader-err");
        let mut command = Command::new(resolve_decapod_bin());
        command
            .current_dir(dir)
            .args(["todo", "add", "identity-leader"])
            .env("DECAPOD_AGENT_ID", "unknown")
            .env("DECAPOD_SESSION_PASSWORD", password)
            .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "10")
            .env("DECAPOD_GROUP_BROKER_REQUEST_ID", "IDENTITY_LEADER")
            .env("DECAPOD_GROUP_BROKER_TEST_HOOK_FILE", &hooks)
            .stdout(std::fs::File::create(&stdout).unwrap())
            .stderr(std::fs::File::create(&stderr).unwrap());
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        let mut leader = ClaimProbeChild(Some(command.spawn().unwrap()));
        assert!(wait_for_hook_line(
            &hooks,
            "post_exec_pre_ack|IDENTITY_LEADER",
            Duration::from_secs(15)
        ));

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let workers: Vec<_> = tasks
            .iter()
            .cloned()
            .map(|(agent, credential, id)| {
                let dir = dir.to_path_buf();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let output = run_contention_command(
                        &dir,
                        &["todo", "claim", "--id", &id],
                        &[
                            ("DECAPOD_AGENT_ID", &agent),
                            ("DECAPOD_SESSION_PASSWORD", &credential),
                            ("DECAPOD_CLAIM_AUTORUN", "0"),
                            ("DECAPOD_GROUP_BROKER_REQUEST_ID", &agent),
                        ],
                        Duration::from_secs(60),
                        "concurrent caller identity claim",
                    );
                    (agent, id, output)
                })
            })
            .collect();
        let results: Vec<_> = workers.into_iter().map(|worker| worker.join()).collect();
        let leader_output = leader.output(&stdout, &stderr, &hooks, "identity leader completion");
        assert!(
            leader_output.status.success(),
            "{}",
            String::from_utf8_lossy(&leader_output.stderr)
        );
        let leader_hooks = std::fs::read_to_string(&hooks).unwrap();
        for (agent, _, _) in &tasks {
            assert!(
                leader_hooks
                    .lines()
                    .any(|line| line.contains(&format!("queued|{agent}"))),
                "claim did not traverse the elected leader: {agent}"
            );
        }
        for result in results {
            let (agent, id, output) = result.unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(response["result"]["status"], "ok", "{response}");
            assert_eq!(response["result"]["assigned_to"], agent, "{response}");
            let fetched = run_contention_setup(dir, &["todo", "get", "--id", &id], &[]);
            assert!(fetched.status.success());
            let persisted: serde_json::Value = serde_json::from_slice(&fetched.stdout).unwrap();
            assert_eq!(persisted["item"]["assigned_to"], agent, "{persisted}");
            eprintln!("caller socket acceptance: {agent} returned and persisted ownership passed");
        }
        eprintln!("caller socket acceptance: PASSED (both callers traversed the shared broker)");
    }
}
