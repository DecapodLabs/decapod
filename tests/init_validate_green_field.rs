// These fixtures exercise project-methodology preparation. Only the first
// regression calls public validate/RPC and proves empty CI evidence is non-ready.
use decapod::core::db::Connection;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.env_remove("GITHUB_ACTIONS").env_remove("BUILDKITE");
    cmd.current_dir(dir).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run decapod")
}

fn run_decapod_with_password(
    dir: &Path,
    args: &[&str],
    password: &str,
    extra_envs: &[(&str, &str)],
) -> std::process::Output {
    let mut envs = vec![
        ("DECAPOD_AGENT_ID", "init-validate-test"),
        ("DECAPOD_SESSION_PASSWORD", password),
        ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
    ];
    for (k, v) in extra_envs {
        envs.push((*k, *v));
    }
    run_decapod(dir, args, &envs)
}

fn setup_initialized_repo(tmp: &tempfile::TempDir) -> String {
    let dir = tmp.path();

    let git_init = Command::new("git")
        .current_dir(dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(git_init.status.success(), "git init failed");

    let out = run_decapod(
        dir,
        &[
            "init",
            "with",
            "--force",
            "--product-name",
            "Test Project",
            "--product-summary",
            "A test project for validation",
            "--primary-language",
            "Rust",
        ],
        &[],
    );
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let acquire = run_decapod(
        dir,
        &["session", "acquire"],
        &[
            ("DECAPOD_AGENT_ID", "init-validate-test"),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        acquire.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );

    let stdout = String::from_utf8_lossy(&acquire.stdout);
    stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .expect("password in session acquire output")
}

#[test]
fn fresh_init_project_prerequisites_do_not_grant_publication_readiness() {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path();
    let password = setup_initialized_repo(&tmp);

    let validate = run_decapod_with_password(
        dir,
        &["govern", "project-check"],
        &password,
        &[("DECAPOD_VALIDATE_TIMEOUT_SECS", "120")],
    );

    let stderr = String::from_utf8_lossy(&validate.stderr);
    let stdout = String::from_utf8_lossy(&validate.stdout);

    assert!(
        validate.status.success(),
        "validate should succeed for fresh init project.\nstdout: {stdout}\nstderr: {stderr}"
    );

    assert!(
        stderr.contains("validation passed") || stdout.contains("validation passed"),
        "expected 'validation passed' in output"
    );

    assert!(
        !stderr.contains("fail=") || stderr.contains("fail=0"),
        "expected no failures in validation output: {stderr}"
    );
    assert!(stdout.contains("publication readiness has not been assessed"));
    let full = run_decapod_with_password(dir, &["validate", "--format", "json"], &password, &[]);
    assert!(
        !full.status.success(),
        "empty required profile must never be ready"
    );
    let report: serde_json::Value = serde_json::from_slice(&full.stdout).unwrap();
    assert_eq!(report["status"], "fail");
    assert_eq!(report["readiness"]["ready"], false);
    assert!(String::from_utf8_lossy(&full.stdout).contains("READINESS_PROFILE_REQUIRED"));

    let rpc = run_decapod_with_password(dir, &["rpc", "--op", "validate.run"], &password, &[]);
    let output = String::from_utf8_lossy(&rpc.stdout);
    assert!(
        output.contains("publication_not_ready"),
        "RPC must use full readiness: {output}"
    );
    assert!(output.contains("READINESS_PROFILE_REQUIRED"));

    let wrong_scope = run_decapod_with_password(
        dir,
        &["govern", "project-check", "--snapshot-prepare"],
        &password,
        &[],
    );
    assert!(!wrong_scope.status.success());
    assert!(String::from_utf8_lossy(&wrong_scope.stderr).contains("PROJECT_CHECK_SCOPE"));
    let wrong_provider = run_decapod_with_password(
        dir,
        &["validate", "--snapshot-prepare", "--format", "json"],
        &password,
        &[],
    );
    assert!(!wrong_provider.status.success());
    assert!(
        String::from_utf8_lossy(&wrong_provider.stdout).contains("READINESS_PROVIDER_MISMATCH")
    );
}

#[test]
fn authority_change_cannot_leave_specs_manifest_config_hash_stale() {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path();
    let password = setup_initialized_repo(&tmp);

    let initial = run_decapod_with_password(
        dir,
        &["govern", "project-check"],
        &password,
        &[("DECAPOD_VALIDATE_TIMEOUT_SECS", "120")],
    );
    assert!(
        initial.status.success(),
        "initial validation should succeed: {}",
        String::from_utf8_lossy(&initial.stderr)
    );

    let override_path = dir.join(".decapod/OVERRIDE.md");
    let original = fs::read_to_string(&override_path).expect("override");
    fs::write(
        &override_path,
        format!("<!-- authority changed -->\n{original}"),
    )
    .expect("change override authority");
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join(".decapod/managed/specs/.manifest.json"))
            .expect("specs manifest"),
    )
    .expect("manifest JSON");
    assert_ne!(
        manifest["config_input_hash"],
        serde_json::Value::String(
            decapod::core::project_specs::config_input_hash(dir).expect("current config hash"),
        )
    );

    let stale = run_decapod_with_password(
        dir,
        &["govern", "project-check"],
        &password,
        &[("DECAPOD_VALIDATE_TIMEOUT_SECS", "120")],
    );
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&stale.stdout),
        String::from_utf8_lossy(&stale.stderr)
    );
    assert!(
        !stale.status.success(),
        "stale authority must fail: {output}"
    );
    assert!(
        output.contains("STALE_VALIDATION_EVIDENCE"),
        "validation must reject the old bound proof after refreshing the manifest: {output}"
    );
    let refreshed_manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join(".decapod/managed/specs/.manifest.json"))
            .expect("refreshed specs manifest"),
    )
    .expect("refreshed manifest JSON");
    assert_eq!(
        refreshed_manifest["config_input_hash"],
        serde_json::Value::String(
            decapod::core::project_specs::config_input_hash(dir).expect("current config hash"),
        ),
        "validation must refresh the manifest before rejecting stale proof"
    );
}

