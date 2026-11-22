//! Loopback HTTP boundary proofs for `decapod cloud {orgs,members,invites,identity,billing}`
//! and matching `cloud.*` JSON RPC operations over Dactyl `0.18.0`.
//!
//! These proofs establish:
//! - zero `.decapod/data/decapod.db` or `todo.db` creation under `backend = "cloud"`,
//! - `--secret-stdin` / `--stdin` enforcement and rejection of CLI/RPC argv secrets,
//! - complete secret & URL fragment redaction from errors/stderr/responses/receipts,
//! - strict HTTPS origin and zero-credential enforcement on billing portal URLs,
//! - typed AAL2 `mfa_step_up_required` and post-send `outcome_unknown` propagation,
//! - `#215` orthogonal org-admin vs paid-repo-seat reporting and `#1836` `overage_cap_exhausted` warnings.

#[cfg(feature = "supabase-cloud")]
#[path = "common/cloud_managed.rs"]
mod managed;

#[cfg(feature = "supabase-cloud")]
mod cloud_admin_proofs {
    use super::managed;
    use decapod::core::cloud_session::Session;
    use serde_json::{Value, json};
    use std::collections::VecDeque;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::Path;
    use std::process::{Command, Output, Stdio};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::Duration;
    use tempfile::TempDir;

    #[derive(Clone, Debug)]
    struct RecordedRequest {
        path: String,
        body: Value,
    }

    struct MockReply {
        status: u16,
        headers: Vec<(String, String)>,
        body: Option<Value>,
        drop_connection: bool,
    }

    impl MockReply {
        fn json(status: u16, body: Value) -> Self {
            Self {
                status,
                headers: Vec::new(),
                body: Some(body),
                drop_connection: false,
            }
        }

        fn with_header(mut self, name: &str, value: &str) -> Self {
            self.headers.push((name.to_owned(), value.to_owned()));
            self
        }

        fn drop_after_read() -> Self {
            Self {
                status: 0,
                headers: Vec::new(),
                body: None,
                drop_connection: true,
            }
        }
    }

    struct MockServer {
        origin: String,
        recorded: Arc<Mutex<Vec<RecordedRequest>>>,
        stop: Arc<std::sync::atomic::AtomicBool>,
        handle: Option<JoinHandle<()>>,
    }

    impl MockServer {
        fn start<F>(handler: F) -> Self
        where
            F: FnMut(&str, &Value, &str) -> MockReply + Send + 'static,
        {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let profile_origin = origin.clone();
            let recorded = Arc::new(Mutex::new(Vec::new()));
            let recorded_ref = Arc::clone(&recorded);
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let stop_ref = Arc::clone(&stop);

            let mut handler = handler;
            let handle = std::thread::spawn(move || {
                while !stop_ref.load(std::sync::atomic::Ordering::Relaxed) {
                    let mut stream = match listener.accept() {
                        Ok((stream, _)) => stream,
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(5));
                            continue;
                        }
                        Err(err) => panic!("mock listener error: {err}"),
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();

                    let mut bytes = Vec::new();
                    let header_end = loop {
                        let mut buf = [0u8; 4096];
                        let n = stream.read(&mut buf).unwrap_or(0);
                        if n == 0 {
                            break None;
                        }
                        bytes.extend_from_slice(&buf[..n]);
                        if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                            break Some(pos + 4);
                        }
                    };
                    let Some(header_end) = header_end else {
                        continue;
                    };
                    let headers_text = String::from_utf8_lossy(&bytes[..header_end]).to_string();
                    let first_line = headers_text.lines().next().unwrap_or("");
                    let mut parts = first_line.split_whitespace();
                    let method = parts.next().unwrap_or("");
                    let path = parts.next().unwrap_or("");

                    let content_length = headers_text
                        .lines()
                        .find_map(|line| {
                            let (name, val) = line.split_once(':')?;
                            if name.trim().eq_ignore_ascii_case("content-length") {
                                val.trim().parse::<usize>().ok()
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                    while bytes.len() < header_end + content_length {
                        let mut buf = [0u8; 4096];
                        let n = stream.read(&mut buf).unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        bytes.extend_from_slice(&buf[..n]);
                    }
                    let body_slice =
                        &bytes[header_end..(header_end + content_length).min(bytes.len())];

                    if method == "GET"
                        && (path == "/functions/v1/decapod-onboarding/config"
                            || path == "/functions/v1/propodus-onboarding/config")
                    {
                        let body = json!({
                            "version": 1,
                            "project_origin": profile_origin,
                            "publishable_key": "sb_publishable_mock",
                            "schema": "dactyl_api",
                        })
                        .to_string();
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        continue;
                    }

                    let parsed_body: Value = if body_slice.is_empty() {
                        Value::Null
                    } else {
                        serde_json::from_slice(body_slice).unwrap_or(Value::Null)
                    };
                    recorded_ref.lock().unwrap().push(RecordedRequest {
                        path: path.to_owned(),
                        body: parsed_body.clone(),
                    });

                    let reply = handler(path, &parsed_body, &headers_text);
                    if reply.drop_connection {
                        drop(stream);
                        continue;
                    }
                    let encoded = reply
                        .body
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "{}".to_owned());
                    let mut raw = format!(
                        "HTTP/1.1 {} Mock\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
                        reply.status,
                        encoded.len()
                    );
                    for (name, val) in reply.headers {
                        raw.push_str(&format!("{name}: {val}\r\n"));
                    }
                    raw.push_str("Connection: close\r\n\r\n");
                    raw.push_str(&encoded);
                    let _ = stream.write_all(raw.as_bytes());
                }
            });

            Self {
                origin,
                recorded,
                stop,
                handle: Some(handle),
            }
        }

