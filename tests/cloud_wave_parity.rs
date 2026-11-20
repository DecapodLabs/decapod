use decapod::core::backend::{BackendRoute, StorageContext};
use decapod::core::cloud_dpop::DeviceKeyStore;
use decapod::core::dactyl_todo::DactylTodoStore;
use decapod::core::storage::{Task, TodoStore};
use decapod::core::todo::{TodoCommand, add_task, get_task, list_tasks};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Barrier};
use tempfile::TempDir;
use time::OffsetDateTime;

fn decapod_bin() -> std::path::PathBuf {
    let binary = Path::new(env!("CARGO_BIN_EXE_decapod"));
    if let Ok(resolved) = binary.canonicalize() {
        return resolved;
    }
    if let Some(root) = std::env::var_os("RUNFILES_DIR")
        && let Ok(resolved) = Path::new(&root).join("_main/decapod").canonicalize()
    {
        return resolved;
    }
    if let Some(parent) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        && let Ok(resolved) = parent.join("decapod").canonicalize()
    {
        return resolved;
    }
    binary.to_path_buf()
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .expect("git must execute");
    assert!(status.success(), "git {args:?} failed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resource_scoped_task_race_has_single_winner_renew_and_deterministic_release() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("dactyl-resource.db");
    let db_str = db_path.to_string_lossy().to_string();
    let conn =
        dactyl_db::Connection::open(dactyl_db::DatastoreRoute::sqlite(db_str)).expect("open");
    conn.atomic(&[
        dactyl_db::Operation::write(
            "CREATE TABLE tasks (
                repo_id TEXT NOT NULL,
                id TEXT PRIMARY KEY,
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
                version INTEGER NOT NULL
            )",
            vec![],
        ),
        dactyl_db::Operation::write(
            "CREATE TABLE events (
                event_id TEXT PRIMARY KEY,
                ts TEXT NOT NULL,
                seq INTEGER NOT NULL,
                stream TEXT NOT NULL,
                subject_kind TEXT NOT NULL,
                subject_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                actor TEXT NOT NULL
            )",
            vec![],
        ),
    ])
    .expect("create schema");

    let route = BackendRoute::Local { path: db_path };
    let context = StorageContext::from_route(route, None).expect("local storage context");
    let store = DactylTodoStore::new(context, "DecapodLabs/decapod");

    let now = OffsetDateTime::now_utc();
    store
        .add_task(
            Task {
                id: "todo_res_1".to_string(),
                hash: "res1".to_string(),
                repo_id: "DecapodLabs/decapod".to_string(),
                title: "Exclusive shared resource".to_string(),
                description: None,
                status: "open".to_string(),
                assignee: None,
                scope: "resource:integration:master".to_string(),
                dir_path: String::new(),
                priority: "high".to_string(),
                category: String::new(),
                tags: vec![],
                created_at: now,
                updated_at: now,
                version: 1,
            },
            "owner-agent".to_string(),
            "intent:res1".to_string(),
        )
        .await
        .expect("add resource task");

    let store_a = store.clone();
    let store_b = store.clone();
    let barrier = Arc::new(Barrier::new(2));
    let b1 = Arc::clone(&barrier);
    let b2 = Arc::clone(&barrier);

    let h1 = std::thread::spawn(move || {
        b1.wait();
        store_a.claim_task_with_lease_blocking("todo_res_1", "agent-alpha".to_string(), 120)
    });
    let h2 = std::thread::spawn(move || {
        b2.wait();
        store_b.claim_task_with_lease_blocking("todo_res_1", "agent-beta".to_string(), 120)
    });

    let r1 = h1.join().expect("thread 1");
    let r2 = h2.join().expect("thread 2");
    let winners: Vec<Task> = [r1, r2].into_iter().filter_map(Result::ok).collect();
    assert_eq!(winners.len(), 1, "at most one agent may win resource claim");
    let winner_actor = winners[0].assignee.clone().expect("winner assignee");
    assert!(store.custody_fence_for("todo_res_1").is_some());

    // Heartbeat renewal succeeds for winner and fails for non-holder.
    let renewed = store
        .renew_task_lease_blocking("todo_res_1", winner_actor.clone(), 300)
        .expect("winner renews lease");
    assert_eq!(renewed.lease_ttl_seconds, 300);
    let loser = if winner_actor == "agent-alpha" {
        "agent-beta"
    } else {
        "agent-alpha"
    };
    assert!(
        store
            .renew_task_lease_blocking("todo_res_1", loser.to_string(), 300)
            .is_err()
    );

    // Deterministic release clears fence and allows next claim.
    store
        .release_task("todo_res_1", winner_actor)
        .await
        .expect("release");
    assert!(store.custody_fence_for("todo_res_1").is_none());
    let next = store
        .claim_task("todo_res_1", loser.to_string())
        .await
        .expect("next agent claims after release");
    assert_eq!(next.assignee.as_deref(), Some(loser));
}

