//! Production CLI session lifecycle with synthetic loopback Auth/RPC responses.
//! This proves agent handoff and private custody, not hosted OAuth or SQL policy.
#![cfg(feature = "supabase-cloud")]

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

fn receive(stream: &mut TcpStream) -> (String, String, Value) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let end = loop {
        let mut buffer = [0; 4096];
        let count = stream.read(&mut buffer).unwrap();
        assert_ne!(count, 0);
        bytes.extend_from_slice(&buffer[..count]);
        assert!(bytes.len() < 65536);
        if let Some(offset) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break offset + 4;
        }
    };
    let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
    let length = headers
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    while bytes.len() < end + length {
        let mut buffer = [0; 4096];
        let count = stream.read(&mut buffer).unwrap();
        assert_ne!(count, 0);
        bytes.extend_from_slice(&buffer[..count]);
    }
    let path = headers
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_owned();
    let body = serde_json::from_slice(&bytes[end..end + length]).unwrap();
    (path, headers.to_ascii_lowercase(), body)
}

fn cli(root: &Path, home: &Path, origin: &str, args: &[&str]) -> Command {
    let mut command = Command::new(std::fs::canonicalize(env!("CARGO_BIN_EXE_decapod")).unwrap());
    command
        .args(args)
        .current_dir(root)
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("DECAPOD_HEADLESS", "1")
        .env("DECAPOD_AGENT_ID", "lifecycle-agent")
        .env("DECAPOD_CLOUD_DATASTORE", "supabase")
        .env("DACTYL_CLOUD_ORIGIN", origin)
        .env(
            "DACTYL_CLOUD_PUBLISHABLE_KEY",
            "sb_publishable_lifecycle_fixture",
        )
        .env_remove("DECAPOD_ACCESS_TOKEN")
        .env_remove("DECAPOD_CLOUD_AUTH_MODE")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .env_remove("DECAPOD_PROPODUS_API_URL");
    command
}

fn safe(output: &Output) {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for secret in [
        "initial-access-secret",
        "initial-refresh-secret",
        "rotated-access-secret",
        "rotated-refresh-secret",
        "one-time-auth-code",
    ] {
        assert!(
            !text.contains(secret),
            "a managed credential escaped into CLI output"
        );
    }
}

