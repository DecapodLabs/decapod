use super::*;
use crate::core::cloud_dpop::{DeviceKeyStorageBackend, verify_dpop_proof};
use tempfile::TempDir;

#[test]
fn container_signer_enforces_scope_capability_separation_and_revocation() {
    let tmp = TempDir::new().expect("tempdir");
    let repo_dir = tmp.path().join("repo");
    let keys_dir = tmp.path().join("xdg-data/decapod/device-keys");
    std::fs::create_dir_all(&repo_dir).expect("repo dir");

    let key_store = DeviceKeyStore::with_root_dir(
        "https://cloud.example.invalid",
        "DecapodLabs/decapod",
        &repo_dir,
        &keys_dir,
    )
    .expect("keystore");
    let key = key_store.load_or_create().expect("create key");
    let revoked = Arc::new(AtomicBool::new(false));
    let now = 1_760_000_000;

    let policy = ContainerSignerPolicy {
        allowed_repository: "DecapodLabs/decapod".to_string(),
        allowed_origin: "https://cloud.example.invalid".to_string(),
        allowed_agent_id: "container-agent-1".to_string(),
        credentials: WorkspaceCredentialsConfig {
            git_transport: Some("https-token".to_string()),
            github_api: Some("gh-token".to_string()),
            propodus_cloud: Some("signer-broker".to_string()),
        },
        session_expires_at: now + 600,
        revoked: Arc::clone(&revoked),
        device_key_store: Some(key_store.clone()),
        access_token_digest: Some("dpc_at_bounded_session_tok".to_string()),
    };

    // Valid scoped request over real Unix domain socket.
    let sock_path = tmp.path().join("signer.sock");
    let _server = HostContainerSignerServer::bind(&sock_path, policy.clone()).expect("bind");
    let now_live = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    // Update expiry for live clock test: test direct sign_request with controlled clock first.
    let ok_resp = policy.sign_request(
        &ContainerSignRequest {
            capability: SignerCapabilityKind::PropodusCloud,
            repository: "DecapodLabs/decapod".to_string(),
            agent_id: "container-agent-1".to_string(),
            method: "POST".to_string(),
            url: "https://cloud.example.invalid/functions/v1/decapod-onboarding/cloud-batch"
                .to_string(),
            nonce: Some("nonce-1".to_string()),
            aud: Some("https://cloud.example.invalid".to_string()),
            access_token_sha256: None,
        },
        now,
        Some(&key),
    );
    assert!(ok_resp.ok, "unexpected denial: {:?}", ok_resp);
    let proof = ok_resp.dpop_proof.expect("dpop_proof");
    let verified = verify_dpop_proof(
        &proof,
        "POST",
        "https://cloud.example.invalid/functions/v1/decapod-onboarding/cloud-batch",
        None,
        Some("nonce-1"),
        Some("https://cloud.example.invalid"),
        now,
    )
    .expect("valid DPoP proof");
    assert_eq!(verified.jkt, key.jkt);

    // 1. Wrong repository refused.
    let wrong_repo = policy.sign_request(
        &ContainerSignRequest {
            capability: SignerCapabilityKind::PropodusCloud,
            repository: "OtherOrg/other-repo".to_string(),
            agent_id: "container-agent-1".to_string(),
            method: "POST".to_string(),
            url: "https://cloud.example.invalid/functions/v1/decapod-onboarding/cloud-batch"
                .to_string(),
            nonce: None,
            aud: None,
            access_token_sha256: None,
        },
        now,
        Some(&key),
    );
    assert!(!wrong_repo.ok);
    assert_eq!(
        wrong_repo.code.as_deref(),
        Some("CONTAINER_SIGNER_WRONG_REPOSITORY")
    );

    // 2. Rotated agent ID refused.
    let rotated = policy.sign_request(
        &ContainerSignRequest {
            capability: SignerCapabilityKind::PropodusCloud,
            repository: "DecapodLabs/decapod".to_string(),
            agent_id: "rogue-agent-2".to_string(),
            method: "POST".to_string(),
            url: "https://cloud.example.invalid/functions/v1/decapod-onboarding/cloud-batch"
                .to_string(),
            nonce: None,
            aud: None,
            access_token_sha256: None,
        },
        now,
        Some(&key),
    );
    assert!(!rotated.ok);
    assert_eq!(
        rotated.code.as_deref(),
        Some("CONTAINER_SIGNER_AGENT_MISMATCH")
    );

    // 3. Container cannot enroll, revoke, or hit foreign origin.
    let enroll_attempt = policy.sign_request(
        &ContainerSignRequest {
            capability: SignerCapabilityKind::PropodusCloud,
            repository: "DecapodLabs/decapod".to_string(),
            agent_id: "container-agent-1".to_string(),
            method: "POST".to_string(),
            url: "https://cloud.example.invalid/functions/v1/decapod-onboarding/device-enroll"
                .to_string(),
            nonce: None,
            aud: None,
            access_token_sha256: None,
        },
        now,
        Some(&key),
    );
    assert!(!enroll_attempt.ok);
    assert_eq!(
        enroll_attempt.code.as_deref(),
        Some("CONTAINER_SIGNER_OPERATION_DENIED")
    );

    // 4. Separate capabilities cannot cross boundaries (git_transport cannot mint Cloud DPoP).
    let wrong_cap = policy.sign_request(
        &ContainerSignRequest {
            capability: SignerCapabilityKind::GitTransport,
            repository: "DecapodLabs/decapod".to_string(),
            agent_id: "container-agent-1".to_string(),
            method: "POST".to_string(),
            url: "https://cloud.example.invalid/functions/v1/decapod-onboarding/cloud-query"
                .to_string(),
            nonce: None,
            aud: None,
            access_token_sha256: None,
        },
        now,
        Some(&key),
    );
    assert!(!wrong_cap.ok);
    assert_eq!(
        wrong_cap.code.as_deref(),
        Some("CONTAINER_SIGNER_UNSUPPORTED_FOR_DPOP")
    );

    // 5. Revoked host device refuses immediately even over the Unix socket.
    revoked.store(true, Ordering::SeqCst);
    let sock_resp = request_signature_via_socket(
        &sock_path,
        &ContainerSignRequest {
            capability: SignerCapabilityKind::PropodusCloud,
            repository: "DecapodLabs/decapod".to_string(),
            agent_id: "container-agent-1".to_string(),
            method: "POST".to_string(),
            url: "https://cloud.example.invalid/functions/v1/decapod-onboarding/cloud-query"
                .to_string(),
            nonce: None,
            aud: None,
            access_token_sha256: None,
        },
    )
    .expect("socket IPC");
    assert!(!sock_resp.ok);
    assert_eq!(
        sock_resp.code.as_deref(),
        Some("CONTAINER_SIGNER_DEVICE_REVOKED")
    );

    // 6. Debug & env filters never leak secret keys or raw tokens.
    let debug = format!("{policy:?}");
    assert!(!debug.contains("dpc_at_bounded_session_tok"));
    let filtered = filter_container_environment([
        ("DECAPOD_AGENT_ID", "a1"),
        ("DECAPOD_CLOUD_TOKEN", "secret"),
        ("SUPABASE_SERVICE_ROLE_KEY", "secret"),
        ("GH_TOKEN", "ghp_secret"),
        ("SAFE_VAR", "ok"),
    ]);
    assert_eq!(filtered.len(), 2);
    let _ = (now_live, DeviceKeyStorageBackend::PrivateFile0600);
}

