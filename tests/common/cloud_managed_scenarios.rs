//! Offline, actual-CLI proofs for the managed connector. These fixtures prove
//! composition, custody and retry behavior; live authorization is server-owned.
#![cfg(feature = "supabase-cloud")]
#![allow(clippy::disallowed_types)]

use super::managed;

use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
use decapod::core::cloud_session::Session;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use tempfile::TempDir;

const CONFIG: &str = "/functions/v1/propodus-onboarding/config";
const START: &str = "/functions/v1/propodus-onboarding/start";
const STATUS: &str = "/functions/v1/propodus-onboarding/status";
const EXCHANGE: &str = "/functions/v1/propodus-onboarding/exchange";
const COMPLETE: &str = "/functions/v1/propodus-onboarding/complete";
const TOKEN: &str = "/auth/v1/token?grant_type=pkce";
const REFRESH: &str = "/auth/v1/token?grant_type=refresh_token";
const QUERY: &str = "/rest/v1/rpc/dactyl_query_v1";
const FLOW: &str = "c09528a3-2921-4a90-ae55-398a215974a8";
const CODE: &str = "synthetic-one-time-auth-code-secret";
const ROTATED_ACCESS: &str = "synthetic-rotated-access-secret";
const ROTATED_REFRESH: &str = "synthetic-rotated-refresh-secret";

#[derive(Clone, Debug)]
struct Request {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Value,
}

fn read_request(stream: &mut TcpStream) -> Request {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let start = loop {
        let mut buffer = [0; 4096];
        let n = stream.read(&mut buffer).unwrap();
        assert!(n > 0, "request ended before headers");
        bytes.extend_from_slice(&buffer[..n]);
        assert!(bytes.len() <= 64 * 1024, "oversized fixture headers");
        if let Some(offset) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            break offset + 4;
        }
    };
    let header = String::from_utf8(bytes[..start].to_vec()).unwrap();
    let mut lines = header.lines();
    let mut first = lines.next().unwrap().split_whitespace();
    let method = first.next().unwrap().to_owned();
    let path = first.next().unwrap().to_owned();
    let headers: BTreeMap<_, _> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    let len: usize = headers
        .get("content-length")
        .map_or(0, |s| s.parse().unwrap());
    assert!(len <= 4 * 1024 * 1024);
    while bytes.len() < start + len {
        let mut buffer = [0; 4096];
        let n = stream.read(&mut buffer).unwrap();
        assert!(n > 0, "request ended before body");
        bytes.extend_from_slice(&buffer[..n]);
    }
    let body = if len == 0 {
        Value::Null
    } else {
        serde_json::from_slice(&bytes[start..start + len]).unwrap()
    };
    Request {
        method,
        path,
        headers,
        body,
    }
}

struct Service {
    repository: String,
    expiry: String,
    authentication_status: &'static str,
    lose_response: Option<&'static str>,
    complete_error: Option<&'static str>,
    read_error: Option<&'static str>,
    slow_refresh: bool,
}

