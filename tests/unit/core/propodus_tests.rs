// Moved from src/decapod/core/propodus.rs
use super::*;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone)]
struct OnboardingStartTransport;

impl PropodusTransport for OnboardingStartTransport {
    fn request(
        &self,
        method: &str,
        url: &str,
        _bearer: &str,
        _body: Option<&[u8]>,
    ) -> Result<PropodusHttpResponse, PropodusClientError> {
        assert_eq!(method, "POST");
        assert!(url.ends_with("/api/onboarding/start"));
        Ok(PropodusHttpResponse {
            status: 200,
            body: br#"{"flow_id":"flow-opaque","bootstrap_url":"https://propodus.example.test/handoff?flow=opaque","expires_at":"2099-01-01T00:00:00Z","poll_after_seconds":2}"#.to_vec(),
        })
    }
}

#[derive(Clone)]
struct RefreshTransport;

impl PropodusTransport for RefreshTransport {
    fn request(
        &self,
        method: &str,
        url: &str,
        _bearer: &str,
        _body: Option<&[u8]>,
    ) -> Result<PropodusHttpResponse, PropodusClientError> {
        assert_eq!(method, "POST");
        assert!(url.ends_with("/api/auth/session/refresh"));
        let response = CloudSessionExchangeResponse {
            credentials: Some(CloudSession {
                access_token: "refreshed-access".to_string(),
                refresh_token: Some("refreshed-refresh".to_string()),
                session_id: Some("refreshed-session".to_string()),
                expires_at: Some("2099-01-01T00:00:00Z".to_string()),
            }),
            session: None,
        };
        Ok(PropodusHttpResponse {
            status: 200,
            body: serde_json::to_vec(&response).unwrap(),
        })
    }
}

#[derive(Clone, Default)]
struct NoRequestTransport {
    calls: Arc<Mutex<usize>>,
}

impl PropodusTransport for NoRequestTransport {
    fn request(
        &self,
        _method: &str,
        _url: &str,
        _bearer: &str,
        _body: Option<&[u8]>,
    ) -> Result<PropodusHttpResponse, PropodusClientError> {
        *self.calls.lock().unwrap() += 1;
        Err(PropodusClientError::Validation(
            "unexpected provider request".to_string(),
        ))
    }
}

#[derive(Clone, Default)]
struct AuthorizedExchangeTransport {
    calls: Arc<Mutex<Vec<String>>>,
}

impl PropodusTransport for AuthorizedExchangeTransport {
    fn request(
        &self,
        method: &str,
        url: &str,
        _bearer: &str,
        _body: Option<&[u8]>,
    ) -> Result<PropodusHttpResponse, PropodusClientError> {
        self.calls.lock().unwrap().push(format!("{method} {url}"));
        if method == "POST" && url.ends_with("/api/onboarding/start") {
            return Ok(PropodusHttpResponse {
                status: 200,
                body: br#"{"flow_id":"flow-authorized","bootstrap_url":"https://propodus.example.test/handoff?flow=authorized","expires_at":"2099-01-01T00:00:00Z","poll_after_seconds":1}"#.to_vec(),
            });
        }
        if method == "GET" && url.contains("/api/onboarding/status") {
            return Ok(PropodusHttpResponse {
                status: 200,
                body: br#"{"flow_id":"flow-authorized","state":"authorized"}"#.to_vec(),
            });
        }
        if method == "POST" && url.ends_with("/api/onboarding/exchange") {
            return Ok(PropodusHttpResponse {
                status: 200,
                body: br#"{"transaction_id":"tx-authorized","repository_id":"DecapodLabs/decapod","code":"opaque-exchange-code"}"#.to_vec(),
            });
        }
        if method == "POST" && url.ends_with("/api/auth/session/exchange") {
            let response = CloudSessionExchangeResponse {
                credentials: Some(CloudSession {
                    access_token: "access-after-exchange".to_string(),
                    refresh_token: Some("refresh-after-exchange".to_string()),
                    session_id: Some("session-after-exchange".to_string()),
                    expires_at: Some("2099-01-01T00:00:00Z".to_string()),
                }),
                session: None,
            };
            return Ok(PropodusHttpResponse {
                status: 200,
                body: serde_json::to_vec(&response).unwrap(),
            });
        }
        Err(PropodusClientError::Validation(
            "unexpected provider request".to_string(),
        ))
    }
}

fn auth_env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

