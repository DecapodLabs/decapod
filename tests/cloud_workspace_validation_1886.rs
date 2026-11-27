//! Integration proofs for #1886: accepted Cloud repositories keep
//! `workspace ensure`, `rpc --op workspace.ensure`, `validate`, `rpc --op validate.run`,
//! and container handoffs operational without local `decapod.db` fallback.
#![cfg(feature = "supabase-cloud")]
#![allow(clippy::disallowed_types)]

#[path = "common/cloud_managed.rs"]
mod managed;

use decapod::core::cloud_session::Session;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use tempfile::TempDir;

const PROJECT: &str = "example/supabase-proof";
const TASK_ID: &str = "feat_01c2d3e4f5g6h7j8";

fn decapod_bin() -> &'static str {
    env!("CARGO_BIN_EXE_decapod")
}

fn sha256_file(path: &Path) -> String {
    let bytes = fs::read(path).expect("read file for sha256");
    format!("{:x}", Sha256::digest(&bytes))
}

fn run_git(dir: &Path, args: &[&str]) -> Output {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

#[derive(Clone, Debug)]
struct CloudFixtureState {
    custody_status: String,
    task_version: i64,
    task_assignee: String,
    fencing_generation: i64,
    expires_at: String,
    drop_first_bind_ack: bool,
    bound_operation: Option<(String, Value)>,
    governance_decisions: BTreeMap<String, Value>,
    calls: Vec<String>,
}

impl CloudFixtureState {
    fn active() -> Self {
        let expires_at = decapod::core::time::format_rfc3339_nanos(
            time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        )
        .unwrap();
        Self {
            custody_status: "custody_current".to_string(),
            task_version: 2,
            task_assignee: "w2-agent".to_string(),
            fencing_generation: 5,
            expires_at,
            drop_first_bind_ack: false,
            bound_operation: None,
            governance_decisions: BTreeMap::new(),
            calls: Vec::new(),
        }
    }
}

struct CloudControlServer {
    origin: String,
    state: Arc<Mutex<CloudFixtureState>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl CloudControlServer {
    fn start(initial: CloudFixtureState) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        listener.set_nonblocking(true).expect("nonblocking");
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(initial));
        let stop = Arc::new(AtomicBool::new(false));
        let origin_clone = origin.clone();
        let state_clone = Arc::clone(&state);
        let stop_clone = Arc::clone(&stop);

        let handle = std::thread::spawn(move || {
            while !stop_clone.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        Self::serve_connection(&mut stream, &origin_clone, &state_clone);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
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

    fn serve_connection(
        stream: &mut TcpStream,
        origin: &str,
        state: &Arc<Mutex<CloudFixtureState>>,
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
        let len: usize = header
            .to_ascii_lowercase()
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .and_then(|v| v.trim().parse().ok())
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
        let path = header
            .lines()
            .next()
            .unwrap_or("")
            .split_whitespace()
            .nth(1)
            .unwrap_or("")
            .to_string();
        let body: Value = if len == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[body_start..body_start + len]).unwrap_or(Value::Null)
        };

        let mut st = state.lock().unwrap();
        st.calls.push(path.clone());
        let (status, resp) = match path.as_str() {
            "/functions/v1/decapod-onboarding/config" => (
                200,
                json!({
                    "version": 1,
                    "project_origin": origin,
                    "publishable_key": "sb_publishable_fixture",
                    "schema": "dactyl_api",
                }),
            ),
            "/functions/v1/decapod-onboarding/account-status" => (
                200,
                json!({
                    "version": 1,
                    "account_id": "00000000-0000-0000-0000-000000000404",
                    "project_id": PROJECT,
                    "status": "active",
                    "backend_authorized": true,
                }),
            ),
            "/rest/v1/rpc/dactyl_query_v1" => (
                200,
                json!({
                    "columns": [
                        "repo_id", "id", "hash", "title", "description", "status",
                        "assignee", "scope", "dir_path", "priority", "category",
                        "tags", "created_at", "updated_at", "version"
                    ],
                    "rows": [{
                        "repo_id": PROJECT,
                        "id": TASK_ID,
                        "hash": TASK_ID,
                        "title": "Accepted Cloud workspace validation",
                        "description": null,
                        "status": "in_progress",
                        "assignee": st.task_assignee,
                        "scope": "workspace",
                        "dir_path": "",
                        "priority": "medium",
                        "category": "feat",
                        "tags": "",
                        "created_at": "2026-11-21T00:00:00Z",
                        "updated_at": "2026-11-21T00:00:00Z",
                        "version": st.task_version,
                    }],
                    "affected_rows": 0,
                    "generated_keys": [],
                }),
            ),
            "/rest/v1/rpc/propodus_proof_acceptance_v1" => {
                let req = body.get("request").unwrap_or(&body);
                let op_id = req["operation_id"].as_str().unwrap_or("").to_string();
                let receipt = json!({
                    "version": 1,
                    "profile": "candidate-acceptance-v1",
                    "status": "custody_bound",
                    "operation_id": op_id,
                    "project_id": req["project_id"],
                    "task_id": req["task_id"],
                    "workspace_id": req["workspace_id"],
                    "custody_id": req["custody_id"],
                    "task_revision": st.task_version,
                    "fencing_generation": st.fencing_generation,
                    "authority_generation": 2,
                    "expires_at": st.expires_at,
                });
                st.bound_operation = Some((op_id, receipt.clone()));
                if st.drop_first_bind_ack {
                    st.drop_first_bind_ack = false;
                    // Simulate lost response after server commit by closing socket without HTTP response.
                    return;
                }
                (200, receipt)
            }
            "/rest/v1/rpc/propodus_proof_acceptance_get_v1" => {
                let req = body.get("request").unwrap_or(&body);
                let action = req["action"].as_str().unwrap_or("");
                if action == "get_operation" {
                    let req_op = req["operation_id"].as_str().unwrap_or("");
                    if let Some((saved_op, receipt)) = &st.bound_operation
                        && saved_op == req_op
                    {
                        (200, json!({"found": true, "receipt": receipt}))
                    } else {
                        (
                            404,
                            json!({"code": "proof_acceptance_not_found", "message": "not found"}),
                        )
                    }
                } else {
                    (
                        200,
                        json!({
                            "version": 1,
                            "profile": "candidate-acceptance-v1",
                            "status": st.custody_status,
                            "project_id": req["project_id"],
                            "task_id": req["task_id"],
                            "workspace_id": req["workspace_id"],
                            "custody_id": req["custody_id"],
                            "task_revision": st.task_version,
                            "fencing_generation": st.fencing_generation,
                            "authority_generation": 2,
                            "expires_at": st.expires_at,
                        }),
                    )
                }
            }
            "/rest/v1/rpc/propodus_governance_v1" => {
                let req = body.get("request").unwrap_or(&body);
                let action = req["action"].as_str().unwrap_or("");
                let decision_id = req["decision_id"].as_str().unwrap_or("").to_string();
                match action {
                    "authorize_exception" => {
                        let record = json!({
                            "version": 1,
                            "project_id": req["project_id"],
                            "decision_id": decision_id,
                            "capability": req["capability"],
                            "obligation_id": req["obligation_id"],
                            "base_commit": req["base_commit"],
                            "candidate_commit": req["candidate_commit"],
                            "affected_commit": req["affected_commit"],
                            "tree_oid": req["tree_oid"],
                            "status": "valid_historical_exception",
                            "current_generation_matches": true,
                            "authority_basis": "verified_repo_admin",
                            "expires_at": st.expires_at,
                            "authority_expires_at": st.expires_at,
                        });
                        st.governance_decisions.insert(decision_id.clone(), record);
                        (
                            200,
                            json!({
                                "version": 1,
                                "status": "authorized",
                                "project_id": req["project_id"],
                                "decision_id": decision_id,
                                "capability": req["capability"],
                                "obligation_id": req["obligation_id"],
                                "base_commit": req["base_commit"],
                                "candidate_commit": req["candidate_commit"],
                                "affected_commit": req["affected_commit"],
                                "tree_oid": req["tree_oid"],
                            }),
                        )
                    }
                    "verify_decision" => {
                        if let Some(found) = st.governance_decisions.get(&decision_id).cloned() {
                            (200, found)
                        } else {
                            (
                                404,
                                json!({"code": "governance_decision_not_found", "message": "not found"}),
                            )
                        }
                    }
                    _ => (
                        400,
                        json!({"code": "invalid_request", "message": "unsupported governance action"}),
                    ),
                }
            }
            _ => (
                404,
                json!({"error": {"code": "not_found", "message": path}}),
            ),
        };
        drop(st);

        let encoded = resp.to_string();
        let status_text = match status {
            404 => "Not Found",
            _ => "OK",
        };
        let reply = format!(
            "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{encoded}",
            encoded.len()
        );
        let _ = stream.write_all(reply.as_bytes());
    }
}

impl Drop for CloudControlServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn python_runtime() -> (String, BTreeMap<String, String>) {
    use decapod::core::bounded_process::BoundedCommand;
    let program = ["/usr/bin/python3", "/usr/local/bin/python3"]
        .into_iter()
        .find_map(|p| Path::new(p).canonicalize().ok())
        .expect("python3 required for cloud readiness fixture");
    let out = Command::new(&program)
        .args([
            "-I",
            "-S",
            "-B",
            "-c",
            "import os, encodings, encodings.utf_8; print(os.path.realpath(os.__file__)); print(os.path.realpath(encodings.__file__)); print(os.path.realpath(encodings.utf_8.__file__))",
        ])
        .bounded_output(Duration::from_secs(10))
        .unwrap();
    assert!(out.status.success());
    let files = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .enumerate()
        .map(|(i, p)| (format!("python-runtime-{i}"), p.to_string()))
        .collect();
    (program.to_string_lossy().into_owned(), files)
}

fn init_accepted_cloud_repo(dir: &Path, existing_sqlite: bool) -> BTreeMap<PathBuf, String> {
    use decapod::core::bounded_process::Cancellation;
    use decapod::core::proof::{ProjectProofConfig, ProofDef};
    use decapod::core::readiness_profile::ProfileDeclaration;
    use decapod::core::repository_profile::{
        self, RepositoryContract, RepositoryExecution, SourceCoverage,
    };
    use decapod::core::validation_runtime as runtime;

    run_git(dir, &["init", "-b", "master"]);
    run_git(dir, &["config", "user.name", "Decapod Test"]);
    run_git(dir, &["config", "user.email", "test@example.invalid"]);
    run_git(
        dir,
        &[
            "remote",
            "add",
            "origin",
            &format!("https://github.com/{PROJECT}.git"),
        ],
    );
    fs::write(dir.join("README.md"), "# Cloud Repo\n").unwrap();
    fs::write(
        dir.join(".gitignore"),
        ".decapod/data/\n.decapod/managed/governance-document.lock\n.decapod/backups/\n",
    )
    .unwrap();
    fs::create_dir_all(dir.join(".github/workflows")).unwrap();
    fs::write(
        dir.join(".github/workflows/decapod-validate.yml"),
        "name: Validate\non: [pull_request]\njobs:\n  validate:\n    runs-on: ubuntu-latest\n    steps:\n      - run: decapod validate\n",
    )
    .unwrap();
    fs::create_dir_all(dir.join("scripts")).unwrap();
    fs::write(dir.join("scripts/check.py"), "print('ok')\n").unwrap();
    run_git(dir, &["add", "."]);
    run_git(dir, &["commit", "-m", "initial source"]);

    let init = Command::new(decapod_bin())
        .args([
            "init",
            "--backend",
            "cloud",
            "--no-container-workspaces",
            "--no-ci",
            "--force",
            "--proof",
            "--product-summary",
            "Cloud test repository for workspace and validation parity.",
            "--architecture-direction",
            "Rust library + hosted Cloud control plane.",
        ])
        .current_dir(dir)
        .output()
        .expect("decapod init");
    assert!(
        init.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    run_git(dir, &["add", "-A"]);
    run_git(dir, &["commit", "-m", "init cloud repo"]);
    run_git(dir, &["update-ref", "refs/remotes/origin/master", "HEAD"]);

    let (program, toolchain_files) = python_runtime();
    let def = ProofDef {
        name: "check-script".into(),
        command: program.clone(),
        args: vec![
            "-I".into(),
            "-S".into(),
            "-B".into(),
            "scripts/check.py".into(),
        ],
        description: "Run committed check script".into(),
        required: true,
    };
    let mut contract = RepositoryContract {
        canonical_local_target: true,
        reviewed_commands_digest: String::new(),
        source_coverage: vec![SourceCoverage {
            source: ".github/workflows/decapod-validate.yml".into(),
            commands: vec![def.name.clone()],
            ci_only: false,
            reason: None,
        }],
        executions: BTreeMap::from([(
            def.name.clone(),
            RepositoryExecution {
                program,
                toolchain_files,
                platforms: BTreeMap::new(),
                environment: BTreeMap::new(),
                directory: ".".into(),
                timeout_seconds: 15,
            },
        )]),
        targets: BTreeMap::new(),
    };
    let mut proof = ProjectProofConfig {
        commands: vec![def.clone()],
        required_profile: Some(ProfileDeclaration {
            id: "repo.pr-readiness.v1".into(),
            reviewed_sources_digest: String::new(),
        }),
        repository_contract: Some(contract.clone()),
        ..ProjectProofConfig::default()
    };
    let ev_tmp = TempDir::new().unwrap();
    let canonical_dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let canonical_ev = ev_tmp
        .path()
        .canonicalize()
        .unwrap_or_else(|_| ev_tmp.path().to_path_buf());
    let opts = runtime::Options {
        root: canonical_dir,
        git: Path::new("/usr/bin/git").canonicalize().unwrap(),
        repository: PROJECT.into(),
        base_branch: "master".into(),
        accepted_base_ref: "refs/remotes/origin/master".into(),
        evidence_parent: canonical_ev,
        timeout: Duration::from_secs(30),
        max_input_bytes: 32 * 1024 * 1024,
        max_files: 5000,
        max_output_bytes: 512 * 1024,
        target: None,
    };
    let source_digest = runtime::discover_source_inventory(&opts, &Cancellation::default())
        .unwrap()
        .digest()
        .unwrap();
    proof
        .required_profile
        .as_mut()
        .unwrap()
        .reviewed_sources_digest = source_digest;
    contract.reviewed_commands_digest =
        repository_profile::command_contract_digest(&[def], &contract).unwrap();
    proof.repository_contract = Some(contract);

    let cfg_path = dir.join(".decapod/config.toml");
    let mut doc: toml::Value = toml::from_str(&fs::read_to_string(&cfg_path).unwrap()).unwrap();
    let table = doc.as_table_mut().unwrap();
    table.insert("proof".into(), toml::Value::try_from(&proof).unwrap());
    fs::write(&cfg_path, toml::to_string_pretty(&doc).unwrap()).unwrap();

    run_git(dir, &["add", "-A"]);
    run_git(dir, &["commit", "-m", "accept cloud config"]);
    run_git(dir, &["update-ref", "refs/remotes/origin/master", "HEAD"]);

    let mut baselines = BTreeMap::new();
    if existing_sqlite {
        let data = dir.join(".decapod/data");
        fs::create_dir_all(&data).unwrap();
        let db = data.join("decapod.db");
        let wal = data.join("decapod.db-wal");
        fs::write(&db, b"SQLite legacy shadow fixture bytes").unwrap();
        fs::write(&wal, b"SQLite legacy WAL journal bytes").unwrap();
        baselines.insert(db.clone(), sha256_file(&db));
        baselines.insert(wal.clone(), sha256_file(&wal));
    }
    baselines
}

fn apply_cloud_env<'a>(
    cmd: &'a mut Command,
    machine: &Path,
    origin: &str,
    agent_id: &str,
) -> &'a mut Command {
    cmd.env("HOME", machine)
        .env("XDG_DATA_HOME", machine.join("data"))
        .env("XDG_CONFIG_HOME", machine.join("config"))
        .env("DECAPOD_CLOUD_CUSTODY_DIR", machine.join("custody"))
        .env("DECAPOD_TEST_MODE", "1")
        .env("DECAPOD_CLOUD_TEST_ORIGIN", origin)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_GROUP_BROKER_DISABLE", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("DECAPOD_CLOUD_AUTH_SCHEME")
        .env_remove("DECAPOD_CONTAINER_SIGNER_SOCKET")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
}