#[test]
fn changed_validation_epoch_preserves_bound_proof_and_requires_new_run() {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path();
    let password = setup_initialized_repo(&tmp);

    let initial = run_decapod_with_password(
        dir,
        &["govern", "project-check"],
        &password,
        &[("DECAPOD_VALIDATE_TIMEOUT_SECS", "120")],
    );
    assert!(
        initial.status.success(),
        "initial validation should succeed: {}",
        String::from_utf8_lossy(&initial.stderr)
    );
    let governance_path = dir.join(decapod::core::governance_document::GOVERNANCE_PATH);
    let governance_before = fs::read(&governance_path).expect("canonical governance proof");
    for section in ["trajectory", "validation"] {
        assert!(
            decapod::core::governance_document::read_section(dir, section)
                .unwrap()
                .is_some()
        );
    }

    let changed_epoch = run_decapod_with_password(
        dir,
        &["govern", "project-check"],
        &password,
        &[
            ("DECAPOD_VALIDATE_TIMEOUT_SECS", "120"),
            ("DECAPOD_VALIDATION_PROFILE", "changed-profile"),
        ],
    );
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&changed_epoch.stdout),
        String::from_utf8_lossy(&changed_epoch.stderr)
    );
    assert!(
        !changed_epoch.status.success(),
        "changed epoch must fail: {output}"
    );
    assert!(
        output.contains("STALE_VALIDATION_EVIDENCE"),
        "validation must provide trajectory recovery: {output}"
    );
    assert_eq!(
        fs::read(&governance_path).expect("governance after stale validation"),
        governance_before,
        "the entire shared document must remain unchanged, including bound trajectory and receipt"
    );
}

