//! Multi-organization, multi-repository, multi-human, multi-agent Cloud
//! tenancy parity integration suite (#1895).
//!
//! Exercises the real `decapod` CLI binary, `DactylTodoStore`, `cloud_project`,
//! `cloud_workspace`, `cloud_proof`, and `cloud_parity_matrix` against a
//! simulated Cloud gateway backed by two physically separate opaque backend
//! SQLite databases (one per organization, two repos per org).

#![cfg(feature = "supabase-cloud")]
#![allow(clippy::disallowed_types)]

#[path = "common/cloud_managed.rs"]
mod managed;

use dactyl_db::{Connection, DatastoreRoute, Operation, OperationKind, Parameter};
use decapod::core::backend::{BackendRoute, CloudDatastore, StorageContext};
use decapod::core::cloud_parity_matrix::{
    ParitySupportClass, build_parity_matrix, cross_check_parity_matrix,
};
use decapod::core::cloud_proof::{
    PROOF_ACCEPTANCE_CONTROL_OP, ProofCaptureRequest, ProofControlTransport,
    ProofEvidenceReference, ProofGitIdentity, ProofSubject, submit_capture,
};
use decapod::core::cloud_session::Session;
use decapod::core::cloud_workspace::{
    CloudCustodyBindParams, CloudCustodyStore, custody_bind_or_reconcile,
};
use decapod::core::dactyl_todo::DactylTodoStore;
use decapod::core::repo_identity::RepositoryIdentity;
use decapod::core::storage::{Task, TodoStore};
use decapod::core::todo::{ClaimMode, TodoCommand, add_task, claim_task, get_task, list_tasks};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use tempfile::TempDir;
use time::OffsetDateTime;

const ORG1_ID: &str = "org-1-alpha";
const ORG2_ID: &str = "org-2-beta";
const REPO_1A: &str = "org1/repo-a";
const REPO_1B: &str = "org1/repo-b";
const REPO_2A: &str = "org2/repo-a";
const REPO_2B: &str = "org2/repo-b";

const HUMAN_A_ID: &str = "00000000-0000-0000-0000-000000000101";
const HUMAN_B_ID: &str = "00000000-0000-0000-0000-000000000202";
const TOKEN_A: &str = "synthetic-bearer-human-a";
const TOKEN_B: &str = "synthetic-bearer-human-b";

// Private server placement secrets that Decapod must NEVER observe or persist.
const OPAQUE_DB_1_FILENAME: &str = "opaque_tenant_store_alpha_991.sqlite";
const OPAQUE_DB_2_FILENAME: &str = "opaque_tenant_store_beta_774.sqlite";
const OPAQUE_DB_ROLE_1: &str = "pg_tenant_alpha_rw";
const OPAQUE_DB_ROLE_2: &str = "pg_tenant_beta_rw";
const OPAQUE_DB_URL_1: &str = "postgres://opaque-alpha-internal:5432/tenant_alpha_991";
const OPAQUE_DB_URL_2: &str = "postgres://opaque-beta-internal:5432/tenant_beta_774";
const OPAQUE_PLACEMENT_GEN: &str = "placement_generation_991_secret";

#[derive(Clone, Debug)]
struct RepoSeatPolicy {
    org_id: &'static str,
    admin: bool,
    seat_assigned: bool,
}

#[derive(Clone, Debug)]
struct GatewayPolicyState {
    // (human_id, repo_id) -> RepoSeatPolicy
    entitlements: BTreeMap<(String, String), RepoSeatPolicy>,
    // Org admin status even when repo_assigned == false (#215)
    org_admins: BTreeMap<(String, String), bool>,
    revoked_tokens: BTreeSet<String>,
    // Track idempotent control operations per org/repo: operation_id -> (repo_id, fingerprint, receipt)
    control_ops: BTreeMap<String, (String, String, Value)>,
    fencing_counter: i64,
}

impl GatewayPolicyState {
    fn default_two_org() -> Self {
        let mut entitlements = BTreeMap::new();
        // Human A:
        // - Org 1 Admin; paid seat on Repo 1A, ZERO paid seat on Repo 1B (#215)
        entitlements.insert(
            (HUMAN_A_ID.into(), REPO_1A.into()),
            RepoSeatPolicy {
                org_id: ORG1_ID,
                admin: true,
                seat_assigned: true,
            },
        );
        entitlements.insert(
            (HUMAN_A_ID.into(), REPO_1B.into()),
            RepoSeatPolicy {
                org_id: ORG1_ID,
                admin: true,
                seat_assigned: false,
            },
        );
        // - Org 2 member; paid seats on Repo 2A & Repo 2B
        entitlements.insert(
            (HUMAN_A_ID.into(), REPO_2A.into()),
            RepoSeatPolicy {
                org_id: ORG2_ID,
                admin: false,
                seat_assigned: true,
            },
        );
        entitlements.insert(
            (HUMAN_A_ID.into(), REPO_2B.into()),
            RepoSeatPolicy {
                org_id: ORG2_ID,
                admin: false,
                seat_assigned: true,
            },
        );

        // Human B:
        // - Org 1 non-admin; paid seat ONLY on Repo 1A; NO seat on Repo 1B
        entitlements.insert(
            (HUMAN_B_ID.into(), REPO_1A.into()),
            RepoSeatPolicy {
                org_id: ORG1_ID,
                admin: false,
                seat_assigned: true,
            },
        );
        entitlements.insert(
            (HUMAN_B_ID.into(), REPO_1B.into()),
            RepoSeatPolicy {
                org_id: ORG1_ID,
                admin: false,
                seat_assigned: false,
            },
        );
        // - Org 2: Human B is completely denied Org 2 (not a member).

        let mut org_admins = BTreeMap::new();
        org_admins.insert((HUMAN_A_ID.into(), ORG1_ID.into()), true);
        org_admins.insert((HUMAN_A_ID.into(), ORG2_ID.into()), false);
        org_admins.insert((HUMAN_B_ID.into(), ORG1_ID.into()), false);
        org_admins.insert((HUMAN_B_ID.into(), ORG2_ID.into()), false);

        Self {
            entitlements,
            org_admins,
            revoked_tokens: BTreeSet::new(),
            control_ops: BTreeMap::new(),
            fencing_counter: 10,
        }
    }
}

