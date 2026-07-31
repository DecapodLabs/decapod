use std::process::Command;
use tempfile::TempDir;

fn git(dir: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn machine_session_files(config_home: &std::path::Path) -> Vec<std::path::PathBuf> {
    let sessions = config_home.join("decapod/sessions");
    let Ok(scopes) = std::fs::read_dir(sessions) else {
        return Vec::new();
    };
    scopes
        .filter_map(Result::ok)
        .flat_map(|scope| std::fs::read_dir(scope.path()).into_iter().flatten())
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect()
}

#[test]
fn test_cloud_opt_in_fails_closed_without_verified_remote() {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path().to_path_buf();

    let init_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--backend", "cloud", "--force", "--proof"])
        .current_dir(&dir)
        .output()
        .expect("decapod init");

    assert!(
        init_out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&init_out.stderr)
    );

    let add_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["todo", "add", "must not fall back", "--format", "json"])
        .current_dir(&dir)
        .output()
        .expect("todo add");

    assert!(
        !add_out.status.success(),
        "cloud mode must not use local SQLite"
    );
    let error = String::from_utf8_lossy(&add_out.stderr);
    assert!(
        error.contains("origin Git remote"),
        "unexpected error: {error}"
    );
}

#[test]
fn test_cloud_init_records_opt_in_without_auth_or_repo_credentials() {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path().to_path_buf();

    let init_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--backend", "cloud", "--force", "--proof"])
        .env_remove("SUPABASE_URL")
        .env_remove("SUPABASE_KEY")
        .current_dir(&dir)
        .output()
        .expect("decapod init");

    assert!(
        init_out.status.success(),
        "cloud opt-in init should not require auth/backend calls: {}",
        String::from_utf8_lossy(&init_out.stderr)
    );

    let config_path = dir.join(".decapod/config.toml");
    let config = std::fs::read_to_string(config_path).unwrap();
    assert!(config.contains("backend = \"cloud\""));
    assert!(!config.contains("[cloud]"));
    assert!(!config.contains("api_url"));
    assert!(!config.contains("project_id"));
    assert!(!config.contains("repo_id"));
    assert!(!config.contains("mode = \"cloud\""));
    assert!(!config.contains("SUPABASE"));
    assert!(!config.contains("supabase"));
    assert!(!config.contains("token"));
    assert!(!dir.join(".decapod/session_token").exists());

    let registration_path = dir.join(".decapod/managed/cloud/init-registration.json");
    let registration = std::fs::read_to_string(registration_path)
        .expect("cloud opt-in should create a mock init registration payload");
    let registration: serde_json::Value =
        serde_json::from_str(&registration).expect("parse cloud init registration");
    assert_eq!(registration["provider"], "vercel");
    assert_eq!(registration["api_url"], "https://project-oqn7i.vercel.app");
    assert_eq!(
        registration["route"],
        "GET /api/health; GET /api/todos?repo_id=<repo>; POST /api/todos; PATCH /api/todos?id=<todo>"
    );
    assert!(
        registration["writes"]
            .as_array()
            .expect("writes array")
            .iter()
            .any(|write| write["table"] == "todos" && write["operation"] == "claim/complete"),
        "registration should model the repo-scoped todo lifecycle"
    );
}

#[test]
fn cloud_init_starts_machine_session_when_identity_and_mock_auth_are_available() {
    let tmp = TempDir::new().expect("tempdir");
    let data_home = TempDir::new().expect("credential data home");
    git(tmp.path(), &["init", "-q"]);
    git(
        tmp.path(),
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:DecapodLabs/decapod.git",
        ],
    );

    let init_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--backend", "cloud", "--force", "--proof"])
        .current_dir(tmp.path())
        .env("DECAPOD_CLOUD_AUTH_MODE", "mock")
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .env("XDG_DATA_HOME", data_home.path())
        .output()
        .expect("cloud init");
    assert!(
        init_out.status.success(),
        "cloud init should complete setup: {}",
        String::from_utf8_lossy(&init_out.stderr)
    );
    assert!(
        data_home.path().join("decapod/session_token.json").exists(),
        "canonical init should establish machine-local cloud custody"
    );
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&init_out.stdout),
        String::from_utf8_lossy(&init_out.stderr)
    );
    assert!(
        output.contains("Cloud setup ready"),
        "unexpected init output: {output}"
    );
    assert!(!output.contains("decapod-test-mock-access"));
    assert!(!output.contains("decapod-test-mock-refresh"));
}

