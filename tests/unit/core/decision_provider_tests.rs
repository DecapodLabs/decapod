use super::*;

#[derive(Clone)]
struct FakeTransport {
    response: Result<JevHttpResponse, String>,
}

impl JevTransport for FakeTransport {
    fn request(&self, _url: &str, _api_key: &str, _body: &[u8]) -> Result<JevHttpResponse, String> {
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
                body: br#"{"model":"jev-latest","answers":{"trajectory_satisfies_intent":{"type":"noul","noul":0.91}}}"#.to_vec(),
            }),
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
            },
        }
    );
}

#[test]
fn jev_unavailable_is_no_observation() {
    let provider = JevDecisionProvider::with_transport(
        Some("test-key".to_string()),
        FakeTransport {
            response: Err("timeout".to_string()),
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

#[test]
fn malformed_or_invalid_jev_responses_are_not_observations() {
    for body in [
        br#"not-json"#.to_vec(),
        br#"{"model":"jev-latest","answers":{}}"#.to_vec(),
        br#"{"model":"jev-latest","answers":{"trajectory_satisfies_intent":{"type":"noul","noul":1.5}}}"#.to_vec(),
    ] {
        let provider = JevDecisionProvider::with_transport(
            Some("test-key".to_string()),
            FakeTransport {
                response: Ok(JevHttpResponse { status: 200, body }),
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
