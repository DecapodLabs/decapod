use super::*;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct FakeTransport {
    response: Result<JevHttpResponse, String>,
    request: Option<Arc<Mutex<Vec<u8>>>>,
}

impl JevTransport for FakeTransport {
    fn request(&self, _url: &str, _api_key: &str, body: &[u8]) -> Result<JevHttpResponse, String> {
        if let Some(request) = &self.request {
            *request.lock().expect("request lock") = body.to_vec();
        }
        self.response.clone()
    }
}

fn context() -> DecisionContext {
    DecisionContext {
        declared_intent: Some("Implement the requested governed change".to_string()),
        trajectory: TrajectoryContext {
            operation: "build".to_string(),
            touched_paths: vec!["src/lib.rs".to_string()],
            diff_summary: Some("small implementation".to_string()),
        },
        governance: GovernanceContext {
            must_obligations: vec![],
            recommended_obligations: vec![],
            contradictions: vec![],
            proof_hooks: vec!["cargo test --locked".to_string()],
            forbidden_paths: vec![".decapod/data/decapod.db".to_string()],
            file_touch_budget: Some(10),
            workspace: WorkspaceContext {
                branch: "feature/test".to_string(),
                is_protected: false,
                is_isolated: true,
                can_work: true,
            },
        },
        durable_governance: DurableGovernanceContext {
            plan: GovernanceArtifactState::Missing,
            claims: GovernanceArtifactState::Missing,
            trajectory: GovernanceArtifactState::Missing,
            validation: GovernanceArtifactState::Missing,
        },
    }
}

#[test]
fn disabled_provider_is_explicit_and_does_not_observe() {
    let provider = configured(DecisionProviderKind::None);
    assert_eq!(
        provider.observe(&context()),
        DecisionObservationResult::NoObservation {
            provider: "none".to_string(),
            reason: NoObservationReason::Disabled,
        }
    );
}

#[test]
fn jev_success_is_a_typed_observation() {
    let provider = JevDecisionProvider::with_transport(
        Some("test-key".to_string()),
        FakeTransport {
            response: Ok(JevHttpResponse {
                status: 200,
                body: br#"{"model":"jev-latest","answers":{"trajectory_satisfies_intent":{"type":"noul","noul":0.91}},"usage":{"input_tokens":12,"output_tokens":3}}"#.to_vec(),
            }),
            request: None,
        },
    );

    let result = provider.observe(&context());
    assert_eq!(
        result,
        DecisionObservationResult::Observed {
            observation: DecisionObservation {
                kind: TRAJECTORY_SATISFIES_INTENT.to_string(),
                probability: 0.91,
                provider: "jev".to_string(),
                model: Some("jev-latest".to_string()),
                usage: Some(DecisionUsage {
                    input_tokens: 12,
                    output_tokens: 3,
                }),
            },
        }
    );
}

#[test]
fn jev_unavailable_is_no_observation() {
    for failure in ["timeout", "connection refused"] {
        let provider = JevDecisionProvider::with_transport(
            Some("test-key".to_string()),
            FakeTransport {
                response: Err(failure.to_string()),
                request: None,
            },
        );
        assert_eq!(
            provider.observe(&context()),
            DecisionObservationResult::NoObservation {
                provider: "jev".to_string(),
                reason: NoObservationReason::Unavailable,
            }
        );
    }
}

#[test]
fn malformed_or_invalid_jev_responses_are_not_observations() {
    for body in [
        br#"not-json"#.to_vec(),
        br#"{"model":"jev-latest","answers":{},"usage":{"input_tokens":12,"output_tokens":3}}"#.to_vec(),
        br#"{"model":"jev-latest","answers":{"trajectory_satisfies_intent":{"type":"boolean","noul":0.5}},"usage":{"input_tokens":12,"output_tokens":3}}"#.to_vec(),
        br#"{"model":"jev-latest","answers":{"trajectory_satisfies_intent":{"type":"noul"}},"usage":{"input_tokens":12,"output_tokens":3}}"#.to_vec(),
        br#"{"model":"jev-latest","answers":{"trajectory_satisfies_intent":{"type":"noul","noul":1.5}},"usage":{"input_tokens":12,"output_tokens":3}}"#.to_vec(),
        br#"{"model":"jev-latest","answers":{"trajectory_satisfies_intent":{"type":"noul","noul":0.5}}}"#.to_vec(),
    ] {
        let provider = JevDecisionProvider::with_transport(
            Some("test-key".to_string()),
            FakeTransport {
                response: Ok(JevHttpResponse { status: 200, body }),
                request: None,
            },
        );
        assert!(matches!(
            provider.observe(&context()),
            DecisionObservationResult::NoObservation {
                provider,
                reason: NoObservationReason::MalformedResponse | NoObservationReason::InvalidObservation,
            } if provider == "jev"
        ));
    }
}