#[test]
fn cloud_cli_preflight_does_not_initialize_local_sqlite() {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path().to_path_buf();
    let init_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--backend", "cloud", "--force", "--proof"])
        .current_dir(&dir)
        .output()
        .expect("decapod init");
    assert!(init_out.status.success());
    git(
        &dir,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:DecapodLabs/decapod.git",
        ],
    );

    let data_home = TempDir::new().expect("credential data home");
    let list_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["todo", "list", "--format", "json"])
        .current_dir(&dir)
        .env_remove("DECAPOD_ACCESS_TOKEN")
        .env("DECAPOD_PROPODUS_API_URL", "http://127.0.0.1:1")
        .env("XDG_DATA_HOME", data_home.path())
        .output()
        .expect("cloud list");
    assert!(!list_out.status.success());
    let error = String::from_utf8_lossy(&list_out.stderr);
    let diagnostic: serde_json::Value =
        serde_json::from_slice(&list_out.stdout).expect("stable cloud auth JSON");
    assert_eq!(diagnostic["schema_version"], "decapod.cloud.auth.v1");
    assert_eq!(diagnostic["status"], "offline");
    assert!(
        diagnostic["next_action"]
            .as_str()
            .unwrap()
            .contains("rerun")
    );
    assert!(!error.contains("DECAPOD_ACCESS_TOKEN"));
    assert!(!error.contains("Bearer"));
    assert!(
        !dir.join(".decapod/data/todo.db").exists(),
        "cloud credential preflight must not initialize local SQLite"
    );
}

#[test]
fn canonical_backend_selection_uses_cloud_without_local_fallback() {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path().to_path_buf();
    let init_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--backend", "cloud", "--force", "--proof"])
        .current_dir(&dir)
        .output()
        .expect("decapod init");
    assert!(init_out.status.success());
    let config_path = dir.join(".decapod/config.toml");
    let config = std::fs::read_to_string(&config_path).expect("config");
    assert!(config.contains("backend = \"cloud\""));
    assert!(!config.contains("mode = "));

    git(
        &dir,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:DecapodLabs/decapod.git",
        ],
    );
    let data_home = TempDir::new().expect("credential data home");
    let list_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["todo", "list", "--format", "json"])
        .current_dir(&dir)
        .env_remove("DECAPOD_ACCESS_TOKEN")
        .env("DECAPOD_PROPODUS_API_URL", "http://127.0.0.1:1")
        .env("XDG_DATA_HOME", data_home.path())
        .output()
        .expect("cloud todo list");

    assert!(
        !list_out.status.success(),
        "missing cloud session must fail closed"
    );
    let diagnostic: serde_json::Value =
        serde_json::from_slice(&list_out.stdout).expect("stable cloud auth JSON");
    assert_eq!(diagnostic["schema_version"], "decapod.cloud.auth.v1");
    assert_eq!(diagnostic["status"], "offline");
    assert!(
        !dir.join(".decapod/data/todo.db").exists(),
        "backend selection must not fall back to local SQLite"
    );
}