struct CloudTenancyGateway {
    _private_db_dir: TempDir,
    org1_db_path: PathBuf,
    org2_db_path: PathBuf,
    origin: String,
    policy: Arc<Mutex<GatewayPolicyState>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl CloudTenancyGateway {
    fn start() -> Self {
        let private_db_dir = tempfile::tempdir().expect("server private db dir");
        let org1_db_path = private_db_dir.path().join(OPAQUE_DB_1_FILENAME);
        let org2_db_path = private_db_dir.path().join(OPAQUE_DB_2_FILENAME);
        init_org_db(&org1_db_path);
        init_org_db(&org2_db_path);

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback gateway");
        listener.set_nonblocking(true).expect("nonblocking");
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let policy = Arc::new(Mutex::new(GatewayPolicyState::default_two_org()));
        let stop = Arc::new(AtomicBool::new(false));

        let origin_c = origin.clone();
        let org1_c = org1_db_path.clone();
        let org2_c = org2_db_path.clone();
        let policy_c = Arc::clone(&policy);
        let stop_c = Arc::clone(&stop);

        let handle = std::thread::spawn(move || {
            while !stop_c.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        Self::serve_connection(&mut stream, &origin_c, &org1_c, &org2_c, &policy_c);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            _private_db_dir: private_db_dir,
            org1_db_path,
            org2_db_path,
            origin,
            policy,
            stop,
            handle: Some(handle),
        }
    }

    fn revoke_seat(&self, human_id: &str, repo_id: &str) {
        let mut guard = self.policy.lock().unwrap();
        if let Some(entry) = guard
            .entitlements
            .get_mut(&(human_id.to_string(), repo_id.to_string()))
        {
            entry.seat_assigned = false;
        }
    }

    fn shutdown_network(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }

    fn org_db_path_for_repo(org1: &Path, org2: &Path, repo: &str) -> Option<PathBuf> {
        if repo.starts_with("org1/") {
            Some(org1.to_path_buf())
        } else if repo.starts_with("org2/") {
            Some(org2.to_path_buf())
        } else {
            None
        }
    }

    fn observer_tasks(&self, org_id: &str, repo_id: &str) -> Vec<(String, String, String, i32)> {
        let db = match org_id {
            ORG1_ID => &self.org1_db_path,
            ORG2_ID => &self.org2_db_path,
            _ => panic!("unknown org {org_id}"),
        };
        let conn = Connection::open(DatastoreRoute::sqlite(db.to_string_lossy().to_string()))
            .expect("observer open org db");
        let rows = conn
            .read(
                "SELECT id, status, COALESCE(assigned_to, '') AS assignee, version FROM org_tasks WHERE repo_id = $1 ORDER BY id ASC",
                &[repo_id.into()],
            )
            .expect("observer tasks query");
        rows.as_slice()
            .iter()
            .map(|r| {
                (
                    r.get_str("id").unwrap(),
                    r.get_str("status").unwrap(),
                    r.get_str("assignee").unwrap(),
                    r.get::<_, i64>("version").unwrap() as i32,
                )
            })
            .collect()
    }

    fn observer_events(&self, org_id: &str, repo_id: &str) -> Vec<(i64, String, String, String)> {
        let db = match org_id {
            ORG1_ID => &self.org1_db_path,
            ORG2_ID => &self.org2_db_path,
            _ => panic!("unknown org {org_id}"),
        };
        let conn = Connection::open(DatastoreRoute::sqlite(db.to_string_lossy().to_string()))
            .expect("observer open org db");
        let rows = conn
            .read(
                "SELECT seq, event_type, subject_id, actor FROM org_events WHERE repo_id = $1 ORDER BY seq ASC",
                &[repo_id.into()],
            )
            .expect("observer events query");
        rows.as_slice()
            .iter()
            .map(|r| {
                (
                    r.get::<_, i64>("seq").unwrap(),
                    r.get_str("event_type").unwrap(),
                    r.get_str("subject_id").unwrap(),
                    r.get_str("actor").unwrap(),
                )
            })
            .collect()
    }

    fn observer_custody(&self, org_id: &str, repo_id: &str) -> Vec<(String, String, i64, i64)> {
        let db = match org_id {
            ORG1_ID => &self.org1_db_path,
            ORG2_ID => &self.org2_db_path,
            _ => panic!("unknown org {org_id}"),
        };
        let conn = Connection::open(DatastoreRoute::sqlite(db.to_string_lossy().to_string()))
            .expect("observer open org db");
        let rows = conn
            .read(
                "SELECT task_id, custody_id, task_revision, fencing_generation FROM org_custody WHERE repo_id = $1 ORDER BY task_id ASC",
                &[repo_id.into()],
            )
            .expect("observer custody query");
        rows.as_slice()
            .iter()
            .map(|r| {
                (
                    r.get_str("task_id").unwrap(),
                    r.get_str("custody_id").unwrap(),
                    r.get::<_, i64>("task_revision").unwrap(),
                    r.get::<_, i64>("fencing_generation").unwrap(),
                )
            })
            .collect()
    }

    fn observer_proofs(
        &self,
        org_id: &str,
        repo_id: &str,
    ) -> Vec<(String, String, String, i64, String)> {
        let db = match org_id {
            ORG1_ID => &self.org1_db_path,
            ORG2_ID => &self.org2_db_path,
            _ => panic!("unknown org {org_id}"),
        };
        let conn = Connection::open(DatastoreRoute::sqlite(db.to_string_lossy().to_string()))
            .expect("observer open org db");
        let rows = conn
            .read(
                "SELECT task_id, source_commit, policy_revision, task_revision, state FROM org_proofs WHERE repo_id = $1 ORDER BY task_id ASC",
                &[repo_id.into()],
            )
            .expect("observer proof query");
        rows.as_slice()
            .iter()
            .map(|r| {
                (
                    r.get_str("task_id").unwrap(),
                    r.get_str("source_commit").unwrap(),
                    r.get_str("policy_revision").unwrap(),
                    r.get::<_, i64>("task_revision").unwrap(),
                    r.get_str("state").unwrap(),
                )
            })
            .collect()
    }

    fn serve_connection(
        stream: &mut TcpStream,
        origin: &str,
        org1_db: &Path,
        org2_db: &Path,
        policy: &Arc<Mutex<GatewayPolicyState>>,
    ) {
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
        let header = String::from_utf8_lossy(&bytes[..body_start]).to_string();
        let mut first_line = header.lines().next().unwrap_or("").split_whitespace();
        let method = first_line.next().unwrap_or("").to_string();
        let path = first_line.next().unwrap_or("").to_string();
        let headers: BTreeMap<String, String> = header
            .lines()
            .skip(1)
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
            .collect();
        let content_len = headers
            .get("content-length")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);
        while bytes.len() < body_start + content_len {
            let mut buf = [0u8; 4096];
            let Ok(n) = stream.read(&mut buf) else {
                return;
            };
            if n == 0 {
                return;
            }
            bytes.extend_from_slice(&buf[..n]);
        }
        let body: Value = if content_len == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[body_start..body_start + content_len])
                .unwrap_or(Value::Null)
        };

        let (status, resp) = Self::route_request(
            origin, org1_db, org2_db, policy, &method, &path, &headers, &body,
        );

        // Verify server NEVER leaks opaque DB names, roles, URLs, or placement generation!
        let encoded = resp.to_string();
        for forbidden in [
            OPAQUE_DB_1_FILENAME,
            OPAQUE_DB_2_FILENAME,
            OPAQUE_DB_ROLE_1,
            OPAQUE_DB_ROLE_2,
            OPAQUE_DB_URL_1,
            OPAQUE_DB_URL_2,
            OPAQUE_PLACEMENT_GEN,
        ] {
            assert!(
                !encoded.contains(forbidden),
                "server response leaked opaque placement detail {forbidden}"
            );
        }