#[test]
fn auth_diagnostic_schema_is_stable_and_secret_free() {
    let diagnostic = CloudAuthDiagnostic::new(
        CloudAuthStatus::Unauthorized,
        "the cloud service rejected the machine session",
        cloud_init_action("to renew the machine session"),
    );
    let value = serde_json::to_value(&diagnostic).expect("serialize diagnostic");
    assert_eq!(
        value,
        serde_json::json!({
            "schema_version": "decapod.cloud.auth.v1",
            "status": "unauthorized",
            "message": "the cloud service rejected the machine session",
            "next_action": "run `decapod init --backend cloud` to renew the machine session"
        })
    );
    let rendered = serde_json::to_string(&value).unwrap();
    for secret in ["access-token", "refresh-token", "Bearer", "flow-opaque"] {
        assert!(!rendered.contains(secret), "diagnostic leaked {secret}");
    }
}

#[test]
fn auth_request_failures_have_actionable_distinct_states() {
    let offline = map_auth_request_error(PropodusClientError::Transport(
        "connection refused".to_string(),
    ));
    assert_eq!(
        cloud_auth_diagnostic(&offline).unwrap().status,
        CloudAuthStatus::Offline
    );

    let unauthorized = map_auth_request_error(PropodusClientError::Service {
        status: 403,
        code: "repository_not_authorized".to_string(),
        message: "provider message is intentionally discarded".to_string(),
    });
    assert_eq!(
        cloud_auth_diagnostic(&unauthorized).unwrap().status,
        CloudAuthStatus::RepositoryDenied
    );
    assert!(!unauthorized.to_string().contains("provider message"));

    let revoked = map_http_error(PropodusHttpResponse {
        status: 401,
        body: br#"{"error":{"code":"session_revoked","message":"secret provider detail"}}"#
            .to_vec(),
    });
    assert_eq!(
        cloud_auth_diagnostic(&revoked).unwrap().status,
        CloudAuthStatus::Revoked
    );
    assert!(!revoked.to_string().contains("secret provider detail"));

    let provider = map_http_error(PropodusHttpResponse {
        status: 503,
        body: br#"{"error":{"code":"provider_down","message":"secret provider detail"}}"#.to_vec(),
    });
    assert_eq!(
        cloud_auth_diagnostic(&provider).unwrap().status,
        CloudAuthStatus::ProviderUnavailable
    );
    assert!(!provider.to_string().contains("secret provider detail"));

    let identity = map_auth_request_error(PropodusClientError::Service {
        status: 403,
        code: "identity_not_authorized".to_string(),
        message: "secret provider detail".to_string(),
    });
    assert_eq!(
        cloud_auth_diagnostic(&identity).unwrap().status,
        CloudAuthStatus::UnauthorizedIdentity
    );
    assert!(!identity.to_string().contains("secret provider detail"));
}

#[test]
fn valid_machine_session_is_reused_without_onboarding_or_provider_requests() {
    let _lock = auth_env_lock();
    let data_home = tempfile::tempdir().expect("data home");
    let old_data_home = std::env::var_os("XDG_DATA_HOME");
    let old_token = std::env::var_os(auth::CLOUD_ACCESS_TOKEN_ENV);
    unsafe {
        std::env::set_var("XDG_DATA_HOME", data_home.path());
        std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV);
    }

    auth::store_machine_session(&CloudSession {
        access_token: "valid-access".to_string(),
        refresh_token: Some("valid-refresh".to_string()),
        session_id: Some("valid-session".to_string()),
        expires_at: Some("2099-01-01T00:00:00Z".to_string()),
    })
    .expect("store valid machine session");
    let identity = RepositoryIdentity {
        canonical_name: "DecapodLabs/decapod".to_string(),
        owner: "DecapodLabs".to_string(),
        repository: "decapod".to_string(),
        remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
    };
    let config = PropodusConfig {
        api_url: "https://propodus.example.test".to_string(),
        repo_id: identity.canonical_name.clone(),
    };
    let transport = NoRequestTransport::default();
    let first = ensure_cloud_session(&config, &identity, transport.clone())
        .expect("valid machine session should be reusable");
    let second = ensure_cloud_session(&config, &identity, transport.clone())
        .expect("restart should reuse the same machine session");
    assert_eq!(first.token, "valid-access");
    assert_eq!(second.token, "valid-access");
    assert_eq!(*transport.calls.lock().unwrap(), 0);

    unsafe {
        match old_data_home {
            Some(value) => std::env::set_var("XDG_DATA_HOME", value),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
        match old_token {
            Some(value) => std::env::set_var(auth::CLOUD_ACCESS_TOKEN_ENV, value),
            None => std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV),
        }
    }
}

