use decapod::core::propodus::{CurlTransport, PropodusClient, PropodusClientError, PropodusConfig};
use std::env;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;

const CANONICAL_REPO_ID: &str = "DecapodLabs/decapod";

fn required_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is required for the live proof"))
}

fn run_decapod(dir: &Path, args: &[&str], agent: &str, token: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(args)
        .current_dir(dir)
        .env("DECAPOD_AGENT_ID", agent)
        .env("DECAPOD_ACCESS_TOKEN", token)
        .env(
            "DECAPOD_PROPODUS_API_URL",
            required_env("DECAPOD_PROPODUS_API_URL"),
        )
        .output()
        .expect("run decapod command")
}

fn prepare_cloud_repo(dir: &Path, remote: &str) {
    let init = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["init", "--backend", "cloud", "--force", "--proof"])
        .current_dir(dir)
        .output()
        .expect("initialize cloud proof repository");
    assert!(
        init.status.success(),
        "cloud init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let remote_result = Command::new("git")
        .args(["remote", "add", "origin", remote])
        .current_dir(dir)
        .output()
        .expect("configure proof remote");
    assert!(remote_result.status.success());
}

#[test]
#[ignore = "requires DECAPOD_PROPODUS_LIVE=1 and protected Propodus inputs"]
fn live_propodus_contract_proves_canonical_scope_and_mutations() {
    assert_eq!(
        env::var("DECAPOD_PROPODUS_LIVE").as_deref(),
        Ok("1"),
        "set DECAPOD_PROPODUS_LIVE=1 to opt into the live proof"
    );

    let api_url = required_env("DECAPOD_PROPODUS_API_URL");
    let credential = required_env("DECAPOD_PROPODUS_ACCESS_TOKEN");
    let disposable_repo_id = required_env("DECAPOD_PROPODUS_DISPOSABLE_REPO_ID");
    assert_ne!(
        disposable_repo_id, CANONICAL_REPO_ID,
        "the disposable repository identifier must not be the canonical repository"
    );

    let config = PropodusConfig {
        api_url,
        repo_id: CANONICAL_REPO_ID.to_string(),
    };
    let client = PropodusClient::with_transport(&config, &credential, CurlTransport::default())
        .expect("live Propodus client configuration");

    client.health_check().expect("Propodus health check");
    client.list_todos().expect("canonical repository list");

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let todo = client
        .create_todo(
            &format!("Decapod live contract proof {nonce}"),
            Some("Completed by the explicit live proof; safe to remove with service tooling."),
            Some("decapod-live-proof"),
        )
        .expect("canonical repository create");
    assert_eq!(todo.repo_id, CANONICAL_REPO_ID);
    client
        .claim_todo(&todo.id, "decapod-live-proof")
        .expect("canonical repository claim");
    client
        .complete_todo(&todo.id, "decapod-live-proof")
        .expect("canonical repository complete");

    let invalid_client = PropodusClient::with_transport(
        &PropodusConfig {
            repo_id: CANONICAL_REPO_ID.to_string(),
            ..config.clone()
        },
        "not-a-valid-propodus-token",
        CurlTransport::default(),
    )
    .expect("invalid-token proof client configuration");
    assert!(
        matches!(
            invalid_client.list_todos(),
            Err(PropodusClientError::Authentication(_))
        ),
        "Propodus must reject an invalid bearer token with 401 authentication failure"
    );

    let unauthorized_config = PropodusConfig {
        repo_id: disposable_repo_id,
        ..config
    };
    let unauthorized =
        PropodusClient::with_transport(&unauthorized_config, &credential, CurlTransport::default())
            .expect("disposable repository client configuration");
    match unauthorized.list_todos() {
        Err(PropodusClientError::Service {
            status: 403, code, ..
        }) => assert_eq!(code, "repository_not_authorized"),
        Err(error) => panic!("expected 403 repository_not_authorized, got {error}"),
        Ok(_) => panic!("unprovisioned repository unexpectedly returned todo data"),
    }
}

#[test]
#[ignore = "requires DECAPOD_PROPODUS_LIVE=1 and protected Propodus inputs"]
fn live_decapod_cloud_commands_share_state_and_reject_forks() {
    assert_eq!(
        env::var("DECAPOD_PROPODUS_LIVE").as_deref(),
        Ok("1"),
        "set DECAPOD_PROPODUS_LIVE=1 to opt into the live proof"
    );
    let api_url = required_env("DECAPOD_PROPODUS_API_URL");
    let credential = required_env("DECAPOD_PROPODUS_ACCESS_TOKEN");

    let canonical = tempdir().expect("canonical proof repository");
    prepare_cloud_repo(
        canonical.path(),
        &api_url,
        "git@github.com:DecapodLabs/decapod.git",
    );
    let title = format!(
        "Decapod command dogfood proof {}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    );
    let add = run_decapod(
        canonical.path(),
        &["todo", "add", &title, "--format", "json"],
        "decapod-live-agent-one",
        &credential,
    );
    assert!(
        add.status.success(),
        "cloud add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let add_json: serde_json::Value = serde_json::from_slice(&add.stdout).expect("add JSON");
    let task_id = add_json["id"].as_str().expect("cloud task ID").to_string();

    for args in [
        vec!["todo", "claim", "--id", task_id.as_str()],
        vec!["todo", "done", "--id", task_id.as_str()],
    ] {
        let output = run_decapod(
            canonical.path(),
            &args,
            "decapod-live-agent-one",
            &credential,
        );
        assert!(
            output.status.success(),
            "cloud lifecycle command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let second_agent_list = run_decapod(
        canonical.path(),
        &["todo", "list", "--status", "all", "--format", "json"],
        "decapod-live-agent-two",
        &credential,
    );
    assert!(second_agent_list.status.success());
    let list = String::from_utf8_lossy(&second_agent_list.stdout);
    assert!(
        list.contains(&task_id),
        "second agent cannot see shared task: {list}"
    );
    assert!(
        list.contains(&title),
        "second agent cannot see task title: {list}"
    );

    let fork = tempdir().expect("fork proof repository");
    prepare_cloud_repo(fork.path(), "git@github.com:someone/decapod.git");
    let fork_list = run_decapod(
        fork.path(),
        &["todo", "list", "--format", "json"],
        "decapod-live-fork-agent",
        &credential,
    );
    assert!(
        !fork_list.status.success(),
        "unauthorized forks must fail closed"
    );
    let fork_error = String::from_utf8_lossy(&fork_list.stderr);
    assert!(
        fork_error.contains("403") || fork_error.contains("repository_not_authorized"),
        "unexpected fork rejection: {fork_error}"
    );
}