#[test]
fn inline_todo_helpers_under_cloud_backend_never_return_unavailable_or_touch_sqlite() {
    let tmp = TempDir::new().expect("tempdir");
    let repo = tmp.path().join("repo");
    let xdg = tmp.path().join("xdg");
    fs::create_dir_all(&repo).expect("repo");
    fs::create_dir_all(&xdg).expect("xdg");
    git(&repo, &["init"]);
    git(&repo, &["config", "user.name", "Decapod Test"]);
    git(&repo, &["config", "user.email", "test@example.invalid"]);
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/DecapodLabs/decapod.git",
        ],
    );
    fs::create_dir_all(repo.join(".decapod")).expect(".decapod");
    fs::write(
        repo.join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n",
    )
    .expect("write config");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "init"]);

    let store_root = repo.join(".decapod/data");
    let add_err = add_task(
        &store_root,
        &TodoCommand::Add {
            title: "Inline cloud task".to_string(),
            description: String::new(),
            priority: "medium".to_string(),
            tags: String::new(),
            owner: String::new(),
            due: None,
            r#ref: String::new(),
            scope: "repo".to_string(),
            dir: None,
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .expect_err("unauthenticated inline add must fail closed");
    assert!(
        !add_err
            .to_string()
            .contains("PUBLIC_CLOUD_BACKEND_UNAVAILABLE"),
        "unexpected unavailable stub: {add_err}"
    );
    assert!(!store_root.join("decapod.db").exists());

    let list_err = list_tasks(&store_root, None, None, None, None, None)
        .expect_err("unauthenticated inline list must fail closed");
    assert!(
        !list_err
            .to_string()
            .contains("PUBLIC_CLOUD_BACKEND_UNAVAILABLE")
    );
    let get_err =
        get_task(&store_root, "todo_1").expect_err("unauthenticated inline get must fail closed");
    assert!(
        !get_err
            .to_string()
            .contains("PUBLIC_CLOUD_BACKEND_UNAVAILABLE")
    );
    assert!(!store_root.join("decapod.db").exists());
}

#[test]
fn cli_cloud_status_logout_revoke_and_workspace_credentials_report() {
    let tmp = TempDir::new().expect("tempdir");
    let repo = tmp.path().join("repo");
    let xdg = tmp.path().join("xdg");
    fs::create_dir_all(&repo).expect("repo");
    fs::create_dir_all(&xdg).expect("xdg");
    git(&repo, &["init"]);
    git(&repo, &["config", "user.name", "Decapod Test"]);
    git(&repo, &["config", "user.email", "test@example.invalid"]);
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/DecapodLabs/decapod.git",
        ],
    );
    fs::create_dir_all(repo.join(".decapod")).expect(".decapod");
    fs::write(
        repo.join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n[workspace.credentials]\ngit_transport = \"https-token\"\ngithub_api = \"gh-token\"\npropodus_cloud = \"signer-broker\"\n",
    )
    .expect("write config");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "init"]);

    let origin = decapod::core::cloud_client::selected_origin().expect("default origin");
    let ks = DeviceKeyStore::with_root_dir(
        &origin,
        "DecapodLabs/decapod",
        &repo,
        &xdg.join("decapod/device-keys"),
    )
    .expect("keystore");
    let created = ks.load_or_create().expect("create device key");

    let out = Command::new(decapod_bin())
        .current_dir(&repo)
        .env("DECAPOD_AGENT_ID", "cloud-wave-tester")
        .env("DECAPOD_GROUP_BROKER_DISABLE", "1")
        .env("XDG_DATA_HOME", &xdg)
        .env("HOME", &xdg)
        .args(["cloud", "status"])
        .output()
        .expect("cloud status");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let status_json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(status_json["device_key_present"], true);
    assert_eq!(status_json["device_jkt"], created.jkt);

    let ws_out = Command::new(decapod_bin())
        .current_dir(&repo)
        .env("DECAPOD_AGENT_ID", "cloud-wave-tester")
        .env("DECAPOD_GROUP_BROKER_DISABLE", "1")
        .env("XDG_DATA_HOME", &xdg)
        .env("HOME", &xdg)
        .args(["workspace", "status"])
        .output()
        .expect("workspace status");
    assert!(
        ws_out.status.success(),
        "{}",
        String::from_utf8_lossy(&ws_out.stderr)
    );
    let ws_json: serde_json::Value = serde_json::from_slice(&ws_out.stdout).expect("ws json");
    assert_eq!(
        ws_json["workspace"]["credentials"]["git_transport"]["status"],
        "available"
    );
    assert_eq!(
        ws_json["workspace"]["credentials"]["github_api"]["status"],
        "available"
    );
    assert_eq!(
        ws_json["workspace"]["credentials"]["propodus_cloud"]["status"],
        "available"
    );

    let logout_out = Command::new(decapod_bin())
        .current_dir(&repo)
        .env("DECAPOD_AGENT_ID", "cloud-wave-tester")
        .env("DECAPOD_GROUP_BROKER_DISABLE", "1")
        .env("XDG_DATA_HOME", &xdg)
        .env("HOME", &xdg)
        .args(["cloud", "--format", "json", "logout"])
        .output()
        .expect("cloud logout");
    assert!(
        logout_out.status.success(),
        "{}",
        String::from_utf8_lossy(&logout_out.stderr)
    );
    let logout_json: serde_json::Value = serde_json::from_slice(&logout_out.stdout).expect("json");
    assert_eq!(logout_json["status"], "logged_out");
    assert!(
        ks.load().expect("reload").is_some(),
        "logout keeps enrolled device key"
    );

    let revoke_out = Command::new(decapod_bin())
        .current_dir(&repo)
        .env("DECAPOD_AGENT_ID", "cloud-wave-tester")
        .env("DECAPOD_GROUP_BROKER_DISABLE", "1")
        .env("XDG_DATA_HOME", &xdg)
        .env("HOME", &xdg)
        .args(["cloud", "--format", "json", "revoke"])
        .output()
        .expect("cloud revoke");
    assert!(
        revoke_out.status.success(),
        "{}",
        String::from_utf8_lossy(&revoke_out.stderr)
    );
    let revoke_json: serde_json::Value = serde_json::from_slice(&revoke_out.stdout).expect("json");
    assert_eq!(revoke_json["status"], "revoked");
    assert_eq!(revoke_json["revoked_device_jkt"], created.jkt);
    assert!(ks.load().expect("reload").is_none());
}

