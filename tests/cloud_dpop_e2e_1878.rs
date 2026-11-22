#![cfg(feature = "supabase-cloud")]
#![allow(clippy::disallowed_types)]

use decapod::core::backend::{BackendRoute, StorageContext};
use decapod::core::cloud_client::{
    connect, connect_control, devices, record_rejected_credential, revoke, status_live,
};
use decapod::core::cloud_dpop::{DeviceKeyStore, verify_dpop_proof};
use decapod::core::cloud_session::{Phase, Session, SessionState, SessionStore};
use decapod::core::container_signer::{
    ContainerSignRequest, ContainerSignerPolicy, HostContainerSignerServer, SignerCapabilityKind,
    WorkspaceCredentialsConfig,
};
use decapod::core::dactyl::DactylBridge;
use decapod::core::dactyl_todo::DactylTodoStore;
use decapod::core::error::{CloudAuthStatus, DecapodError};
use decapod::core::repo_identity::resolve_repository_identity;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use tempfile::TempDir;

const PROJECT: &str = "DecapodLabs/dpop-proof-repo";
const HUMAN_JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.human_fixture_sub.sig";
const CHALLENGE_ID: &str = "11111111-1111-4111-8111-111111111111";
const DEVICE_KEY_ID: &str = "22222222-2222-4222-8222-222222222222";

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvRestoreGuard {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl EnvRestoreGuard {
    fn set(vars: &[(&'static str, Option<&str>)]) -> Self {
        let mut saved = Vec::with_capacity(vars.len());
        for (key, value) in vars {
            saved.push((*key, std::env::var_os(key)));
            unsafe {
                match value {
                    Some(v) => std::env::set_var(key, v),
                    None => std::env::remove_var(key),
                }
            }
        }
        Self { saved }
    }
}

impl Drop for EnvRestoreGuard {
    fn drop(&mut self) {
        for (key, value) in self.saved.drain(..).rev() {
            unsafe {
                match value {
                    Some(v) => std::env::set_var(key, v),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecordedCall {
    path: String,
    authorization: Option<String>,
    has_dpop: bool,
}

#[derive(Clone, Debug)]
struct GatewayFixtureState {
    origin: String,
    enrolled_jkt: Option<String>,
    active_access_token: Option<String>,
    active_refresh_token: Option<String>,
    initial_expires_in: i64,
    session_nonce_issued: bool,
    disable_device_endpoints: bool,
    fail_refresh_with_code: Option<&'static str>,
    fail_query_with_code: Option<&'static str>,
    calls: Vec<RecordedCall>,
    enroll_body: Option<Value>,
    refresh_body: Option<Value>,
    control_body: Option<Value>,
    revoke_body: Option<Value>,
}

impl GatewayFixtureState {
    fn new(origin: String, initial_expires_in: i64) -> Self {
        Self {
            origin,
            enrolled_jkt: None,
            active_access_token: None,
            active_refresh_token: None,
            initial_expires_in,
            session_nonce_issued: false,
            disable_device_endpoints: false,
            fail_refresh_with_code: None,
            fail_query_with_code: None,
            calls: Vec::new(),
            enroll_body: None,
            refresh_body: None,
            control_body: None,
            revoke_body: None,
        }
    }
}

struct PropodusGatewayServer {
    origin: String,
    state: Arc<Mutex<GatewayFixtureState>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl PropodusGatewayServer {
    fn start(initial_expires_in: i64) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener.set_nonblocking(true).expect("nonblocking");
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(GatewayFixtureState::new(
            origin.clone(),
            initial_expires_in,
        )));
        let stop = Arc::new(AtomicBool::new(false));
        let state_clone = Arc::clone(&state);
        let stop_clone = Arc::clone(&stop);

        let handle = std::thread::spawn(move || {
            while !stop_clone.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => Self::serve_connection(&mut stream, &state_clone),
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            origin,
            state,
            stop,
            handle: Some(handle),
        }
    }

    fn serve_connection(stream: &mut TcpStream, state: &Arc<Mutex<GatewayFixtureState>>) {
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut bytes = Vec::new();
        let body_start = loop {
            let mut buf = [0u8; 4096];
            let Ok(n) = stream.read(&mut buf) else {
                return;
            };
            if n == 0 {
                return;
            }
            bytes.extend_from_slice(&buf[..n]);
            if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
        };
        let header_raw = String::from_utf8_lossy(&bytes[..body_start]).to_string();
        let mut headers = BTreeMap::new();
        for line in header_raw.lines().skip(1) {
            if let Some((name, val)) = line.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), val.trim().to_string());
            }
        }
        let len: usize = headers
            .get("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        while bytes.len() < body_start + len {
            let mut buf = [0u8; 4096];
            let Ok(n) = stream.read(&mut buf) else {
                return;
            };
            if n == 0 {
                return;
            }
            bytes.extend_from_slice(&buf[..n]);
        }
        let path = header_raw
            .lines()
            .next()
            .unwrap_or("")
            .split_whitespace()
            .nth(1)
            .unwrap_or("")
            .to_string();
        let method = header_raw
            .lines()
            .next()
            .unwrap_or("")
            .split_whitespace()
            .next()
            .unwrap_or("POST")
            .to_string();
        let body: Value = if len == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[body_start..body_start + len]).unwrap_or(Value::Null)
        };

        let auth = headers.get("authorization").cloned();
        let dpop = headers.get("dpop").cloned();
        let now_secs = time::OffsetDateTime::now_utc().unix_timestamp();

        let mut st = state.lock().unwrap();
        let origin = st.origin.clone();
        st.calls.push(RecordedCall {
            path: path.clone(),
            authorization: auth.clone(),
            has_dpop: dpop.is_some(),
        });

        let mut extra_headers = String::new();
        let (status, resp): (u16, Value) = match path.as_str() {
            "/functions/v1/decapod-onboarding/config" => (
                200,
                json!({
                    "version": 1,
                    "project_origin": origin,
                    "publishable_key": "sb_publishable_fixture",
                    "schema": "dactyl_api",
                }),
            ),
            "/functions/v1/decapod-onboarding/account-status" => {
                // Mirror Propodus accounts.js: reject DPoP / device tokens and DPoP proofs.
                let is_device = auth
                    .as_deref()
                    .is_some_and(|a| a.starts_with("DPoP ") || a.contains("dpc_at_"));
                if is_device || dpop.is_some() {
                    (
                        403,
                        json!({"error": {"code": "mfa_step_up_required", "message": "human bearer only"}}),
                    )
                } else if auth.as_deref() != Some(&format!("Bearer {HUMAN_JWT}")) {
                    (401, json!({"error": {"code": "invalid_token"}}))
                } else {
                    (
                        200,
                        json!({
                            "version": 1,
                            "account_id": "00000000-0000-0000-0000-000000000404",
                            "project_id": PROJECT,
                            "status": "active",
                            "backend_authorized": true,
                            "resource_limits": {
                                "plan_tier": "pro",
                                "paid_overages": {"status": "healthy"}
                            }
                        }),
                    )
                }
            }
            "/functions/v1/decapod-onboarding/device-challenge" => {
                if st.disable_device_endpoints {
                    (404, json!({"error": {"code": "not_found"}}))
                } else if auth.as_deref() != Some(&format!("Bearer {HUMAN_JWT}")) || dpop.is_some()
                {
                    (401, json!({"error": {"code": "invalid_token"}}))
                } else {
                    (
                        200,
                        json!({
                            "version": 1,
                            "challenge_id": CHALLENGE_ID,
                            "nonce": "challenge-nonce-1234567890",
                            "audience": origin,
                        }),
                    )
                }
            }
            "/functions/v1/decapod-onboarding/device-enroll" => {
                if st.disable_device_endpoints {
                    (404, json!({"error": {"code": "not_found"}}))
                } else if auth.as_deref() != Some(&format!("Bearer {HUMAN_JWT}")) {
                    (401, json!({"error": {"code": "invalid_token"}}))
                } else {
                    assert!(body.get("public_key").is_none());
                    assert!(body.get("ssh_public_key").is_none());
                    assert!(body.get("gpg_key").is_none());
                    assert!(body.get("dpop_proof").is_none());
                    assert_eq!(body["challenge_id"], CHALLENGE_ID);
                    assert_eq!(body["nonce"], "challenge-nonce-1234567890");
                    let jwt = dpop.as_deref().expect("DPoP header on device-enroll");
                    let full_url = format!("{origin}{path}");
                    let verified = verify_dpop_proof(
                        jwt,
                        &method,
                        &full_url,
                        Some(HUMAN_JWT),
                        Some("challenge-nonce-1234567890"),
                        Some(&origin),
                        now_secs,
                    )
                    .expect("valid device-enroll DPoP proof");
                    st.enrolled_jkt = Some(verified.jkt.clone());
                    st.enroll_body = Some(body.clone());
                    (
                        200,
                        json!({
                            "version": 1,
                            "device_key_id": DEVICE_KEY_ID,
                            "jkt": verified.jkt,
                            "status": "enrolled",
                        }),
                    )
                }
            }
            "/functions/v1/decapod-onboarding/device-session" => {
                if st.disable_device_endpoints {
                    (404, json!({"error": {"code": "not_found"}}))
                } else if auth.as_deref() != Some(&format!("Bearer {HUMAN_JWT}")) {
                    (401, json!({"error": {"code": "invalid_token"}}))
                } else if !st.session_nonce_issued {
                    st.session_nonce_issued = true;
                    extra_headers.push_str("DPoP-Nonce: server-nonce-session-1\r\n");
                    (
                        401,
                        json!({"error": {"code": "use_dpop_nonce", "message": "DPoP nonce required"}}),
                    )
                } else {
                    let jwt = dpop.as_deref().expect("DPoP header on device-session");
                    let full_url = format!("{origin}{path}");
                    let verified = verify_dpop_proof(
                        jwt,
                        &method,
                        &full_url,
                        Some(HUMAN_JWT),
                        Some("server-nonce-session-1"),
                        None,
                        now_secs,
                    )
                    .expect("valid device-session DPoP proof");
                    assert_eq!(st.enrolled_jkt.as_deref(), Some(verified.jkt.as_str()));
                    let at = "dpc_at_initial_tok_1".to_string();
                    let rt = "dpc_rt_initial_tok_1".to_string();
                    st.active_access_token = Some(at.clone());
                    st.active_refresh_token = Some(rt.clone());
                    (
                        200,
                        json!({
                            "version": 1,
                            "token_type": "DPoP",
                            "access_token": at,
                            "refresh_token": rt,
                            "expires_in": st.initial_expires_in,
                            "device_key_id": DEVICE_KEY_ID,
                        }),
                    )
                }
            }
            "/functions/v1/decapod-onboarding/device-refresh" => {
                if let Some(code) = st.fail_refresh_with_code {
                    (401, json!({"error": {"code": code, "message": code}}))
                } else {
                    let jwt = dpop.as_deref().expect("DPoP header on device-refresh");
                    let full_url = format!("{origin}{path}");
                    let expected_at = st
                        .active_access_token
                        .as_deref()
                        .expect("active DPoP access token before refresh");
                    let verified = verify_dpop_proof(
                        jwt,
                        &method,
                        &full_url,
                        Some(expected_at),
                        None,
                        None,
                        now_secs,
                    )
                    .expect("valid device-refresh DPoP proof");
                    assert_eq!(auth.as_deref(), Some(&*format!("DPoP {expected_at}")));
                    assert!(
                        auth.as_deref().is_none_or(|a| !a.starts_with("Bearer ")),
                        "device-refresh must not send Bearer"
                    );
                    assert_eq!(
                        body["refresh_token"].as_str(),
                        st.active_refresh_token.as_deref()
                    );
                    assert_eq!(st.enrolled_jkt.as_deref(), Some(verified.jkt.as_str()));
                    st.refresh_body = Some(body.clone());
                    let next_at = "dpc_at_rotated_tok_2".to_string();
                    let next_rt = "dpc_rt_rotated_tok_2".to_string();
                    st.active_access_token = Some(next_at.clone());
                    st.active_refresh_token = Some(next_rt.clone());
                    (
                        200,
                        json!({
                            "version": 1,
                            "token_type": "DPoP",
                            "access_token": next_at,
                            "refresh_token": next_rt,
                            "expires_in": 900,
                        }),
                    )
                }
            }
            "/functions/v1/decapod-onboarding/cloud-query" => {
                if let Some(code) = st.fail_query_with_code {
                    (401, json!({"error": {"code": code, "message": code}}))
                } else {
                    let expected_at = st
                        .active_access_token
                        .as_deref()
                        .expect("issued DPoP access token");
                    assert_eq!(auth.as_deref(), Some(&*format!("DPoP {expected_at}")));
                    let jwt = dpop.as_deref().expect("DPoP header on cloud-query");
                    let full_url = format!("{origin}{path}");
                    let verified = verify_dpop_proof(
                        jwt,
                        &method,
                        &full_url,
                        Some(expected_at),
                        None,
                        None,
                        now_secs,
                    )
                    .expect("valid cloud-query DPoP proof");
                    assert_eq!(st.enrolled_jkt.as_deref(), Some(verified.jkt.as_str()));
                    (
                        200,
                        json!({
                            "columns": [
                                "repo_id", "id", "hash", "title", "description", "status",
                                "assignee", "scope", "dir_path", "priority", "category",
                                "tags", "created_at", "updated_at", "version"
                            ],
                            "rows": [],
                            "affected_rows": 0,
                            "generated_keys": [],
                        }),
                    )
                }
            }
            "/functions/v1/decapod-onboarding/cloud-batch" => {
                let expected_at = st
                    .active_access_token
                    .as_deref()
                    .expect("issued DPoP access token");
                assert_eq!(auth.as_deref(), Some(&*format!("DPoP {expected_at}")));
                let jwt = dpop.as_deref().expect("DPoP header on cloud-batch");
                let full_url = format!("{origin}{path}");
                let verified = verify_dpop_proof(
                    jwt,
                    &method,
                    &full_url,
                    Some(expected_at),
                    None,
                    None,
                    now_secs,
                )
                .expect("valid cloud-batch DPoP proof");
                assert_eq!(st.enrolled_jkt.as_deref(), Some(verified.jkt.as_str()));
                (
                    200,
                    json!({
                        "results": [{
                            "columns": [],
                            "rows": [],
                            "affected_rows": 1,
                            "generated_keys": [],
                        }]
                    }),
                )
            }
            "/functions/v1/decapod-onboarding/cloud-control" => {
                let expected_at = st
                    .active_access_token
                    .as_deref()
                    .expect("issued DPoP access token");
                assert_eq!(auth.as_deref(), Some(&*format!("DPoP {expected_at}")));
                let jwt = dpop.as_deref().expect("DPoP header on cloud-control");
                let full_url = format!("{origin}{path}");
                let verified = verify_dpop_proof(
                    jwt,
                    &method,
                    &full_url,
                    Some(expected_at),
                    None,
                    None,
                    now_secs,
                )
                .expect("valid cloud-control DPoP proof");
                assert_eq!(st.enrolled_jkt.as_deref(), Some(verified.jkt.as_str()));
                st.control_body = Some(body.clone());
                (
                    200,
                    json!({"version": 1, "decision": "allow", "allowed": true}),
                )
            }
            "/functions/v1/decapod-onboarding/device-revoke" => {
                st.revoke_body = Some(body.clone());
                if body["action"] == "logout" {
                    let expected_at = st
                        .active_access_token
                        .as_deref()
                        .expect("issued DPoP access token");
                    assert_eq!(auth.as_deref(), Some(&*format!("DPoP {expected_at}")));
                    let jwt = dpop
                        .as_deref()
                        .expect("DPoP header on device-revoke logout");
                    let full_url = format!("{origin}{path}");
                    verify_dpop_proof(
                        jwt,
                        &method,
                        &full_url,
                        Some(expected_at),
                        None,
                        None,
                        now_secs,
                    )
                    .expect("valid device-revoke DPoP proof");
                } else {
                    assert_eq!(auth.as_deref(), Some(&*format!("Bearer {HUMAN_JWT}")));
                    assert!(dpop.is_none());
                }
                (200, json!({"version": 1, "status": "revoked"}))
            }
            _ => (404, json!({"error": {"code": "not_found", "path": path}})),
        };
        drop(st);

        let encoded = resp.to_string();
        let status_text = match status {
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            _ => "OK",
        };
        let reply = format!(
            "HTTP/1.1 {status} {status_text}\r\n{extra_headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{encoded}",
            encoded.len()
        );
        let _ = stream.write_all(reply.as_bytes());
    }
}

impl Drop for PropodusGatewayServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn init_cloud_repo(root: &Path) {
    let repo = root.join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init"]);
    git(&repo, &["config", "user.name", "Decapod DPoP Proof"]);
    git(&repo, &["config", "user.email", "dpop@example.invalid"]);
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            &format!("https://github.com/{PROJECT}.git"),
        ],
    );
    fs::create_dir_all(repo.join(".decapod/data")).unwrap();
    fs::write(
        repo.join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n[workspace.credentials]\npropodus_cloud = \"signer-broker\"\n",
    )
    .unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "init"]);
}

