use decapod::CloudRuntimeConfig;
use decapod::core::auth::{CredentialSource, resolve_cloud_credential};
use decapod::core::propodus::{
    PROPODUS_CONTRACT_ID, PROPODUS_CONTRACT_VERSION, PropodusClient, PropodusClientError,
    PropodusConfig, PropodusError, PropodusHttpResponse, PropodusListResponse,
    PropodusMutationResponse, PropodusTodo, PropodusTransport,
};
use decapod::core::repo_identity::RepositoryIdentity;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

type RecordedRequest = (String, String, String, Option<Vec<u8>>);

#[derive(Clone, Default)]
struct FakePropodusService {
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    responses: Arc<Mutex<VecDeque<PropodusHttpResponse>>>,
}

impl FakePropodusService {
    fn with_responses(responses: Vec<PropodusHttpResponse>) -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(responses.into_iter().collect())),
        }
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl PropodusTransport for FakePropodusService {
    fn request(
        &self,
        method: &str,
        url: &str,
        bearer: &str,
        body: Option<&[u8]>,
    ) -> Result<PropodusHttpResponse, PropodusClientError> {
        self.requests.lock().unwrap().push((
            method.to_string(),
            url.to_string(),
            bearer.to_string(),
            body.map(ToOwned::to_owned),
        ));
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| PropodusClientError::Transport("fake response exhausted".to_string()))
    }
}

fn config() -> PropodusConfig {
    PropodusConfig {
        api_url: "https://propodus.example.test".to_string(),
        repo_id: "DecapodLabs/decapod".to_string(),
    }
}

fn todo(status: &str) -> PropodusTodo {
    PropodusTodo {
        id: "todo-1".to_string(),
        repo_id: "DecapodLabs/decapod".to_string(),
        title: "phase proof".to_string(),
        description: Some("local fake".to_string()),
        status: status.to_string(),
        actor: None,
        created_at: None,
        updated_at: None,
        version: Some(1),
    }
}

fn json<T: serde::Serialize>(value: &T) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}

#[test]
fn fixture_declares_versioned_contract_and_failure_taxonomy() {
    let raw = include_str!("fixtures/propodus/todo-contract-v1.json");
    let fixture: serde_json::Value = serde_json::from_str(raw).unwrap();
    assert_eq!(fixture["contract_id"], PROPODUS_CONTRACT_ID);
    assert_eq!(fixture["contract_version"], PROPODUS_CONTRACT_VERSION);
    assert_eq!(fixture["routes"]["list"], "GET /api/todos?repo_id=<repo>");
    let errors = fixture["error_examples"].as_array().unwrap();
    assert!(errors.iter().any(|error| error["status"] == 401));
    assert!(errors.iter().any(|error| error["status"] == 403));
    assert!(errors.iter().any(|error| error["status"] == 409));
}

#[test]
fn onboarding_fixture_declares_provider_neutral_routes_and_safety_boundary() {
    let raw = include_str!("fixtures/propodus/onboarding-contract-v1.json");
    let fixture: serde_json::Value = serde_json::from_str(raw).unwrap();
    assert_eq!(fixture["contract_id"], "decapod.cloud.onboarding");
    assert_eq!(fixture["contract_version"], "v1");
    assert_eq!(
        fixture["routes"]["exchange"],
        "POST /api/onboarding/exchange"
    );
    assert_eq!(
        fixture["routes"]["session_exchange"],
        "POST /api/auth/session/exchange"
    );
    assert_eq!(
        fixture["repository_binding"]["canonical_name"],
        "DecapodLabs/decapod"
    );
    assert!(
        fixture["safety"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value.as_str().unwrap().contains("repository configuration"))
    );
}

#[test]
fn credential_precedence_is_explicit_environment_then_machine_file() {
    let explicit = resolve_cloud_credential(
        Some("explicit-token"),
        Some("environment-token"),
        Some("machine-token"),
    )
    .unwrap();
    assert_eq!(explicit.source, CredentialSource::Explicit);
    assert_eq!(explicit.token, "explicit-token");

    let environment =
        resolve_cloud_credential(None, Some("environment-token"), Some("machine-token")).unwrap();
    assert_eq!(environment.source, CredentialSource::Environment);

    let machine = resolve_cloud_credential(None, None, Some("machine-token")).unwrap();
    assert_eq!(machine.source, CredentialSource::MachineFile);
}

