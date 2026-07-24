use std::process::Command;
use tempfile::TempDir;

#[test]
fn test_cloud_opt_in_keeps_local_state_usable() {
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

    let title = format!(
        "Test local task with cloud opt-in {}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );

    let add_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["todo", "add", &title, "--format", "json"])
        .current_dir(&dir)
        .output()
        .expect("todo add");

    assert!(
        add_out.status.success(),
        "todo add failed: {}",
        String::from_utf8_lossy(&add_out.stderr)
    );

    let add_json: serde_json::Value =
        serde_json::from_slice(&add_out.stdout).expect("parse todo add json");
    let task_id = add_json["id"].as_str().expect("task id").to_string();

    let list_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["todo", "list", "--format", "json"])
        .current_dir(&dir)
        .output()
        .expect("todo list");

    assert!(
        list_out.status.success(),
        "todo list failed: {}",
        String::from_utf8_lossy(&list_out.stderr)
    );

    let list_stdout = String::from_utf8_lossy(&list_out.stdout);
    assert!(
        list_stdout.contains(&title),
        "Task '{}' not found in list output after cloud opt-in: {}",
        title,
        list_stdout
    );
    assert!(
        list_stdout.contains(&task_id),
        "Cloud task ID '{}' not found in list output",
        task_id
    );

    let get_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["todo", "get", "--id", &task_id, "--format", "json"])
        .current_dir(&dir)
        .output()
        .expect("todo get");

    assert!(
        get_out.status.success(),
        "todo get failed: {}",
        String::from_utf8_lossy(&get_out.stderr)
    );

    let get_stdout = String::from_utf8_lossy(&get_out.stdout);
    assert!(
        get_stdout.contains(&title),
        "Task get did not return expected title"
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
    assert!(config.contains("mode = \"local\""));
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
    assert_eq!(registration["route"], "POST /api/decapod/init/register");
    assert!(
        registration["writes"]
            .as_array()
            .expect("writes array")
            .iter()
            .any(|write| write["table"] == "init_events" && write["operation"] == "insert"),
        "registration should model backend-owned init event insert"
    );
}
