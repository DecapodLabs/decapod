use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_cloud_opt_in_fails_closed_without_verified_remote() {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path().to_path_buf();

    let init_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--mode", "cloud", "--force", "--proof"])
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
        .args(["init", "--mode", "cloud", "--force", "--proof"])
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
    assert!(config.contains("[cloud]"));
    assert!(config.contains("enabled = true"));
    assert!(config.contains("experimental = true"));
    assert!(config.contains("provider = \"vercel\""));
    assert!(config.contains("api_url = \"https://decapod-cloud.vercel.app\""));
    assert!(config.contains("mode = \"cloud\""));
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
    assert_eq!(registration["api_url"], "https://decapod-cloud.vercel.app");
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