#[test]
fn init_once_then_agent_resumes_login_refreshes_and_keeps_other_repositories_separate() {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    for args in [
        vec!["init", "-q"],
        vec![
            "remote",
            "add",
            "origin",
            "https://github.com/example/luxury.git",
        ],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(root.path())
                .status()
                .unwrap()
                .success()
        );
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server_origin = origin.clone();
    let server = std::thread::spawn(move || {
        let flow = "00000000-0000-4000-8000-000000000001";
        let mut polls = Vec::new();
        for (index, expected) in [
            "start",
            "status",
            "status",
            "exchange",
            "pkce",
            "complete",
            "query",
            "query",
            "refresh_token",
            "query",
            "start",
            "status",
        ]
        .iter()
        .enumerate()
        {
            let deadline = Instant::now() + Duration::from_secs(30);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(pair) => break pair,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(error) => panic!("missing {expected} request: {error}"),
                }
            };
            let (path, headers, body) = receive(&mut stream);
            let expected_path = match *expected {
                "pkce" | "refresh_token" => format!("/auth/v1/token?grant_type={expected}"),
                "query" => "/rest/v1/rpc/dactyl_query_v1".into(),
                action => format!("/functions/v1/propodus-onboarding/{action}"),
            };
            assert_eq!(path, expected_path);
            assert!(headers.contains("apikey: sb_publishable_lifecycle_fixture"));
            let response = match *expected {
                "start" => {
                    assert_eq!(
                        body["repository"],
                        if index == 0 {
                            "example/luxury"
                        } else {
                            "example/other"
                        }
                    );
                    assert!(body.get("poll_secret").is_none());
                    json!({"flow_id":flow, "browser_url":format!("{server_origin}/functions/v1/propodus-onboarding/begin?flow_id={flow}&ticket=fixture"), "expires_at":"2099-01-01T00:00:00Z"})
                }
                "status" => {
                    polls.push(body["poll_secret"].clone());
                    json!({"status":if index == 2 { "ready" } else { "pending" }})
                }
                "exchange" => json!({"auth_code":"one-time-auth-code"}),
                "pkce" => {
                    assert_eq!(body["auth_code"], "one-time-auth-code");
                    json!({"access_token":"initial-access-secret", "refresh_token":"initial-refresh-secret", "expires_in":900})
                }
                "refresh_token" => {
                    assert_eq!(body["refresh_token"], "initial-refresh-secret");
                    json!({"access_token":"rotated-access-secret", "refresh_token":"rotated-refresh-secret", "expires_in":900})
                }
                "complete" => {
                    assert!(headers.contains("authorization: bearer initial-access-secret"));
                    json!({"status":"complete"})
                }
                "query" => {
                    assert!(headers.contains(if index <= 7 {
                        "authorization: bearer initial-access-secret"
                    } else {
                        "authorization: bearer rotated-access-secret"
                    }));
                    assert_eq!(
                        body["request"]["context"]["payload"]["route"]["Cloud"]["repository"]["canonical_name"],
                        "example/luxury"
                    );
                    assert!(!body.to_string().contains("access-secret"));
                    if index == 7 {
                        json!({"code":"session_revoked","message":"fixture session revoked"})
                    } else {
                        json!({"columns":[],"rows":[],"affected_rows":0,"generated_keys":[]})
                    }
                }
                _ => unreachable!(),
            };
            let response = response.to_string();
            let status = if index == 7 { 401 } else { 200 };
            write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
        }
        assert_eq!(polls[0], polls[1], "agent retry must resume its saved flow");
        assert_ne!(
            polls[1], polls[2],
            "a new repository requires independent sign-in custody"
        );
    });

    let init = cli(
        root.path(),
        home.path(),
        &origin,
        &[
            "init",
            "--backend",
            "cloud",
            "--github-users",
            "fixture-user",
            "--proof",
        ],
    )
    .output()
    .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    assert!(
        String::from_utf8_lossy(&init.stdout).contains("Your agent will connect this repository")
    );
    let pending = cli(root.path(), home.path(), &origin, &["session", "acquire"])
        .output()
        .unwrap();
    safe(&pending);
    assert!(!pending.status.success());
    let handoff: Value = serde_json::from_slice(&pending.stdout).unwrap();
    assert_eq!(handoff["status"], "onboarding_pending");
    assert!(
        handoff["authorization_url"]
            .as_str()
            .unwrap()
            .starts_with(&origin)
    );
    assert!(
        !handoff["next_action"]
            .as_str()
            .unwrap()
            .contains("decapod init")
    );

    let connected = cli(
        root.path(),
        home.path(),
        &origin,
        &["todo", "list", "--format", "json"],
    )
    .output()
    .unwrap();
    safe(&connected);
    assert!(
        connected.status.success(),
        "{}",
        String::from_utf8_lossy(&connected.stderr)
    );
    let record = home
        .path()
        .join("data/decapod/services")
        .join(format!("{:x}", Sha256::digest(origin.as_bytes())))
        .join("repositories")
        .join(format!("{:x}", Sha256::digest(b"example/luxury")))
        .join("session_token.json");
    let session: Value = serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    assert_eq!(session["access_token"], "initial-access-secret");
    let rejected = cli(
        root.path(),
        home.path(),
        &origin,
        &["todo", "list", "--format", "json"],
    )
    .output()
    .unwrap();
    safe(&rejected);
    assert!(
        !rejected.status.success(),
        "a rejected request must not be replayed"
    );
    let refreshed = cli(root.path(), home.path(), &origin, &["session", "acquire"])
        .output()
        .unwrap();
    safe(&refreshed);
    assert!(
        refreshed.status.success(),
        "{}",
        String::from_utf8_lossy(&refreshed.stderr)
    );
    let rotated: Value = serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
    assert_eq!(rotated["refresh_token"], "rotated-refresh-secret");
    let resumed = cli(
        root.path(),
        home.path(),
        &origin,
        &["todo", "list", "--format", "json"],
    )
    .output()
    .unwrap();
    safe(&resumed);
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    assert!(!root.path().join(".decapod/data/decapod.db").exists());
    assert!(!root.path().join(".decapod/data/todo.db").exists());
    assert!(
        Command::new("git")
            .args([
                "remote",
                "set-url",
                "origin",
                "https://github.com/example/other.git"
            ])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success()
    );
    let other = cli(
        root.path(),
        home.path(),
        &origin,
        &["todo", "add", "Another repository task", "--format", "json"],
    )
    .output()
    .unwrap();
    safe(&other);
    assert!(!other.status.success());
    let handoff: Value = serde_json::from_slice(&other.stdout).unwrap();
    assert_eq!(handoff["status"], "blocked_before_submit");
    assert_eq!(handoff["failure_kind"], "onboarding_pending");
    assert!(
        handoff["authorization_url"]
            .as_str()
            .unwrap()
            .starts_with(&origin)
    );
    assert!(
        handoff["next_action"]
            .as_str()
            .unwrap()
            .contains(handoff["operation_id"].as_str().unwrap())
    );
    server.join().unwrap();
}