fn seed_human_session(origin: &str, repo: &Path) {
    let store = SessionStore::for_project(origin, PROJECT, repo).expect("session store");
    let mut guard = store.lock(Duration::from_secs(3)).expect("lock");
    guard
        .replace(SessionState {
            phase: Phase::Ready,
            session: Some(Session {
                access_token: HUMAN_JWT.to_string(),
                refresh_token: Some("human_rt_fixture".to_string()),
                expires_at: time::OffsetDateTime::now_utc().unix_timestamp() + 3600,
            }),
            bootstrap: None,
        })
        .expect("seed human session");
}

#[test]
fn dpop_device_lifecycle_routes_query_batch_control_container_refresh_and_logout() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = TempDir::new().expect("tempdir");
    init_cloud_repo(tmp.path());
    let repo = tmp.path().join("repo");
    let xdg = tmp.path().join("xdg");
    fs::create_dir_all(&xdg).unwrap();

    let server = PropodusGatewayServer::start(30);
    let _env = EnvRestoreGuard::set(&[
        ("DECAPOD_TEST_MODE", Some("1")),
        ("DECAPOD_CLOUD_TEST_ORIGIN", Some(&server.origin)),
        ("HOME", Some(xdg.to_str().unwrap())),
        ("XDG_DATA_HOME", Some(xdg.to_str().unwrap())),
        ("DECAPOD_CLOUD_AUTH_SCHEME", None),
        ("DECAPOD_CONTAINER_SIGNER_SOCKET", None),
        ("DECAPOD_AGENT_ID", Some("e2e-agent")),
    ]);

    let identity = resolve_repository_identity(&repo).expect("identity");
    seed_human_session(&server.origin, &repo);

    // 1. connect() auto-enrolls Ed25519 device key (with aud=origin + challenge_id),
    // retries 401 use_dpop_nonce on /device-session, caches dpc_at_/dpc_rt_ sidecar,
    // and checks readiness via /cloud-query with Authorization: DPoP + DPoP header.
    let ctx = connect(&repo, &identity, false, true).expect("connect upgrades to DPoP");
    assert_eq!(ctx.bearer(), Some("dpc_at_initial_tok_1"));

    // 2. Write batch goes through /cloud-batch with DPoP proof.
    let bridge =
        DactylBridge::from_storage_context(&ctx, dactyl_db::AccessMode::ReadWrite).expect("bridge");
    let batch = bridge
        .atomic(&[dactyl_db::Operation::write(
            "UPDATE tasks SET status = $1 WHERE id = $2",
            vec!["done".into(), "todo_1".into()],
        )])
        .expect("atomic cloud-batch succeeds");
    assert_eq!(batch.results.len(), 1);

    // 3. status_live and devices --live continue using human Bearer JWT on /account-status
    // without triggering Propodus mfa_step_up_required.
    let live = status_live(&repo, &identity).expect("status_live succeeds");
    assert_eq!(live["spend_cap_status"], "healthy");
    let devs = devices(&repo, &identity, true).expect("devices --live succeeds");
    assert_eq!(devs["server_readiness"], "checked");

    // 4. Next connect_control() observes expires_in=30 inside REFRESH_LEAD_SECONDS=60,
    // rotates dpc_at_/dpc_rt_ via /device-refresh, and routes /cloud-control with DPoP.
    let (ctrl_connector, routed, ctrl_bearer) =
        connect_control(&repo, &identity).expect("connect_control refreshes DPoP session");
    assert_eq!(routed.canonical_name, PROJECT);
    assert_eq!(ctrl_bearer, "dpc_at_rotated_tok_2");
    assert!(ctrl_connector.uses_device_gateway());
    let ctrl_resp = ctrl_connector
        .control_request(
            "propodus_repository_access_v1",
            &json!({"version": 1, "project_id": PROJECT}),
            Some(&ctrl_bearer),
            dactyl_db::OperationKind::Read,
            dactyl_db::ResourceBudget::default(),
        )
        .expect("cloud-control request succeeds");
    assert_eq!(ctrl_resp["allowed"], true);

    // 5. Container signer broker: signs /cloud-query and /cloud-control via Unix socket,
    // denies /device-enroll, and never copies private keys into container env/workdir.
    let key_store =
        DeviceKeyStore::for_project(&server.origin, &identity.canonical_name, &repo).unwrap();
    let sock_path = tmp.path().join("container-signer.sock");
    let now_secs = time::OffsetDateTime::now_utc().unix_timestamp();
    let policy = ContainerSignerPolicy {
        allowed_repository: identity.canonical_name.clone(),
        allowed_origin: server.origin.clone(),
        allowed_agent_id: "container-agent".to_string(),
        credentials: WorkspaceCredentialsConfig {
            propodus_cloud: Some("signer-broker".to_string()),
            ..Default::default()
        },
        session_expires_at: now_secs + 600,
        revoked: Arc::new(AtomicBool::new(false)),
        device_key_store: Some(key_store),
        access_token_digest: None,
    };
    let denied_enroll = policy.sign_request(
        &ContainerSignRequest {
            capability: SignerCapabilityKind::PropodusCloud,
            repository: identity.canonical_name.clone(),
            agent_id: "container-agent".to_string(),
            method: "POST".to_string(),
            url: format!(
                "{}/functions/v1/decapod-onboarding/device-enroll",
                server.origin
            ),
            nonce: None,
            aud: None,
            access_token_sha256: None,
        },
        now_secs,
        None,
    );
    assert!(!denied_enroll.ok);
    let broker_server =
        HostContainerSignerServer::bind(&sock_path, policy).expect("bind host signer socket");
    {
        let _sock_env = EnvRestoreGuard::set(&[
            (
                "DECAPOD_CONTAINER_SIGNER_SOCKET",
                Some(broker_server.socket_path().to_str().unwrap()),
            ),
            ("DECAPOD_AGENT_ID", Some("container-agent")),
        ]);
        let container_ctx =
            connect(&repo, &identity, false, true).expect("container connect uses signer socket");
        assert_eq!(container_ctx.bearer(), Some("dpc_at_rotated_tok_2"));
        DactylTodoStore::new(container_ctx, PROJECT)
            .check_read_access()
            .expect("container cloud-query via socket signer");
        let (sock_ctrl, _, sock_bearer) =
            connect_control(&repo, &identity).expect("container connect_control");
        let sock_ctrl_resp = sock_ctrl
            .control_request(
                "propodus_repository_access_v1",
                &json!({"version": 1, "project_id": PROJECT}),
                Some(&sock_bearer),
                dactyl_db::OperationKind::Read,
                dactyl_db::ResourceBudget::default(),
            )
            .expect("container cloud-control via socket signer");
        assert_eq!(sock_ctrl_resp["decision"], "allow");
    }
    drop(broker_server);

    // 6. Session logout revokes the active DPoP token over /device-revoke using DPoP header.
    let logout_report = revoke(&repo, &identity, false, None, true, false).expect("session logout");
    assert_eq!(logout_report["operation"], "logout");
    assert_eq!(logout_report["remote_revoked"], true);

    // Verify observed gateway paths and zero local SQLite files under .decapod/data.
    let st = server.state.lock().unwrap();
    let paths: Vec<&str> = st.calls.iter().map(|c| c.path.as_str()).collect();
    assert!(paths.contains(&"/functions/v1/decapod-onboarding/device-challenge"));
    assert!(paths.contains(&"/functions/v1/decapod-onboarding/device-enroll"));
    assert!(
        paths
            .iter()
            .filter(|p| **p == "/functions/v1/decapod-onboarding/device-session")
            .count()
            >= 2
    );
    assert!(paths.contains(&"/functions/v1/decapod-onboarding/device-refresh"));
    assert!(paths.contains(&"/functions/v1/decapod-onboarding/cloud-query"));
    assert!(paths.contains(&"/functions/v1/decapod-onboarding/cloud-batch"));
    assert!(paths.contains(&"/functions/v1/decapod-onboarding/cloud-control"));
    assert!(paths.contains(&"/functions/v1/decapod-onboarding/device-revoke"));
    assert!(
        st.revoke_body
            .as_ref()
            .is_some_and(|b| b["action"] == "logout")
    );
    assert!(!repo.join(".decapod/data/decapod.db").exists());
    assert!(!repo.join(".decapod/data/todo.db").exists());
}