#[test]
fn local_owner_repair_db_recover_enforces_tier_policy_and_executes_when_opted_in() {
    use decapod::core::dactyl::DactylBridge;
    use decapod::core::db_maintenance::{
        CapabilityDecision, DbMaintenanceCapability, VerifiedMaintenanceActor,
        evaluate_capability_decision, resolve_canonical_policy, run_maintain_recover,
    };

    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    let data = repo.join(".decapod/data");
    fs::create_dir_all(&data).expect("data dir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&data, fs::Permissions::from_mode(0o700)).expect("chmod 0700");
    }

    let run_git = |args: &[&str]| {
        let out = Command::new("git")
            .current_dir(&repo)
            .args(args)
            .output()
            .expect("git");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    };
    run_git(&["init", "-b", "main"]);
    run_git(&["config", "user.name", "Test"]);
    run_git(&["config", "user.email", "test@example.com"]);
    run_git(&[
        "remote",
        "add",
        "origin",
        "https://github.com/example/owner-repair.git",
    ]);

    // Seed canonical config with owner_repair_enabled = false first.
    let write_and_commit_config = |owner_repair_enabled: bool| {
        fs::write(
            repo.join(".decapod/config.toml"),
            format!(
                "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"local\"\n[governance.db_maintenance]\nenabled = true\nowner_repair_enabled = {owner_repair_enabled}\nrequired_tier = \"owner\"\nallowed = [\"db.inspect\", \"db.verify\", \"db.backup\", \"db.recover\"]\n[governance.rbac.actor_roles]\nowner-actor = [\"owner\"]\nwriter-actor = [\"write\"]\n"
            ),
        )
        .expect("write config");
        run_git(&["add", ".decapod/config.toml"]);
        run_git(&["commit", "-m", "update maintenance config"]);
    };
    write_and_commit_config(false);

    // Seed a valid canonical local SQLite database.
    {
        let db = DactylBridge::open_canonical(&data, dactyl_db::AccessMode::ReadWrite)
            .expect("open local canonical db");
        db.write(
            "CREATE TABLE IF NOT EXISTS owner_repair_probe (id INTEGER PRIMARY KEY, val TEXT)",
            &[],
        )
        .expect("create table");
        db.write(
            "INSERT INTO owner_repair_probe(id, val) VALUES (1, 'preserved')",
            &[],
        )
        .expect("insert row");
    }

    let owner_actor = VerifiedMaintenanceActor {
        agent_id: "owner-actor".to_string(),
        session_token: "test-token".to_string(),
        task_id: None,
        worktree_id: "wt-owner".to_string(),
        branch: "agent/owner/repair".to_string(),
    };
    let writer_actor = VerifiedMaintenanceActor {
        agent_id: "writer-actor".to_string(),
        session_token: "test-token".to_string(),
        task_id: None,
        worktree_id: "wt-writer".to_string(),
        branch: "agent/writer/repair".to_string(),
    };

    // 1. Policy decision denies db.recover when owner_repair_enabled = false.
    let disabled_policy = resolve_canonical_policy(&repo).expect("disabled policy");
    let (decision, _, _) =
        evaluate_capability_decision(&disabled_policy, DbMaintenanceCapability::Recover, None);
    assert_eq!(decision, CapabilityDecision::Denied);

    // 2. Enable owner_repair_enabled = true on canonical main, and test unmerged violation.
    write_and_commit_config(true);
    let mut unmerged_policy = resolve_canonical_policy(&repo).expect("enabled policy");
    let enabled_policy = unmerged_policy.clone();
    unmerged_policy.unmerged_branch_violation = Some("branch diverged from main".to_string());
    let (decision, _, _) =
        evaluate_capability_decision(&unmerged_policy, DbMaintenanceCapability::Recover, None);
    assert_eq!(decision, CapabilityDecision::Denied);

    // 3. Policy decision allows db.recover when owner_repair_enabled = true and merged,
    //    but run_maintain_recover denies non-owner RBAC actor.
    let (decision, _, _) =
        evaluate_capability_decision(&enabled_policy, DbMaintenanceCapability::Recover, None);
    assert_eq!(decision, CapabilityDecision::Allowed);

    let archive_denied = Path::new(".decapod/backups/denied-archive.db");
    let (code, msg) = run_maintain_recover(&enabled_policy, &writer_actor, Some(archive_denied))
        .expect_err("must deny non-owner role");
    assert_eq!(code, "OWNER_REPAIR_AUTHORITY_REQUIRED");
    assert!(msg.contains("owner"), "unexpected message: {msg}");
    assert!(!repo.join(archive_denied).exists());

    // 4. Allowed when owner + merged + project opted in.
    // On Linux/Android Dactyl executes full atomic dump/reload recovery; on
    // macOS Dactyl fails closed with an explicit platform capability error
    // without touching or corrupting the original database.
    let archive_ok = Path::new(".decapod/backups/owner-archive.db");
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let (report, evidence) =
            run_maintain_recover(&enabled_policy, &owner_actor, Some(archive_ok))
                .expect("local owner repair must succeed when project opted in");
        assert_eq!(report["status"], "ok");
        assert_eq!(report["operation"], "recover");
        assert_eq!(report["automatic_repair"], false);
        assert!(evidence.is_some());
        assert!(
            repo.join(archive_ok).exists(),
            "original database must be archived"
        );
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        let (code, msg) = run_maintain_recover(&enabled_policy, &owner_actor, Some(archive_ok))
            .expect_err("non-Linux platforms fail closed at Dactyl atomic rename boundary");
        assert_eq!(code, "RECOVER_FAILED");
        assert!(
            msg.contains("Capability") || msg.contains("atomic no-replace rename"),
            "unexpected non-Linux recovery error: {msg}"
        );
    }

    // Verify table data remains intact after owner-repair check/execution.
    let db = DactylBridge::open_canonical(&data, dactyl_db::AccessMode::ReadOnly)
        .expect("reopen recovered db");
    let rows = db
        .read("SELECT val FROM owner_repair_probe WHERE id = 1", &[])
        .expect("query recovered row");
    assert_eq!(rows.as_slice().len(), 1);
    assert_eq!(
        rows.as_slice()[0].get::<usize, String>(0).expect("val"),
        "preserved"
    );
}