#[test]
fn cloud_config_binds_client_to_verified_repository_identity() {
    let identity = RepositoryIdentity {
        canonical_name: "DecapodLabs/decapod".to_string(),
        owner: "DecapodLabs".to_string(),
        repository: "decapod".to_string(),
        remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
    };
    let runtime = CloudRuntimeConfig {
        provider: "vercel".to_string(),
        api_url: "https://propodus.example.test".to_string(),
    };

    let config = PropodusConfig::for_repository(&runtime, &identity);

    assert_eq!(config.api_url, runtime.api_url);
    assert_eq!(config.repo_id, identity.canonical_name);
}

#[test]
fn fake_service_proves_list_create_claim_and_complete_wire_contract() {
    let fake = FakePropodusService::with_responses(vec![
        PropodusHttpResponse {
            status: 200,
            body: json(&PropodusListResponse {
                ok: true,
                todos: vec![todo("pending")],
                error: None,
            }),
        },
        PropodusHttpResponse {
            status: 201,
            body: json(&PropodusMutationResponse {
                ok: true,
                todo: Some(todo("pending")),
                error: None,
            }),
        },
        PropodusHttpResponse {
            status: 200,
            body: json(&PropodusMutationResponse {
                ok: true,
                todo: Some(todo("in_progress")),
                error: None,
            }),
        },
        PropodusHttpResponse {
            status: 200,
            body: json(&PropodusMutationResponse {
                ok: true,
                todo: Some(todo("completed")),
                error: None,
            }),
        },
    ]);
    let client = PropodusClient::with_transport(&config(), "test-bearer", fake.clone()).unwrap();

    assert_eq!(client.list_todos().unwrap().len(), 1);
    assert_eq!(
        client
            .create_todo("phase proof", Some("local fake"), Some("test-agent"))
            .unwrap()
            .id,
        "todo-1"
    );
    client.claim_todo("todo-1", "test-agent").unwrap();
    client.complete_todo("todo-1", "test-agent").unwrap();

    let requests = fake.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[0].0, "GET");
    assert!(
        requests[0]
            .1
            .ends_with("/api/todos?repo_id=DecapodLabs%2Fdecapod")
    );
    assert_eq!(requests[0].2, "test-bearer");
    assert_eq!(requests[1].0, "POST");
    assert_eq!(requests[2].0, "PATCH");
    assert!(String::from_utf8_lossy(requests[2].3.as_ref().unwrap()).contains("in_progress"));
    assert!(String::from_utf8_lossy(requests[3].3.as_ref().unwrap()).contains("completed"));
}

#[test]
fn failure_statuses_remain_distinct_and_do_not_decode_as_success() {
    let cases = [
        (401, "authentication_required", "Authentication"),
        (403, "repository_not_authorized", "Service"),
        (403, "organization_seat_required", "Service"),
        (404, "todo_not_found", "NotFound"),
        (409, "todo_conflict", "Conflict"),
    ];
    for (status, code, kind) in cases {
        let fake = FakePropodusService::with_responses(vec![PropodusHttpResponse {
            status,
            body: json(&PropodusErrorEnvelopeForTest {
                error: PropodusError {
                    code: code.to_string(),
                    message: "controlled fake failure".to_string(),
                },
            }),
        }]);
        let client = PropodusClient::with_transport(&config(), "test-bearer", fake).unwrap();
        let error = client.list_todos().unwrap_err();
        let rendered = format!("{error:?}");
        assert!(rendered.contains(kind), "{status} mapped to {rendered}");
        if status != 401 {
            assert!(rendered.contains("controlled fake failure"));
        } else {
            assert!(rendered.contains("machine session"));
            assert!(!rendered.contains("controlled fake failure"));
        }
    }
}

#[derive(serde::Serialize)]
struct PropodusErrorEnvelopeForTest {
    error: PropodusError,
}