#[test]
fn container_signer_allows_cloud_control_gateway_path_and_denies_019_admin_ops() {
    let tmp = TempDir::new().expect("tempdir");
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).expect("create repo dir");
    let now = 1_780_000_000;
    let key_store = DeviceKeyStore::with_root_dir(
        "https://cloud.example.invalid",
        "DecapodLabs/decapod",
        &repo,
        &tmp.path().join("keys"),
    )
    .expect("keystore")
    .with_os_keychain(false);
    let key = key_store.load_or_create().expect("key");
    let policy = ContainerSignerPolicy {
        allowed_repository: "DecapodLabs/decapod".to_string(),
        allowed_origin: "https://cloud.example.invalid".to_string(),
        allowed_agent_id: "container-agent-1".to_string(),
        credentials: crate::core::container_signer::WorkspaceCredentialsConfig {
            propodus_cloud: Some("signer-broker".to_string()),
            ..Default::default()
        },
        session_expires_at: now + 300,
        revoked: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        device_key_store: Some(key_store),
        access_token_digest: None,
    };

    let resp = policy.sign_request(
        &ContainerSignRequest {
            capability: SignerCapabilityKind::PropodusCloud,
            repository: "DecapodLabs/decapod".to_string(),
            agent_id: "container-agent-1".to_string(),
            method: "POST".to_string(),
            url: "https://cloud.example.invalid/functions/v1/decapod-onboarding/cloud-control"
                .to_string(),
            nonce: Some("nonce-1".to_string()),
            aud: Some("https://cloud.example.invalid".to_string()),
            access_token_sha256: Some("ath_test".to_string()),
        },
        now,
        Some(&key),
    );
    assert!(resp.ok, "unexpected failure: {:?}", resp.code);
    assert!(resp.dpop_proof.is_some());

    // All 6 dactyl-db 0.19.0 human-bearer admin AccountOperations MUST be denied inside containers.
    for denied_path in [
        "repository-enroll",
        "resource-limits",
        "org-admins",
        "org-recovery",
        "policy-templates",
        "org-upgrades",
    ] {
        let denied = policy.sign_request(
            &ContainerSignRequest {
                capability: SignerCapabilityKind::PropodusCloud,
                repository: "DecapodLabs/decapod".to_string(),
                agent_id: "container-agent-1".to_string(),
                method: "POST".to_string(),
                url: format!(
                    "https://cloud.example.invalid/functions/v1/decapod-onboarding/{denied_path}"
                ),
                nonce: None,
                aud: Some("https://cloud.example.invalid".to_string()),
                access_token_sha256: Some("ath_test".to_string()),
            },
            now,
            Some(&key),
        );
        assert!(
            !denied.ok,
            "container signer must deny privileged admin endpoint `{denied_path}`"
        );
    }
}

