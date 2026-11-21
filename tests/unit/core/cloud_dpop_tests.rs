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

struct MockOsKeystoreDriver {
    available: bool,
    fail_store: bool,
    entries: std::sync::Mutex<std::collections::BTreeMap<String, String>>,
}

impl TestKeystoreDriver for MockOsKeystoreDriver {
    fn available(&self) -> bool {
        self.available
    }

    fn load(&self, scope_key: &str) -> Result<Option<String>, DecapodError> {
        Ok(self.entries.lock().unwrap().get(scope_key).cloned())
    }

    fn store(&self, scope_key: &str, payload: &str) -> Result<(), DecapodError> {
        if self.fail_store {
            return Err(DecapodError::ValidationError(
                "simulated OS keystore failure".into(),
            ));
        }
        self.entries
            .lock()
            .unwrap()
            .insert(scope_key.to_string(), payload.to_string());
        Ok(())
    }

    fn delete(&self, scope_key: &str) -> Result<(), DecapodError> {
        self.entries.lock().unwrap().remove(scope_key);
        Ok(())
    }
}

#[test]
fn cross_platform_keystore_backends_support_os_custody_and_strict_file_fallback() {
    assert!(!linux_secret_service_available_from(
        None,
        Some(Path::new("/usr/bin/secret-tool"))
    ));
    assert!(!linux_secret_service_available_from(
        Some(""),
        Some(Path::new("/usr/bin/secret-tool"))
    ));
    assert!(linux_secret_service_available_from(
        Some("unix:path=/run/user/1000/bus"),
        Some(Path::new("/usr/bin/secret-tool"))
    ));
    assert_eq!(
        secret_tool_lookup_args("scope_1"),
        vec!["lookup", "service", KEYSTORE_SERVICE, "account", "scope_1"]
    );
    assert_eq!(
        secret_tool_store_args("scope_1"),
        vec![
            "store",
            "--label",
            "Decapod Cloud Device Key (scope_1)",
            "service",
            KEYSTORE_SERVICE,
            "account",
            "scope_1"
        ]
    );
    assert_eq!(
        secret_tool_clear_args("scope_1"),
        vec!["clear", "service", KEYSTORE_SERVICE, "account", "scope_1"]
    );
    assert_eq!(
        windows_credential_target("scope_1"),
        format!("{KEYSTORE_SERVICE}/scope_1")
    );
    assert!(!windows_credential_manager_available_from(None, None));
    assert!(windows_credential_manager_available_from(
        Some(Path::new(
            "C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"
        )),
        Some("Console")
    ));

    assert!(command_on_path("__decapod_missing_cmd_xyz__").is_none());

    let tmp = TempDir::new().expect("tempdir");
    let repo_root = tmp.path().join("repo");
    std::fs::create_dir_all(&repo_root).expect("create repo root");
    // 1. Healthy OS keychain driver stores without leaving a plaintext key file,
    // and exposes non-secret inventory + server registration metadata.
    let driver = std::sync::Arc::new(MockOsKeystoreDriver {
        available: true,
        fail_store: false,
        entries: std::sync::Mutex::new(std::collections::BTreeMap::new()),
    });
    let _guard = install_test_keystore_driver(driver.clone());
    let store = DeviceKeyStore::with_root_dir(
        "https://service.example",
        "DecapodLabs/decapod",
        &repo_root,
        &tmp.path().join("keys"),
    )
    .expect("keystore")
    .with_os_keychain(true);
    let created = store.load_or_create().expect("create via OS keychain");
    assert_eq!(created.backend, DeviceKeyStorageBackend::OsKeychain);
    assert!(
        !store.file_path().exists(),
        "plaintext fallback file must not exist when OS keychain succeeds"
    );
    store
        .record_device_key_id(&created.jkt, "11111111-1111-4111-8111-111111111111")
        .expect("record device_key_id");
    let inv = store
        .inspect("https://service.example", "DecapodLabs/decapod")
        .expect("inspect")
        .expect("inventory entry");
    assert_eq!(inv.jkt, created.jkt);
    assert_eq!(inv.backend, "os_keychain");
    assert_eq!(
        inv.device_key_id.as_deref(),
        Some("11111111-1111-4111-8111-111111111111")
    );
    assert!(inv.active);

    // 2. When the OS keychain write fails/is headless, DeviceKeyStore falls back
    // cleanly to strict mode-0600 PrivateFile0600 without losing custody.
    let failing_driver = std::sync::Arc::new(MockOsKeystoreDriver {
        available: true,
        fail_store: true,
        entries: std::sync::Mutex::new(std::collections::BTreeMap::new()),
    });
    let _guard2 = install_test_keystore_driver(failing_driver);
    let fallback_store = DeviceKeyStore::with_root_dir(
        "https://service.example",
        "DecapodLabs/other",
        &repo_root,
        &tmp.path().join("keys-fallback"),
    )
    .expect("fallback keystore")
    .with_os_keychain(true);
    let fallback_key = fallback_store
        .load_or_create()
        .expect("fallback create must succeed");
    assert_eq!(
        fallback_key.backend,
        DeviceKeyStorageBackend::PrivateFile0600
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(fallback_store.file_path())
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
    let inv2 = fallback_store
        .inspect("https://service.example", "DecapodLabs/other")
        .expect("inspect")
        .expect("inventory");
    assert_eq!(inv2.backend, "private_file_0600");
    let inv_json = serde_json::to_string(&inv2).unwrap();
    assert!(!inv_json.contains("pkcs8"));
    assert!(!inv_json.contains("private_key"));
    assert!(!inv_json.contains("\"d\""));
}
