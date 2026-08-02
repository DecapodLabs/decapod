// Moved from src/decapod/core/cloud_backend.rs
use super::{
    CloudOnboardingEndpoints, CloudOnboardingHandoff, CloudOnboardingStartRequest,
    CloudOnboardingStartResponse, CloudOnboardingState, CloudSession,
    CloudSessionExchangeRequest, CloudSessionRefreshRequest,
};
use crate::core::repo_identity::resolve_repository_identity_from_remote;

#[test]
fn onboarding_handoff_is_provider_neutral_and_headless_safe() {
    let handoff = CloudOnboardingHandoff::new(
        "https://cloud.example.test/onboard/one-time",
        "2030-01-01T00:00:00Z",
    )
    .expect("valid handoff");

    assert_eq!(handoff.state, CloudOnboardingState::Pending);
    assert_eq!(handoff.poll_after_seconds, 2);
    assert!(handoff.terminal_instruction().contains("one-time"));
    assert!(!handoff.terminal_instruction().contains("token"));
}

#[test]
fn onboarding_handoff_rejects_untrusted_or_unbounded_values() {
    assert!(
        CloudOnboardingHandoff::new(
            "http://cloud.example.test/onboard",
            "2030-01-01T00:00:00Z",
        )
        .is_err()
    );
    assert!(
        CloudOnboardingHandoff::new(
            "https://cloud.example.test/onboard?access_token=raw",
            "2030-01-01T00:00:00Z",
        )
        .is_err()
    );
    assert!(CloudOnboardingHandoff::new("https://cloud.example.test/onboard", "",).is_err());
}

#[test]
fn onboarding_contract_binds_remote_identity_without_provider_policy() {
    let identity =
        resolve_repository_identity_from_remote("git@github.com:DecapodLabs/decapod.git")
            .expect("repository identity");
    let request = CloudOnboardingStartRequest::for_repository(&identity);
    assert_eq!(request.contract_version, "v1");
    assert_eq!(request.repository.canonical_name, "DecapodLabs/decapod");
    assert_eq!(request.repository.owner, "DecapodLabs");
    assert_eq!(request.repository.repository, "decapod");
    let encoded = serde_json::to_string(&request).expect("request JSON");
    assert!(!encoded.contains("token"));
}

#[test]
fn onboarding_endpoints_are_bounded_and_credentials_never_enter_urls() {
    let endpoints =
        CloudOnboardingEndpoints::new("https://cloud.example.test/").expect("endpoints");
    assert_eq!(
        endpoints.start(),
        "https://cloud.example.test/api/onboarding/start"
    );
    assert_eq!(
        endpoints.status("flow/one".trim()).expect("status URL"),
        "https://cloud.example.test/api/onboarding/status?flow=flow%2Fone"
    );
    assert_eq!(
        endpoints.exchange(),
        "https://cloud.example.test/api/onboarding/exchange"
    );
    assert_eq!(
        endpoints.session_exchange(),
        "https://cloud.example.test/api/auth/session/exchange"
    );
    assert_eq!(
        endpoints.refresh(),
        "https://cloud.example.test/api/auth/session/refresh"
    );
    assert!(endpoints.status("flow one").is_err());
}

#[test]
fn exchange_and_refresh_payloads_validate_opaque_credentials() {
    let exchange = CloudSessionExchangeRequest::new("code-123").expect("exchange request");
    assert_eq!(exchange.code, "code-123");
    assert!(CloudSessionExchangeRequest::new("code 123").is_err());

    let session = CloudSession {
        access_token: "access-opaque".to_string(),
        refresh_token: Some("refresh-opaque".to_string()),
        session_id: Some("session-opaque".to_string()),
        expires_at: Some("2030-01-01T00:00:00Z".to_string()),
    };
    session.validate().expect("session");
    assert!(session.redacted_summary().contains("refresh=true"));
    assert!(!session.redacted_summary().contains("access-opaque"));

    let refresh =
        CloudSessionRefreshRequest::new("session-opaque", "refresh-opaque").expect("refresh");
    assert_eq!(refresh.session_id, "session-opaque");
    assert!(CloudSessionRefreshRequest::new(" ", "refresh-opaque").is_err());
}

#[test]
fn onboarding_start_response_produces_safe_handoff() {
    let (flow_id, handoff) = CloudOnboardingStartResponse {
        flow_id: "flow-123".to_string(),
        bootstrap_url: "https://cloud.example.test/onboard/opaque".to_string(),
        expires_at: "2030-01-01T00:00:00Z".to_string(),
        poll_after_seconds: Some(4),
    }
    .into_handoff()
    .expect("handoff");
    assert_eq!(flow_id, "flow-123");
    assert_eq!(handoff.poll_after_seconds, 4);
    assert!(
        CloudOnboardingStartResponse {
            flow_id: "flow-123".to_string(),
            bootstrap_url: "https://cloud.example.test/onboard/opaque".to_string(),
            expires_at: "2030-01-01T00:00:00Z".to_string(),
            poll_after_seconds: Some(0),
        }
        .into_handoff()
        .is_err()
    );
}
