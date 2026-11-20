use super::*;
use tempfile::TempDir;

#[test]
fn dpop_keypair_generation_and_rfc9449_proof_roundtrip() {
    let key = DeviceKeyMaterial::generate(DeviceKeyStorageBackend::PrivateFile0600)
        .expect("generate device key");
    assert_eq!(key.public_jwk.kty, "OKP");
    assert_eq!(key.public_jwk.crv, "Ed25519");
    assert!(!key.jkt.is_empty());

    // Debug output must never leak private key bytes.
    let debug = format!("{key:?}");
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("pkcs8_b64url"));

    let token = "dpc_at_test_access_token_12345";
    let now = 1_760_000_000;
    let proof = key
        .sign_dpop_proof(
            "post",
            "https://cloud.example.invalid/functions/v1/decapod-onboarding/cloud-query?ignored=1#frag",
            Some(token),
            Some("srv_nonce_abc123"),
            Some("https://cloud.example.invalid"),
            Some(now),
        )
        .expect("sign dpop proof");

    let claims = verify_dpop_proof(
        &proof,
        "POST",
        "https://cloud.example.invalid/functions/v1/decapod-onboarding/cloud-query",
        Some(token),
        Some("srv_nonce_abc123"),
        Some("https://cloud.example.invalid"),
        now + 5,
    )
    .expect("verify dpop proof");

    assert_eq!(claims.jkt, key.jkt);
    assert_eq!(claims.htm, "POST");
    assert_eq!(
        claims.htu,
        "https://cloud.example.invalid/functions/v1/decapod-onboarding/cloud-query"
    );

    // Wrong token must fail ath verification.
    let wrong_tok = verify_dpop_proof(
        &proof,
        "POST",
        "https://cloud.example.invalid/functions/v1/decapod-onboarding/cloud-query",
        Some("dpc_at_wrong_token"),
        Some("srv_nonce_abc123"),
        Some("https://cloud.example.invalid"),
        now + 5,
    );
    assert!(wrong_tok.is_err());

    // Wrong method or skewed timestamp must fail.
    assert!(
        verify_dpop_proof(
            &proof,
            "GET",
            "https://cloud.example.invalid/functions/v1/decapod-onboarding/cloud-query",
            Some(token),
            Some("srv_nonce_abc123"),
            Some("https://cloud.example.invalid"),
            now + 5,
        )
        .is_err()
    );
    assert!(
        verify_dpop_proof(
            &proof,
            "POST",
            "https://cloud.example.invalid/functions/v1/decapod-onboarding/cloud-query",
            Some(token),
            Some("srv_nonce_abc123"),
            Some("https://cloud.example.invalid"),
            now + MAX_DPOP_CLOCK_SKEW_SECONDS + 10,
        )
        .is_err()
    );
}

#[test]
fn dpop_verifier_rejects_private_jwk_parameters() {
    let header = base64url_encode(
        br#"{"typ":"dpop+jwt","alg":"EdDSA","jwk":{"kty":"OKP","crv":"Ed25519","x":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","d":"secret"}}"#,
    );
    let claims = base64url_encode(
        br#"{"jti":"1234567890abcdef","htm":"POST","htu":"https://example.invalid/x","iat":1760000000}"#,
    );
    let sig = base64url_encode(&[0u8; 64]);
    let forged = format!("{header}.{claims}.{sig}");
    let err = verify_dpop_proof(
        &forged,
        "POST",
        "https://example.invalid/x",
        None,
        None,
        None,
        1_760_000_000,
    )
    .expect_err("private jwk fields must be rejected");
    assert!(err.to_string().contains("private key parameters"));
}

#[test]
fn private_file_keystore_enforces_scope_permissions_and_external_location() {
    let tmp = TempDir::new().expect("tempdir");
    let repo_dir = tmp.path().join("repo");
    let keys_dir = tmp.path().join("xdg-data/decapod/device-keys");
    fs::create_dir_all(&repo_dir).expect("create repo dir");

    // Refuses storing device keys inside repository checkout.
    assert!(
        DeviceKeyStore::with_root_dir(
            "https://cloud.example.invalid",
            "DecapodLabs/decapod",
            &repo_dir,
            &repo_dir.join(".decapod/device-keys"),
        )
        .is_err()
    );

    let store_a = DeviceKeyStore::with_root_dir(
        "https://cloud.example.invalid",
        "DecapodLabs/decapod",
        &repo_dir,
        &keys_dir,
    )
    .expect("store A");
    let store_b = DeviceKeyStore::with_root_dir(
        "https://cloud.example.invalid",
        "DecapodLabs/propodus",
        &repo_dir,
        &keys_dir,
    )
    .expect("store B");

    let key_a = store_a.load_or_create().expect("create key A");
    let key_a_again = store_a.load_or_create().expect("reload key A");
    let key_b = store_b.load_or_create().expect("create key B");
    assert_eq!(key_a.jkt, key_a_again.jkt);
    assert_ne!(key_a.jkt, key_b.jkt);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(store_a.file_path())
            .expect("key metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);

        // Tampered permissive mode must fail closed.
        let mut perms = fs::metadata(store_a.file_path())
            .expect("key metadata")
            .permissions();
        perms.set_mode(0o644);
        fs::set_permissions(store_a.file_path(), perms).expect("chmod 644");
        let err = store_a.load().expect_err("world/group readable must fail");
        assert!(err.to_string().contains("DEVICE_KEY_INSECURE_PERMISSIONS"));
    }

    store_a.delete().expect("delete key A");
    assert!(store_a.load().expect("missing after delete").is_none());
}

#[test]
fn device_dpop_signer_implements_dactyl_request_proof_signer_with_nonce_tracking() {
    use dactyl_db::{AuthScheme, RequestProofInput, RequestProofSigner};

    let key = DeviceKeyMaterial::generate(DeviceKeyStorageBackend::PrivateFile0600).expect("key");
    let signer = DeviceDpopSigner::new(key);
    let url = "https://cloud.example.invalid/rest/v1/rpc/propodus_repository_access_v1";
    signer.observe_nonce(url, "server-nonce-1");
    let ath = dpop_access_token_hash("dpc_at_synthetic_tok").expect("ath");
    let input = RequestProofInput::new("POST", url, Some(&ath), None);
    let proof = signer.sign_request(&input).expect("sign DPoP request");
    assert_eq!(proof.scheme(), AuthScheme::DPoP);
}