        let status_text = match status {
            200 => "OK",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            409 => "Conflict",
            _ => "Error",
        };
        let reply = format!(
            "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{encoded}",
            encoded.len()
        );
        let _ = stream.write_all(reply.as_bytes());
    }

    #[allow(clippy::too_many_arguments)]
    fn route_request(
        origin: &str,
        org1_db: &Path,
        org2_db: &Path,
        policy: &Arc<Mutex<GatewayPolicyState>>,
        method: &str,
        path: &str,
        headers: &BTreeMap<String, String>,
        body: &Value,
    ) -> (u16, Value) {
        if path == "/functions/v1/decapod-onboarding/config" && method == "GET" {
            return (
                200,
                json!({
                    "version": 1,
                    "project_origin": origin,
                    "publishable_key": "sb_publishable_fixture",
                    "schema": "dactyl_api",
                }),
            );
        }

        let Some(auth_header) = headers.get("authorization") else {
            return (
                401,
                json!({"code": "authentication_required", "message": "missing bearer token"}),
            );
        };
        let token = auth_header
            .strip_prefix("Bearer ")
            .or_else(|| auth_header.strip_prefix("bearer "))
            .unwrap_or("")
            .trim();

        let human_id = match token {
            TOKEN_A => HUMAN_A_ID,
            TOKEN_B => HUMAN_B_ID,
            _ => {
                return (
                    401,
                    json!({"code": "session_revoked", "message": "invalid or revoked bearer token"}),
                );
            }
        };

        let mut pol = policy.lock().unwrap();
        if pol.revoked_tokens.contains(token) {
            return (
                401,
                json!({"code": "session_revoked", "message": "session revoked"}),
            );
        }

        match path {
            "/functions/v1/decapod-onboarding/account-status" => {
                if let Some(project_id) = body.get("project_id").and_then(Value::as_str) {
                    let Some(entry) = pol
                        .entitlements
                        .get(&(human_id.to_string(), project_id.to_string()))
                        .cloned()
                    else {
                        let org_guess = if project_id.starts_with("org1/") {
                            Some(ORG1_ID)
                        } else if project_id.starts_with("org2/") {
                            Some(ORG2_ID)
                        } else {
                            None
                        };
                        return (
                            200,
                            json!({
                                "version": 1,
                                "account_id": human_id,
                                "project_id": project_id,
                                "organization_id": org_guess,
                                "cloud_org_id": org_guess,
                                "status": "authorization_denied",
                                "authorization_state": "authorization_denied",
                                "admin": false,
                                "repo_assigned": false,
                                "backend_authorized": false
                            }),
                        );
                    };
                    let (status_str, auth_state) = if entry.seat_assigned {
                        ("active", "active")
                    } else if entry.admin {
                        ("admin_approval_required", "admin_approval_required")
                    } else {
                        ("entitlement_required", "entitlement_required")
                    };
                    return (
                        200,
                        json!({
                            "version": 1,
                            "account_id": human_id,
                            "project_id": project_id,
                            "organization_id": entry.org_id,
                            "cloud_org_id": entry.org_id,
                            "status": status_str,
                            "authorization_state": auth_state,
                            "admin": entry.admin,
                            "repo_assigned": entry.seat_assigned,
                            "backend_authorized": entry.seat_assigned
                        }),
                    );
                }

                // Org-admin inspection surface (#215): an org admin can inspect org admin
                // status on any repo in their org even with zero paid repo seat, without
                // granting repository governance data access.
                if body.get("admin_inspection").and_then(Value::as_bool) == Some(true)
                    && let Some(org_id) = body.get("organization_id").and_then(Value::as_str)
                {
                    let is_admin = pol
                        .org_admins
                        .get(&(human_id.to_string(), org_id.to_string()))
                        .copied()
                        .unwrap_or(false);
                    if !is_admin {
                        return (
                            403,
                            json!({"code": "authorization_denied", "message": "not organization admin"}),
                        );
                    }
                    let repo = body
                        .get("target_repo")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let seat_assigned = pol
                        .entitlements
                        .get(&(human_id.to_string(), repo.to_string()))
                        .is_some_and(|e| e.seat_assigned);
                    return (
                        200,
                        json!({
                            "version": 1,
                            "account_id": human_id,
                            "organization_id": org_id,
                            "cloud_org_id": org_id,
                            "admin": true,
                            "repo_assigned": seat_assigned,
                            "backend_authorized": false,
                            "status": "admin_settings_only",
                            "note": "Organization admin status grants zero Decapod governance data access."
                        }),
                    );
                }

                // Directory listing: only repos/orgs associated with human_id
                let mut orgs = BTreeSet::new();
                let mut projects = Vec::new();
                for ((h, repo), entry) in &pol.entitlements {
                    if h == human_id {
                        orgs.insert(entry.org_id);
                        projects.push(json!({
                            "project_id": repo,
                            "name": repo,
                            "organization_id": entry.org_id
                        }));
                    }
                }
                (
                    200,
                    json!({
                        "version": 1,
                        "account_id": human_id,
                        "status": "registered",
                        "backend_authorized": false,
                        "organizations": orgs.into_iter().map(|id| json!({"id": id})).collect::<Vec<_>>(),
                        "projects": projects
                    }),
                )
            }
            "/rest/v1/rpc/dactyl_query_v1" => {
                let req = &body["request"];
                let Some(repo) = req
                    .pointer("/context/payload/route/Cloud/repository/canonical_name")
                    .and_then(Value::as_str)
                else {
                    return (
                        400,
                        json!({"code": "invalid_context", "message": "missing repository scope"}),
                    );
                };
                if let Err(err) = authorize_repo_data(&pol, human_id, repo) {
                    return err;
                }
                let Some(db_path) = Self::org_db_path_for_repo(org1_db, org2_db, repo) else {
                    return (
                        403,
                        json!({"code": "repository_not_authorized", "message": "unknown repository"}),
                    );
                };
                let sql = req["sql"].as_str().unwrap_or("");
                let params: Vec<Parameter> =
                    serde_json::from_value(req["params"].clone()).unwrap_or_default();
                let op = Operation::read(sql, params);
                execute_single_op(&db_path, repo, &op)
            }
            "/rest/v1/rpc/dactyl_read_batch_v1" | "/rest/v1/rpc/dactyl_batch_v1" => {
                let req = &body["request"];
                let Some(repo) = req
                    .pointer("/context/payload/route/Cloud/repository/canonical_name")
                    .and_then(Value::as_str)
                else {
                    return (
                        400,
                        json!({"code": "invalid_context", "message": "missing repository scope"}),
                    );
                };
                if let Err(err) = authorize_repo_data(&pol, human_id, repo) {
                    return err;
                }
                let Some(db_path) = Self::org_db_path_for_repo(org1_db, org2_db, repo) else {
                    return (
                        403,
                        json!({"code": "repository_not_authorized", "message": "unknown repository"}),
                    );
                };
                let ops: Vec<Operation> =
                    serde_json::from_value(req["operations"].clone()).unwrap_or_default();
                execute_batch_ops(&db_path, repo, &ops)
            }
            "/rest/v1/rpc/propodus_proof_acceptance_v1" => {
                let req = body.get("request").unwrap_or(body);
                let repo = req["project_id"].as_str().unwrap_or("");
                if let Err(err) = authorize_repo_data(&pol, human_id, repo) {
                    return err;
                }
                let Some(db_path) = Self::org_db_path_for_repo(org1_db, org2_db, repo) else {
                    return (
                        403,
                        json!({"code": "repository_not_authorized", "message": "unknown repository"}),
                    );
                };
                let op_id = req["operation_id"].as_str().unwrap_or("").to_string();
                let action = req["action"].as_str().unwrap_or("");
                let fp = format!(
                    "{:x}",
                    Sha256::digest(
                        json!({
                            "action": action,
                            "project_id": repo,
                            "body": req
                        })
                        .to_string()
                        .as_bytes()
                    )
                );
                if let Some((saved_repo, saved_fp, saved_receipt)) = pol.control_ops.get(&op_id) {
                    if saved_repo != repo || saved_fp != &fp {
                        return (
                            409,
                            json!({
                                "code": "idempotency_conflict",
                                "message": "operation_id cannot be replayed across repositories or changed payloads"
                            }),
                        );
                    }
                    return (200, saved_receipt.clone());
                }

                let conn = Connection::open(DatastoreRoute::sqlite(
                    db_path.to_string_lossy().to_string(),
                ))
                .unwrap();

                match action {
                    "custody_bind" => {
                        let task_id = req["task_id"].as_str().unwrap_or("");
                        let workspace_id = req["workspace_id"].as_str().unwrap_or("");
                        let custody_id = req["custody_id"].as_str().unwrap_or("");
                        let expected_rev = req["native_task_version"].as_i64().unwrap_or(0);
                        let rows = conn
                            .read(
                                "SELECT version FROM org_tasks WHERE repo_id = $1 AND id = $2",
                                &[repo.into(), task_id.into()],
                            )
                            .unwrap();
                        let Some(row) = rows.as_slice().first() else {
                            return (
                                404,
                                json!({"code": "task_not_found", "message": "task missing in authorized repository"}),
                            );
                        };
                        let live_rev = row.get::<_, i64>("version").unwrap();
                        if live_rev != expected_rev {
                            return (
                                409,
                                json!({"code": "version_conflict", "message": "stale task_revision in custody bind"}),
                            );
                        }
                        pol.fencing_counter += 1;
                        let fencing_gen = pol.fencing_counter;
                        let expires_at = decapod::core::time::format_rfc3339_nanos(
                            OffsetDateTime::now_utc() + time::Duration::hours(1),
                        )
                        .unwrap();
                        conn.write(
                            "INSERT OR REPLACE INTO org_custody (repo_id, task_id, workspace_id, custody_id, task_revision, fencing_generation, expires_at, operation_id) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                            &[
                                repo.into(),
                                task_id.into(),
                                workspace_id.into(),
                                custody_id.into(),
                                live_rev.into(),
                                fencing_gen.into(),
                                expires_at.clone().into(),
                                op_id.clone().into(),
                            ],
                        )
                        .unwrap();
                        let receipt = json!({
                            "version": 1,
                            "profile": "candidate-acceptance-v1",
                            "status": "custody_bound",
                            "operation_id": op_id,
                            "project_id": repo,
                            "task_id": task_id,
                            "workspace_id": workspace_id,
                            "custody_id": custody_id,
                            "task_revision": live_rev,
                            "fencing_generation": fencing_gen,
                            "authority_generation": 1,
                            "expires_at": expires_at,
                        });
                        pol.control_ops
                            .insert(op_id, (repo.to_string(), fp, receipt.clone()));
                        (200, receipt)
                    }
                    "capture" => {
                        let sub_raw = req["submission"].as_str().unwrap_or("");
                        let sub: Value = serde_json::from_str(sub_raw).unwrap_or(Value::Null);
                        let subject = &sub["subject"];
                        let task_id = subject["task_id"].as_str().unwrap_or("");
                        let sub_repo = subject["repository_id"].as_str().unwrap_or("");
                        if sub_repo != repo {
                            return (
                                403,
                                json!({"code": "repository_not_authorized", "message": "cross-repo proof capture rejected"}),
                            );
                        }
                        let expected_rev = req["native_task_version"].as_i64().unwrap_or(0);
                        let rows = conn
                            .read(
                                "SELECT version FROM org_tasks WHERE repo_id = $1 AND id = $2",
                                &[repo.into(), task_id.into()],
                            )
                            .unwrap();
                        let Some(row) = rows.as_slice().first() else {
                            return (
                                404,
                                json!({"code": "task_not_found", "message": "task not found"}),
                            );
                        };
                        let live_rev = row.get::<_, i64>("version").unwrap();
                        if live_rev != expected_rev {
                            return (
                                409,
                                json!({"code": "version_conflict", "message": "stale task_revision in proof capture"}),
                            );
                        }
                        let source_commit = subject["candidate"]["commit"].as_str().unwrap_or("");
                        let policy_revision = subject["policy_digest"].as_str().unwrap_or("");
                        conn.write(
                            "INSERT OR REPLACE INTO org_proofs (repo_id, task_id, operation_id, source_commit, policy_revision, task_revision, state) VALUES ($1, $2, $3, $4, $5, $6, 'captured')",
                            &[
                                repo.into(),
                                task_id.into(),
                                op_id.clone().into(),
                                source_commit.into(),
                                policy_revision.into(),
                                live_rev.into(),
                            ],
                        )
                        .unwrap();
                        let receipt = json!({
                            "version": 1,
                            "status": "captured",
                            "operation_id": op_id,
                            "project_id": repo,
                            "task_id": task_id,
                            "task_revision": live_rev,
                            "source_commit": source_commit,
                            "policy_revision": policy_revision,
                            "acceptance_supported": false,
                        });
                        pol.control_ops
                            .insert(op_id, (repo.to_string(), fp, receipt.clone()));
                        (200, receipt)
                    }
                    _ => (
                        400,
                        json!({"code": "invalid_request", "message": "unsupported control action"}),
                    ),
                }
            }
            "/rest/v1/rpc/propodus_proof_acceptance_get_v1" => {
                let req = body.get("request").unwrap_or(body);
                let repo = req["project_id"].as_str().unwrap_or("");
                if let Err(err) = authorize_repo_data(&pol, human_id, repo) {
                    return err;
                }
                let action = req["action"].as_str().unwrap_or("");
                if action == "get_operation" {
                    let op_id = req["operation_id"].as_str().unwrap_or("");
                    if let Some((saved_repo, _, receipt)) = pol.control_ops.get(op_id)
                        && saved_repo == repo
                    {
                        return (200, json!({"found": true, "receipt": receipt}));
                    }
                    return (
                        404,
                        json!({"code": "proof_acceptance_not_found", "message": "not found"}),
                    );
                }
                let Some(db_path) = Self::org_db_path_for_repo(org1_db, org2_db, repo) else {
                    return (
                        403,
                        json!({"code": "repository_not_authorized", "message": "unknown repository"}),
                    );
                };
                let conn = Connection::open(DatastoreRoute::sqlite(
                    db_path.to_string_lossy().to_string(),
                ))
                .unwrap();
                let task_id = req["task_id"].as_str().unwrap_or("");
                let custody_id = req["custody_id"].as_str().unwrap_or("");
                let rows = conn
                    .read(
                        "SELECT task_revision, fencing_generation, expires_at FROM org_custody WHERE repo_id = $1 AND task_id = $2 AND custody_id = $3",
                        &[repo.into(), task_id.into(), custody_id.into()],
                    )
                    .unwrap();
                let Some(row) = rows.as_slice().first() else {
                    return (
                        404,
                        json!({"code": "custody_not_found", "message": "custody binding missing"}),
                    );
                };
                (
                    200,
                    json!({
                        "version": 1,
                        "profile": "candidate-acceptance-v1",
                        "status": "custody_current",
                        "project_id": repo,
                        "task_id": task_id,
                        "workspace_id": req["workspace_id"],
                        "custody_id": custody_id,
                        "task_revision": row.get::<_, i64>("task_revision").unwrap(),
                        "fencing_generation": row.get::<_, i64>("fencing_generation").unwrap(),
                        "authority_generation": 1,
                        "expires_at": row.get_str("expires_at").unwrap(),
                    }),
                )
            }
            _ => (
                404,
                json!({"code": "not_found", "message": "unknown gateway route"}),
            ),
        }
    }
}