#[test]
fn matching_pending_onboarding_is_reused_without_duplicate_start() {
    let _lock = auth_env_lock();
    let data_home = tempfile::tempdir().expect("data home");
    let old_data_home = std::env::var_os("XDG_DATA_HOME");
    let old_token = std::env::var_os(auth::CLOUD_ACCESS_TOKEN_ENV);
    let old_headless = std::env::var_os("DECAPOD_HEADLESS");
    unsafe {
        std::env::set_var("XDG_DATA_HOME", data_home.path());
        std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV);
        std::env::set_var("DECAPOD_HEADLESS", "1");
    }

    let identity = RepositoryIdentity {
        canonical_name: "DecapodLabs/decapod".to_string(),
        owner: "DecapodLabs".to_string(),
        repository: "decapod".to_string(),
        remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
    };
    let config = PropodusConfig {
        api_url: "https://propodus.example.test".to_string(),
        repo_id: identity.canonical_name.clone(),
    };
    auth::store_pending_cloud_onboarding(&auth::PendingCloudOnboarding {
        api_url: config.api_url.clone(),
        repo_id: identity.canonical_name.clone(),
        flow_id: "flow-pending".to_string(),
        url: "https://propodus.example.test/handoff?flow=pending".to_string(),
        expires_at: "2099-01-01T00:00:00Z".to_string(),
    })
    .expect("store pending onboarding");

    let transport = NoRequestTransport::default();
    let error = ensure_cloud_session(&config, &identity, transport.clone())
        .expect_err("headless pending onboarding should return a resumable state");
    assert_eq!(
        cloud_auth_diagnostic(&error).unwrap().status,
        CloudAuthStatus::OnboardingPending
    );
    assert_eq!(*transport.calls.lock().unwrap(), 0);
    assert!(auth::load_pending_cloud_onboarding().unwrap().is_some());

    unsafe {
        match old_data_home {
            Some(value) => std::env::set_var("XDG_DATA_HOME", value),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
        match old_token {
            Some(value) => std::env::set_var(auth::CLOUD_ACCESS_TOKEN_ENV, value),
            None => std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV),
        }
        match old_headless {
            Some(value) => std::env::set_var("DECAPOD_HEADLESS", value),
            None => std::env::remove_var("DECAPOD_HEADLESS"),
        }
    }
}