impl Service {
    fn respond(&mut self, origin: &str, request: &Request) -> Option<(u16, Value)> {
        if request.path == CONFIG {
            assert_eq!(request.method, "GET");
            assert!(!request.headers.contains_key("authorization"));
            assert!(!request.headers.contains_key("apikey"));
            return Some((
                200,
                json!({"version":1,"project_origin":origin,
                "publishable_key":"sb_publishable_fixture","schema":"dactyl_api"}),
            ));
        }
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.headers.get("apikey").map(String::as_str),
            Some("sb_publishable_fixture")
        );
        if !matches!(request.path.as_str(), COMPLETE | QUERY) {
            assert!(!request.headers.contains_key("authorization"));
        }
        if self.lose_response == Some(request.path.as_str()) {
            return None;
        }
        let body = match request.path.as_str() {
            START => {
                assert_eq!(request.body["version"], 1);
                self.repository = request.body["repository"].as_str().unwrap().to_owned();
                assert_eq!(request.body["pkce_challenge"].as_str().unwrap().len(), 43);
                assert_eq!(request.body["poll_challenge"].as_str().unwrap().len(), 43);
                json!({"flow_id":FLOW,"browser_url":format!("{origin}/functions/v1/propodus-onboarding/begin?flow_id={FLOW}&ticket={}", "A".repeat(43)),"expires_at":self.expiry})
            }
            STATUS => {
                json!({"status":self.authentication_status,"repository":self.repository,"expires_at":self.expiry})
            }
            EXCHANGE => json!({"auth_code":CODE,"repository":self.repository}),
            TOKEN => {
                assert_eq!(request.body["auth_code"], CODE);
                assert_eq!(request.body["code_verifier"].as_str().unwrap().len(), 43);
                json!({"access_token":managed::ACCESS,"refresh_token":managed::REFRESH,"token_type":"bearer","expires_in":900})
            }
            REFRESH => {
                assert_eq!(request.body["refresh_token"], managed::REFRESH);
                if self.slow_refresh {
                    std::thread::sleep(Duration::from_millis(150));
                }
                json!({"access_token":ROTATED_ACCESS,"refresh_token":ROTATED_REFRESH,"token_type":"bearer","expires_in":900})
            }
            COMPLETE => {
                assert_eq!(request.body["flow_id"], FLOW);
                assert_eq!(request.body["poll_secret"].as_str().unwrap().len(), 43);
                if let Some(code) = self.complete_error {
                    return Some((
                        403,
                        json!({"code":code,"message":format!("private service detail {CODE} {}",managed::ACCESS)}),
                    ));
                }
                json!({"status":"complete","repository":self.repository})
            }
            QUERY => {
                assert_eq!(
                    request.headers.get("content-profile").map(String::as_str),
                    Some("dactyl_api")
                );
                assert_eq!(request.body["request"]["version"], 1);
                if let Some(code) = self.read_error {
                    let status = if code == "PGRST301" { 401 } else { 403 };
                    return Some((
                        status,
                        json!({"code":code,"message":format!("private service detail {CODE} {}",managed::ACCESS)}),
                    ));
                }
                json!({"columns":[],"rows":[],"affected_rows":0,"generated_keys":[]})
            }
            other => panic!("unexpected managed fixture route: {other}"),
        };
        Some((200, body))
    }
}