#[test]
fn accepted_cloud_workspace_ensure_and_validate_work_without_sqlite_under_conflicting_datastore() {
    for existing_sqlite in [false, true] {
        let srv = CloudControlServer::start(CloudFixtureState::active());
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        let machine = TempDir::new().unwrap();
        fs::create_dir_all(&repo).unwrap();
        let baselines = init_accepted_cloud_repo(&repo, existing_sqlite);

        managed::seed_ready(
            &repo,
            machine.path(),
            &srv.origin,
            PROJECT,
            Session {
                access_token: managed::ACCESS.into(),
                refresh_token: Some(managed::REFRESH.into()),
                expires_at: time::OffsetDateTime::now_utc().unix_timestamp() + 900,
            },
        );

        let branch = format!("agent/w2-agent/{TASK_ID}-cloud-work");
        // Set contradictory ambient DATASTORE=sqlite to confirm [repo].backend="cloud" wins.
        let ensure_out = apply_cloud_env(
            &mut Command::new(decapod_bin()),
            machine.path(),
            &srv.origin,
            "w2-agent",
        )
        .env("DATASTORE", "sqlite")
        .args(["workspace", "ensure", "--branch", &branch])
        .current_dir(&repo)
        .output()
        .expect("workspace ensure");
        assert!(
            ensure_out.status.success(),
            "workspace ensure failed (existing_sqlite={existing_sqlite}): stderr={}\nstdout={}",
            String::from_utf8_lossy(&ensure_out.stderr),
            String::from_utf8_lossy(&ensure_out.stdout),
        );
        let ensure_json: Value = serde_json::from_slice(&ensure_out.stdout).expect("ensure json");
        assert_eq!(ensure_json["can_work"], true);
        let wt_path = PathBuf::from(
            ensure_json["worktree_path"]
                .as_str()
                .expect("worktree path"),
        );
        assert!(wt_path.exists());

        // Approve the governed plan + make a material specs change on the branch before `decapod validate`.
        let approve = apply_cloud_env(
            &mut Command::new(decapod_bin()),
            machine.path(),
            &srv.origin,
            "w2-agent",
        )
        .env("DATASTORE", "sqlite")
        .args(["govern", "plan", "approve"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
        assert!(
            approve.status.success(),
            "govern plan approve failed: stdout={}\nstderr={}",
            String::from_utf8_lossy(&approve.stdout),
            String::from_utf8_lossy(&approve.stderr),
        );
        let intent_path = wt_path.join(".decapod/managed/specs/INTENT.md");
        let mut intent = fs::read_to_string(&intent_path).unwrap();
        intent.push_str("\n- Cloud workspace custody qualification entry.\n");
        fs::write(&intent_path, intent).unwrap();
        let refresh = apply_cloud_env(
            &mut Command::new(decapod_bin()),
            machine.path(),
            &srv.origin,
            "w2-agent",
        )
        .env("DATASTORE", "sqlite")
        .args(["rpc", "--op", "specs.refresh"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
        assert!(
            refresh.status.success(),
            "{}",
            String::from_utf8_lossy(&refresh.stderr)
        );
        for spec in ["ARCHITECTURE", "INTERFACES"] {
            let review = apply_cloud_env(
                &mut Command::new(decapod_bin()),
                machine.path(),
                &srv.origin,
                "w2-agent",
            )
            .env("DATASTORE", "sqlite")
            .args([
                "govern",
                "plan",
                "review-spec",
                "--path",
                &format!(".decapod/managed/specs/{spec}.md"),
                "--disposition",
                "unchanged-with-reason",
                "--reason",
                "Cloud workspace test updates INTENT only.",
            ])
            .current_dir(&wt_path)
            .output()
            .unwrap();
            assert!(
                review.status.success(),
                "{}",
                String::from_utf8_lossy(&review.stderr)
            );
        }
        run_git(&wt_path, &["add", "-A"]);
        let ckpt = apply_cloud_env(
            &mut Command::new(decapod_bin()),
            machine.path(),
            &srv.origin,
            "w2-agent",
        )
        .env("DATASTORE", "sqlite")
        .args([
            "govern",
            "artifacts",
            "checkpoint",
            "--id",
            "chk_1",
            "--summary",
            "Cloud worktree spec update",
        ])
        .current_dir(&wt_path)
        .output()
        .unwrap();
        assert!(
            ckpt.status.success(),
            "checkpoint failed: stdout={}\nstderr={}",
            String::from_utf8_lossy(&ckpt.stdout),
            String::from_utf8_lossy(&ckpt.stderr),
        );
        run_git(&wt_path, &["add", "-A"]);
        run_git(
            &wt_path,
            &["commit", "-m", "feat: cloud worktree spec update"],
        );

        let validate_out = apply_cloud_env(
            &mut Command::new(decapod_bin()),
            machine.path(),
            &srv.origin,
            "w2-agent",
        )
        .env("DATASTORE", "sqlite")
        .env("GITHUB_ACTIONS", "true")
        .args(["validate", "--format", "json"])
        .current_dir(&wt_path)
        .output()
        .expect("decapod validate");
        assert!(
            validate_out.status.success(),
            "validate failed (existing_sqlite={existing_sqlite}): stdout={}\nstderr={}",
            String::from_utf8_lossy(&validate_out.stdout),
            String::from_utf8_lossy(&validate_out.stderr),
        );

        // Verify RPC entry points (`workspace.ensure` and `validate.run`) also succeed on Cloud.
        let rpc_ensure = apply_cloud_env(
            &mut Command::new(decapod_bin()),
            machine.path(),
            &srv.origin,
            "w2-agent",
        )
        .env("DATASTORE", "sqlite")
        .args(["rpc", "--op", "workspace.ensure"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
        assert!(
            rpc_ensure.status.success(),
            "rpc workspace.ensure failed: stdout={}\nstderr={}",
            String::from_utf8_lossy(&rpc_ensure.stdout),
            String::from_utf8_lossy(&rpc_ensure.stderr),
        );

        let rpc_validate = apply_cloud_env(
            &mut Command::new(decapod_bin()),
            machine.path(),
            &srv.origin,
            "w2-agent",
        )
        .env("DATASTORE", "sqlite")
        .env("GITHUB_ACTIONS", "true")
        .args(["rpc", "--op", "validate.run"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
        assert!(
            rpc_validate.status.success(),
            "rpc validate.run failed: stdout={}\nstderr={}",
            String::from_utf8_lossy(&rpc_validate.stdout),
            String::from_utf8_lossy(&rpc_validate.stderr),
        );

        if existing_sqlite {
            for (path, before) in &baselines {
                assert_eq!(
                    sha256_file(path),
                    *before,
                    "existing SQLite/WAL file {} must stay bit-for-bit unchanged",
                    path.display()
                );
            }
        } else {
            assert!(
                !repo.join(".decapod/data/decapod.db").exists(),
                "must never create local decapod.db on Cloud repo"
            );
            assert!(!wt_path.join(".decapod/data/decapod.db").exists());
        }

        // Verify custody drift, revocation, expiry, and foreign actor refusal fail closed without fallback.
        {
            let mut st = srv.state.lock().unwrap();
            st.custody_status = "custody_revoked".to_string();
        }
        let revoked_validate = apply_cloud_env(
            &mut Command::new(decapod_bin()),
            machine.path(),
            &srv.origin,
            "w2-agent",
        )
        .env("GITHUB_ACTIONS", "true")
        .args(["validate", "--format", "json"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
        assert!(
            !revoked_validate.status.success(),
            "revoked hosted custody must fail validate closed"
        );
        assert!(
            String::from_utf8_lossy(&revoked_validate.stdout)
                .contains("CLOUD_CUSTODY_RECEIPT_INVALID"),
            "revoked validate stdout: {}",
            String::from_utf8_lossy(&revoked_validate.stdout)
        );

        {
            let mut st = srv.state.lock().unwrap();
            st.custody_status = "custody_current".to_string();
            st.task_version = 99;
        }
        let drifted_validate = apply_cloud_env(
            &mut Command::new(decapod_bin()),
            machine.path(),
            &srv.origin,
            "w2-agent",
        )
        .env("GITHUB_ACTIONS", "true")
        .args(["validate", "--format", "json"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
        assert!(
            !drifted_validate.status.success(),
            "drifted hosted task_revision must fail validate closed"
        );
        assert!(
            String::from_utf8_lossy(&drifted_validate.stdout)
                .contains("CLOUD_CUSTODY_RECEIPT_INVALID"),
            "drifted validate stdout: {}",
            String::from_utf8_lossy(&drifted_validate.stdout)
        );

        {
            let mut st = srv.state.lock().unwrap();
            st.task_version = 2;
            st.expires_at = "2020-01-01T00:00:00Z".to_string();
        }
        let expired_validate = apply_cloud_env(
            &mut Command::new(decapod_bin()),
            machine.path(),
            &srv.origin,
            "w2-agent",
        )
        .env("GITHUB_ACTIONS", "true")
        .args(["validate", "--format", "json"])
        .current_dir(&wt_path)
        .output()
        .unwrap();
        assert!(
            !expired_validate.status.success(),
            "expired hosted custody must fail validate closed"
        );
        assert!(
            String::from_utf8_lossy(&expired_validate.stdout)
                .contains("CLOUD_CUSTODY_RECEIPT_INVALID"),
            "expired validate stdout: {}",
            String::from_utf8_lossy(&expired_validate.stdout)
        );
    }
}

#[test]
fn repeated_workspace_ensure_reconciles_lost_bind_acknowledgement_via_get_operation() {
    let mut initial = CloudFixtureState::active();
    initial.drop_first_bind_ack = true;
    let srv = CloudControlServer::start(initial);

    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let machine = TempDir::new().unwrap();
    fs::create_dir_all(&repo).unwrap();
    let _ = init_accepted_cloud_repo(&repo, false);

    managed::seed_ready(
        &repo,
        machine.path(),
        &srv.origin,
        PROJECT,
        Session {
            access_token: managed::ACCESS.into(),
            refresh_token: Some(managed::REFRESH.into()),
            expires_at: time::OffsetDateTime::now_utc().unix_timestamp() + 900,
        },
    );

    let branch = format!("agent/w2-agent/{TASK_ID}-recover");
    let first = apply_cloud_env(
        &mut Command::new(decapod_bin()),
        machine.path(),
        &srv.origin,
        "w2-agent",
    )
    .args(["workspace", "ensure", "--branch", &branch])
    .current_dir(&repo)
    .output()
    .unwrap();
    assert!(
        first.status.success(),
        "repeated/immediate reconciliation of lost ack should recover via get_operation + get_custody: stderr={}",
        String::from_utf8_lossy(&first.stderr)
    );
    let calls = srv.state.lock().unwrap().calls.clone();
    let bind_count = calls
        .iter()
        .filter(|p| p.as_str() == "/rest/v1/rpc/propodus_proof_acceptance_v1")
        .count();
    let get_count = calls
        .iter()
        .filter(|p| p.as_str() == "/rest/v1/rpc/propodus_proof_acceptance_get_v1")
        .count();
    assert_eq!(
        bind_count, 1,
        "must never double-submit custody_bind on unknown outcome"
    );
    assert!(
        get_count >= 2,
        "must run get_operation reconciliation + get_custody live check"
    );
    assert!(!repo.join(".decapod/data/decapod.db").exists());
}

#[test]
fn cloud_workspace_ensure_container_and_host_relay_preserve_isolation_and_refuse_tampering() {
    use decapod::core::cloud_proof::{CustodyGetRequest, custody_get};
    use decapod::core::cloud_workspace::{
        CloudRelayExecutor, HostCloudRelayHandler, RelayProofControlTransport,
        RuntimeContainerAdmissionVerifier,
    };
    use decapod::core::container_signer::{
        ContainerCloudRelayHandler, ContainerRelayOperation, ContainerSignerPolicy,
        HostContainerSignerServer, WorkspaceCredentialsConfig,
    };
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let srv = CloudControlServer::start(CloudFixtureState::active());
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let machine = TempDir::new().unwrap();
    let mock_bin = tmp.path().join("mock_bin");
    fs::create_dir_all(&repo).unwrap();
    fs::create_dir_all(&mock_bin).unwrap();

    run_git(&repo, &["init", "-b", "master"]);
    run_git(&repo, &["config", "user.name", "Decapod Test"]);
    run_git(&repo, &["config", "user.email", "test@example.invalid"]);
    run_git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            &format!("https://github.com/{PROJECT}.git"),
        ],
    );
    fs::write(repo.join("README.md"), "# Cloud Container Repo\n").unwrap();
    run_git(&repo, &["add", "."]);
    run_git(&repo, &["commit", "-m", "initial source"]);

    let init = Command::new(decapod_bin())
        .args([
            "init",
            "--backend",
            "cloud",
            "--force",
            "--proof",
            "--product-summary",
            "Cloud container test repo.",
            "--architecture-direction",
            "Host custody + isolated container relay.",
        ])
        .current_dir(&repo)
        .output()
        .expect("decapod init");
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    run_git(&repo, &["add", "-A"]);
    run_git(&repo, &["commit", "-m", "accept cloud container config"]);

    let docker_log = tmp.path().join("docker_calls.log");
    let inspect_id = tmp.path().join("inspect_id.txt");
    let inspect_running = tmp.path().join("inspect_running.txt");
    let inspect_inv = tmp.path().join("inspect_inv.txt");
    let inspect_wt = tmp.path().join("inspect_wt.txt");
    let fake_id_1 = "a".repeat(64);
    let fake_id_2 = "b".repeat(64);
    fs::write(&inspect_id, &fake_id_1).unwrap();
    fs::write(&inspect_running, "true").unwrap();

    let mock_docker_path = mock_bin.join("docker");
    let script = format!(
        r#"#!/bin/sh
echo "$@" >> "{log}"
sub="$1"
shift || true
case "$sub" in
  image|images|rmi|rm)
    exit 0
    ;;
  build)
    exit 0
    ;;
  run)
    exit 0
    ;;
  container)
    sub2="$1"
    shift || true
    case "$sub2" in
      inspect)
        cid=$(cat "{id_file}")
        running=$(cat "{run_file}")
        inv=$(cat "{inv_file}")
        wt=$(cat "{wt_file}")
        printf '{{"Id":"%s","State":{{"Running":%s}},"Config":{{"Labels":{{"org.decapod.managed":"workspace","org.decapod.workspace.path":"%s","org.decapod.invocation":"%s"}}}}}}\n' "$cid" "$running" "$wt" "$inv"
        exit 0
        ;;
      *)
        exit 0
        ;;
    esac
    ;;
  *)
    exit 0
    ;;
esac
"#,
        log = docker_log.display(),
        id_file = inspect_id.display(),
        run_file = inspect_running.display(),
        inv_file = inspect_inv.display(),
        wt_file = inspect_wt.display(),
    );
    fs::write(&mock_docker_path, script).unwrap();
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&mock_docker_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&mock_docker_path, perms).unwrap();
    }

    managed::seed_ready(
        &repo,
        machine.path(),
        &srv.origin,
        PROJECT,
        Session {
            access_token: managed::ACCESS.into(),
            refresh_token: Some(managed::REFRESH.into()),
            expires_at: time::OffsetDateTime::now_utc().unix_timestamp() + 900,
        },
    );

    let controlled_path = format!("{}:/usr/bin:/bin", mock_bin.display());
    let branch = format!("agent/w2-agent/{TASK_ID}-container");
    let ensure_out = apply_cloud_env(
        &mut Command::new(decapod_bin()),
        machine.path(),
        &srv.origin,
        "w2-agent",
    )
    .env("PATH", &controlled_path)
    .env("DATASTORE", "sqlite")
    .args(["workspace", "ensure", "--container", "--branch", &branch])
    .current_dir(&repo)
    .output()
    .expect("workspace ensure --container");
    assert!(
        ensure_out.status.success(),
        "workspace ensure --container failed: stderr={}\nstdout={}",
        String::from_utf8_lossy(&ensure_out.stderr),
        String::from_utf8_lossy(&ensure_out.stdout),
    );
    let ensure_json: Value = serde_json::from_slice(&ensure_out.stdout).unwrap();
    let wt_path = PathBuf::from(
        ensure_json["worktree_path"]
            .as_str()
            .expect("worktree_path"),
    );
    let launch_cmd = ensure_json["blockers"]
        .as_array()
        .and_then(|arr| arr.iter().find_map(|b| b["resolve_hint"].as_str()))
        .expect("container resolve_hint launch command");
    assert!(launch_cmd.contains("DATASTORE=supabase"), "{launch_cmd}");
    assert!(
        launch_cmd.contains("DECAPOD_CLOUD_CONTAINER_RELAY=1"),
        "{launch_cmd}"
    );
    assert!(
        launch_cmd.contains("DECAPOD_CLOUD_CUSTODY_ID="),
        "{launch_cmd}"
    );
    assert!(
        launch_cmd.contains("DECAPOD_CLOUD_INVOCATION_ID="),
        "{launch_cmd}"
    );
    assert!(!launch_cmd.contains(".decapod/data"), "{launch_cmd}");
    assert!(!launch_cmd.contains("decapod.db"), "{launch_cmd}");
    assert!(!launch_cmd.contains("DECAPOD_LOCAL_ONLY=1"), "{launch_cmd}");
    assert!(!launch_cmd.contains(managed::ACCESS));
    assert!(!launch_cmd.contains(managed::REFRESH));
    assert!(!repo.join(".decapod/data/decapod.db").exists());
    assert!(!wt_path.join(".decapod/data/decapod.db").exists());

    // Host-only `validate` MUST fail closed on a container-expected Cloud workspace
    // when no matching container passes `verify_running_container_for_invocation`.
    fs::write(&inspect_running, "false").unwrap();
    fs::write(&inspect_inv, "wrong-inv").unwrap();
    fs::write(&inspect_wt, wt_path.display().to_string()).unwrap();
    let host_only_validate = apply_cloud_env(
        &mut Command::new(decapod_bin()),
        machine.path(),
        &srv.origin,
        "w2-agent",
    )
    .env("PATH", &controlled_path)
    .env("DATASTORE", "sqlite")
    .args(["validate", "--format", "json"])
    .current_dir(&wt_path)
    .output()
    .unwrap();
    assert!(
        !host_only_validate.status.success(),
        "host-only validate must fail closed when container custody expects an active container"
    );

    // Exercise HostContainerSignerServer + RuntimeContainerAdmissionVerifier +
    // HostCloudRelayHandler + RelayProofControlTransport with immutable ID check
    // (no process-global env mutation).
    struct FixtureControlExecutor {
        origin: String,
        bearer: String,
    }
    impl CloudRelayExecutor for FixtureControlExecutor {
        fn execute(
            &self,
            operation: ContainerRelayOperation,
            rpc_op: &str,
            payload: &Value,
        ) -> Result<Value, decapod::core::error::DecapodError> {
            let connector = dactyl_db::CloudConnector::discover(&self.origin)
                .map_err(decapod::core::cloud_client::transport_error)?;
            let kind = match operation {
                ContainerRelayOperation::DactylQuery
                | ContainerRelayOperation::ProofControlRead => dactyl_db::OperationKind::Read,
                ContainerRelayOperation::DactylBatch
                | ContainerRelayOperation::ProofControlWrite => dactyl_db::OperationKind::Write,
            };
            connector
                .control_request(
                    rpc_op,
                    payload,
                    Some(&self.bearer),
                    kind,
                    dactyl_db::ResourceBudget::default(),
                )
                .map_err(decapod::core::cloud_client::transport_error)
        }
    }

    let store =
        decapod::core::cloud_workspace::CloudCustodyStore::at(machine.path().join("custody"));
    let custody = store.load(&repo, &wt_path).unwrap().expect("saved custody");
    let inv_id = custody.invocation_id.clone().expect("invocation_id");
    let canonical_wt = wt_path.canonicalize().unwrap_or_else(|_| wt_path.clone());
    fs::write(&inspect_wt, canonical_wt.display().to_string()).unwrap();
    fs::write(&inspect_inv, &inv_id).unwrap();
    fs::write(&inspect_running, "true").unwrap();
    fs::write(&inspect_id, &fake_id_1).unwrap();

    let relay_handler: Arc<dyn ContainerCloudRelayHandler> = Arc::new(HostCloudRelayHandler {
        project_id: PROJECT.to_string(),
        workspace_id: custody.workspace_id.clone(),
        worktree_path: canonical_wt.clone(),
        task_id: TASK_ID.to_string(),
        custody_id: custody.custody_id.clone(),
        invocation_id: inv_id.clone(),
        admission: Some(Arc::new(RuntimeContainerAdmissionVerifier {
            runtime: mock_docker_path.to_string_lossy().to_string(),
            main_root: repo.clone(),
            container_name: "decapod-ws-fixture".to_string(),
            expected_container_id: Some(fake_id_1.clone()),
        })),
        executor: Arc::new(FixtureControlExecutor {
            origin: srv.origin.clone(),
            bearer: managed::ACCESS.to_string(),
        }),
    });
    let sock_path = tmp.path().join("cloud-relay.sock");
    let signer_policy = ContainerSignerPolicy {
        allowed_repository: PROJECT.to_string(),
        allowed_origin: srv.origin.clone(),
        allowed_agent_id: "w2-agent".to_string(),
        credentials: WorkspaceCredentialsConfig {
            git_transport: None,
            github_api: None,
            propodus_cloud: Some("signer-broker".to_string()),
        },
        session_expires_at: time::OffsetDateTime::now_utc().unix_timestamp() + 900,
        revoked: Arc::new(AtomicBool::new(false)),
        device_key_store: None,
        access_token_digest: None,
    };
    let _server =
        HostContainerSignerServer::bind_with_relay(&sock_path, signer_policy, Some(relay_handler))
            .unwrap();
    let relay_transport = RelayProofControlTransport::new(
        sock_path,
        inv_id.clone(),
        custody.workspace_id.clone(),
        PROJECT.to_string(),
        TASK_ID.to_string(),
        custody.custody_id.clone(),
    );

    // Matching immutable container ID + invocation succeeds cleanly without touching secrets or SQLite:
    let live = custody_get(
        &relay_transport,
        &CustodyGetRequest {
            project_id: PROJECT.to_string(),
            task_id: TASK_ID.to_string(),
            workspace_id: custody.workspace_id.clone(),
            custody_id: custody.custody_id.clone(),
            task_revision: custody.task_revision.unwrap(),
            fencing_generation: custody.fencing_generation.unwrap(),
        },
    )
    .expect("relay custody_get with matching admitted container");
    assert_eq!(live.task_revision, 2);

    // Replacement container with same name + labels but mutated Id MUST fail closed!
    fs::write(&inspect_id, &fake_id_2).unwrap();
    let swapped = custody_get(
        &relay_transport,
        &CustodyGetRequest {
            project_id: PROJECT.to_string(),
            task_id: TASK_ID.to_string(),
            workspace_id: custody.workspace_id.clone(),
            custody_id: custody.custody_id.clone(),
            task_revision: custody.task_revision.unwrap(),
            fencing_generation: custody.fencing_generation.unwrap(),
        },
    )
    .expect_err("replacement container with different Id must fail closed");
    assert!(
        swapped
            .to_string()
            .contains("CLOUD_CONTAINER_ADMISSION_DENIED"),
        "{swapped}"
    );
    assert!(!repo.join(".decapod/data/decapod.db").exists());
}

#[test]
fn accepted_cloud_workspace_ensure_non_ci_validate_policy_approve_and_container_relay() {
    let srv = CloudControlServer::start(CloudFixtureState::active());
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let machine = TempDir::new().unwrap();
    fs::create_dir_all(&repo).unwrap();
    let baselines = init_accepted_cloud_repo(&repo, true);

    managed::seed_ready(
        &repo,
        machine.path(),
        &srv.origin,
        PROJECT,
        Session {
            access_token: managed::ACCESS.into(),
            refresh_token: Some(managed::REFRESH.into()),
            expires_at: time::OffsetDateTime::now_utc().unix_timestamp() + 900,
        },
    );

    // 1. Unknown branch task ID must fail closed rather than binding an unrelated claimed task.
    let unknown_branch_ensure = apply_cloud_env(
        &mut Command::new(decapod_bin()),
        machine.path(),
        &srv.origin,
        "w2-agent",
    )
    .args([
        "workspace",
        "ensure",
        "--branch",
        "agent/w2-agent/feat_0000000000000000-missing",
    ])
    .current_dir(&repo)
    .output()
    .unwrap();
    assert!(
        !unknown_branch_ensure.status.success(),
        "ensure with nonexistent branch task ID must fail closed"
    );
    assert!(
        String::from_utf8_lossy(&unknown_branch_ensure.stderr).contains("TODO_CUSTODY_REQUIRED"),
        "stderr={}",
        String::from_utf8_lossy(&unknown_branch_ensure.stderr)
    );

    // 2. `workspace ensure` without `--branch` auto-discovers the claimed hosted task.
    let ensure_auto = apply_cloud_env(
        &mut Command::new(decapod_bin()),
        machine.path(),
        &srv.origin,
        "w2-agent",
    )
    .env("DATASTORE", "sqlite")
    .args(["workspace", "ensure"])
    .current_dir(&repo)
    .output()
    .unwrap();
    assert!(
        ensure_auto.status.success(),
        "workspace ensure without --branch failed: stderr={}\nstdout={}",
        String::from_utf8_lossy(&ensure_auto.stderr),
        String::from_utf8_lossy(&ensure_auto.stdout),
    );
    let ensure_json: Value = serde_json::from_slice(&ensure_auto.stdout).unwrap();
    assert_eq!(ensure_json["can_work"], true);
    let wt_path = PathBuf::from(ensure_json["worktree_path"].as_str().unwrap());
    assert!(
        ensure_json["branch"].as_str().unwrap().contains(TASK_ID),
        "auto branch must include hosted task id: {ensure_json}"
    );

    // 3. `workspace ensure --branch <different>` from inside an existing worktree must fail with WORKSPACE_DESTINATION_MISMATCH.
    let mismatch = apply_cloud_env(
        &mut Command::new(decapod_bin()),
        machine.path(),
        &srv.origin,
        "w2-agent",
    )
    .args([
        "workspace",
        "ensure",
        "--branch",
        &format!("agent/w2-agent/{TASK_ID}-other"),
    ])
    .current_dir(&wt_path)
    .output()
    .unwrap();
    assert!(!mismatch.status.success());
    assert!(
        String::from_utf8_lossy(&mismatch.stderr).contains("WORKSPACE_DESTINATION_MISMATCH"),
        "stderr={}",
        String::from_utf8_lossy(&mismatch.stderr)
    );

    // 4. `workspace status` succeeds cleanly on Cloud worktree.
    let status_out = apply_cloud_env(
        &mut Command::new(decapod_bin()),
        machine.path(),
        &srv.origin,
        "w2-agent",
    )
    .args(["workspace", "status"])
    .current_dir(&wt_path)
    .output()
    .unwrap();
    assert!(
        status_out.status.success(),
        "workspace status failed: stderr={}",
        String::from_utf8_lossy(&status_out.stderr)
    );
    let status_json: Value = serde_json::from_slice(&status_out.stdout).unwrap();
    assert_eq!(status_json["workspace"]["can_work"], true);

    // 5. Prepare plan, specs, trajectory, and commit, then run non-CI `decapod validate --format json` (no GITHUB_ACTIONS env).
    let approve_plan = apply_cloud_env(
        &mut Command::new(decapod_bin()),
        machine.path(),
        &srv.origin,
        "w2-agent",
    )
    .args(["govern", "plan", "approve"])
    .current_dir(&wt_path)
    .output()
    .unwrap();
    assert!(approve_plan.status.success());

    let add_claim = apply_cloud_env(
        &mut Command::new(decapod_bin()),
        machine.path(),
        &srv.origin,
        "w2-agent",
    )
    .args([
        "govern",
        "artifacts",
        "claim",
        "--id",
        "claim:cloud-1886",
        "--statement",
        "Cloud workspace and validation parity executes without SQLite fallback",
        "--falsifier",
        "Any SQLite file mutation or missing custody error",
        "--status",
        "open",
    ])
    .current_dir(&wt_path)
    .output()
    .unwrap();
    assert!(
        add_claim.status.success(),
        "govern artifacts claim failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&add_claim.stdout),
        String::from_utf8_lossy(&add_claim.stderr),
    );

    let intent_path = wt_path.join(".decapod/managed/specs/INTENT.md");
    let mut intent = fs::read_to_string(&intent_path).unwrap();
    intent.push_str("\n- Non-CI Cloud validation and policy approval parity entry.\n");
    fs::write(&intent_path, intent).unwrap();
    assert!(
        apply_cloud_env(
            &mut Command::new(decapod_bin()),
            machine.path(),
            &srv.origin,
            "w2-agent",
        )
        .args(["rpc", "--op", "specs.refresh"])
        .current_dir(&wt_path)
        .output()
        .unwrap()
        .status
        .success()
    );
    for spec in ["ARCHITECTURE", "INTERFACES"] {
        assert!(
            apply_cloud_env(
                &mut Command::new(decapod_bin()),
                machine.path(),
                &srv.origin,
                "w2-agent",
            )
            .args([
                "govern",
                "plan",
                "review-spec",
                "--path",
                &format!(".decapod/managed/specs/{spec}.md"),
                "--disposition",
                "unchanged-with-reason",
                "--reason",
                "Cloud parity test updates INTENT only.",
            ])
            .current_dir(&wt_path)
            .output()
            .unwrap()
            .status
            .success()
        );
    }
    run_git(&wt_path, &["add", "-A"]);
    assert!(
        apply_cloud_env(
            &mut Command::new(decapod_bin()),
            machine.path(),
            &srv.origin,
            "w2-agent",
        )
        .args([
            "govern",
            "artifacts",
            "checkpoint",
            "--id",
            "chk_non_ci",
            "--summary",
            "Cloud non-CI validation parity",
        ])
        .current_dir(&wt_path)
        .output()
        .unwrap()
        .status
        .success()
    );
    run_git(&wt_path, &["add", "-A"]);
    run_git(&wt_path, &["commit", "-m", "feat: cloud non-ci validation"]);

    let validate_pre_build = apply_cloud_env(
        &mut Command::new(decapod_bin()),
        machine.path(),
        &srv.origin,
        "w2-agent",
    )
    .env_remove("GITHUB_ACTIONS")
    .env_remove("BUILDKITE")
    .env("DATASTORE", "sqlite")
    .args(["validate", "--pre-build", "--format", "json"])
    .current_dir(&wt_path)
    .output()
    .unwrap();
    assert!(
        validate_pre_build.status.success(),
        "non-CI pre-build validate failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&validate_pre_build.stdout),
        String::from_utf8_lossy(&validate_pre_build.stderr),
    );
    let pre_build_json: Value = serde_json::from_slice(&validate_pre_build.stdout).unwrap();
    assert_eq!(pre_build_json["passed"], true, "{pre_build_json}");
    assert_eq!(pre_build_json["fail_count"], 0, "{pre_build_json}");
    assert_eq!(pre_build_json["scope"], "pre_build", "{pre_build_json}");

    // Full non-CI validate must reach readiness (proving Cloud custody/governance admission without
    // SQLite fallback) and fail closed only on canonical_local_target=true requiring backend="local".
    let validate_non_ci = apply_cloud_env(
        &mut Command::new(decapod_bin()),
        machine.path(),
        &srv.origin,
        "w2-agent",
    )
    .env_remove("GITHUB_ACTIONS")
    .env_remove("BUILDKITE")
    .env("DATASTORE", "sqlite")
    .args(["validate", "--format", "json"])
    .current_dir(&wt_path)
    .output()
    .unwrap();
    assert!(
        !validate_non_ci.status.success(),
        "canonical_local_target=true on backend=cloud must fail closed: stdout={}",
        String::from_utf8_lossy(&validate_non_ci.stdout),
    );
    let non_ci_stdout = String::from_utf8_lossy(&validate_non_ci.stdout);
    assert!(
        non_ci_stdout.contains("LOCAL_TARGET_POLICY_UNACCEPTED"),
        "expected LOCAL_TARGET_POLICY_UNACCEPTED, got: {non_ci_stdout}"
    );
    assert!(
        !non_ci_stdout.contains("PUBLIC_CLOUD_BACKEND_UNAVAILABLE"),
        "unexpected PUBLIC_CLOUD_BACKEND_UNAVAILABLE: {non_ci_stdout}"
    );

    let validate_ci = apply_cloud_env(
        &mut Command::new(decapod_bin()),
        machine.path(),
        &srv.origin,
        "w2-agent",
    )
    .env("GITHUB_ACTIONS", "true")
    .env("DATASTORE", "sqlite")
    .args(["validate", "--format", "json"])
    .current_dir(&wt_path)
    .output()
    .unwrap();
    assert!(
        validate_ci.status.success(),
        "CI-local validate failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&validate_ci.stdout),
        String::from_utf8_lossy(&validate_ci.stderr),
    );
    let val_json: Value = serde_json::from_slice(&validate_ci.stdout).unwrap();
    assert_eq!(val_json["status"], "ok", "{val_json}");
    assert_eq!(val_json["report"]["fail_count"], 0, "{val_json}");
    assert_eq!(val_json["scope"], "ci_local", "{val_json}");

    // 6. Verify `decapod govern policy approve --scope repo.pr-readiness.v1` works on Cloud (not blocked by PUBLIC_CLOUD_BACKEND_UNAVAILABLE).
    let base_sha = String::from_utf8(
        run_git(
            &wt_path,
            &["rev-parse", "refs/remotes/origin/master^{commit}"],
        )
        .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    let head_sha = String::from_utf8(run_git(&wt_path, &["rev-parse", "HEAD^{commit}"]).stdout)
        .unwrap()
        .trim()
        .to_string();
    let accepted_cfg: toml::Value = toml::from_slice(
        &run_git(
            &wt_path,
            &["show", &format!("{base_sha}:.decapod/config.toml")],
        )
        .stdout,
    )
    .unwrap();
    let candidate_cfg: toml::Value = toml::from_slice(
        &run_git(
            &wt_path,
            &["show", &format!("{head_sha}:.decapod/config.toml")],
        )
        .stdout,
    )
    .unwrap();
    let transition_id = decapod::core::repository_profile::repo_policy_transition_id(
        &base_sha,
        &head_sha,
        accepted_cfg["proof"]["required_profile"]["reviewed_sources_digest"]
            .as_str()
            .unwrap(),
        candidate_cfg["proof"]["required_profile"]["reviewed_sources_digest"]
            .as_str()
            .unwrap(),
        accepted_cfg["proof"]["repository_contract"]["reviewed_commands_digest"]
            .as_str()
            .unwrap(),
        candidate_cfg["proof"]["repository_contract"]["reviewed_commands_digest"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let policy_approve = apply_cloud_env(
        &mut Command::new(decapod_bin()),
        machine.path(),
        &srv.origin,
        "w2-agent",
    )
    .args([
        "govern",
        "policy",
        "approve",
        "--id",
        &transition_id,
        "--scope",
        decapod::core::repository_profile::POLICY_TRANSITION_SCOPE,
    ])
    .current_dir(&wt_path)
    .output()
    .unwrap();
    assert!(
        policy_approve.status.success(),
        "govern policy approve on Cloud failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&policy_approve.stdout),
        String::from_utf8_lossy(&policy_approve.stderr),
    );
    assert!(
        String::from_utf8_lossy(&policy_approve.stdout)
            .contains("DECAPOD_REPO_TRANSITION_DECISION_ID="),
        "{}",
        String::from_utf8_lossy(&policy_approve.stdout)
    );

    // 7. Verify `workspace ensure --container` inside an already-running Cloud container relay succeeds without host credentials.
    use decapod::core::cloud_workspace::{
        CloudCustodyStore, CloudRelayExecutor, HostCloudRelayHandler,
    };
    use decapod::core::container_signer::{
        ContainerCloudRelayHandler, ContainerRelayOperation, ContainerSignerPolicy,
        HostContainerSignerServer, WorkspaceCredentialsConfig,
    };
    struct RelayExec {
        origin: String,
    }
    impl CloudRelayExecutor for RelayExec {
        fn execute(
            &self,
            operation: ContainerRelayOperation,
            rpc_op: &str,
            payload: &Value,
        ) -> Result<Value, decapod::core::error::DecapodError> {
            let connector = dactyl_db::CloudConnector::discover(&self.origin)
                .map_err(decapod::core::cloud_client::transport_error)?;
            let kind = match operation {
                ContainerRelayOperation::DactylQuery
                | ContainerRelayOperation::ProofControlRead => dactyl_db::OperationKind::Read,
                ContainerRelayOperation::DactylBatch
                | ContainerRelayOperation::ProofControlWrite => dactyl_db::OperationKind::Write,
            };
            connector
                .control_request(
                    rpc_op,
                    payload,
                    Some(managed::ACCESS),
                    kind,
                    dactyl_db::ResourceBudget::default(),
                )
                .map_err(decapod::core::cloud_client::transport_error)
        }
    }
    let custody = CloudCustodyStore::at(machine.path().join("custody"))
        .load(&repo, &wt_path)
        .unwrap()
        .expect("saved custody");
    let canonical_wt = wt_path.canonicalize().unwrap_or_else(|_| wt_path.clone());
    let relay_handler: Arc<dyn ContainerCloudRelayHandler> = Arc::new(HostCloudRelayHandler {
        project_id: PROJECT.to_string(),
        workspace_id: custody.workspace_id.clone(),
        worktree_path: canonical_wt,
        task_id: TASK_ID.to_string(),
        custody_id: custody.custody_id.clone(),
        invocation_id: "inv_inside_container".to_string(),
        admission: None,
        executor: Arc::new(RelayExec {
            origin: srv.origin.clone(),
        }),
    });
    let sock_path = tmp.path().join("inside-relay.sock");
    let _relay_server = HostContainerSignerServer::bind_with_relay(
        &sock_path,
        ContainerSignerPolicy {
            allowed_repository: PROJECT.to_string(),
            allowed_origin: srv.origin.clone(),
            allowed_agent_id: "w2-agent".to_string(),
            credentials: WorkspaceCredentialsConfig {
                git_transport: None,
                github_api: None,
                propodus_cloud: Some("signer-broker".to_string()),
            },
            session_expires_at: time::OffsetDateTime::now_utc().unix_timestamp() + 900,
            revoked: Arc::new(AtomicBool::new(false)),
            device_key_store: None,
            access_token_digest: None,
        },
        Some(relay_handler),
    )
    .unwrap();
    let empty_container_home = TempDir::new().unwrap();
    let in_container_ensure = apply_cloud_env(
        &mut Command::new(decapod_bin()),
        empty_container_home.path(),
        &srv.origin,
        "w2-agent",
    )
    .env("DECAPOD_CONTAINER", "1")
    .env("DECAPOD_CLOUD_CONTAINER_RELAY", "1")
    .env("DECAPOD_CONTAINER_SIGNER_SOCKET", &sock_path)
    .env("DECAPOD_CLOUD_PROJECT_ID", PROJECT)
    .env("DECAPOD_CLOUD_TASK_ID", TASK_ID)
    .env("DECAPOD_CLOUD_WORKSPACE_ID", &custody.workspace_id)
    .env("DECAPOD_CLOUD_CUSTODY_ID", &custody.custody_id)
    .env("DECAPOD_CLOUD_INVOCATION_ID", "inv_inside_container")
    .env(
        "DECAPOD_CLOUD_TASK_REVISION",
        custody.task_revision.unwrap().to_string(),
    )
    .env(
        "DECAPOD_CLOUD_FENCING_GENERATION",
        custody.fencing_generation.unwrap().to_string(),
    )
    .args(["workspace", "ensure", "--container"])
    .current_dir(&wt_path)
    .output()
    .unwrap();
    assert!(
        in_container_ensure.status.success(),
        "in-container workspace ensure --container failed: stderr={}\nstdout={}",
        String::from_utf8_lossy(&in_container_ensure.stderr),
        String::from_utf8_lossy(&in_container_ensure.stdout),
    );
    let in_container_json: Value = serde_json::from_slice(&in_container_ensure.stdout).unwrap();
    assert_eq!(in_container_json["in_container"], true);
    assert_eq!(in_container_json["can_work"], true);

    let in_container_validate = apply_cloud_env(
        &mut Command::new(decapod_bin()),
        empty_container_home.path(),
        &srv.origin,
        "w2-agent",
    )
    .env("DECAPOD_CONTAINER", "1")
    .env("DECAPOD_CLOUD_CONTAINER_RELAY", "1")
    .env("DECAPOD_CONTAINER_SIGNER_SOCKET", &sock_path)
    .env("DECAPOD_CLOUD_PROJECT_ID", PROJECT)
    .env("DECAPOD_CLOUD_TASK_ID", TASK_ID)
    .env("DECAPOD_CLOUD_WORKSPACE_ID", &custody.workspace_id)
    .env("DECAPOD_CLOUD_CUSTODY_ID", &custody.custody_id)
    .env("DECAPOD_CLOUD_INVOCATION_ID", "inv_inside_container")
    .env(
        "DECAPOD_CLOUD_TASK_REVISION",
        custody.task_revision.unwrap().to_string(),
    )
    .env(
        "DECAPOD_CLOUD_FENCING_GENERATION",
        custody.fencing_generation.unwrap().to_string(),
    )
    .env("GITHUB_ACTIONS", "true")
    .args(["validate", "--format", "json"])
    .current_dir(&wt_path)
    .output()
    .unwrap();
    assert!(
        in_container_validate.status.success(),
        "in-container validate failed: stderr={}\nstdout={}",
        String::from_utf8_lossy(&in_container_validate.stderr),
        String::from_utf8_lossy(&in_container_validate.stdout),
    );
    let in_container_val_json: Value =
        serde_json::from_slice(&in_container_validate.stdout).unwrap();
    assert_eq!(
        in_container_val_json["status"], "ok",
        "{in_container_val_json}"
    );
    assert_eq!(
        in_container_val_json["report"]["fail_count"], 0,
        "{in_container_val_json}"
    );
    assert_eq!(
        in_container_val_json["scope"], "ci_local",
        "{in_container_val_json}"
    );

    for (path, before) in &baselines {
        assert_eq!(
            sha256_file(path),
            *before,
            "existing SQLite/WAL file {} must stay bit-for-bit unchanged",
            path.display()
        );
    }
}