#[test]
fn existing_project_init_migrates_watcher_before_runtime_reads() {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path();
    let password = setup_initialized_repo(&tmp);
    let data = dir.join(".decapod/data");
    let legacy = data.join("watcher.events.jsonl");
    fs::write(
        &legacy,
        "{\"event_id\":\"upgrade-watcher-1\",\"ts\":\"1785620000Z\",\"event_type\":\"watcher.run\",\"actor\":\"watcher\"}\n",
    )
    .expect("write legacy watcher event");

    let init = run_decapod(dir, &["init"], &[]);
    assert!(
        init.status.success(),
        "existing-project init failed: {}{}",
        String::from_utf8_lossy(&init.stdout),
        String::from_utf8_lossy(&init.stderr)
    );
    let conn = Connection::open(data.join("decapod.db")).expect("open canonical db");
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE stream = 'watcher' AND event_id = 'upgrade-watcher-1'",
            [],
            |row| row.get(0),
        )
        .expect("query migrated watcher event");
    assert_eq!(count, 1);
    drop(conn);

    assert!(
        !legacy.exists(),
        "successful initialization must retire the migrated legacy input"
    );
    let validate = run_decapod_with_password(
        dir,
        &["govern", "project-check"],
        &password,
        &[("DECAPOD_VALIDATE_TIMEOUT_SECS", "120")],
    );
    let validate_output = format!(
        "{}{}",
        String::from_utf8_lossy(&validate.stdout),
        String::from_utf8_lossy(&validate.stderr)
    );
    assert!(validate.status.success(), "{validate_output}");
    assert!(!validate_output.contains("Watcher audit trail missing"));

    let health = run_decapod_with_password(dir, &["govern", "health", "summary"], &password, &[]);
    let health_output = String::from_utf8_lossy(&health.stdout);
    assert!(
        health.status.success(),
        "{}",
        String::from_utf8_lossy(&health.stderr)
    );
    assert!(health_output.contains("upgrade-watcher-1") || health_output.contains("1785620000Z"));

    let recorder = run_decapod_with_password(
        dir,
        &["trace", "flight-recorder", "timeline", "--format", "json"],
        &password,
        &[],
    );
    let recorder_output = String::from_utf8_lossy(&recorder.stdout);
    assert!(
        recorder.status.success(),
        "{}",
        String::from_utf8_lossy(&recorder.stderr)
    );
    assert!(recorder_output.contains("upgrade-watcher-1"));
}

#[test]
fn fresh_init_validate_with_real_git_workspace() {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path();
    let password = setup_initialized_repo(&tmp);

    let validate = run_decapod_with_password(
        dir,
        &["govern", "project-check"],
        &password,
        &[("DECAPOD_VALIDATE_TIMEOUT_SECS", "120")],
    );

    let stderr = String::from_utf8_lossy(&validate.stderr);
    let stdout = String::from_utf8_lossy(&validate.stdout);

    assert!(
        validate.status.success(),
        "validate should succeed for fresh init project with git workspace.\nstdout: {stdout}\nstderr: {stderr}"
    );
}

#[test]
fn re_init_preserves_validation_green() {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path();
    let password = setup_initialized_repo(&tmp);

    let validate1 = run_decapod_with_password(
        dir,
        &["govern", "project-check"],
        &password,
        &[("DECAPOD_VALIDATE_TIMEOUT_SECS", "120")],
    );
    assert!(
        validate1.status.success(),
        "first validate failed: {}",
        String::from_utf8_lossy(&validate1.stderr)
    );
    let before_config = fs::read(dir.join(".decapod/config.toml")).expect("config before reinit");
    let before_manifest = fs::read(dir.join(".decapod/managed/specs/.manifest.json"))
        .expect("manifest before reinit");
    let before_specs =
        fs::read(dir.join(".decapod/managed/specs/INTENT.md")).expect("spec before reinit");

    let reinit = run_decapod_with_password(dir, &["init", "--force"], &password, &[]);
    assert!(
        reinit.status.success(),
        "re-init failed: {}",
        String::from_utf8_lossy(&reinit.stderr)
    );
    assert_eq!(
        before_config,
        fs::read(dir.join(".decapod/config.toml")).expect("config after reinit"),
        "force re-init must preserve project config authority"
    );
    assert_eq!(
        before_manifest,
        fs::read(dir.join(".decapod/managed/specs/.manifest.json")).expect("manifest after reinit"),
        "force re-init must preserve the specs manifest"
    );
    assert_eq!(
        before_specs,
        fs::read(dir.join(".decapod/managed/specs/INTENT.md")).expect("spec after reinit"),
        "force re-init must preserve authored living specs"
    );

    let validate2 = run_decapod_with_password(
        dir,
        &["govern", "project-check"],
        &password,
        &[("DECAPOD_VALIDATE_TIMEOUT_SECS", "120")],
    );
    let stderr = String::from_utf8_lossy(&validate2.stderr);

    assert!(
        validate2.status.success(),
        "validate after re-init should succeed.\nstderr: {stderr}"
    );
}