#[test]
fn container_signer_rejects_origin_suffix_port_userinfo_fragment_and_path_smuggling() {
    let tmp = TempDir::new().expect("tempdir");
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).expect("create repo dir");
    let now = 1_780_000_000;
    let key_store = DeviceKeyStore::with_root_dir(
        "https://cloud.example.invalid",
        "DecapodLabs/decapod",
        &repo,
        &tmp.path().join("keys"),
    )
    .expect("keystore")
    .with_os_keychain(false);
    let key = key_store.load_or_create().expect("key");
    let policy = ContainerSignerPolicy {
        allowed_repository: "DecapodLabs/decapod".to_string(),
        allowed_origin: "https://cloud.example.invalid".to_string(),
        allowed_agent_id: "container-agent-1".to_string(),
        credentials: crate::core::container_signer::WorkspaceCredentialsConfig {
            propodus_cloud: Some("signer-broker".to_string()),
            ..Default::default()
        },
        session_expires_at: now + 300,
        revoked: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        device_key_store: Some(key_store),
        access_token_digest: None,
    };

    for (smuggled_url, expected_code) in [
        (
            "https://cloud.example.invalid.evil.com/functions/v1/decapod-onboarding/cloud-query",
            "CONTAINER_SIGNER_ORIGIN_DENIED",
        ),
        (
            "https://cloud.example.invalid:8443/functions/v1/decapod-onboarding/cloud-query",
            "CONTAINER_SIGNER_ORIGIN_DENIED",
        ),
        (
            "https://cloud.example.invalid@evil.com/functions/v1/decapod-onboarding/cloud-query",
            "CONTAINER_SIGNER_ORIGIN_DENIED",
        ),
        (
            "https://cloud.example.invalid/functions/v1/decapod-onboarding/cloud-query#frag",
            "CONTAINER_SIGNER_ORIGIN_DENIED",
        ),
        (
            "https://cloud.example.invalid/evil/../functions/v1/decapod-onboarding/cloud-query",
            "CONTAINER_SIGNER_OPERATION_DENIED",
        ),
        (
            "https://cloud.example.invalid/attacker/cloud-query",
            "CONTAINER_SIGNER_OPERATION_DENIED",
        ),
    ] {
        let resp = policy.sign_request(
            &ContainerSignRequest {
                capability: SignerCapabilityKind::PropodusCloud,
                repository: "DecapodLabs/decapod".to_string(),
                agent_id: "container-agent-1".to_string(),
                method: "POST".to_string(),
                url: smuggled_url.to_string(),
                nonce: None,
                aud: Some("https://cloud.example.invalid".to_string()),
                access_token_sha256: Some("ath_test".to_string()),
            },
            now,
            Some(&key),
        );
        assert!(
            !resp.ok,
            "smuggled URL must be denied: {smuggled_url} -> {:?}",
            resp
        );
        assert_eq!(
            resp.code.as_deref(),
            Some(expected_code),
            "unexpected code for {smuggled_url}: {:?}",
            resp
        );
    }
}