#[test]
fn cloud_login_requires_a_project_origin() {
    let tmp = TempDir::new().expect("tempdir");
    let data_home = TempDir::new().expect("credential data home");
    let login_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["cloud", "login"])
        .current_dir(tmp.path())
        .env("XDG_DATA_HOME", data_home.path())
        .output()
        .expect("cloud login");
    assert!(!login_out.status.success());
    let error = String::from_utf8_lossy(&login_out.stderr);
    assert!(
        error.contains("origin Git remote"),
        "unexpected login error: {error}"
    );
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&login_out.stdout),
        String::from_utf8_lossy(&login_out.stderr)
    );
    assert!(output.contains("decapod init --backend cloud"));
}

#[test]
fn cloud_session_acquire_keeps_local_custody_when_cloud_preflight_fails() {
    let tmp = TempDir::new().expect("project tempdir");
    let data_home = TempDir::new().expect("credential data home");
    let config_home = TempDir::new().expect("config home");
    let init_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--backend", "cloud", "--force", "--proof"])
        .current_dir(tmp.path())
        .output()
        .expect("cloud init");
    assert!(init_out.status.success());
    git(
        tmp.path(),
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:DecapodLabs/decapod.git",
        ],
    );

    let session_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["session", "acquire"])
        .current_dir(tmp.path())
        .env_remove("DECAPOD_ACCESS_TOKEN")
        .env("DECAPOD_PROPODUS_API_URL", "http://127.0.0.1:1")
        .env("XDG_DATA_HOME", data_home.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .output()
        .expect("cloud session acquire");
    assert!(!session_out.status.success());
    let diagnostic: serde_json::Value =
        serde_json::from_slice(&session_out.stdout).expect("stable cloud auth JSON");
    assert_eq!(diagnostic["schema_version"], "decapod.cloud.auth.v1");
    assert_eq!(diagnostic["status"], "offline");
    let stdout = String::from_utf8_lossy(&session_out.stdout);
    let stderr = String::from_utf8_lossy(&session_out.stderr);
    assert!(!stdout.contains("Token:") && !stdout.contains("Password:"));
    assert!(!stdout.contains("session_token") && !stderr.contains("Bearer"));
    assert_eq!(machine_session_files(config_home.path()).len(), 1);
    assert!(
        !tmp.path().join(".decapod/data/todo.db").exists(),
        "cloud session acquisition must not initialize local todo SQLite"
    );
    assert!(
        !tmp.path().join(".decapod/session_token.json").exists(),
        "cloud session acquisition must not write repository credentials"
    );
}

#[test]
fn cloud_session_acquire_uses_explicit_mock_onboarding_in_validation_harness() {
    let tmp = TempDir::new().expect("project tempdir");
    let data_home = TempDir::new().expect("credential data home");
    let config_home = TempDir::new().expect("config home");
    let init_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--backend", "cloud", "--force", "--proof"])
        .current_dir(tmp.path())
        .output()
        .expect("cloud init");
    assert!(init_out.status.success());
    git(
        tmp.path(),
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:DecapodLabs/decapod.git",
        ],
    );

    let session_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["session", "acquire"])
        .current_dir(tmp.path())
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .env("DECAPOD_CLOUD_AUTH_MODE", "mock")
        .env("XDG_DATA_HOME", data_home.path())
        .env("XDG_CONFIG_HOME", config_home.path())
        .output()
        .expect("mock cloud session acquire");
    assert!(
        session_out.status.success(),
        "mock cloud session acquire failed: {}",
        String::from_utf8_lossy(&session_out.stderr)
    );
    assert!(String::from_utf8_lossy(&session_out.stdout).contains("Password: "));
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&session_out.stdout),
        String::from_utf8_lossy(&session_out.stderr)
    );
    assert!(!output.contains("decapod-test-mock-access"));
    assert!(!output.contains("decapod-test-mock-refresh"));
    let machine_session = data_home.path().join("decapod/session_token.json");
    let stored = std::fs::read_to_string(machine_session).expect("mock machine session");
    assert!(stored.contains("decapod-test-mock-access"));
    assert!(stored.contains("decapod-test-mock-refresh"));
    assert!(!tmp.path().join(".decapod/data/todo.db").exists());
}