struct Fixture {
    project: TempDir,
    machine: TempDir,
    origin: String,
    requests: Arc<Mutex<Vec<Request>>>,
    service: Arc<Mutex<Service>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Fixture {
    fn new(repository: &str) -> Self {
        let project = tempfile::tempdir().unwrap();
        let machine = tempfile::tempdir().unwrap();
        git(project.path(), &["init", "--quiet"]);
        git(
            project.path(),
            &[
                "remote",
                "add",
                "origin",
                &format!("https://github.com/{repository}.git"),
            ],
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let service = Arc::new(Mutex::new(Service {
            repository: repository.to_owned(),
            expiry: (Utc::now() + ChronoDuration::minutes(10))
                .to_rfc3339_opts(SecondsFormat::Secs, true),
            authentication_status: "ready",
            lose_response: None,
            complete_error: None,
            read_error: None,
            slow_refresh: false,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let request_log = requests.clone();
        let worker_service = service.clone();
        let worker_stop = stop.clone();
        let worker_origin = origin.clone();
        let worker = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("fixture accept failed: {error}"),
                };
                let request = read_request(&mut stream);
                request_log.lock().unwrap().push(request.clone());
                let reply = worker_service
                    .lock()
                    .unwrap()
                    .respond(&worker_origin, &request);
                if let Some((status, body)) = reply {
                    let body = body.to_string();
                    write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                }
            }
        });
        Self {
            project,
            machine,
            origin,
            requests,
            service,
            stop,
            worker: Some(worker),
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command =
            Command::new(std::fs::canonicalize(env!("CARGO_BIN_EXE_decapod")).unwrap());
        command
            .args(args)
            .current_dir(self.project.path())
            .env("HOME", self.machine.path())
            .env("XDG_CONFIG_HOME", self.machine.path().join("config"))
            .env("XDG_DATA_HOME", self.machine.path().join("data"))
            .env("DECAPOD_TEST_MODE", "1")
            .env("DECAPOD_CLOUD_TEST_ORIGIN", &self.origin)
            .env("DECAPOD_HEADLESS", "1")
            .env("DECAPOD_AGENT_ID", "managed-fixture")
            .env_remove("DECAPOD_CLOUD_AUTH_MODE")
            .env_remove("DECAPOD_ACCESS_TOKEN")
            .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
            .env_remove("DECAPOD_MANAGED_TEST_SEED")
            .stdin(Stdio::null());
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    fn init(&self) -> Output {
        self.run(&["init", "--backend", "cloud", "--force"])
    }
    fn proof(&self) {
        let result = self.run(&["init", "--backend", "cloud", "--force", "--proof"]);
        assert!(result.status.success(), "{}", output(&result));
    }
    fn status(&self) -> Value {
        let result = self.run(&["cloud", "status"]);
        assert!(result.status.success(), "{}", output(&result));
        serde_json::from_slice(&result.stdout).unwrap()
    }
    fn count(&self, route: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.path == route)
            .count()
    }
    fn seed(&self, expired: bool) {
        managed::seed_ready(
            self.project.path(),
            self.machine.path(),
            &self.origin,
            &self.service.lock().unwrap().repository,
            Session {
                access_token: managed::ACCESS.into(),
                refresh_token: Some(managed::REFRESH.into()),
                expires_at: Utc::now().timestamp() + if expired { -10 } else { 900 },
            },
        );
    }
    fn assert_no_fallback(&self) {
        for path in [".decapod/data/decapod.db", ".decapod/data/todo.db"] {
            assert!(
                !self.project.path().join(path).exists(),
                "unexpected local fallback: {path}"
            );
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let result = worker.join();
            if !std::thread::panicking() {
                result.unwrap();
            }
        }
    }
}

fn git(root: &Path, args: &[&str]) {
    let result = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
fn output(result: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    )
}
fn failed(result: &Output, status: &str) {
    assert!(
        !result.status.success(),
        "unexpected success: {}",
        output(result)
    );
    assert!(
        output(result).contains(status),
        "expected {status}: {}",
        output(result)
    );
    assert!(!output(result).contains("Cloud setup ready"));
    for secret in [
        managed::ACCESS,
        managed::REFRESH,
        ROTATED_ACCESS,
        ROTATED_REFRESH,
        CODE,
    ] {
        assert!(
            !output(result).contains(secret),
            "credential escaped output"
        );
    }
}
fn challenge(secret: &str) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let digest = Sha256::digest(secret.as_bytes());
    let mut value = String::new();
    for chunk in digest.chunks(3) {
        let bits = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        value.push(ALPHABET[((bits >> 18) & 63) as usize] as char);
        value.push(ALPHABET[((bits >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            value.push(ALPHABET[((bits >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            value.push(ALPHABET[(bits & 63) as usize] as char);
        }
    }
    value
}

#[test]
fn init_once_then_agent_login_resumes_the_private_flow_and_checks_repository_read_access() {
    let fixture = Fixture::new("DecapodLabs/ManagedFixture");
    let first = fixture.init();
    failed(&first, "onboarding_pending");
    assert!(output(&first).contains("/begin?flow_id="));
    assert!(output(&first).contains("\"authorization_url\""));
    assert!(output(&first).contains("without waiting for another user message"));
    assert!(output(&first).contains("your agent handles Decapod from here"));
    let config: toml::Value = toml::from_str(
        &std::fs::read_to_string(fixture.project.path().join(".decapod/config.toml")).unwrap(),
    )
    .unwrap();
    assert_eq!(config["repo"]["backend"].as_str(), Some("cloud"));
    for field in [
        "cloud_repository",
        "cloud_github_users",
        "api_url",
        "provider",
        "driver",
    ] {
        assert!(
            config["repo"].get(field).is_none(),
            "unexpected project field {field}"
        );
    }
    assert!(!output(&first).contains("USD15"));
    assert_eq!(fixture.count(START), 1);
    assert_eq!(fixture.count(STATUS), 0);
    assert_eq!(fixture.status()["phase"], "waiting");

    let second = fixture.run(&["cloud", "login"]);
    assert!(second.status.success(), "{}", output(&second));
    assert!(output(&second).contains("Cloud setup ready for DecapodLabs/ManagedFixture"));
    assert_eq!(fixture.status()["phase"], "ready");
    for route in [START, STATUS, EXCHANGE, TOKEN, COMPLETE, QUERY] {
        assert_eq!(
            fixture.count(route),
            1,
            "unexpected request count for {route}"
        );
    }
    let requests = fixture.requests.lock().unwrap();
    let start = requests.iter().find(|r| r.path == START).unwrap();
    let status = requests.iter().find(|r| r.path == STATUS).unwrap();
    let token = requests.iter().find(|r| r.path == TOKEN).unwrap();
    assert_eq!(
        start.body["pkce_challenge"],
        challenge(token.body["code_verifier"].as_str().unwrap())
    );
    assert_eq!(
        start.body["poll_challenge"],
        challenge(status.body["poll_secret"].as_str().unwrap())
    );
    let query = requests.iter().find(|r| r.path == QUERY).unwrap();
    assert_eq!(query.body["request"]["access_mode"], "read_only");
    assert_eq!(
        query.body["request"]["params"],
        json!(["_decapod_readiness_probe_"])
    );
    assert!(
        query.body["request"]["sql"]
            .as_str()
            .unwrap()
            .contains("FROM tasks WHERE id = $1")
    );
    assert_eq!(
        query.headers["authorization"],
        format!("Bearer {}", managed::ACCESS)
    );
    for secret in [
        token.body["code_verifier"].as_str().unwrap(),
        status.body["poll_secret"].as_str().unwrap(),
        managed::ACCESS,
        managed::REFRESH,
        CODE,
    ] {
        assert!(!output(&first).contains(secret));
        assert!(!output(&second).contains(secret));
        assert_repository_excludes(fixture.project.path(), secret);
    }
    drop(requests);
    let listed = fixture.run(&["todo", "list", "--format", "json"]);
    assert!(listed.status.success(), "{}", output(&listed));
    assert_eq!(
        fixture.count(START),
        1,
        "normal agent use must reuse sign-in"
    );
    assert_eq!(fixture.count(QUERY), 2);
    fixture.assert_no_fallback();
}

#[test]
fn pending_task_creation_keeps_the_browser_link_and_original_operation_id() {
    let fixture = Fixture::new("example/managed-proof");
    fixture.proof();
    fixture.service.lock().unwrap().authentication_status = "pending";
    let first = fixture.run(&["todo", "add", "Preserve this task", "--format", "json"]);
    failed(&first, "onboarding_pending");
    let first: Value = serde_json::from_slice(&first.stdout).unwrap();
    let url = first["authorization_url"].as_str().unwrap();
    assert!(url.starts_with(&fixture.origin));
    assert!(url.contains("/begin?flow_id="));
    let operation = first["operation_id"].as_str().unwrap();
    let action = first["next_action"].as_str().unwrap();
    assert!(action.contains("decapod cloud login"));
    assert!(action.contains(&format!("--operation-id {operation}")));
    let pending_login = fixture.run(&["cloud", "login", "--format", "json"]);
    failed(&pending_login, "onboarding_pending");
    let login: Value = serde_json::from_slice(&pending_login.stdout).unwrap();
    assert_eq!(login["authorization_url"], first["authorization_url"]);
    assert!(String::from_utf8_lossy(&pending_login.stderr).contains(url));
    let retry = fixture.run(&[
        "todo",
        "add",
        "Preserve this task",
        "--format",
        "json",
        "--operation-id",
        operation,
    ]);
    failed(&retry, "onboarding_pending");
    let retry: Value = serde_json::from_slice(&retry.stdout).unwrap();
    assert_eq!(retry["operation_id"], first["operation_id"]);
    assert_eq!(retry["authorization_url"], first["authorization_url"]);
    assert_eq!(fixture.count(START), 1);
    assert_eq!(
        fixture.count(QUERY),
        0,
        "pending login must not dispatch task data"
    );
    fixture.assert_no_fallback();
}

fn assert_repository_excludes(root: &Path, secret: &str) {
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == ".git" {
            continue;
        }
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            assert_repository_excludes(&entry.path(), secret);
        } else if kind.is_file() {
            let bytes = std::fs::read(entry.path()).unwrap();
            assert!(
                !bytes
                    .windows(secret.len())
                    .any(|value| value == secret.as_bytes()),
                "credential entered repository: {}",
                entry.path().display()
            );
        }
    }
}

#[test]
fn cached_session_reuse_rechecks_access_without_repeating_signin() {
    let fixture = Fixture::new("example/managed-proof");
    fixture.proof();
    fixture.seed(false);
    for _ in 0..2 {
        let result = fixture.init();
        assert!(result.status.success(), "{}", output(&result));
    }
    for route in [START, STATUS, EXCHANGE, TOKEN, REFRESH, COMPLETE] {
        assert_eq!(fixture.count(route), 0);
    }
    assert_eq!(fixture.count(QUERY), 2);
    fixture.assert_no_fallback();
}

#[test]
fn concurrent_expired_session_commands_rotate_the_refresh_token_once() {
    let fixture = Fixture::new("example/managed-proof");
    fixture.proof();
    fixture.seed(true);
    fixture.service.lock().unwrap().slow_refresh = true;
    let args = ["todo", "list", "--format", "json"];
    let first = fixture
        .command(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let second = fixture
        .command(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    for result in [
        first.wait_with_output().unwrap(),
        second.wait_with_output().unwrap(),
    ] {
        assert!(result.status.success(), "{}", output(&result));
    }
    assert_eq!(fixture.count(REFRESH), 1);
    assert_eq!(fixture.count(START), 0);
    let requests = fixture.requests.lock().unwrap();
    let queries: Vec<_> = requests.iter().filter(|r| r.path == QUERY).collect();
    assert_eq!(queries.len(), 2);
    for query in queries {
        assert_eq!(
            query.headers["authorization"],
            format!("Bearer {ROTATED_ACCESS}")
        );
    }
    drop(requests);
    assert_eq!(fixture.status()["phase"], "ready");
    fixture.assert_no_fallback();
}

#[test]
fn lost_one_time_responses_require_explicit_restart_and_are_never_replayed() {
    for route in [START, EXCHANGE, TOKEN, REFRESH] {
        let fixture = Fixture::new("example/managed-proof");
        if route == REFRESH {
            fixture.proof();
            fixture.seed(true);
        } else if route != START {
            failed(&fixture.init(), "onboarding_pending");
        }
        fixture.service.lock().unwrap().lose_response = Some(route);
        failed(&fixture.init(), "restart_required");
        assert_eq!(fixture.count(route), 1);
        assert_eq!(fixture.status()["phase"], "restart_required");
        fixture.service.lock().unwrap().lose_response = None;
        failed(&fixture.init(), "restart_required");
        assert_eq!(
            fixture.count(route),
            1,
            "one-time request was replayed after loss: {route}"
        );
        let restart = fixture.run(&["cloud", "login", "--restart"]);
        failed(&restart, "onboarding_pending");
        assert_eq!(fixture.status()["phase"], "waiting");
        fixture.assert_no_fallback();
    }
}

#[test]
fn lost_account_completion_response_rechecks_the_same_saved_session_without_replaying_exchanges() {
    let fixture = Fixture::new("example/managed-proof");
    failed(&fixture.init(), "onboarding_pending");
    fixture.service.lock().unwrap().lose_response = Some(COMPLETE);

    let uncertain = fixture.init();
    failed(&uncertain, "onboarding_pending");
    assert!(!output(&uncertain).contains("--restart-cloud-signin"));
    let status = fixture.status();
    assert_eq!(status["phase"], "checking_readiness");
    assert_eq!(status["credential_present"], true);
    for route in [START, STATUS, EXCHANGE, TOKEN, COMPLETE] {
        assert_eq!(
            fixture.count(route),
            1,
            "unexpected request count after completion loss: {route}"
        );
    }
    assert_eq!(fixture.count(QUERY), 0);
    assert_eq!(fixture.count(REFRESH), 0);

    fixture.service.lock().unwrap().lose_response = None;
    let ready = fixture.init();
    assert!(ready.status.success(), "{}", output(&ready));
    assert!(output(&ready).contains("Cloud setup ready"));
    assert_eq!(fixture.status()["phase"], "ready");
    for route in [START, STATUS, EXCHANGE, TOKEN] {
        assert_eq!(
            fixture.count(route),
            1,
            "one-time sign-in or polling was repeated: {route}"
        );
    }
    assert_eq!(fixture.count(COMPLETE), 2);
    assert_eq!(fixture.count(QUERY), 1);
    assert_eq!(fixture.count(REFRESH), 0);
    let requests = fixture.requests.lock().unwrap();
    let completions: Vec<_> = requests
        .iter()
        .filter(|request| request.path == COMPLETE)
        .collect();
    assert_eq!(completions[0].body, completions[1].body);
    assert_eq!(
        completions[0].headers["authorization"],
        format!("Bearer {}", managed::ACCESS)
    );
    assert_eq!(
        completions[0].headers["authorization"],
        completions[1].headers["authorization"]
    );
    drop(requests);
    fixture.assert_no_fallback();
}

#[test]
fn browser_completion_and_pending_entitlement_never_claim_ready() {
    let fixture = Fixture::new("example/managed-proof");
    failed(&fixture.init(), "onboarding_pending");
    fixture.service.lock().unwrap().authentication_status = "pending";
    failed(&fixture.init(), "onboarding_pending");
    assert_eq!(fixture.count(EXCHANGE), 0);
    fixture.service.lock().unwrap().authentication_status = "ready";
    // Payment/seat/quote requirements are server-owned. Each supported reason
    // keeps the issued credential pending; no browser result implies access.
    for reason in [
        "entitlement_required",
        "seat_unassigned",
        "quote_confirmation_required",
    ] {
        fixture.service.lock().unwrap().complete_error = Some(reason);
        failed(&fixture.init(), "action_required");
        assert_eq!(fixture.status()["phase"], "checking_readiness");
        assert_eq!(fixture.count(QUERY), 0);
        assert_eq!(fixture.count(EXCHANGE), 1);
        assert_eq!(fixture.count(TOKEN), 1);
    }
    fixture.service.lock().unwrap().complete_error = None;
    let ready = fixture.init();
    assert!(ready.status.success(), "{}", output(&ready));
    assert_eq!(fixture.count(COMPLETE), 4);
    assert_eq!(fixture.count(QUERY), 1);
}

#[test]
fn denied_completion_and_current_read_authorization_both_fail_closed() {
    let fixture = Fixture::new("example/managed-proof");
    failed(&fixture.init(), "onboarding_pending");
    fixture.service.lock().unwrap().complete_error = Some("authorization_denied");
    failed(&fixture.init(), "repository_denied");
    assert_eq!(fixture.count(QUERY), 0);
    fixture.service.lock().unwrap().complete_error = None;
    fixture.service.lock().unwrap().read_error = Some("entitlement_required");
    failed(&fixture.init(), "action_required");
    assert_eq!(fixture.status()["server_readiness"], "not_checked");
    fixture.service.lock().unwrap().read_error = Some("42501");
    failed(&fixture.init(), "repository_denied");
    assert_eq!(fixture.count(TOKEN), 1);
    assert_eq!(fixture.count(QUERY), 2);
    fixture.assert_no_fallback();
}

#[test]
fn server_rejection_of_cached_session_blocks_further_dispatch_until_explicit_restart() {
    let fixture = Fixture::new("example/managed-proof");
    fixture.proof();
    fixture.seed(false);
    fixture.service.lock().unwrap().read_error = Some("PGRST301");
    let rejected = fixture.run(&["todo", "list", "--format", "json"]);
    failed(&rejected, "revoked");
    assert_eq!(fixture.count(QUERY), 1);
    assert_eq!(fixture.status()["phase"], "restart_required");
    fixture.service.lock().unwrap().read_error = None;
    failed(
        &fixture.run(&["todo", "list", "--format", "json"]),
        "restart_required",
    );
    assert_eq!(fixture.count(QUERY), 1);
    assert_eq!(fixture.count(REFRESH), 0);
    let restarted = fixture.run(&[
        "init",
        "--backend",
        "cloud",
        "--force",
        "--restart-cloud-signin",
    ]);
    failed(&restarted, "onboarding_pending");
    assert_eq!(fixture.count(START), 1);
    fixture.assert_no_fallback();
}

#[test]
fn credentials_are_isolated_by_origin_repository_and_project() {
    let original = Fixture::new("example/managed-proof");
    original.proof();
    original.seed(false);
    let other_origin = Fixture::new("example/managed-proof");
    let output_origin = original
        .command(&["init", "--backend", "cloud", "--force"])
        .env("DECAPOD_CLOUD_TEST_ORIGIN", &other_origin.origin)
        .output()
        .unwrap();
    failed(&output_origin, "onboarding_pending");
    assert_eq!(other_origin.count(START), 1);
    assert_eq!(other_origin.count(QUERY), 0);
    git(
        original.project.path(),
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/example/other.git",
        ],
    );
    failed(&original.init(), "onboarding_pending");
    assert_eq!(original.count(START), 1);
    assert_eq!(original.count(QUERY), 0);
    let other_project = Fixture::new("example/managed-proof");
    let result = other_project
        .command(&["init", "--backend", "cloud", "--force"])
        .env("HOME", original.machine.path())
        .env("XDG_DATA_HOME", original.machine.path().join("data"))
        .env("DECAPOD_CLOUD_TEST_ORIGIN", &original.origin)
        .output()
        .unwrap();
    failed(&result, "onboarding_pending");
    assert_eq!(original.count(START), 2);
    for fixture in [&original, &other_origin] {
        assert!(
            fixture
                .requests
                .lock()
                .unwrap()
                .iter()
                .all(|request| !request.headers.contains_key("authorization"))
        );
        fixture.assert_no_fallback();
    }
}

#[test]
fn proof_and_local_selection_do_not_create_or_consume_cloud_credentials() {
    let fixture = Fixture::new("example/managed-proof");
    let proof = fixture
        .command(&["init", "--backend", "cloud", "--force", "--proof"])
        .env("DECAPOD_CLOUD_AUTH_MODE", "mock")
        .env("DECAPOD_ACCESS_TOKEN", managed::ACCESS)
        .output()
        .unwrap();
    assert!(proof.status.success(), "{}", output(&proof));
    assert!(output(&proof).contains("sign-in and server readiness were not evaluated"));
    assert!(!output(&proof).contains("Cloud setup ready"));
    assert!(
        !fixture
            .machine
            .path()
            .join("data/decapod/cloud-sessions")
            .exists()
    );
    assert!(
        !fixture
            .project
            .path()
            .join(".decapod/managed/cloud/init-registration.json")
            .exists()
    );
    let local = fixture
        .command(&["init", "--backend", "local", "--force", "--proof"])
        .env(
            "DECAPOD_CLOUD_TEST_ORIGIN",
            "https://invalid-fixture.example.test",
        )
        .env("DECAPOD_ACCESS_TOKEN", managed::ACCESS)
        .output()
        .unwrap();
    assert!(local.status.success(), "{}", output(&local));
    assert!(fixture.requests.lock().unwrap().is_empty());
    assert!(
        std::fs::read_to_string(fixture.project.path().join(".decapod/config.toml"))
            .unwrap()
            .contains("backend = \"local\"")
    );
}

#[test]
fn fixture_origin_requires_explicit_test_mode_and_literal_loopback() {
    let fixture = Fixture::new("example/managed-proof");
    fixture.proof();
    for (origin, test_mode) in [(&*fixture.origin, "0"), ("https://example.test", "1")] {
        let result = fixture
            .command(&["cloud", "status"])
            .env("DECAPOD_CLOUD_TEST_ORIGIN", origin)
            .env("DECAPOD_TEST_MODE", test_mode)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!output(&result).contains(managed::ACCESS));
    }
    assert!(fixture.requests.lock().unwrap().is_empty());
}

#[test]
fn unsupported_cloud_startup_cannot_create_a_local_database_and_activate_is_file_only() {
    let fixture = Fixture::new("example/managed-proof");
    fixture.proof();
    // Schema reaches normal startup without first requiring an authoring
    // worktree or session; the cloud guard itself must stop local startup.
    let unsupported = fixture.run(&["data", "schema"]);
    assert!(!unsupported.status.success());
    assert!(
        output(&unsupported).contains("cloud mode never falls back to local SQLite"),
        "{}",
        output(&unsupported)
    );
    let activated = fixture.run(&["activate"]);
    assert!(activated.status.success(), "{}", output(&activated));
    assert!(output(&activated).contains("decapod.activate: ok"));
    fn assert_no_databases(path: &Path) {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() == ".git" {
                continue;
            }
            if entry.file_type().unwrap().is_dir() {
                assert_no_databases(&entry.path());
            } else {
                assert_ne!(
                    entry.path().extension().and_then(|s| s.to_str()),
                    Some("db"),
                    "cloud startup created a local database"
                );
            }
        }
    }
    assert_no_databases(fixture.project.path());
    assert!(fixture.requests.lock().unwrap().is_empty());
}