impl Drop for CloudTenancyGateway {
    fn drop(&mut self) {
        self.shutdown_network();
    }
}

fn authorize_repo_data(
    pol: &GatewayPolicyState,
    human_id: &str,
    repo: &str,
) -> Result<(), (u16, Value)> {
    let Some(entry) = pol
        .entitlements
        .get(&(human_id.to_string(), repo.to_string()))
    else {
        return Err((
            403,
            json!({"code": "repository_not_authorized", "message": "repository not authorized for caller"}),
        ));
    };
    if !entry.seat_assigned {
        return Err((
            403,
            json!({"code": "entitlement_required", "message": "paid repository seat required (admin status grants zero data access)"}),
        ));
    }
    Ok(())
}

fn init_org_db(path: &Path) {
    let conn = Connection::open(DatastoreRoute::sqlite(path.to_string_lossy().to_string()))
        .expect("init org db");
    conn.atomic(&[
        Operation::write(
            "CREATE TABLE IF NOT EXISTS org_tasks (
                repo_id TEXT NOT NULL,
                id TEXT NOT NULL,
                hash TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT,
                tags TEXT,
                owner TEXT,
                status TEXT NOT NULL,
                dir_path TEXT,
                scope TEXT,
                priority TEXT,
                category TEXT,
                assigned_to TEXT,
                assigned_at TEXT,
                completed_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                version INTEGER NOT NULL,
                PRIMARY KEY (repo_id, id)
            )",
            vec![],
        ),
        Operation::write(
            "CREATE TABLE IF NOT EXISTS org_events (
                repo_id TEXT NOT NULL,
                event_id TEXT NOT NULL,
                ts TEXT NOT NULL,
                seq INTEGER NOT NULL,
                stream TEXT NOT NULL,
                subject_kind TEXT NOT NULL,
                subject_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                actor TEXT NOT NULL,
                PRIMARY KEY (repo_id, event_id)
            )",
            vec![],
        ),
        Operation::write(
            "CREATE TABLE IF NOT EXISTS org_custody (
                repo_id TEXT NOT NULL,
                task_id TEXT NOT NULL,
                workspace_id TEXT NOT NULL,
                custody_id TEXT NOT NULL,
                task_revision INTEGER NOT NULL,
                fencing_generation INTEGER NOT NULL,
                expires_at TEXT NOT NULL,
                operation_id TEXT NOT NULL,
                PRIMARY KEY (repo_id, task_id)
            )",
            vec![],
        ),
        Operation::write(
            "CREATE TABLE IF NOT EXISTS org_proofs (
                repo_id TEXT NOT NULL,
                task_id TEXT NOT NULL,
                operation_id TEXT NOT NULL,
                source_commit TEXT NOT NULL,
                policy_revision TEXT NOT NULL,
                task_revision INTEGER NOT NULL,
                state TEXT NOT NULL,
                PRIMARY KEY (repo_id, operation_id)
            )",
            vec![],
        ),
    ])
    .expect("create org schema");
}

fn map_repo_scoped_op(authorized_repo: &str, op: &Operation) -> Result<Operation, (u16, Value)> {
    let sql = op.sql();
    let params = op.params();
    if sql.contains("FROM tasks ORDER BY updated_at DESC, id ASC") {
        return Ok(Operation::read(
            "SELECT repo_id, id, hash, title, description, status, assigned_to AS assignee, scope, dir_path, priority, category, tags, created_at, updated_at, version FROM org_tasks WHERE repo_id = $1 ORDER BY updated_at DESC, id ASC",
            vec![authorized_repo.into()],
        ));
    }
    if sql.contains("FROM tasks WHERE id = $1") {
        let mut p = params.to_vec();
        p.push(authorized_repo.into());
        return Ok(Operation::read(
            "SELECT repo_id, id, hash, title, description, status, assigned_to AS assignee, scope, dir_path, priority, category, tags, created_at, updated_at, version FROM org_tasks WHERE id = $1 AND repo_id = $2",
            p,
        ));
    }
    if sql.contains("FROM events WHERE event_id = $1") {
        let mut p = params.to_vec();
        p.push(authorized_repo.into());
        return Ok(Operation::read(
            "SELECT event_id, payload, subject_id FROM org_events WHERE event_id = $1 AND repo_id = $2",
            p,
        ));
    }
    if sql.starts_with("INSERT INTO tasks (") {
        if params.first() != Some(&Parameter::Text(authorized_repo.to_string())) {
            return Err((
                403,
                json!({"code": "repository_not_authorized", "message": "insert repo_id mismatch"}),
            ));
        }
        return Ok(Operation::write(
            sql.replacen("INSERT INTO tasks (", "INSERT INTO org_tasks (", 1),
            params.to_vec(),
        ));
    }
    if sql.starts_with("UPDATE tasks SET status = 'in_progress'") {
        let mut p = params.to_vec();
        p.push(authorized_repo.into());
        let rewritten = format!(
            "{} AND repo_id = $4",
            sql.replacen("UPDATE tasks ", "UPDATE org_tasks ", 1)
        );
        return Ok(Operation::write(rewritten, p));
    }
    if sql.starts_with("UPDATE tasks SET status = 'open'")
        || sql.starts_with("UPDATE tasks SET status = 'completed'")
    {
        let mut p = params.to_vec();
        p.push(authorized_repo.into());
        let rewritten = format!(
            "{} AND repo_id = $4",
            sql.replacen("UPDATE tasks ", "UPDATE org_tasks ", 1)
        );
        return Ok(Operation::write(rewritten, p));
    }
    if sql.contains("INSERT INTO events (event_id, ts, seq") {
        let mut p = params.to_vec();
        p.push(authorized_repo.into());
        let rewritten = "WITH event_params(event_id, event_ts, task_id, event_type, event_payload, event_actor) AS (
             VALUES ($1, $2, $3, $4, $5, $6)
         )
         INSERT INTO org_events (repo_id, event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
         SELECT $7,
                event_params.event_id,
                event_params.event_ts,
                COALESCE((SELECT MAX(seq) FROM org_events WHERE repo_id = $7 AND stream = 'todo'), 0) + 1,
                'todo',
                'task',
                event_params.task_id,
                event_params.event_type,
                event_params.event_payload,
                event_params.event_actor
         FROM event_params
         WHERE EXISTS (
             SELECT 1 FROM org_tasks
             WHERE org_tasks.repo_id = $7
               AND org_tasks.id = event_params.task_id
               AND org_tasks.updated_at = event_params.event_ts
               AND (
                   (event_params.event_type = 'task.add' AND org_tasks.version = 1)
                   OR (event_params.event_type = 'task.claim' AND org_tasks.status = 'in_progress' AND org_tasks.assigned_to = event_params.event_actor)
                   OR (event_params.event_type = 'task.release' AND org_tasks.status = 'open' AND (org_tasks.assigned_to = '' OR org_tasks.assigned_to IS NULL))
                   OR (event_params.event_type = 'task.done' AND org_tasks.status = 'completed' AND org_tasks.assigned_to = event_params.event_actor)
               )
         )";
        return Ok(Operation::write(rewritten, p));
    }
    Err((
        400,
        json!({"code": "unsupported_query", "message": "unsupported gateway SQL statement"}),
    ))
}

