use decapod::core::propodus::{CurlTransport, PropodusClient, PropodusClientError, PropodusConfig};
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

const CANONICAL_REPO_ID: &str = "DecapodLabs/decapod";

fn required_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is required for the live proof"))
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
        project_id: "decapod-live-proof".to_string(),
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
