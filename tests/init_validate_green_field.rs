use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
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
fn fresh_init_validate_comes_back_green() {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path();
    let password = setup_initialized_repo(&tmp);

    let validate = run_decapod_with_password(
        dir,
        &["validate"],
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
}

#[test]
fn fresh_init_validate_with_real_git_workspace() {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path();
    let password = setup_initialized_repo(&tmp);

    let validate = run_decapod_with_password(
        dir,
        &["validate"],
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
        &["validate"],
        &password,
        &[("DECAPOD_VALIDATE_TIMEOUT_SECS", "120")],
    );
    assert!(
        validate1.status.success(),
        "first validate failed: {}",
        String::from_utf8_lossy(&validate1.stderr)
    );

    let reinit = run_decapod_with_password(dir, &["init", "--force"], &password, &[]);
    assert!(
        reinit.status.success(),
        "re-init failed: {}",
        String::from_utf8_lossy(&reinit.stderr)
    );

    let validate2 = run_decapod_with_password(
        dir,
        &["validate"],
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
        &["validate"],
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
        &["validate"],
        &password,
        &[("DECAPOD_VALIDATE_TIMEOUT_SECS", "120")],
    );

    let stderr = String::from_utf8_lossy(&validate.stderr);

    assert!(
        validate.status.success(),
        "validate should succeed with complex init options.\nstderr: {stderr}"
    );
}