fn rows_to_rpc_json(rows: dactyl_db::Rows) -> Value {
    let columns = rows
        .as_slice()
        .first()
        .map(|r| r.columns.clone())
        .unwrap_or_default();
    let encoded_rows: Vec<Value> = rows
        .as_slice()
        .iter()
        .map(|row| {
            let mut obj = serde_json::Map::new();
            for (col, val) in row.columns.iter().zip(row.values.iter()) {
                obj.insert(col.clone(), val.clone());
            }
            Value::Object(obj)
        })
        .collect();
    json!({
        "columns": columns,
        "rows": encoded_rows,
        "affected_rows": 0,
        "generated_keys": []
    })
}

fn execute_single_op(db_path: &Path, repo: &str, op: &Operation) -> (u16, Value) {
    let mapped = match map_repo_scoped_op(repo, op) {
        Ok(m) => m,
        Err(err) => return err,
    };
    let conn = Connection::open(DatastoreRoute::sqlite(
        db_path.to_string_lossy().to_string(),
    ))
    .expect("open org db");
    match conn.read(mapped.sql(), mapped.params()) {
        Ok(rows) => (200, rows_to_rpc_json(rows)),
        Err(err) => (
            500,
            json!({"code": "storage_failure", "message": err.to_string()}),
        ),
    }
}

fn execute_batch_ops(db_path: &Path, repo: &str, ops: &[Operation]) -> (u16, Value) {
    let mut mapped = Vec::with_capacity(ops.len());
    for op in ops {
        match map_repo_scoped_op(repo, op) {
            Ok(m) => mapped.push(m),
            Err(err) => return err,
        }
    }
    let conn = Connection::open(DatastoreRoute::sqlite(
        db_path.to_string_lossy().to_string(),
    ))
    .expect("open org db");
    match conn.atomic(&mapped) {
        Ok(atomic_res) => {
            let mut results = Vec::with_capacity(mapped.len());
            for (op, res) in mapped.iter().zip(atomic_res.results) {
                match (op.kind(), res) {
                    (OperationKind::Read, decapod::core::dactyl::OperationResult::Rows(rows)) => {
                        results.push(rows_to_rpc_json(rows));
                    }
                    (_, decapod::core::dactyl::OperationResult::Write(w)) => {
                        results.push(json!({
                            "columns": [],
                            "rows": [],
                            "affected_rows": w.affected_rows,
                            "generated_keys": []
                        }));
                    }
                    _ => {
                        return (
                            500,
                            json!({"code": "storage_failure", "message": "unexpected batch result shape"}),
                        );
                    }
                }
            }
            (200, json!({ "results": results }))
        }
        Err(err) => (
            409,
            json!({"code": "row_conflict", "message": err.to_string()}),
        ),
    }
}

fn decapod_bin() -> PathBuf {
    fs::canonicalize(env!("CARGO_BIN_EXE_decapod")).expect("decapod binary")
}

fn run_git(dir: &Path, args: &[&str]) -> Output {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git must execute");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn init_cloud_repo(dir: &Path, repo_slug: &str) -> String {
    run_git(dir, &["init", "-b", "master"]);
    run_git(dir, &["config", "user.name", "Cloud Tenancy Test"]);
    run_git(dir, &["config", "user.email", "tenancy@example.invalid"]);
    run_git(
        dir,
        &[
            "remote",
            "add",
            "origin",
            &format!("https://github.com/{repo_slug}.git"),
        ],
    );
    fs::create_dir_all(dir.join(".decapod")).unwrap();
    fs::write(
        dir.join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\nproduct_summary = \"Cloud tenancy test repo\"\narchitecture_direction = \"cli\"\ncontainer_workspaces = false\n[custody]\nisolation = \"worktree\"\n",
    )
    .unwrap();
    fs::write(dir.join("README.md"), format!("# {repo_slug}\n")).unwrap();
    run_git(dir, &["add", "-A"]);
    run_git(dir, &["commit", "-m", "init cloud repo"]);
    let head = run_git(dir, &["rev-parse", "HEAD"]);
    String::from_utf8(head.stdout).unwrap().trim().to_string()
}