        fn start_sequence(replies: Vec<(&'static str, MockReply)>) -> Self {
            let queue = Arc::new(Mutex::new(VecDeque::from(replies)));
            Self::start(move |path, _body, _headers| {
                let mut q = queue.lock().unwrap();
                let (expected_suffix, reply) = q
                    .pop_front()
                    .unwrap_or_else(|| panic!("unexpected request to {path}"));
                assert!(
                    path.ends_with(expected_suffix),
                    "expected path ending with `{expected_suffix}`, got `{path}`"
                );
                reply
            })
        }

        fn recorded(&self) -> Vec<RecordedRequest> {
            self.recorded.lock().unwrap().clone()
        }
    }

    impl Drop for MockServer {
        fn drop(&mut self) {
            self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    fn setup_cloud_repo() -> TempDir {
        let root = tempfile::tempdir().unwrap();
        for args in [
            vec!["init", "-q"],
            vec![
                "remote",
                "add",
                "origin",
                "https://github.com/example/supabase-proof.git",
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
        std::fs::create_dir_all(root.path().join(".decapod")).unwrap();
        std::fs::write(
            root.path().join(".decapod/config.toml"),
            "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n",
        )
        .unwrap();
        root
    }

    fn seed_bearer(root: &Path, machine: &Path, origin: &str, token: &str) {
        managed::seed_ready(
            root,
            machine,
            origin,
            "example/supabase-proof",
            Session {
                access_token: token.into(),
                refresh_token: Some(managed::REFRESH.into()),
                expires_at: time::OffsetDateTime::now_utc().unix_timestamp() + 900,
            },
        );
    }

    fn run_decapod(
        root: &Path,
        machine: &Path,
        origin: &str,
        args: &[&str],
        stdin_bytes: Option<&[u8]>,
    ) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
        cmd.args(args)
            .current_dir(root)
            .env("HOME", machine)
            .env("XDG_DATA_HOME", machine.join("data"))
            .env("XDG_CONFIG_HOME", machine.join("config"))
            .env("DECAPOD_TEST_MODE", "1")
            .env("DECAPOD_CLOUD_TEST_ORIGIN", origin)
            .env("DECAPOD_HEADLESS", "1")
            .env("DECAPOD_AGENT_ID", "cloud-admin-proof")
            .env_remove("DECAPOD_CLOUD_AUTH_SCHEME")
            .env_remove("DECAPOD_CONTAINER_SIGNER_SOCKET")
            .env_remove("DECAPOD_SESSION_PASSWORD")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if stdin_bytes.is_some() {
            cmd.stdin(Stdio::piped());
        } else {
            cmd.stdin(Stdio::null());
        }
        let mut child = cmd.spawn().unwrap();
        if let Some(bytes) = stdin_bytes {
            child.stdin.take().unwrap().write_all(bytes).unwrap();
        }
        child.wait_with_output().unwrap()
    }

    fn assert_no_sqlite_created(root: &Path) {
        assert!(
            !root.join(".decapod/data/decapod.db").exists(),
            "decapod.db must never be created under backend = \"cloud\""
        );
        assert!(
            !root.join(".decapod/data/todo.db").exists(),
            "todo.db must never be created under backend = \"cloud\""
        );
        assert!(
            !root.join(".decapod/data").exists(),
            ".decapod/data directory must not be initialized by cloud admin commands"
        );
    }

    fn fake_jwt(payload: Value) -> String {
        let header = decapod::core::cloud_dpop::base64url_encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let claims = decapod::core::cloud_dpop::base64url_encode(payload.to_string().as_bytes());
        format!("{header}.{claims}.sig")
    }

    #[test]
    fn cloud_orgs_surfaces_paid_repo_count_and_orthogonal_role_without_sqlite() {
        let root = setup_cloud_repo();
        let machine = TempDir::new().unwrap();
        let server = MockServer::start_sequence(vec![
            (
                "/account-status",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "organizations": [
                            {
                                "id": "org-uuid-1",
                                "cloud_org_id": "cloud_org_acme",
                                "scm_provider": "github",
                                "scm_owner_id": "42",
                                "scm_owner_slug": "acme-labs",
                                "admin": true
                            }
                        ],
                        "projects": [
                            {
                                "project_id": "acme/unseated-repo",
                                "organization_id": "org-uuid-1",
                                "backend_authorized": false
                            }
                        ]
                    }),
                ),
            ),
            (
                "/billing-manage",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "status": "active",
                        "active_paid_repositories": 4,
                        "quantity": 10
                    }),
                )
                .with_header("Deprecation", "true")
                .with_header("Sunset", "Wed, 31 Dec 2026 23:59:59 GMT")
                .with_header("Decapod-Cloud-Warning", "upgrade cloud admin schema"),
            ),
            (
                "/account-status",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "organizations": [
                            {
                                "id": "org-uuid-1",
                                "cloud_org_id": "cloud_org_acme",
                                "scm_provider": "github",
                                "scm_owner_slug": "acme-labs",
                                "admin": true
                            }
                        ],
                        "projects": []
                    }),
                ),
            ),
            (
                "/billing-manage",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "active_paid_repositories": 4
                    }),
                ),
            ),
        ]);
        seed_bearer(root.path(), machine.path(), &server.origin, managed::ACCESS);

        let cli_out = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &["cloud", "--format", "json", "orgs"],
            None,
        );
        assert!(
            cli_out.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&cli_out.stderr)
        );
        let parsed: Value = serde_json::from_slice(&cli_out.stdout).unwrap();
        assert_eq!(parsed["schema_version"], "decapod.cloud.orgs.v1");
        let org = &parsed["organizations"][0];
        assert_eq!(org["organization_id"], "org-uuid-1");
        assert_eq!(org["cloud_org_id"], "cloud_org_acme");
        assert_eq!(org["slug"], "acme-labs");
        assert_eq!(org["admin"], true);
        assert_eq!(org["paid_repo_count"], 4);
        assert_eq!(org["paid_repo_seat_assigned"], false);
        assert_eq!(org["org_admin_without_paid_seat"], true);
        assert!(
            parsed["access_model_note"]
                .as_str()
                .unwrap()
                .contains("#215")
        );
        assert!(
            parsed["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w["code"] == "endpoint_deprecated")
        );

        let rpc_out = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &["rpc", "--op", "cloud.orgs", "--params", "{}"],
            None,
        );
        assert!(
            rpc_out.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&rpc_out.stderr)
        );
        let rpc_json: Value = serde_json::from_slice(&rpc_out.stdout).unwrap();
        assert_eq!(rpc_json["success"], true);
        assert_eq!(rpc_json["result"]["organizations"][0]["paid_repo_count"], 4);

        let recorded = server.recorded();
        assert!(recorded[0].path.ends_with("/account-status"));
        assert!(recorded[1].path.ends_with("/billing-manage"));
        assert_eq!(
            recorded[1].body,
            json!({"version": 1, "action": "status", "organization_id": "org-uuid-1"})
        );
        assert_no_sqlite_created(root.path());
    }

    #[test]
    fn cloud_members_assign_resolves_slot_to_account_id_and_blocks_pending_slots() {
        let member_uuid = "22222222-2222-4222-8222-222222222222";
        let owner_uuid = "11111111-1111-4111-8111-111111111111";
        let root = setup_cloud_repo();
        let machine = TempDir::new().unwrap();
        let server = MockServer::start_sequence(vec![
            // assign: status -> resolve org
            (
                "/account-status",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "projects": [{"project_id": "example/supabase-proof", "organization_id": "org-uuid-1"}]
                    }),
                ),
            ),
            // assign: page 1 of members -> next: 1
            (
                "/account-register",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "members": [{"slot": 1, "account_id": owner_uuid, "pending": false}],
                        "next": 1
                    }),
                ),
            ),
            // assign: page 2 -> slot 2
            (
                "/account-register",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "members": [
                            {"slot": 2, "account_id": member_uuid, "pending": false},
                            {"slot": 3, "account_id": null, "pending": true}
                        ],
                        "next": null
                    }),
                ),
            ),
            // assign: billing-manage
            (
                "/billing-manage",
                MockReply::json(200, json!({"version": 1, "status": "assigned"})),
            ),
            // pending slot assign: resolve org + members -> fails closed without billing-manage
            (
                "/account-status",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "projects": [{"project_id": "example/supabase-proof", "organization_id": "org-uuid-1"}]
                    }),
                ),
            ),
            (
                "/account-register",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "members": [{"slot": 3, "account_id": null, "pending": true}],
                        "next": null
                    }),
                ),
            ),
            // transfer-ownership via RPC
            (
                "/account-register",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "status": "ownership_transferred",
                        "admin_revision": 8
                    }),
                ),
            ),
            // revoke-seat (org slot) via CLI
            (
                "/account-register",
                MockReply::json(
                    200,
                    json!({"version": 1, "status": "seat_revoked", "slot": 2}),
                ),
            ),
            // project-settings via RPC --stdin
            (
                "/account-register",
                MockReply::json(
                    200,
                    json!({"version": 1, "status": "project_updated", "project_name": "Renamed Repo"}),
                ),
            ),
        ]);
        seed_bearer(root.path(), machine.path(), &server.origin, managed::ACCESS);

        let assign_out = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "cloud",
                "--format",
                "json",
                "members",
                "assign",
                "--project-id",
                "example/supabase-proof",
                "--slot",
                "2",
            ],
            None,
        );
        assert!(
            assign_out.status.success(),
            "{}",
            String::from_utf8_lossy(&assign_out.stderr)
        );
        let assign_json: Value = serde_json::from_slice(&assign_out.stdout).unwrap();
        assert_eq!(assign_json["account_id"], member_uuid);

        let pending_err = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "cloud",
                "--format",
                "json",
                "members",
                "assign",
                "--project-id",
                "example/supabase-proof",
                "--slot",
                "3",
            ],
            None,
        );
        assert!(!pending_err.status.success());
        assert!(
            String::from_utf8_lossy(&pending_err.stderr).contains("pending acceptance"),
            "{}",
            String::from_utf8_lossy(&pending_err.stderr)
        );

        let transfer_rpc = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "rpc",
                "--op",
                "cloud.members",
                "--params",
                &format!(
                    r#"{{"operation":"transfer-ownership","organization_id":"org-uuid-1","member_account_id":"{member_uuid}","expected_revision":7}}"#
                ),
            ],
            None,
        );
        assert!(transfer_rpc.status.success());

        let revoke_seat_out = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "cloud",
                "--format",
                "json",
                "members",
                "revoke-seat",
                "--organization-id",
                "org-uuid-1",
                "--slot",
                "2",
            ],
            None,
        );
        assert!(revoke_seat_out.status.success());

        let settings_stdin = serde_json::json!({
            "op": "cloud.members",
            "params": {
                "operation": "project-settings",
                "organization_id": "org-uuid-1",
                "project_id": "example/supabase-proof",
                "project_name": "Renamed Repo"
            }
        })
        .to_string();
        let settings_out = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &["rpc", "--stdin"],
            Some(settings_stdin.as_bytes()),
        );
        assert!(settings_out.status.success());

        let recorded = server.recorded();
        assert_eq!(
            recorded[3].body,
            json!({
                "version": 1,
                "action": "assign",
                "organization_id": "org-uuid-1",
                "project_id": "example/supabase-proof",
                "account_id": member_uuid
            })
        );
        assert_eq!(
            recorded[6].body,
            json!({
                "version": 1,
                "operation": "transfer_ownership",
                "organization_id": "org-uuid-1",
                "member_account_id": member_uuid,
                "expected_revision": 7
            })
        );
        assert_no_sqlite_created(root.path());
    }

    #[test]
    fn cloud_admin_surfaces_mfa_step_up_and_single_send_outcome_unknown() {
        let member_uuid = "22222222-2222-4222-8222-222222222222";
        let root = setup_cloud_repo();
        let machine = TempDir::new().unwrap();
        let aal1_jwt = fake_jwt(json!({ "sub": "user-1", "aal": "aal1" }));

        let server = MockServer::start_sequence(vec![
            // CLI assign -> 403 mfa_step_up_required
            (
                "/account-register",
                MockReply::json(
                    200,
                    json!({"version": 1, "members": [{"slot": 2, "account_id": member_uuid, "pending": false}]}),
                ),
            ),
            (
                "/billing-manage",
                MockReply::json(
                    403,
                    json!({"error": {"code": "mfa_step_up_required", "message": "aal2 required"}}),
                ),
            ),
            // RPC transfer-ownership -> 500 (post-send outcome_unknown, MUST NOT RETRY)
            (
                "/account-register",
                MockReply::json(500, json!({"error": "upstream crash"})),
            ),
            // CLI billing portal -> dropped TCP connection (post-send outcome_unknown, MUST NOT RETRY)
            ("/billing-portal", MockReply::drop_after_read()),
        ]);
        seed_bearer(root.path(), machine.path(), &server.origin, &aal1_jwt);

        let mfa_out = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "cloud",
                "--format",
                "json",
                "members",
                "assign",
                "--organization-id",
                "org-uuid-1",
                "--project-id",
                "example/supabase-proof",
                "--slot",
                "2",
            ],
            None,
        );
        assert!(!mfa_out.status.success());
        let mfa_diag: Value = serde_json::from_slice(&mfa_out.stdout).unwrap();
        assert_eq!(mfa_diag["status"], "mfa_required");
        assert_eq!(mfa_diag["error_code"], "mfa_step_up_required");

        let unknown_rpc = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "rpc",
                "--op",
                "cloud.members",
                "--params",
                &format!(
                    r#"{{"operation":"transfer-ownership","organization_id":"org-uuid-1","member_account_id":"{member_uuid}"}}"#
                ),
            ],
            None,
        );
        assert!(!unknown_rpc.status.success());
        let rpc_diag: Value = serde_json::from_slice(&unknown_rpc.stdout).unwrap();
        assert_eq!(rpc_diag["success"], false);
        assert_eq!(rpc_diag["error"]["code"], "outcome_unknown");
        assert_eq!(rpc_diag["error"]["details"]["status"], "outcome_unknown");

        let drop_cli = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "cloud",
                "--format",
                "json",
                "billing",
                "portal",
                "--organization-id",
                "org-uuid-1",
            ],
            None,
        );
        assert!(!drop_cli.status.success());
        let drop_diag: Value = serde_json::from_slice(&drop_cli.stdout).unwrap();
        assert_eq!(drop_diag["status"], "outcome_unknown");
        assert_eq!(drop_diag["error_code"], "outcome_unknown");

        // Confirm zero retries happened after post-send 500 or connection drop:
        assert_eq!(server.recorded().len(), 4);
        assert_no_sqlite_created(root.path());
    }

    #[test]
    fn cloud_invites_and_identity_enforce_stdin_secrets_redaction_and_zero_paid_seats() {
        let recipient_uuid = "33333333-3333-4333-8333-333333333333";
        let invite_uuid = "44444444-4444-4444-8444-444444444444";
        let intent_uuid = "55555555-5555-4555-8555-555555555555";
        let root = setup_cloud_repo();
        let machine = TempDir::new().unwrap();

        let server = MockServer::start_sequence(vec![
            // invites create
            (
                "/account-register",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "status": "invitation_created",
                        "organization_id": "org-uuid-1",
                        "invitation_id": invite_uuid,
                        "slot": 4,
                        "expires_at": "2026-12-01T00:00:00Z",
                        "invitation_secret": "one_time_invite_secret_987",
                        "invitation_url": format!("https://cloud.decapod.sh/accept?invitation_id={invite_uuid}#invitation_secret=one_time_invite_secret_987")
                    }),
                ),
            ),
            // invites create with invalid non-HTTPS invitation_url -> must redact fragment in error!
            (
                "/account-register",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "status": "invitation_created",
                        "organization_id": "org-uuid-1",
                        "invitation_id": invite_uuid,
                        "invitation_secret": "leaked_frag_secret_abc",
                        "invitation_url": "http://insecure.example.com/accept#invitation_secret=leaked_frag_secret_abc"
                    }),
                ),
            ),
            // invites accept via --secret-stdin CLI
            (
                "/account-register",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "status": "invitation_accepted",
                        "organization_id": "org-uuid-1",
                        "backend_authorized": false
                    }),
                ),
            ),
            // invites accept via RPC --stdin
            (
                "/account-register",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "status": "invitation_accepted",
                        "organization_id": "org-uuid-1",
                        "backend_authorized": false
                    }),
                ),
            ),
            // identity confirm-link via --secret-stdin CLI
            (
                "/account-register",
                MockReply::json(200, json!({"version": 1, "status": "verified"})),
            ),
            // identity resolve-quarantine via CLI
            (
                "/account-register",
                MockReply::json(200, json!({"version": 1, "status": "resolved"})),
            ),
            // identity revoke-link via RPC --stdin
            (
                "/account-register",
                MockReply::json(200, json!({"version": 1, "status": "revoked"})),
            ),
        ]);
        seed_bearer(root.path(), machine.path(), &server.origin, managed::ACCESS);

        // Reject argv secret flags on CLI before any HTTP traffic
        let bad_cli_flag = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "cloud",
                "invites",
                "accept",
                "--invitation-id",
                invite_uuid,
                "--invitation-secret",
                "forbidden_argv_secret",
            ],
            None,
        );
        assert!(!bad_cli_flag.status.success());

        let missing_stdin_flag = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &["cloud", "invites", "accept", "--invitation-id", invite_uuid],
            None,
        );
        assert!(!missing_stdin_flag.status.success());

        // Reject argv secrets on RPC --op --params BEFORE any HTTP traffic
        let bad_rpc_argv = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "rpc",
                "--op",
                "cloud.invites",
                "--params",
                &format!(
                    r#"{{"operation":"accept","invitation_id":"{invite_uuid}","invitation_secret":"ARG_SECRET_MUST_NOT_PASS"}}"#
                ),
            ],
            None,
        );
        assert!(!bad_rpc_argv.status.success());
        assert!(
            !String::from_utf8_lossy(&bad_rpc_argv.stderr).contains("ARG_SECRET_MUST_NOT_PASS")
        );

        let bad_identity_rpc_argv = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "rpc",
                "--op",
                "cloud.identity",
                "--params",
                &format!(
                    r#"{{"operation":"confirm-link","intent_id":"{intent_uuid}","stepup_secret":"ARG_STEPUP_MUST_NOT_PASS"}}"#
                ),
            ],
            None,
        );
        assert!(!bad_identity_rpc_argv.status.success());
        assert!(
            !String::from_utf8_lossy(&bad_identity_rpc_argv.stderr)
                .contains("ARG_STEPUP_MUST_NOT_PASS")
        );

        // Confirm zero HTTP requests happened from rejected argv secret attempts
        assert!(server.recorded().is_empty());

        // 1. invites create -> zero paid seats explicit notice
        let create_out = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "cloud",
                "--format",
                "json",
                "invites",
                "create",
                "--organization-id",
                "org-uuid-1",
                "--recipient-account-id",
                recipient_uuid,
            ],
            None,
        );
        assert!(create_out.status.success());
        let create_json: Value = serde_json::from_slice(&create_out.stdout).unwrap();
        assert_eq!(create_json["membership_grants_paid_seats"], false);
        assert!(
            create_json["seat_assignment_note"]
                .as_str()
                .unwrap()
                .contains("zero paid repository seats")
        );

        // 2. invites create invalid non-HTTPS URL -> redacts fragment in error output!
        let bad_url_err = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "cloud",
                "--format",
                "json",
                "invites",
                "create",
                "--organization-id",
                "org-uuid-1",
                "--recipient-account-id",
                recipient_uuid,
            ],
            None,
        );
        assert!(!bad_url_err.status.success());
        let stderr_text = String::from_utf8_lossy(&bad_url_err.stderr);
        assert!(
            !stderr_text.contains("leaked_frag_secret_abc"),
            "stderr leaked invitation secret fragment: {stderr_text}"
        );

        // 3. invites accept via CLI --secret-stdin
        let accept_cli = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "cloud",
                "--format",
                "json",
                "invites",
                "accept",
                "--invitation-id",
                invite_uuid,
                "--secret-stdin",
            ],
            Some(b"stdin_invite_secret_xyz\n"),
        );
        assert!(
            accept_cli.status.success(),
            "{}",
            String::from_utf8_lossy(&accept_cli.stderr)
        );
        let accept_json: Value = serde_json::from_slice(&accept_cli.stdout).unwrap();
        assert_eq!(accept_json["membership_grants_paid_seats"], false);
        assert!(!String::from_utf8_lossy(&accept_cli.stdout).contains("stdin_invite_secret_xyz"));

        // 4. invites accept via RPC --stdin
        let rpc_accept_in = json!({
            "op": "cloud.invites",
            "params": {
                "operation": "accept",
                "invitation_id": invite_uuid,
                "invitation_secret": "rpc_stdin_secret_777"
            }
        })
        .to_string();
        let rpc_accept = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &["rpc", "--stdin"],
            Some(rpc_accept_in.as_bytes()),
        );
        assert!(rpc_accept.status.success());
        assert!(!String::from_utf8_lossy(&rpc_accept.stdout).contains("rpc_stdin_secret_777"));

        // 5. identity confirm-link via --secret-stdin
        let confirm_cli = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "cloud",
                "--format",
                "json",
                "identity",
                "confirm-link",
                "--intent-id",
                intent_uuid,
                "--secret-stdin",
            ],
            Some(b"stepup_secret_stdin_val\n"),
        );
        assert!(confirm_cli.status.success());
        assert!(!String::from_utf8_lossy(&confirm_cli.stdout).contains("stepup_secret_stdin_val"));

        // 6. identity resolve-quarantine
        let resolve_cli = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "cloud",
                "--format",
                "json",
                "identity",
                "resolve-quarantine",
                "--provider",
                "google",
                "--decision",
                "revoke",
            ],
            None,
        );
        assert!(resolve_cli.status.success());

        // 7. identity revoke-link via RPC --stdin
        let revoke_in = json!({
            "op": "cloud.identity",
            "params": { "operation": "revoke-link", "provider": "gitlab" }
        })
        .to_string();
        let revoke_rpc = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &["rpc", "--stdin"],
            Some(revoke_in.as_bytes()),
        );
        assert!(revoke_rpc.status.success());

        let recorded = server.recorded();
        assert_eq!(
            recorded[2].body["invitation_secret"],
            "stdin_invite_secret_xyz"
        );
        assert_eq!(
            recorded[3].body["invitation_secret"],
            "rpc_stdin_secret_777"
        );
        assert_eq!(recorded[4].body["stepup_secret"], "stepup_secret_stdin_val");
        assert_eq!(recorded[5].body["provider"], "google");
        assert_eq!(recorded[5].body["decision"], "revoke");
        assert_eq!(recorded[6].body["provider"], "gitlab");
        assert_no_sqlite_created(root.path());
    }

    #[test]
    fn cloud_billing_portal_enforces_https_zero_credentials_and_limits_overage_cap_exhausted() {
        let root = setup_cloud_repo();
        let machine = TempDir::new().unwrap();
        let server = MockServer::start_sequence(vec![
            // valid HTTPS portal URL
            (
                "/billing-portal",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "portal_url": "https://billing.stripe.com/p/session/live_safe_session?locale=en"
                    }),
                ),
            ),
            // invalid non-HTTPS portal URL -> rejected!
            (
                "/billing-portal",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "portal_url": "http://billing.stripe.com/p/session/insecure"
                    }),
                ),
            ),
            // invalid userinfo portal URL -> rejected!
            (
                "/billing-portal",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "portal_url": "https://user:secret@billing.stripe.com/p/session/123"
                    }),
                ),
            ),
            // invalid token query param -> rejected!
            (
                "/billing-portal",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "portal_url": "https://billing.stripe.com/p/session/123?access_token=bad"
                    }),
                ),
            ),
            // limits with overage_cap_exhausted
            (
                "/account-status",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "backend_authorized": true,
                        "resource_limits": {
                            "window_month": "2026-11",
                            "paid_overages": {
                                "opted_in": true,
                                "monthly_spend_cap_cents": 500,
                                "spent_cents": 500,
                                "remaining_spend_cents": 0,
                                "status": "overage_cap_exhausted"
                            },
                            "metrics": []
                        }
                    }),
                )
                .with_header("Decapod-Cloud-Notice", "spend cap reached"),
            ),
            // status --live with overage_cap_exhausted
            (
                "/account-status",
                MockReply::json(
                    200,
                    json!({
                        "version": 1,
                        "backend_authorized": true,
                        "resource_limits": {
                            "window_month": "2026-11",
                            "paid_overages": {
                                "opted_in": true,
                                "monthly_spend_cap_cents": 500,
                                "spent_cents": 500,
                                "remaining_spend_cents": 0,
                                "status": "overage_cap_exhausted"
                            },
                            "metrics": []
                        }
                    }),
                ),
            ),
        ]);
        seed_bearer(root.path(), machine.path(), &server.origin, managed::ACCESS);

        let valid_portal = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "cloud",
                "--format",
                "json",
                "billing",
                "portal",
                "--organization-id",
                "org-uuid-1",
            ],
            None,
        );
        assert!(valid_portal.status.success());
        let valid_json: Value = serde_json::from_slice(&valid_portal.stdout).unwrap();
        assert_eq!(
            valid_json["portal_url"],
            "https://billing.stripe.com/p/session/live_safe_session?locale=en"
        );

        for _ in 0..3 {
            let bad_portal = run_decapod(
                root.path(),
                machine.path(),
                &server.origin,
                &[
                    "rpc",
                    "--op",
                    "cloud.billing",
                    "--params",
                    r#"{"operation":"portal","organization_id":"org-uuid-1"}"#,
                ],
                None,
            );
            assert!(!bad_portal.status.success());
            let err_json: Value = serde_json::from_slice(&bad_portal.stdout).unwrap();
            assert_eq!(err_json["success"], false);
        }

        let limits_cli = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &[
                "cloud",
                "--format",
                "json",
                "billing",
                "limits",
                "--project-id",
                "example/supabase-proof",
            ],
            None,
        );
        assert!(limits_cli.status.success());
        let limits_json: Value = serde_json::from_slice(&limits_cli.stdout).unwrap();
        assert_eq!(limits_json["spend_cap_status"], "overage_cap_exhausted");
        assert_eq!(limits_json["spend_cap_warning"], true);
        assert_eq!(limits_json["warnings"][0]["value"], "spend cap reached");

        let status_live = run_decapod(
            root.path(),
            machine.path(),
            &server.origin,
            &["cloud", "--format", "json", "status", "--live"],
            None,
        );
        assert!(status_live.status.success());
        let live_json: Value = serde_json::from_slice(&status_live.stdout).unwrap();
        assert_eq!(live_json["spend_cap_status"], "overage_cap_exhausted");
        assert_eq!(live_json["spend_cap_warning"], true);
        assert_no_sqlite_created(root.path());
    }
}