#[test]
fn one_time_onboarding_and_session_exchange_persist_machine_session() {
    let _lock = auth_env_lock();
    let data_home = tempfile::tempdir().expect("data home");
    let old_data_home = std::env::var_os("XDG_DATA_HOME");
    let old_token = std::env::var_os(auth::CLOUD_ACCESS_TOKEN_ENV);
    unsafe {
        std::env::set_var("XDG_DATA_HOME", data_home.path());
        std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV);
    }

    let identity = RepositoryIdentity {
        canonical_name: "DecapodLabs/decapod".to_string(),
        owner: "DecapodLabs".to_string(),
        repository: "decapod".to_string(),
        remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
    };
    let config = PropodusConfig {
        api_url: "https://propodus.example.test".to_string(),
        repo_id: identity.canonical_name.clone(),
    };
    let transport = AuthorizedExchangeTransport::default();
    let credential =
        complete_cloud_onboarding_with_mode(&config, &identity, transport.clone(), false, true)
            .expect("authorized onboarding should complete");
    assert_eq!(credential.token, "access-after-exchange");
    assert_eq!(
        auth::load_machine_session().unwrap().unwrap().token,
        "access-after-exchange"
    );
    assert!(auth::load_pending_cloud_onboarding().unwrap().is_none());
    let calls = transport.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 4);
    assert!(calls[0].ends_with("POST https://propodus.example.test/api/onboarding/start"));
    assert!(calls[1].contains("GET https://propodus.example.test/api/onboarding/status"));
    assert!(calls[2].ends_with("POST https://propodus.example.test/api/onboarding/exchange"));
    assert!(calls[3].ends_with("POST https://propodus.example.test/api/auth/session/exchange"));
    let files: Vec<_> = std::fs::read_dir(data_home.path().join("decapod"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert!(
        !files
            .iter()
            .any(|name| name.to_string_lossy().contains("tmp"))
    );

    unsafe {
        match old_data_home {
            Some(value) => std::env::set_var("XDG_DATA_HOME", value),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
        match old_token {
            Some(value) => std::env::set_var(auth::CLOUD_ACCESS_TOKEN_ENV, value),
            None => std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV),
        }
    }
}

#[test]
fn noninteractive_first_use_persists_handoff_and_returns_pending() {
    let _lock = auth_env_lock();
    let data_home = tempfile::tempdir().expect("data home");
    let old_data_home = std::env::var_os("XDG_DATA_HOME");
    let old_token = std::env::var_os(auth::CLOUD_ACCESS_TOKEN_ENV);
    let old_headless = std::env::var_os("DECAPOD_HEADLESS");
    unsafe {
        std::env::set_var("XDG_DATA_HOME", data_home.path());
        std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV);
        std::env::set_var("DECAPOD_HEADLESS", "1");
    }

    let identity = RepositoryIdentity {
        canonical_name: "DecapodLabs/decapod".to_string(),
        owner: "DecapodLabs".to_string(),
        repository: "decapod".to_string(),
        remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
    };
    let config = PropodusConfig {
        api_url: "https://propodus.example.test".to_string(),
        repo_id: identity.canonical_name.clone(),
    };
    let error = ensure_cloud_session(&config, &identity, OnboardingStartTransport)
        .expect_err("noninteractive onboarding must pause for the human handoff");
    assert_eq!(
        cloud_auth_diagnostic(&error).unwrap().status,
        CloudAuthStatus::NonInteractive
    );
    assert!(
        data_home.path().join("decapod/onboarding.json").is_file(),
        "handoff custody must be machine-local"
    );
    assert!(!error.to_string().contains("flow-opaque"));
    assert!(!error.to_string().contains("Bearer"));

    unsafe {
        match old_data_home {
            Some(value) => std::env::set_var("XDG_DATA_HOME", value),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
        match old_token {
            Some(value) => std::env::set_var(auth::CLOUD_ACCESS_TOKEN_ENV, value),
            None => std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV),
        }
        match old_headless {
            Some(value) => std::env::set_var("DECAPOD_HEADLESS", value),
            None => std::env::remove_var("DECAPOD_HEADLESS"),
        }
    }
}

#[test]
fn expired_machine_session_refreshes_without_human_interaction() {
    let _lock = auth_env_lock();
    let data_home = tempfile::tempdir().expect("data home");
    let old_data_home = std::env::var_os("XDG_DATA_HOME");
    let old_token = std::env::var_os(auth::CLOUD_ACCESS_TOKEN_ENV);
    unsafe {
        std::env::set_var("XDG_DATA_HOME", data_home.path());
        std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV);
    }

    auth::store_machine_session(&CloudSession {
        access_token: "expired-access".to_string(),
        refresh_token: Some("refresh-token".to_string()),
        session_id: Some("session-id".to_string()),
        expires_at: Some("2000-01-01T00:00:00Z".to_string()),
    })
    .expect("store expired machine session");
    let identity = RepositoryIdentity {
        canonical_name: "DecapodLabs/decapod".to_string(),
        owner: "DecapodLabs".to_string(),
        repository: "decapod".to_string(),
        remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
    };
    let config = PropodusConfig {
        api_url: "https://propodus.example.test".to_string(),
        repo_id: identity.canonical_name.clone(),
    };
    let credential = ensure_cloud_session(&config, &identity, RefreshTransport)
        .expect("expired machine session should refresh");
    assert_eq!(credential.token, "refreshed-access");
    assert_eq!(credential.source, auth::CredentialSource::MachineFile);
    assert_eq!(auth::load_pending_cloud_onboarding().unwrap(), None);
    let stored = std::fs::read_to_string(data_home.path().join("decapod/session_token.json"))
        .expect("rotated machine session");
    assert!(stored.contains("refreshed-access"));
    assert!(stored.contains("refreshed-refresh"));
    assert!(
        !std::fs::read_dir(data_home.path().join("decapod"))
            .unwrap()
            .any(|entry| entry.unwrap().file_name().to_string_lossy().contains("tmp"))
    );

    unsafe {
        match old_data_home {
            Some(value) => std::env::set_var("XDG_DATA_HOME", value),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
        match old_token {
            Some(value) => std::env::set_var(auth::CLOUD_ACCESS_TOKEN_ENV, value),
            None => std::env::remove_var(auth::CLOUD_ACCESS_TOKEN_ENV),
        }
    }
}