#[test]
fn dpop_failures_fail_closed_and_strict_mode_rejects_missing_device_endpoints() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = TempDir::new().expect("tempdir");
    init_cloud_repo(tmp.path());
    let repo = tmp.path().join("repo");
    let xdg = tmp.path().join("xdg");
    fs::create_dir_all(&xdg).unwrap();

    let server = PropodusGatewayServer::start(15);
    let _env = EnvRestoreGuard::set(&[
        ("DECAPOD_TEST_MODE", Some("1")),
        ("DECAPOD_CLOUD_TEST_ORIGIN", Some(&server.origin)),
        ("HOME", Some(xdg.to_str().unwrap())),
        ("XDG_DATA_HOME", Some(xdg.to_str().unwrap())),
        ("DECAPOD_CLOUD_AUTH_SCHEME", Some("dpop")),
        ("DECAPOD_CONTAINER_SIGNER_SOCKET", None),
        ("DECAPOD_AGENT_ID", Some("e2e-fail-closed")),
    ]);

    let identity = resolve_repository_identity(&repo).expect("identity");
    seed_human_session(&server.origin, &repo);

    // Enroll and get initial short-lived (15s) device session.
    let ctx = connect(&repo, &identity, false, true).expect("initial DPoP connect");
    assert_eq!(ctx.bearer(), Some("dpc_at_initial_tok_1"));

    // 1. Reused refresh token returns Revoked and clears device session sidecar without local fallback.
    server.state.lock().unwrap().fail_refresh_with_code = Some("refresh_reuse_detected");
    let reuse_err = connect(&repo, &identity, false, false).expect_err("refresh reuse fails");
    match reuse_err {
        DecapodError::CloudAuth(diag) => assert_eq!(diag.status, CloudAuthStatus::Revoked),
        other => panic!("expected CloudAuth Revoked, got {other:?}"),
    }

    // 2. When /cloud-query reports device_revoked, diagnostic maps to DeviceRevoked.
    {
        let mut st = server.state.lock().unwrap();
        st.fail_refresh_with_code = None;
        st.fail_query_with_code = Some("device_revoked");
        st.session_nonce_issued = false;
    }
    let revoked_err = connect(&repo, &identity, false, true).expect_err("device_revoked fails");
    match revoked_err {
        DecapodError::CloudAuth(diag) => assert_eq!(diag.status, CloudAuthStatus::DeviceRevoked),
        other => panic!("expected CloudAuth DeviceRevoked, got {other:?}"),
    }

    // 3. Explicit record_rejected_credential clears a matching DPoP token and strict mode refuses 404 downgrade.
    {
        let mut st = server.state.lock().unwrap();
        st.fail_query_with_code = None;
        st.session_nonce_issued = false;
    }
    let ctx2 = connect(&repo, &identity, false, false).expect("fresh device session exchange");
    assert_eq!(ctx2.bearer(), Some("dpc_at_initial_tok_1"));
    record_rejected_credential(&repo, &ctx2).expect("clear rejected sidecar");

    // Verify unused device key + missing endpoints under DECAPOD_CLOUD_AUTH_SCHEME=dpop fails closed.
    revoke(&repo, &identity, false, None, false, true).expect("local revoke");
    seed_human_session(&server.origin, &repo);
    {
        let mut st = server.state.lock().unwrap();
        st.disable_device_endpoints = true;
    }
    let err = connect(&repo, &identity, false, false)
        .expect_err("strict dpop mode must not fall back on 404 device endpoints");
    assert!(matches!(err, DecapodError::CloudAuth(_)));
    let _ = BackendRoute::cloud(identity, &server.origin);
    let _ = StorageContext::from_route;
}