fn run_cli(dir: &Path, machine: &Path, origin: &str, agent_id: &str, args: &[&str]) -> Output {
    let out = Command::new(decapod_bin())
        .current_dir(dir)
        .env("HOME", machine)
        .env("XDG_CONFIG_HOME", machine.join("config"))
        .env("XDG_DATA_HOME", machine.join("data"))
        .env("DECAPOD_TEST_MODE", "1")
        .env("DECAPOD_CLOUD_TEST_ORIGIN", origin)
        .env("DECAPOD_HEADLESS", "1")
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env("DECAPOD_GROUP_BROKER_DISABLE", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("DECAPOD_ACCESS_TOKEN")
        .env_remove("DECAPOD_CLOUD_AUTH_MODE")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("execute decapod");

    assert_no_placement_leaks(&out.stdout);
    assert_no_placement_leaks(&out.stderr);
    out
}

fn assert_no_placement_leaks(bytes: &[u8]) {
    let text = String::from_utf8_lossy(bytes);
    for secret in [
        OPAQUE_DB_1_FILENAME,
        OPAQUE_DB_2_FILENAME,
        OPAQUE_DB_ROLE_1,
        OPAQUE_DB_ROLE_2,
        OPAQUE_DB_URL_1,
        OPAQUE_DB_URL_2,
        OPAQUE_PLACEMENT_GEN,
    ] {
        assert!(
            !text.contains(secret),
            "client artifact or output leaked opaque server placement '{secret}'"
        );
    }
}

fn assert_no_local_sqlite(paths: &[&Path]) {
    for root in paths {
        for rel in [".decapod/data/decapod.db", ".decapod/data/todo.db"] {
            assert!(
                !root.join(rel).exists(),
                "cloud mode must never create or fall back to {}",
                root.join(rel).display()
            );
        }
    }
}

fn seed_human_session(
    repo_dir: &Path,
    machine_dir: &Path,
    origin: &str,
    repo_slug: &str,
    token: &str,
) {
    managed::seed_ready(
        repo_dir,
        machine_dir,
        origin,
        repo_slug,
        Session {
            access_token: token.to_string(),
            refresh_token: Some(format!("{token}-refresh")),
            expires_at: OffsetDateTime::now_utc().unix_timestamp() + 3600,
        },
    );
}

fn cloud_store(origin: &str, repo_slug: &str, token: &str) -> DactylTodoStore {
    let (owner, repo) = repo_slug.split_once('/').unwrap();
    let identity = RepositoryIdentity {
        canonical_name: repo_slug.to_string(),
        owner: owner.to_string(),
        repository: repo.to_string(),
        remote_url: format!("https://github.com/{repo_slug}.git"),
    };
    let route = BackendRoute::cloud(identity, origin).unwrap();
    let context = StorageContext::from_route(route, Some(token))
        .unwrap()
        .with_cloud_datastore(CloudDatastore::Managed)
        .unwrap();
    DactylTodoStore::new(context, repo_slug)
}

#[derive(Clone)]
struct GatewayTransport {
    origin: String,
    token: String,
}

impl ProofControlTransport for GatewayTransport {
    fn write_control(
        &self,
        op_name: &str,
        payload: &Value,
    ) -> Result<Value, decapod::core::cloud_proof::CloudControlError> {
        reqwest_blocking_post(
            &format!("{}/rest/v1/rpc/{op_name}", self.origin),
            &self.token,
            &json!({ "request": payload }),
        )
    }

    fn read_control(
        &self,
        op_name: &str,
        payload: &Value,
    ) -> Result<Value, decapod::core::cloud_proof::CloudControlError> {
        reqwest_blocking_post(
            &format!("{}/rest/v1/rpc/{op_name}", self.origin),
            &self.token,
            &json!({ "request": payload }),
        )
    }
}

#[allow(clippy::disallowed_methods)]
fn reqwest_blocking_post(
    url: &str,
    token: &str,
    body: &Value,
) -> Result<Value, decapod::core::cloud_proof::CloudControlError> {
    use decapod::core::cloud_proof::{CloudControlError, classify_control_status};
    let rest = url.strip_prefix("http://").unwrap();
    let (addr, path) = rest.split_once('/').unwrap();
    let mut stream = match TcpStream::connect(addr) {
        Ok(s) => s,
        Err(err) => {
            return Err(classify_control_status(
                None,
                "network_unavailable",
                &err.to_string(),
                false,
            ));
        }
    };
    let body_bytes = serde_json::to_vec(body).unwrap();
    let req = format!(
        "POST /{path} HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );
    if let Err(err) = stream.write_all(req.as_bytes()) {
        return Err(classify_control_status(
            None,
            "outcome_unknown",
            &err.to_string(),
            true,
        ));
    }
    if let Err(err) = stream.write_all(&body_bytes) {
        return Err(classify_control_status(
            None,
            "outcome_unknown",
            &err.to_string(),
            true,
        ));
    }
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    let raw_str = String::from_utf8_lossy(&raw);
    let Some((head, resp_body)) = raw_str.split_once("\r\n\r\n") else {
        return Err(classify_control_status(
            None,
            "outcome_unknown",
            "severed response",
            true,
        ));
    };
    let status: u16 = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);
    let parsed: Value = serde_json::from_str(resp_body).unwrap_or(Value::Null);
    if (200..300).contains(&status) {
        Ok(parsed)
    } else {
        let code = parsed["code"].as_str().unwrap_or("control_denied");
        let detail = parsed["message"].as_str().unwrap_or("control rejected");
        let err: CloudControlError = classify_control_status(Some(status), code, detail, false);
        Err(err)
    }
}

#[test]
fn multi_org_cloud_tenancy_parity_and_no_sqlite_fallback() {
    let mut gw = CloudTenancyGateway::start();
    let tmp = TempDir::new().unwrap();

    let repo_1a = tmp.path().join("repo_1a");
    let repo_1b = tmp.path().join("repo_1b");
    let repo_2a = tmp.path().join("repo_2a");
    let repo_2b = tmp.path().join("repo_2b");
    for d in [&repo_1a, &repo_1b, &repo_2a, &repo_2b] {
        fs::create_dir_all(d).unwrap();
    }
    let head_1a = init_cloud_repo(&repo_1a, REPO_1A);
    let _head_1b = init_cloud_repo(&repo_1b, REPO_1B);
    let _head_2a = init_cloud_repo(&repo_2a, REPO_2A);
    let _head_2b = init_cloud_repo(&repo_2b, REPO_2B);

    // Multi-worktree setup on Repo 1A for Agent 1 and Agent 2
    let wt_1a_agent2 = tmp.path().join("repo_1a_wt_agent2");
    run_git(
        &repo_1a,
        &[
            "worktree",
            "add",
            "-b",
            "agent/agent-beta/work",
            wt_1a_agent2.to_str().unwrap(),
        ],
    );

    let machine_a = tmp.path().join("machine_human_a");
    let machine_b = tmp.path().join("machine_human_b");
    fs::create_dir_all(&machine_a).unwrap();
    fs::create_dir_all(&machine_b).unwrap();

    // Seed Human A sessions across all 4 repos and Human B across repos
    for (dir, slug) in [
        (&repo_1a, REPO_1A),
        (&repo_1b, REPO_1B),
        (&repo_2a, REPO_2A),
        (&repo_2b, REPO_2B),
    ] {
        seed_human_session(dir, &machine_a, &gw.origin, slug, TOKEN_A);
        seed_human_session(dir, &machine_b, &gw.origin, slug, TOKEN_B);
    }

    assert_no_local_sqlite(&[&repo_1a, &repo_1b, &repo_2a, &repo_2b, &wt_1a_agent2]);

    // 1. Human A lists projects and switches cleanly across Org 1 (Repo 1A) and Org 2 (Repo 2A).
    let list_a = run_cli(
        &repo_1a,
        &machine_a,
        &gw.origin,
        "agent-alpha",
        &["cloud", "--format", "json", "projects"],
    );
    assert!(
        list_a.status.success(),
        "{}",
        String::from_utf8_lossy(&list_a.stderr)
    );
    let use_1a = run_cli(
        &repo_1a,
        &machine_a,
        &gw.origin,
        "agent-alpha",
        &[
            "cloud", "--format", "json", "project", "use", REPO_1A, "--org", ORG1_ID,
        ],
    );
    assert!(
        use_1a.status.success(),
        "{}",
        String::from_utf8_lossy(&use_1a.stderr)
    );
    // Cross-org selection collision must fail closed!
    let wrong_org = run_cli(
        &repo_1a,
        &machine_a,
        &gw.origin,
        "agent-alpha",
        &[
            "cloud", "--format", "json", "project", "use", REPO_1A, "--org", ORG2_ID,
        ],
    );
    assert!(
        !wrong_org.status.success(),
        "wrong org selection must fail closed"
    );

    let use_2a = run_cli(
        &repo_2a,
        &machine_a,
        &gw.origin,
        "agent-alpha",
        &[
            "cloud", "--format", "json", "project", "use", REPO_2A, "--org", ORG2_ID,
        ],
    );
    assert!(
        use_2a.status.success(),
        "{}",
        String::from_utf8_lossy(&use_2a.stderr)
    );

    // 2. Human B is denied Org 2 and denied Repo 1B in Org 1.
    let b_org2 = run_cli(
        &repo_2a,
        &machine_b,
        &gw.origin,
        "agent-beta",
        &[
            "cloud", "--format", "json", "project", "use", REPO_2A, "--org", ORG2_ID,
        ],
    );
    assert!(!b_org2.status.success(), "Human B must be denied Org 2");

    let b_todo_org2 = run_cli(
        &repo_2a,
        &machine_b,
        &gw.origin,
        "agent-beta",
        &["todo", "--format", "json", "list"],
    );
    assert!(
        !b_todo_org2.status.success(),
        "Human B must be denied Org 2 todo list"
    );

    let b_todo_1b = run_cli(
        &repo_1b,
        &machine_b,
        &gw.origin,
        "agent-beta",
        &["todo", "--format", "json", "list"],
    );
    assert!(
        !b_todo_1b.status.success(),
        "Human B must be denied Org 1 Repo 1B"
    );

    // 3. #215: Human A is Org 1 admin with ZERO paid seat on Repo 1B.
    // Org-admin inspection succeeds with admin=true, repo_assigned=false, backend_authorized=false;
    // yet Decapod governance project selection & todo read/write on Repo 1B FAIL CLOSED!
    let admin_check = reqwest_blocking_post(
        &format!(
            "{}/functions/v1/decapod-onboarding/account-status",
            gw.origin
        ),
        TOKEN_A,
        &json!({
            "version": 1,
            "organization_id": ORG1_ID,
            "target_repo": REPO_1B,
            "admin_inspection": true
        }),
    )
    .expect("org admin inspection must succeed");
    assert_eq!(admin_check["admin"], true);
    assert_eq!(admin_check["repo_assigned"], false);
    assert_eq!(admin_check["backend_authorized"], false);

    let a_use_1b = run_cli(
        &repo_1b,
        &machine_a,
        &gw.origin,
        "agent-alpha",
        &[
            "cloud", "--format", "json", "project", "use", REPO_1B, "--org", ORG1_ID,
        ],
    );
    assert!(
        !a_use_1b.status.success(),
        "Org admin without paid seat on Repo 1B must be denied project use"
    );
    let combined_1b = format!(
        "{}{}",
        String::from_utf8_lossy(&a_use_1b.stdout),
        String::from_utf8_lossy(&a_use_1b.stderr)
    );
    assert!(
        combined_1b.contains("admin=true, repo_assigned=false")
            || combined_1b.contains("paid seat")
            || combined_1b.contains("admin_approval_required"),
        "expected #215 admin-vs-seat diagnostic, got: {combined_1b}"
    );

    let a_todo_1b_read = run_cli(
        &repo_1b,
        &machine_a,
        &gw.origin,
        "agent-alpha",
        &["todo", "--format", "json", "list"],
    );
    assert!(
        !a_todo_1b_read.status.success(),
        "Org admin without paid seat denied governance read on Repo 1B"
    );
    let a_todo_1b_write = run_cli(
        &repo_1b,
        &machine_a,
        &gw.origin,
        "agent-alpha",
        &["todo", "--format", "json", "add", "Should be denied"],
    );
    assert!(
        !a_todo_1b_write.status.success(),
        "Org admin without paid seat denied governance write on Repo 1B"
    );

    assert_no_local_sqlite(&[&repo_1a, &repo_1b, &repo_2a, &repo_2b, &wt_1a_agent2]);

    // 4. Multi-agent, multi-worktree execution in Org 1 Repo 1A + Org 2 Repo 2A/2B
    let add_1a = run_cli(
        &repo_1a,
        &machine_a,
        &gw.origin,
        "agent-alpha",
        &["todo", "--format", "json", "add", "Org1 RepoA shared task"],
    );
    assert!(
        add_1a.status.success(),
        "add_1a failed: {}{}",
        String::from_utf8_lossy(&add_1a.stdout),
        String::from_utf8_lossy(&add_1a.stderr)
    );
    let add_1a_json: Value = serde_json::from_slice(&add_1a.stdout).unwrap();
    let task_1a_id = add_1a_json["id"].as_str().unwrap().to_string();

    // Agent 2 in second worktree claims, or concurrent CAS race via DactylTodoStore
    let add_2a = run_cli(
        &repo_2a,
        &machine_a,
        &gw.origin,
        "agent-alpha",
        &[
            "todo",
            "--format",
            "json",
            "add",
            "Org2 RepoA isolated task",
        ],
    );
    assert!(
        add_2a.status.success(),
        "add_2a failed: {}",
        String::from_utf8_lossy(&add_2a.stderr)
    );
    let task_2a_id = serde_json::from_slice::<Value>(&add_2a.stdout).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Repo 2A list does NOT see Repo 1A task, and Repo 1A list does NOT see Repo 2A task.
    let list_1a_wt2 = run_cli(
        &wt_1a_agent2,
        &machine_a,
        &gw.origin,
        "agent-beta",
        &["todo", "--format", "json", "list"],
    );
    assert!(list_1a_wt2.status.success());
    let items_1a = serde_json::from_slice::<Value>(&list_1a_wt2.stdout).unwrap()["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(items_1a.len(), 1);
    assert_eq!(items_1a[0]["id"], task_1a_id);

    // Concurrent CAS claim race on Org 1 Repo 1A task
    let store_a = cloud_store(&gw.origin, REPO_1A, TOKEN_A);
    let store_b = cloud_store(&gw.origin, REPO_1A, TOKEN_B);
    let tid = task_1a_id.clone();
    let tid2 = task_1a_id.clone();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let b1 = Arc::clone(&barrier);
    let b2 = Arc::clone(&barrier);
    let h1 = std::thread::spawn(move || {
        b1.wait();
        store_a.claim_task_with_lease_blocking(&tid, "agent-alpha".into(), 300)
    });
    let h2 = std::thread::spawn(move || {
        b2.wait();
        store_b.claim_task_with_lease_blocking(&tid2, "agent-beta".into(), 300)
    });
    let c1 = h1.join().unwrap();
    let c2 = h2.join().unwrap();
    let winners: Vec<Task> = [&c1, &c2]
        .into_iter()
        .filter_map(|r| r.as_ref().ok().cloned())
        .collect();
    assert_eq!(
        winners.len(),
        1,
        "CAS race must admit exactly one winner (c1={c1:?}, c2={c2:?})"
    );
    let winner_actor = winners[0].assignee.clone().unwrap();
    let winner_token = if winner_actor == "agent-alpha" {
        TOKEN_A
    } else {
        TOKEN_B
    };
    let loser_actor = if winner_actor == "agent-alpha" {
        "agent-beta"
    } else {
        "agent-alpha"
    };

    // Non-holder release & completion must fail closed
    let loser_store = cloud_store(&gw.origin, REPO_1A, TOKEN_A);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert!(
        rt.block_on(loser_store.release_task(&task_1a_id, loser_actor.into()))
            .is_err()
    );
    assert!(
        rt.block_on(loser_store.complete_task(&task_1a_id, loser_actor.into(), "hack".into()))
            .is_err()
    );

    // Custody bind on winner worktree + commit/policy-bound proof capture
    let custody_dir = tmp.path().join("custody_store");
    let custody_store = CloudCustodyStore::at(&custody_dir);
    let transport_a = GatewayTransport {
        origin: gw.origin.clone(),
        token: winner_token.to_string(),
    };
    let (custody_rec, custody_receipt) = custody_bind_or_reconcile(
        &transport_a,
        &custody_store,
        &repo_1a,
        &CloudCustodyBindParams {
            origin: gw.origin.clone(),
            project_id: REPO_1A.to_string(),
            task_id: task_1a_id.clone(),
            task_title: "Org1 RepoA shared task".to_string(),
            native_task_version: i64::from(winners[0].version),
            workspace_id: "ws-repo1a-winner".to_string(),
            worktree_path: repo_1a.clone(),
            branch: "agent/winner/work".to_string(),
            actor: winner_actor.clone(),
            ttl_seconds: 1800,
            invocation_id: None,
            container_expected: false,
        },
    )
    .expect("custody bind succeeds");
    assert_eq!(custody_receipt.task_revision, i64::from(winners[0].version));

    // Replay same operation_id across a DIFFERENT repo (Repo 2A) -> must fail closed!
    let bound_op_id = {
        let pol = gw.policy.lock().unwrap();
        pol.control_ops.keys().next().cloned().unwrap()
    };
    let cross_repo_replay = transport_a.write_control(
        PROOF_ACCEPTANCE_CONTROL_OP,
        &json!({
            "version": 1,
            "action": "custody_bind",
            "project_id": REPO_2A,
            "operation_id": bound_op_id,
            "task_id": task_2a_id,
            "workspace_id": "ws-evil",
            "custody_id": "custody-evil",
            "native_task_version": 1,
            "ttl_seconds": 1800
        }),
    );
    assert!(
        cross_repo_replay.is_err(),
        "replaying operation_id across repos must fail closed"
    );

    let evidence_digest = format!(
        "sha256:{:x}",
        Sha256::digest(b"cloud-parity-proof-evidence")
    );
    let proof_ev = vec![ProofEvidenceReference {
        kind: "validation_report".into(),
        digest: evidence_digest.clone(),
        bytes: 128,
        provenance_digest: evidence_digest,
    }];

    // Stale task_revision in proof capture must fail closed!
    let stale_capture = submit_capture(
        &transport_a,
        REPO_1A,
        999,
        &ProofCaptureRequest {
            contract: "propodus.proof-capture.v1".into(),
            contract_digest: "sha256:0000".into(),
            operation_id: "op-stale-rev".into(),
            subject: ProofSubject {
                repository_id: REPO_1A.into(),
                task_id: task_1a_id.clone(),
                workspace_id: "ws-repo1a-winner".into(),
                custody_id: custody_rec.custody_id.clone(),
                task_revision: 999,
                fencing_generation: custody_receipt.fencing_generation,
                candidate: ProofGitIdentity {
                    commit: head_1a.clone(),
                    tree: "tree".into(),
                },
                accepted_target: ProofGitIdentity {
                    commit: head_1a.clone(),
                    tree: "tree".into(),
                },
                integration: None,
                policy_digest: "sha256:policy-v1".into(),
                requirements_digest: "sha256:req-v1".into(),
                material_digest: "sha256:mat-v1".into(),
                prerequisites_digest: "sha256:pre-v1".into(),
            },
            evidence: proof_ev.clone(),
        },
    );
    assert!(
        stale_capture.is_err(),
        "stale task_revision must fail closed"
    );

    // Valid commit-and-policy-bound proof capture succeeds!
    let valid_capture = submit_capture(
        &transport_a,
        REPO_1A,
        i64::from(winners[0].version),
        &ProofCaptureRequest {
            contract: "propodus.proof-capture.v1".into(),
            contract_digest: "sha256:0000".into(),
            operation_id: "op-valid-capture-1a".into(),
            subject: ProofSubject {
                repository_id: REPO_1A.into(),
                task_id: task_1a_id.clone(),
                workspace_id: "ws-repo1a-winner".into(),
                custody_id: custody_rec.custody_id.clone(),
                task_revision: i64::from(winners[0].version),
                fencing_generation: custody_receipt.fencing_generation,
                candidate: ProofGitIdentity {
                    commit: head_1a.clone(),
                    tree: "tree".into(),
                },
                accepted_target: ProofGitIdentity {
                    commit: head_1a.clone(),
                    tree: "tree".into(),
                },
                integration: None,
                policy_digest: "sha256:policy-v1".into(),
                requirements_digest: "sha256:req-v1".into(),
                material_digest: "sha256:mat-v1".into(),
                prerequisites_digest: "sha256:pre-v1".into(),
            },
            evidence: proof_ev.clone(),
        },
    )
    .expect("commit-bound proof capture succeeds");
    assert_eq!(valid_capture["status"], "captured");
    assert_eq!(valid_capture["acceptance_supported"], false);

    // Fabricated / mismatched repo_id inside ProofCaptureRequest rejected by server
    let fabricated_capture = submit_capture(
        &transport_a,
        REPO_2A,
        1,
        &ProofCaptureRequest {
            contract: "propodus.proof-capture.v1".into(),
            contract_digest: "sha256:0000".into(),
            operation_id: "op-fabricated-scope".into(),
            subject: ProofSubject {
                repository_id: REPO_1A.into(),
                task_id: task_1a_id.clone(),
                workspace_id: "ws-repo1a-winner".into(),
                custody_id: custody_rec.custody_id.clone(),
                task_revision: 1,
                fencing_generation: custody_receipt.fencing_generation,
                candidate: ProofGitIdentity {
                    commit: head_1a.clone(),
                    tree: "tree".into(),
                },
                accepted_target: ProofGitIdentity {
                    commit: head_1a.clone(),
                    tree: "tree".into(),
                },
                integration: None,
                policy_digest: "sha256:policy-v1".into(),
                requirements_digest: "sha256:req-v1".into(),
                material_digest: "sha256:mat-v1".into(),
                prerequisites_digest: "sha256:pre-v1".into(),
            },
            evidence: proof_ev,
        },
    );
    assert!(
        fabricated_capture.is_err(),
        "fabricated mismatched repo scope must fail closed"
    );

    // Complete task as winner
    let winner_store = cloud_store(&gw.origin, REPO_1A, winner_token);
    let completed = rt
        .block_on(winner_store.complete_task(&task_1a_id, winner_actor.clone(), "done".into()))
        .expect("winner completes task");
    assert_eq!(completed.status, "completed");
    assert_eq!(completed.version, 3);

    // Verify hosted renew without active custody / non-holder fails closed without SQLite fallback
    let renew_err = winner_store
        .renew_task_lease_blocking(&task_1a_id, winner_actor.clone(), 120)
        .expect_err("cloud lease renew without active bound custody must fail closed");
    assert!(
        renew_err.to_string().contains("LEASE_RENEW_DENIED")
            || renew_err.to_string().contains("CLOUD_CUSTODY_UNBOUND"),
        "unexpected renew_err: {renew_err}"
    );
    let edit_err = run_cli(
        &repo_1a,
        &machine_a,
        &gw.origin,
        &winner_actor,
        &["todo", "edit", "--id", &task_1a_id, "--title", "nope"],
    );
    assert!(
        !edit_err.status.success(),
        "todo.edit must fail closed in cloud mode"
    );

    // 5. Independent Observer verifies exact DB separation, logical repo isolation, ordered events, lease/CAS, and proof!
    let obs_1a_tasks = gw.observer_tasks(ORG1_ID, REPO_1A);
    assert_eq!(obs_1a_tasks.len(), 1);
    assert_eq!(obs_1a_tasks[0].0, task_1a_id);
    assert_eq!(obs_1a_tasks[0].1, "completed");
    assert_eq!(obs_1a_tasks[0].3, 3);
    assert!(gw.observer_tasks(ORG1_ID, REPO_1B).is_empty());
    assert_eq!(gw.observer_tasks(ORG2_ID, REPO_2A).len(), 1);
    assert_eq!(gw.observer_tasks(ORG2_ID, REPO_2A)[0].0, task_2a_id);
    assert!(gw.observer_tasks(ORG2_ID, REPO_2B).is_empty());

    let obs_1a_events = gw.observer_events(ORG1_ID, REPO_1A);
    assert_eq!(
        obs_1a_events
            .iter()
            .map(|(seq, etype, _, _)| (*seq, etype.as_str()))
            .collect::<Vec<_>>(),
        vec![(1, "task.add"), (2, "task.claim"), (3, "task.done")]
    );
    let obs_1a_custody = gw.observer_custody(ORG1_ID, REPO_1A);
    assert_eq!(obs_1a_custody.len(), 1);
    assert_eq!(obs_1a_custody[0].0, task_1a_id);
    let obs_1a_proofs = gw.observer_proofs(ORG1_ID, REPO_1A);
    assert_eq!(obs_1a_proofs.len(), 1);
    assert_eq!(obs_1a_proofs[0].1, head_1a);
    assert_eq!(obs_1a_proofs[0].2, "sha256:policy-v1");
    assert_eq!(obs_1a_proofs[0].4, "captured");

    // 6. Mid-session seat revocation on Repo 2A fails closed immediately across CLI restarts!
    gw.revoke_seat(HUMAN_A_ID, REPO_2A);
    let revoked_read = run_cli(
        &repo_2a,
        &machine_a,
        &gw.origin,
        "agent-alpha",
        &["todo", "--format", "json", "list"],
    );
    assert!(
        !revoked_read.status.success(),
        "revoked seat must fail closed"
    );
    let revoked_write = run_cli(
        &repo_2a,
        &machine_a,
        &gw.origin,
        "agent-alpha",
        &["todo", "--format", "json", "add", "After revocation"],
    );
    assert!(
        !revoked_write.status.success(),
        "revoked seat write must fail closed"
    );

    // 7. Network cut fails closed across CLI restarts and multiple worktrees with ZERO SQLite fallback!
    gw.shutdown_network();
    for dir in [&repo_1a, &wt_1a_agent2, &repo_2a] {
        let offline_read = run_cli(
            dir,
            &machine_a,
            &gw.origin,
            "agent-alpha",
            &["todo", "--format", "json", "list"],
        );
        assert!(
            !offline_read.status.success(),
            "network cut must fail closed"
        );
        let offline_write = run_cli(
            dir,
            &machine_a,
            &gw.origin,
            "agent-beta",
            &["todo", "--format", "json", "add", "Offline write denied"],
        );
        assert!(
            !offline_write.status.success(),
            "network cut write must fail closed"
        );
    }
    assert_no_local_sqlite(&[&repo_1a, &repo_1b, &repo_2a, &repo_2b, &wt_1a_agent2]);

    // Verify custody files & session files never contain opaque server placement secrets.
    for entry in walkdir::WalkDir::new(tmp.path())
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        if let Ok(bytes) = fs::read(entry.path()) {
            assert_no_placement_leaks(&bytes);
        }
    }

    // 8. Cross-check parity matrix CLI & RPC against actual local SQLite execution for supported subset!
    let parity_cli = run_cli(
        &repo_1a,
        &machine_a,
        &gw.origin,
        "agent-alpha",
        &["capabilities", "parity", "--format", "json"],
    );
    assert!(
        parity_cli.status.success(),
        "capabilities parity failed: {}",
        String::from_utf8_lossy(&parity_cli.stderr)
    );
    let parity_report: Value = serde_json::from_slice(&parity_cli.stdout).unwrap();
    assert_eq!(parity_report["repository_backend"], "cloud");
    assert_eq!(parity_report["local_sqlite_path_present"], false);
    assert_eq!(parity_report["blanket_parity_claimed"], false);
    assert_eq!(parity_report["cross_check"]["consistent"], true);

    let parity_rpc = run_cli(
        &repo_1a,
        &machine_a,
        &gw.origin,
        "agent-alpha",
        &["rpc", "--op", "interop.cloud_parity", "--params", "{}"],
    );
    assert!(
        parity_rpc.status.success(),
        "interop.cloud_parity RPC failed: {}",
        String::from_utf8_lossy(&parity_rpc.stderr)
    );
    let parity_rpc_json: Value = serde_json::from_slice(&parity_rpc.stdout).unwrap();
    assert_eq!(parity_rpc_json["success"], true);
    assert_eq!(
        parity_rpc_json["result"]["inspection"]["matrix"]["matrix_sha256"],
        parity_report["matrix"]["matrix_sha256"]
    );
    assert_no_local_sqlite(&[&repo_1a, &repo_1b, &repo_2a, &repo_2b, &wt_1a_agent2]);

    // Cross-check logical operation parity on local SQLite for the supported subset
    let matrix = build_parity_matrix();
    assert!(cross_check_parity_matrix(&matrix).consistent);
    let local_repo = tmp.path().join("local_parity_repo");
    fs::create_dir_all(local_repo.join(".decapod/data")).unwrap();
    let local_store_root = local_repo.join(".decapod/data");
    decapod::core::todo::initialize_todo_db(&local_store_root).unwrap();

    let local_add = add_task(
        &local_store_root,
        &TodoCommand::Add {
            title: "Local parity task".into(),
            description: String::new(),
            priority: "medium".into(),
            tags: String::new(),
            owner: "agent-alpha".into(),
            due: None,
            r#ref: String::new(),
            scope: "repo".into(),
            dir: None,
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .expect("local add");
    let local_id = local_add["id"].as_str().unwrap().to_string();
    let local_listed = list_tasks(&local_store_root, None, None, None, None, None).unwrap();
    assert!(local_listed.iter().any(|t| t.id == local_id));
    let local_got = get_task(&local_store_root, &local_id).unwrap().unwrap();
    assert_eq!(local_got.id, local_id);
    let local_claim = claim_task(
        &local_store_root,
        &local_id,
        "agent-alpha",
        ClaimMode::Exclusive,
    )
    .unwrap();
    assert_eq!(local_claim["status"], "ok");

    // Verify matrix includes all supported logical parity operations exercised above
    let logical_ops: BTreeSet<&str> = matrix
        .operations
        .iter()
        .filter(|e| e.support_class == ParitySupportClass::SupportedLogicalParity)
        .map(|e| e.operation_id.as_str())
        .collect();
    for expected in [
        "todo.add",
        "todo.list",
        "todo.get",
        "todo.claim",
        "todo.release",
        "todo.done",
        "workspace.custody.bind",
        "proof.lifecycle.bind",
    ] {
        assert!(
            logical_ops.contains(expected),
            "parity matrix missing supported logical operation {expected}"
        );
    }
}