#[test]
fn upgrade_from_older_config_version_reports_clear_error() {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path();

    let git_init = Command::new("git")
        .current_dir(dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(git_init.status.success(), "git init failed");

    let out = run_decapod(
        dir,
        &[
            "init",
            "with",
            "--force",
            "--product-name",
            "Upgrade Test",
            "--product-summary",
            "Test upgrade path",
            "--primary-language",
            "Rust",
        ],
        &[],
    );
    assert!(
        out.status.success(),
        "initial decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config_path = dir.join(".decapod").join("config.toml");
    let config_content = fs::read_to_string(&config_path).expect("read config.toml");

    let old_config =
        config_content.replace(r#"schema_version = "1.0.0""#, r#"schema_version = "0.9.0""#);
    fs::write(&config_path, old_config).expect("write old config version");

    let acquire = run_decapod(
        dir,
        &["session", "acquire"],
        &[
            ("DECAPOD_AGENT_ID", "init-validate-test"),
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
        .expect("password in session acquire output");

    let validate = run_decapod_with_password(
        dir,
        &["govern", "project-check"],
        &password,
        &[("DECAPOD_VALIDATE_TIMEOUT_SECS", "120")],
    );

    let stderr = String::from_utf8_lossy(&validate.stderr);
    let stdout = String::from_utf8_lossy(&validate.stdout);

    assert!(
        stdout.contains("fail=") && stdout.contains("schema_version must be 1.0.0"),
        "error message should clearly indicate schema_version issue.\nstdout: {stdout}\nstderr: {stderr}"
    );
}

#[test]
fn init_validate_with_various_config_options() {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path();

    let git_init = Command::new("git")
        .current_dir(dir)
        .args(["init", "-b", "main"])
        .output()
        .expect("git init");
    assert!(git_init.status.success(), "git init failed");

    let out = run_decapod(
        dir,
        &[
            "init",
            "with",
            "--force",
            "--product-name",
            "Multi Option Test",
            "--product-summary",
            "Testing multiple init options",
            "--primary-language",
            "rust,python",
            "--surface",
            "cli,backend",
            "--architecture-direction",
            "Microservice with CLI frontend",
            "--done-criteria",
            "all tests pass and validate is green",
        ],
        &[],
    );
    assert!(
        out.status.success(),
        "decapod init with options failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let acquire = run_decapod(
        dir,
        &["session", "acquire"],
        &[
            ("DECAPOD_AGENT_ID", "init-validate-test"),
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
        .expect("password in session acquire output");

    let validate = run_decapod_with_password(
        dir,
        &["govern", "project-check"],
        &password,
        &[("DECAPOD_VALIDATE_TIMEOUT_SECS", "120")],
    );

    let stderr = String::from_utf8_lossy(&validate.stderr);

    assert!(
        validate.status.success(),
        "validate should succeed with complex init options.\nstderr: {stderr}"
    );
}

fn enable_remote_validation_fixture(dir: &Path) {
    let path = dir.join(".decapod/config.toml");
    let mut config: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let table = config.as_table_mut().unwrap();
    let proof = table
        .entry("proof")
        .or_insert_with(|| toml::Value::Table(Default::default()));
    proof.as_table_mut().unwrap().insert(
        "validation_buildkite_preflights".into(),
        toml::Value::Boolean(true),
    );
    fs::write(path, toml::to_string_pretty(&config).unwrap()).unwrap();
}

#[test]
fn ci_local_validate_and_rpc_run_real_checks_without_remote_readiness_or_receipts() {
    use decapod::core::governance_document::read_section;
    for provider in ["GITHUB_ACTIONS", "BUILDKITE"] {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        let password = setup_initialized_repo(&tmp);
        enable_remote_validation_fixture(dir);
        // Existing partial evidence must neither be renewed nor promoted by CI-local validate.
        let prepared = run_decapod_with_password(dir, &["govern", "project-check"], &password, &[]);
        assert!(
            prepared.status.success(),
            "{}",
            String::from_utf8_lossy(&prepared.stderr)
        );
        let receipt = read_section(dir, "validation").unwrap();
        let trajectory = read_section(dir, "trajectory").unwrap();
        let awareness_path = dir.join(".decapod/managed/awareness/init-validate-test.json");
        let awareness = fs::read(&awareness_path).unwrap();
        for command in ["validate", "v"] {
            let output = run_decapod_with_password(
                dir,
                &[command, "--format", "json"],
                &password,
                &[(provider, "true")],
            );
            assert!(
                output.status.success(),
                "{provider}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(result["scope"], "ci_local");
            assert_eq!(result["publication_ready"], false);
            assert!(result["readiness"].is_null());
            assert_eq!(result["report"]["fail_count"], 0);
            assert!(result["report"]["pass_count"].as_u64().unwrap() > 0);
            assert!(
                !result["report"]["gate_timings"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                result["report"]["ci_prediction"]["result"],
                "ci_local_checks_passed"
            );
        }
        let output = run_decapod_with_password(
            dir,
            &["rpc", "--op", "validate.run"],
            &password,
            &[(provider, "true")],
        );
        let rpc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(rpc["success"], true, "{rpc}");
        assert_eq!(rpc["result"]["scope"], "ci_local");
        assert_eq!(rpc["result"]["publication_ready"], false);
        assert!(rpc["result"]["checks"]["pass_count"].as_u64().unwrap() > 0);
        assert_eq!(read_section(dir, "validation").unwrap(), receipt);
        assert_eq!(read_section(dir, "trajectory").unwrap(), trajectory);
        assert_eq!(fs::read(awareness_path).unwrap(), awareness);
        // The same local repository cannot reuse CI-local success as whole proof.
        let outside_ci = run_decapod_with_password(
            dir,
            &["validate", "--format", "json"],
            &password,
            &[("CI", "true")],
        );
        assert!(!outside_ci.status.success());
        let full: serde_json::Value = serde_json::from_slice(&outside_ci.stdout).unwrap();
        assert_eq!(full["scope"], "publication_readiness");
        assert_eq!(full["publication_ready"], false);
        assert_eq!(full["readiness"]["ready"], false);
        assert!(String::from_utf8_lossy(&outside_ci.stdout).contains("READINESS_PROFILE_REQUIRED"));
    }
}

#[test]
fn ci_local_validate_rejects_snapshot_execution_in_cli_and_rpc() {
    let tmp = TempDir::new().unwrap();
    let password = setup_initialized_repo(&tmp);
    enable_remote_validation_fixture(tmp.path());
    for provider in ["GITHUB_ACTIONS", "BUILDKITE"] {
        let output = run_decapod_with_password(
            tmp.path(),
            &["validate", "--snapshot-prepare"],
            &password,
            &[(provider, "true")],
        );
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("CI_LOCAL_SCOPE"));
        let output = run_decapod_with_password(
            tmp.path(),
            &[
                "rpc",
                "--op",
                "validate.run",
                "--params",
                r#"{"snapshot_prepare":true}"#,
            ],
            &password,
            &[(provider, "true")],
        );
        assert!(
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
            .contains("CI_LOCAL_SCOPE")
        );
    }
}