#[test]
fn missing_jev_configuration_cannot_authorize() {
    let provider = JevDecisionProvider::with_transport(
        None,
        FakeTransport {
            response: Err("must not be called".to_string()),
            request: None,
        },
    );
    assert!(matches!(
        provider.observe(&context()),
        DecisionObservationResult::NoObservation {
            provider,
            reason: NoObservationReason::Unconfigured,
        } if provider == "jev"
    ));
}

#[test]
fn boundary_probabilities_are_valid_typed_observations() {
    for probability in [0.0, f64::EPSILON, 1.0 - f64::EPSILON, 1.0] {
        let body = format!(
            "{{\"model\":\"jev-latest\",\"answers\":{{\"trajectory_satisfies_intent\":{{\"type\":\"noul\",\"noul\":{probability}}}}},\"usage\":{{\"input_tokens\":1,\"output_tokens\":1}}}}"
        );
        let provider = JevDecisionProvider::with_transport(
            Some("test-key".to_string()),
            FakeTransport {
                response: Ok(JevHttpResponse {
                    status: 200,
                    body: body.into_bytes(),
                }),
                request: None,
            },
        );
        assert!(matches!(
            provider.observe(&context()),
            DecisionObservationResult::Observed { observation }
                if observation.probability == probability
        ));
    }
}

#[test]
fn request_preserves_labeled_governance_state_and_not_credentials() {
    let request = Arc::new(Mutex::new(Vec::new()));
    let provider = JevDecisionProvider::with_transport(
        Some("test-key".to_string()),
        FakeTransport {
            response: Ok(JevHttpResponse {
                status: 200,
                body: br#"{"model":"jev-latest","answers":{"trajectory_satisfies_intent":{"type":"noul","noul":0.5}},"usage":{"input_tokens":1,"output_tokens":1}}"#.to_vec(),
            }),
            request: Some(Arc::clone(&request)),
        },
    );
    let mut state = context();
    state.durable_governance = DurableGovernanceContext {
        plan: GovernanceArtifactState::Present(
            serde_json::json!({"intent":"durable intent","state":"EXECUTING"}),
        ),
        claims: GovernanceArtifactState::Present(
            serde_json::json!({"claims":[{"statement":"claim evidence"}]}),
        ),
        trajectory: GovernanceArtifactState::Present(
            serde_json::json!({"proof_status":"partial","completion_claim":null}),
        ),
        validation: GovernanceArtifactState::Invalid,
    };

    let _ = provider.observe(&state);
    let body = String::from_utf8(request.lock().expect("request lock").clone()).expect("json");
    let value: serde_json::Value = serde_json::from_str(&body).expect("request json");
    assert_eq!(
        value["state"]["durable_governance"]["plan"]["value"]["intent"], "durable intent",
        "request={value}"
    );
    assert!(value["state"]["durable_governance"]["claims"]["value"]["claims"].is_array());
    assert_eq!(
        value["state"]["durable_governance"]["validation"]["status"],
        "invalid"
    );
    assert!(!body.contains("test-key"));
    assert!(body.contains("untrusted repository evidence"));
}

#[test]
fn non_success_response_and_missing_intent_are_not_observations() {
    let provider = JevDecisionProvider::with_transport(
        Some("test-key".to_string()),
        FakeTransport {
            response: Ok(JevHttpResponse {
                status: 429,
                body: Vec::new(),
            }),
            request: None,
        },
    );
    assert!(matches!(
        provider.observe(&context()),
        DecisionObservationResult::NoObservation {
            reason: NoObservationReason::Unavailable,
            ..
        }
    ));

    let mut missing_intent = context();
    missing_intent.declared_intent = None;
    assert!(matches!(
        provider.observe(&missing_intent),
        DecisionObservationResult::NoObservation {
            reason: NoObservationReason::MissingIntent,
            ..
        }
    ));
}