#[cfg(feature = "supabase-cloud")]
#[test]
fn dactyl_018_dpop_signer_and_control_request_wire_contract() {
    use decapod::core::cloud_dpop::{
        DeviceDpopSigner, DeviceKeyMaterial, DeviceKeyStorageBackend, verify_dpop_proof,
    };
    use decapod::core::cloud_proof::{
        CustodyBindRequest, DactylProofControlTransport, custody_bind, new_uuid_v4,
    };
    use decapod::core::db_maintenance::{
        RepoAccessTier, evaluate_cloud_repair_authority, fetch_cloud_repository_access_status,
    };
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let origin = format!("http://{addr}");
    type ObservedCall = (String, Option<String>, Option<String>);
    let observed_headers: Arc<Mutex<Vec<ObservedCall>>> = Arc::new(Mutex::new(Vec::new()));
    let observed_clone = Arc::clone(&observed_headers);
    let origin_for_server = origin.clone();

    let server_thread = std::thread::spawn(move || {
        // Expect 4 requests:
        // 1. GET /functions/v1/decapod-onboarding/config
        // 2. POST /rest/v1/rpc/propodus_repository_access_v1 (status)
        // 3. POST /rest/v1/rpc/propodus_repository_access_v1 (evaluate_repair_authority)
        // 4. POST /rest/v1/rpc/propodus_proof_acceptance_v1 (custody_bind)
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = vec![0u8; 32768];
            let n = stream.read(&mut buf).expect("read");
            let req = String::from_utf8_lossy(&buf[..n]);
            let first_line = req.lines().next().unwrap_or("");
            let path = first_line
                .split_whitespace()
                .nth(1)
                .unwrap_or("")
                .to_string();
            let mut auth = None;
            let mut dpop = None;
            for line in req.lines().skip(1) {
                if line.is_empty() {
                    break;
                }
                if let Some((k, v)) = line.split_once(':') {
                    if k.eq_ignore_ascii_case("authorization") {
                        auth = Some(v.trim().to_string());
                    } else if k.eq_ignore_ascii_case("dpop") {
                        dpop = Some(v.trim().to_string());
                    }
                }
            }
            observed_clone
                .lock()
                .expect("lock")
                .push((path.clone(), auth, dpop));

            let body = if path == "/functions/v1/decapod-onboarding/config" {
                serde_json::json!({
                    "version": 1,
                    "project_origin": origin_for_server,
                    "publishable_key": "sb_publishable_test_fixture",
                    "schema": "dactyl_api",
                })
                .to_string()
            } else if path == "/rest/v1/rpc/propodus_repository_access_v1" {
                if req.contains("\"evaluate_repair_authority\"") {
                    serde_json::json!({
                        "version": 1,
                        "decision": "allowed",
                        "effective_tier": "owner",
                        "owner_repair_enabled": true,
                        "policy_active": true,
                    })
                    .to_string()
                } else {
                    serde_json::json!({
                        "version": 1,
                        "effective_tier": "owner",
                        "owner_repair_enabled": true,
                        "policy_active": true,
                        "policy_hash": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                        "authority_generation": 2,
                    })
                    .to_string()
                }
            } else {
                serde_json::json!({
                    "task_revision": 2,
                    "fencing_generation": 5,
                    "expires_at": "2026-11-19T00:00:00Z",
                })
                .to_string()
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).expect("write");
        }
    });

    let key = DeviceKeyMaterial::generate(DeviceKeyStorageBackend::PrivateFile0600).expect("key");
    let expected_jkt = key.jkt.clone();
    let signer = Arc::new(DeviceDpopSigner::new(key));
    let connector = dactyl_db::CloudConnector::discover(&origin)
        .expect("discover canonical decapod-onboarding config")
        .with_proof_signer(signer);
    let bearer = "synthetic-dpop-access-token";

    let status = fetch_cloud_repository_access_status(&connector, bearer, "example/repo")
        .expect("repository access status");
    assert_eq!(status.effective_tier, RepoAccessTier::Owner);
    assert!(status.repair_eligible);
    assert!(status.policy_bound);
    assert_eq!(status.authority_generation, Some(2));

    let eval = evaluate_cloud_repair_authority(
        &connector,
        bearer,
        "example/repo",
        &new_uuid_v4().expect("uuid"),
        false,
        "owner repair preflight test",
    )
    .expect("evaluate repair authority");
    assert_eq!(eval["decision"], "allowed");

    let transport = DactylProofControlTransport::new(connector, bearer.to_string());
    let receipt = custody_bind(
        &transport,
        &CustodyBindRequest {
            project_id: "example/repo".to_string(),
            operation_id: new_uuid_v4().expect("uuid"),
            task_id: "resource:db-migration".to_string(),
            workspace_id: "ws-1".to_string(),
            custody_id: "custody-1".to_string(),
            native_task_version: 2,
            ttl_seconds: 600,
        },
    )
    .expect("custody bind via Dactyl control_request");
    assert_eq!(receipt.task_revision, 2);
    assert_eq!(receipt.fencing_generation, 5);

    server_thread.join().expect("server thread");

    let calls = observed_headers.lock().expect("lock");
    assert_eq!(calls.len(), 4);
    assert_eq!(calls[0].0, "/functions/v1/decapod-onboarding/config");
    assert!(calls[0].1.is_none(), "discovery must not carry auth header");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    for (path, auth, dpop) in calls.iter().skip(1) {
        assert_eq!(
            auth.as_deref(),
            Some("DPoP synthetic-dpop-access-token"),
            "control_request {path} must use Authorization: DPoP"
        );
        let proof = dpop.as_deref().expect("DPoP header required");
        let verified = verify_dpop_proof(
            proof,
            "POST",
            &format!("{origin}{path}"),
            Some(bearer),
            None,
            None,
            now,
        )
        .expect("DPoP proof must verify against method, htu, and ath");
        assert_eq!(verified.jkt, expected_jkt);
    }
}
