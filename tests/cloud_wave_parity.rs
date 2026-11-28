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
        CustodyBindRequest, CustodyGetRequest, DactylProofControlTransport, custody_bind,
        custody_get, new_uuid_v4,
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
        // Expect 5 requests:
        // 1. GET /functions/v1/decapod-onboarding/config
        // 2. POST /rest/v1/rpc/propodus_repository_access_v1 (status)
        // 3. POST /rest/v1/rpc/propodus_repository_access_v1 (evaluate_repair_authority)
        // 4. POST /rest/v1/rpc/propodus_proof_acceptance_v1 (custody_bind)
        // 5. POST /rest/v1/rpc/propodus_proof_acceptance_get_v1 (get_custody)
        for _ in 0..5 {
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
                let current = path == "/rest/v1/rpc/propodus_proof_acceptance_get_v1";
                serde_json::json!({
                    "version": 1,
                    "profile": "candidate-acceptance-v1",
                    "status": if current { "custody_current" } else { "custody_bound" },
                    "operation_id": "2b7919d7-2efd-4854-bb6d-796759b9e71a",
                    "project_id": "example/repo",
                    "task_id": "resource:db-migration",
                    "workspace_id": "ws-1",
                    "custody_id": "custody-1",
                    "task_revision": 2,
                    "fencing_generation": 5,
                    "authority_generation": 2,
                    "expires_at": decapod::core::time::format_rfc3339_nanos(
                        time::OffsetDateTime::now_utc() + time::Duration::hours(1)
                    ).expect("expiry"),
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
            operation_id: "2b7919d7-2efd-4854-bb6d-796759b9e71a".into(),
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
    let current = custody_get(
        &transport,
        &CustodyGetRequest {
            project_id: "example/repo".into(),
            task_id: "resource:db-migration".into(),
            workspace_id: "ws-1".into(),
            custody_id: "custody-1".into(),
            task_revision: receipt.task_revision,
            fencing_generation: receipt.fencing_generation,
        },
    )
    .expect("fresh custody observation via Dactyl control_request");
    assert_eq!(current.task_revision, receipt.task_revision);
    assert_eq!(current.fencing_generation, receipt.fencing_generation);

    server_thread.join().expect("server thread");

    let calls = observed_headers.lock().expect("lock");
    assert_eq!(calls.len(), 5);
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

#[cfg(feature = "supabase-cloud")]
#[test]
fn cloud_delegated_grants_037_rpc_wire_contract() {
    use decapod::core::cloud_dpop::{DeviceDpopSigner, DeviceKeyMaterial, DeviceKeyStorageBackend};
    use decapod::core::db_maintenance::{
        CapabilityDecision, DbMaintenanceCapability, RepoAccessTier,
        evaluate_cloud_capability_with_delegated_grant, fetch_cloud_delegated_grant_status,
        fetch_cloud_repository_access_status, resolve_canonical_policy,
    };
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    let tmp = TempDir::new().expect("tempdir");
    let repo = tmp.path().join("cloud-repo");
    fs::create_dir_all(repo.join(".decapod")).expect("mkdir .decapod");
    git(&repo, &["init", "-b", "main"]);
    git(&repo, &["config", "user.name", "Test"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/DecapodLabs/decapod.git",
        ],
    );
    fs::write(
        repo.join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n[governance.db_maintenance]\nenabled = true\nallowed = [\"db.inspect\", \"db.verify\", \"db.backup\"]\n",
    )
    .expect("write config");
    git(&repo, &["add", ".decapod/config.toml"]);
    git(&repo, &["commit", "-m", "cloud config"]);

    let policy = resolve_canonical_policy(&repo).expect("resolve canonical cloud policy");
    let policy_sha = policy.policy_sha256.clone();
    let repo_id = policy.repo_identity.clone();

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let origin = format!("http://{addr}");
    let observed_paths: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let observed_clone = Arc::clone(&observed_paths);
    let origin_for_server = origin.clone();
    let server_policy_sha = policy_sha.clone();
    let server_repo_id = repo_id.clone();

    let server_thread = std::thread::spawn(move || {
        // 1. discovery
        // 2. propodus_repository_access_v1
        // 3. propodus_delegated_grants_v1 (active grant with future expires_at & policy_digest)
        // 4. propodus_delegated_grants_v1 (revoked grant)
        // 5. propodus_delegated_grants_v1 (expired via past expires_at)
        for _ in 0..5 {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = vec![0u8; 32768];
            let n = stream.read(&mut buf).expect("read");
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = req
                .lines()
                .next()
                .unwrap_or("")
                .split_whitespace()
                .nth(1)
                .unwrap_or("")
                .to_string();
            observed_clone.lock().expect("lock").push(path.clone());

            let future_exp = decapod::core::time::format_rfc3339_nanos(
                time::OffsetDateTime::now_utc() + time::Duration::hours(1),
            )
            .expect("future");
            let past_exp = decapod::core::time::format_rfc3339_nanos(
                time::OffsetDateTime::now_utc() - time::Duration::minutes(5),
            )
            .expect("past");

            let body = if path == "/functions/v1/decapod-onboarding/config" {
                serde_json::json!({
                    "version": 1,
                    "project_origin": origin_for_server,
                    "publishable_key": "sb_publishable_test_fixture",
                    "schema": "dactyl_api",
                })
                .to_string()
            } else if path == "/rest/v1/rpc/propodus_repository_access_v1" {
                serde_json::json!({
                    "version": 1,
                    "repository": server_repo_id,
                    "effective_tier": "write",
                    "seat_authorized": true,
                    "policy_bound": true,
                    "policy_revision": 4,
                    "policy_hash": server_policy_sha,
                    "authority_generation": 2,
                })
                .to_string()
            } else if req.contains("grant-revoked") {
                serde_json::json!({
                    "version": 1,
                    "status": "ok",
                    "grant": {
                        "grant_id": "grant-revoked",
                        "repository": server_repo_id,
                        "revoked": true,
                        "expires_at": future_exp,
                        "revocation_generation": 2,
                        "policy_generation": 4,
                        "session_generation": 3,
                        "policy_digest": server_policy_sha,
                        "allowed_operations": ["db.verify"]
                    }
                })
                .to_string()
            } else if req.contains("grant-expired") {
                serde_json::json!({
                    "version": 1,
                    "status": "ok",
                    "grant": {
                        "grant_id": "grant-expired",
                        "repository": server_repo_id,
                        "revoked": false,
                        "expires_at": past_exp,
                        "revocation_generation": 2,
                        "policy_generation": 4,
                        "session_generation": 3,
                        "policy_digest": server_policy_sha,
                        "allowed_operations": ["db.verify"]
                    }
                })
                .to_string()
            } else {
                serde_json::json!({
                    "version": 1,
                    "status": "ok",
                    "grant": {
                        "grant_id": "grant-active",
                        "repository": server_repo_id,
                        "revoked": false,
                        "expires_at": future_exp,
                        "revocation_generation": 2,
                        "policy_generation": 4,
                        "session_generation": 3,
                        "policy_digest": server_policy_sha,
                        "allowed_operations": ["db.verify"]
                    }
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
    let signer = Arc::new(DeviceDpopSigner::new(key));
    let connector = dactyl_db::CloudConnector::discover(&origin)
        .expect("discover")
        .with_proof_signer(signer);
    let bearer = "synthetic-dpop-access-token";

    let access =
        fetch_cloud_repository_access_status(&connector, bearer, &repo_id).expect("access status");
    assert_eq!(access.effective_tier, RepoAccessTier::Write);

    let active_grant =
        fetch_cloud_delegated_grant_status(&connector, bearer, &repo_id, "grant-active")
            .expect("active grant");
    assert!(!active_grant.expired);
    assert_eq!(
        active_grant.grant_policy_digest.as_deref(),
        Some(policy_sha.as_str())
    );
    assert_eq!(
        evaluate_cloud_capability_with_delegated_grant(
            &policy,
            DbMaintenanceCapability::Verify,
            Some(policy_sha.as_str()),
            Some(&access),
            Some(&active_grant),
        )
        .0,
        CapabilityDecision::Allowed
    );
    assert_eq!(
        evaluate_cloud_capability_with_delegated_grant(
            &policy,
            DbMaintenanceCapability::Backup,
            Some(policy_sha.as_str()),
            Some(&access),
            Some(&active_grant),
        )
        .1,
        "CLOUD_DELEGATED_OPERATION_NOT_GRANTED"
    );

    let revoked_grant =
        fetch_cloud_delegated_grant_status(&connector, bearer, &repo_id, "grant-revoked")
            .expect("revoked grant");
    assert_eq!(
        evaluate_cloud_capability_with_delegated_grant(
            &policy,
            DbMaintenanceCapability::Verify,
            Some(policy_sha.as_str()),
            Some(&access),
            Some(&revoked_grant),
        )
        .1,
        "CLOUD_DELEGATED_GRANT_REVOKED"
    );

    let expired_grant =
        fetch_cloud_delegated_grant_status(&connector, bearer, &repo_id, "grant-expired")
            .expect("expired grant");
    assert!(expired_grant.expired);
    assert_eq!(
        evaluate_cloud_capability_with_delegated_grant(
            &policy,
            DbMaintenanceCapability::Verify,
            Some(policy_sha.as_str()),
            Some(&access),
            Some(&expired_grant),
        )
        .1,
        "CLOUD_DELEGATED_GRANT_EXPIRED"
    );

    server_thread.join().expect("server");
    let paths = observed_paths.lock().expect("lock");
    assert_eq!(
        paths
            .iter()
            .filter(|p| p.as_str() == "/rest/v1/rpc/propodus_delegated_grants_v1")
            .count(),
        3
    );
}

#[cfg(feature = "supabase-cloud")]
#[test]
fn cli_and_rpc_db_maintenance_enforce_propodus_037_delegated_grants_end_to_end() {
    use decapod::core::cloud_dpop::verify_dpop_proof;
    use decapod::core::cloud_session::{Phase, Session, SessionState, SessionStore};
    use decapod::core::db_maintenance::resolve_canonical_policy;
    use decapod::core::repo_identity::resolve_repository_identity;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    const PROJECT: &str = "DecapodLabs/decapod";
    const HUMAN_JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.cli_grant_fixture.sig";
    const CHALLENGE_ID: &str = "11111111-1111-4111-8111-111111111111";
    const DEVICE_KEY_ID: &str = "22222222-2222-4222-8222-222222222222";
    const ACCESS_TOK: &str = "dpc_at_cli_grant_fixture_1";
    const REFRESH_TOK: &str = "dpc_rt_cli_grant_fixture_1";

    let tmp = TempDir::new().expect("tempdir");
    let home_dir = tmp.path().join("home");
    let xdg_dir = tmp.path().join("xdg");
    let repo = tmp.path().join("cloud-cli-repo");
    fs::create_dir_all(&home_dir).expect("mkdir home");
    fs::create_dir_all(&xdg_dir).expect("mkdir xdg");
    fs::create_dir_all(repo.join(".decapod/workspaces")).expect("mkdir workspaces");

    git(&repo, &["init", "-b", "master"]);
    git(&repo, &["config", "user.name", "Test"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/DecapodLabs/decapod.git",
        ],
    );
    let isolated_cmd = |cwd: &Path, agent_id: &str, password: Option<&str>| -> Command {
        let mut cmd = Command::new(decapod_bin());
        for key in [
            "GITHUB_ACTIONS",
            "BUILDKITE",
            "GITHUB_EVENT_NAME",
            "GITHUB_WORKSPACE",
            "BUILDKITE_BUILD_CHECKOUT_PATH",
            "CHANGE_EVENT",
            "CHANGE_BASE",
            "CHANGE_HEAD",
            "DECAPOD_VALIDATE_REQUIRE_CHANGE_CONTEXT",
            "DECAPOD_VALIDATE_SKIP_GIT_GATES",
            "DECAPOD_VALIDATE_SKIP_TOOLING_GATES",
            "DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES",
            "DECAPOD_SESSION_PASSWORD",
            "DECAPOD_CLOUD_TEST_ORIGIN",
            "DECAPOD_TEST_MODE",
            "DECAPOD_USER_ID",
            "DECAPOD_ROLE",
        ] {
            cmd.env_remove(key);
        }
        cmd.current_dir(cwd)
            .env("HOME", &home_dir)
            .env("XDG_CONFIG_HOME", &xdg_dir)
            .env("XDG_DATA_HOME", &xdg_dir)
            .env("DECAPOD_CLAIM_AUTORUN", "0")
            .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1")
            .env("DECAPOD_AGENT_ID", agent_id);
        if let Some(pw) = password {
            cmd.env("DECAPOD_SESSION_PASSWORD", pw);
        }
        cmd
    };

    // Acquire local agent session first, remove any bootstrap local decapod.db,
    // then commit canonical [repo].backend = "cloud" before spawning the worktree.
    fs::write(
        repo.join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"local\"\n[governance.db_maintenance]\nenabled = true\nallowed = [\"db.inspect\", \"db.verify\", \"db.backup\"]\n",
    )
    .expect("write local bootstrap config");
    let acq = isolated_cmd(&repo, "cloud-ops", None)
        .args(["session", "acquire"])
        .output()
        .expect("session acquire");
    assert!(
        acq.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&acq.stderr)
    );
    let agent_pw = String::from_utf8_lossy(&acq.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("Password: ").map(|s| s.trim().to_string()))
        .expect("session password");
    let _ = fs::remove_dir_all(repo.join(".decapod/data"));

    fs::write(
        repo.join(".decapod/config.toml"),
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n[governance.db_maintenance]\nenabled = true\nallowed = [\"db.inspect\", \"db.verify\", \"db.backup\"]\n",
    )
    .expect("write config");
    git(&repo, &["add", ".decapod/config.toml"]);
    git(&repo, &["commit", "-m", "cloud config"]);

    let worktree = repo.join(".decapod/workspaces/cloud-ops-todo-01m8fx1234-grant");
    git(
        &repo,
        &[
            "worktree",
            "add",
            worktree.to_str().unwrap(),
            "-b",
            "agent/cloud-ops/01m8fx1234-grant",
        ],
    );

    let policy = resolve_canonical_policy(&worktree).expect("resolve canonical cloud policy");
    let policy_sha = policy.policy_sha256.clone();
    let repo_id = policy.repo_identity.clone();

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    listener.set_nonblocking(true).expect("nonblocking");
    let addr = listener.local_addr().expect("local addr");
    let origin = format!("http://{addr}");
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop);
    let observed_control_ops: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let ops_clone = Arc::clone(&observed_control_ops);
    let server_origin = origin.clone();
    let server_policy_sha = policy_sha.clone();
    let server_repo_id = repo_id.clone();

    let server_thread = std::thread::spawn(move || {
        let mut enrolled_jkt: Option<String> = None;
        let mut session_nonce_issued = false;
        while !stop_clone.load(Ordering::Relaxed) {
            let (mut stream, _) = match listener.accept() {
                Ok(conn) => conn,
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(_) => break,
            };
            stream.set_nonblocking(false).expect("blocking");
            let mut bytes = Vec::new();
            let mut buf = [0u8; 4096];
            let header_end;
            loop {
                let n = match stream.read(&mut buf) {
                    Ok(0) => {
                        header_end = None;
                        break;
                    }
                    Ok(n) => n,
                    Err(_) => {
                        header_end = None;
                        break;
                    }
                };
                bytes.extend_from_slice(&buf[..n]);
                if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    header_end = Some(pos + 4);
                    break;
                }
            }
            let Some(body_start) = header_end else {
                continue;
            };
            let header_raw = String::from_utf8_lossy(&bytes[..body_start]).to_string();
            let mut headers = BTreeMap::new();
            for line in header_raw.lines().skip(1) {
                if let Some((k, v)) = line.split_once(':') {
                    headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
                }
            }
            let content_len: usize = headers
                .get("content-length")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            while bytes.len() < body_start + content_len {
                let n = stream.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    break;
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
            let body: Value = if content_len == 0 {
                Value::Null
            } else {
                serde_json::from_slice(&bytes[body_start..body_start + content_len])
                    .unwrap_or(Value::Null)
            };
            let auth = headers.get("authorization").cloned();
            let dpop = headers.get("dpop").cloned();
            let now_secs = time::OffsetDateTime::now_utc().unix_timestamp();

            let future_exp = decapod::core::time::format_rfc3339_nanos(
                time::OffsetDateTime::now_utc() + time::Duration::hours(1),
            )
            .expect("future");
            let past_exp = decapod::core::time::format_rfc3339_nanos(
                time::OffsetDateTime::now_utc() - time::Duration::minutes(5),
            )
            .expect("past");

            let mut extra_headers = String::new();
            let (status, resp): (u16, Value) = match path.as_str() {
                "/functions/v1/decapod-onboarding/config" => (
                    200,
                    json!({
                        "version": 1,
                        "project_origin": server_origin,
                        "publishable_key": "sb_publishable_test_fixture",
                        "schema": "dactyl_api",
                    }),
                ),
                "/functions/v1/decapod-onboarding/account-status" => {
                    if auth.as_deref() != Some(&format!("Bearer {HUMAN_JWT}")) || dpop.is_some() {
                        (401, json!({"error": {"code": "invalid_token"}}))
                    } else {
                        (
                            200,
                            json!({
                                "version": 1,
                                "account_id": "00000000-0000-4000-8000-000000000404",
                                "project_id": PROJECT,
                                "repository": PROJECT,
                                "organization_id": "00000000-0000-4000-8000-000000000001",
                                "status": "active",
                                "backend_authorized": true,
                                "resource_limits": {
                                    "plan_tier": "free",
                                    "paid_overages": {"status": "healthy"}
                                }
                            }),
                        )
                    }
                }
                "/functions/v1/decapod-onboarding/device-challenge" => (
                    200,
                    json!({
                        "version": 1,
                        "challenge_id": CHALLENGE_ID,
                        "nonce": "challenge-nonce-1234567890",
                        "audience": server_origin,
                    }),
                ),
                "/functions/v1/decapod-onboarding/device-enroll" => {
                    let jwt = dpop.as_deref().expect("DPoP header on device-enroll");
                    let full_url = format!("{server_origin}{path}");
                    let verified = verify_dpop_proof(
                        jwt,
                        &method,
                        &full_url,
                        Some(HUMAN_JWT),
                        Some("challenge-nonce-1234567890"),
                        Some(&server_origin),
                        now_secs,
                    )
                    .expect("valid device-enroll DPoP proof");
                    enrolled_jkt = Some(verified.jkt.clone());
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
                "/functions/v1/decapod-onboarding/device-session" => {
                    if !session_nonce_issued {
                        session_nonce_issued = true;
                        extra_headers.push_str("DPoP-Nonce: server-nonce-session-1\r\n");
                        (
                            401,
                            json!({"error": {"code": "use_dpop_nonce", "message": "DPoP nonce required"}}),
                        )
                    } else {
                        let jwt = dpop.as_deref().expect("DPoP header on device-session");
                        let full_url = format!("{server_origin}{path}");
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
                        assert_eq!(enrolled_jkt.as_deref(), Some(verified.jkt.as_str()));
                        (
                            200,
                            json!({
                                "version": 1,
                                "token_type": "DPoP",
                                "access_token": ACCESS_TOK,
                                "refresh_token": REFRESH_TOK,
                                "expires_in": 900,
                                "device_key_id": DEVICE_KEY_ID,
                            }),
                        )
                    }
                }
                "/functions/v1/decapod-onboarding/cloud-control"
                | "/rest/v1/rpc/propodus_repository_access_v1"
                | "/rest/v1/rpc/propodus_delegated_grants_v1" => {
                    let jwt = dpop.as_deref().expect("DPoP header on cloud-control");
                    let full_url = format!("{server_origin}{path}");
                    let verified = verify_dpop_proof(
                        jwt,
                        &method,
                        &full_url,
                        Some(ACCESS_TOK),
                        None,
                        None,
                        now_secs,
                    )
                    .expect("valid cloud-control DPoP proof");
                    assert_eq!(auth.as_deref(), Some(&*format!("DPoP {ACCESS_TOK}")));
                    assert_eq!(enrolled_jkt.as_deref(), Some(verified.jkt.as_str()));

                    let (fn_name, req_payload) =
                        if path == "/functions/v1/decapod-onboarding/cloud-control" {
                            (
                                body["function"].as_str().unwrap_or("").to_string(),
                                &body["request"],
                            )
                        } else {
                            (path.rsplit('/').next().unwrap_or("").to_string(), &body)
                        };
                    ops_clone.lock().expect("lock").push(fn_name.clone());

                    if fn_name == "propodus_repository_access_v1" {
                        (
                            200,
                            json!({
                                "version": 1,
                                "repository": server_repo_id,
                                "effective_tier": "write",
                                "seat_authorized": true,
                                "policy_bound": true,
                                "policy_revision": 5,
                                "policy_hash": server_policy_sha,
                                "accepted_commit": "deadbeef",
                                "max_delegated_tier": "write",
                                "require_owner_for_repair": true,
                                "authority_generation": 3,
                                "repair_eligible": false
                            }),
                        )
                    } else if fn_name == "propodus_delegated_grants_v1" {
                        let grant_id = req_payload["grant_id"].as_str().unwrap_or("");
                        match grant_id {
                            "grant-active" => (
                                200,
                                json!({
                                    "grant_id": "grant-active",
                                    "repository": server_repo_id,
                                    "revoked": false,
                                    "expires_at": future_exp,
                                    "revocation_generation": 3,
                                    "observed_revocation_generation": 3,
                                    "policy_generation": 5,
                                    "observed_policy_generation": 5,
                                    "session_generation": 2,
                                    "observed_session_generation": 2,
                                    "grant_policy_digest": server_policy_sha,
                                    "allowed_operations": ["db.verify"]
                                }),
                            ),
                            "grant-revoked" => (
                                200,
                                json!({
                                    "grant_id": "grant-revoked",
                                    "repository": server_repo_id,
                                    "revoked": true,
                                    "expires_at": future_exp,
                                    "revocation_generation": 3,
                                    "observed_revocation_generation": 3,
                                    "policy_generation": 5,
                                    "observed_policy_generation": 5,
                                    "session_generation": 2,
                                    "observed_session_generation": 2,
                                    "grant_policy_digest": server_policy_sha,
                                    "allowed_operations": ["db.verify"]
                                }),
                            ),
                            "grant-expired" => (
                                200,
                                json!({
                                    "grant_id": "grant-expired",
                                    "repository": server_repo_id,
                                    "revoked": false,
                                    "expires_at": past_exp,
                                    "revocation_generation": 3,
                                    "observed_revocation_generation": 3,
                                    "policy_generation": 5,
                                    "observed_policy_generation": 5,
                                    "session_generation": 2,
                                    "observed_session_generation": 2,
                                    "grant_policy_digest": server_policy_sha,
                                    "allowed_operations": ["db.verify"]
                                }),
                            ),
                            _ => (404, json!({"error": {"code": "grant_not_found"}})),
                        }
                    } else {
                        (404, json!({"error": {"code": "unknown_control_op"}}))
                    }
                }
                _ => (404, json!({"error": {"code": "not_found"}})),
            };

            let body_str = resp.to_string();
            let http = format!(
                "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body_str}",
                body_str.len()
            );
            let _ = stream.write_all(http.as_bytes());
        }
    });

    // 1. Before any Cloud human session is seeded, CLI fails closed with CLOUD_PRINCIPAL_UNVERIFIED.
    let unauth_verify = isolated_cmd(&worktree, "cloud-ops", Some(&agent_pw))
        .env("DECAPOD_TEST_MODE", "1")
        .env("DECAPOD_CLOUD_TEST_ORIGIN", &origin)
        .args(["data", "db", "maintain", "verify"])
        .output()
        .expect("unauth verify");
    assert!(!unauth_verify.status.success());
    assert!(
        String::from_utf8_lossy(&unauth_verify.stderr).contains("CLOUD_PRINCIPAL_UNVERIFIED"),
        "stderr was: {}",
        String::from_utf8_lossy(&unauth_verify.stderr)
    );

    // 2. Seed a human Cloud session on the isolated XDG store (host/local test env).
    let identity = resolve_repository_identity(&repo).expect("identity");
    assert_eq!(identity.canonical_name, PROJECT);
    {
        // Write SessionStore under isolated HOME / XDG_DATA_HOME.
        let store_path_dir = xdg_dir.join("decapod/cloud/sessions");
        let _ = store_path_dir;
        // Let's seed using `SessionStore::for_project` only via process env guard? No: `tests/cloud_wave_parity.rs` has no ENV_LOCK yet,
        // or we can set env under a Mutex and drop before spawning child processes.
        static SEED_LOCK: Mutex<()> = Mutex::new(());
        let _guard = SEED_LOCK.lock().unwrap();
        let prev_home = std::env::var_os("HOME");
        let prev_xdg_cfg = std::env::var_os("XDG_CONFIG_HOME");
        let prev_xdg_data = std::env::var_os("XDG_DATA_HOME");
        unsafe {
            std::env::set_var("HOME", &home_dir);
            std::env::set_var("XDG_CONFIG_HOME", &xdg_dir);
            std::env::set_var("XDG_DATA_HOME", &xdg_dir);
        }
        let store = SessionStore::for_project(&origin, PROJECT, &repo).expect("session store");
        let mut lock = store.lock(Duration::from_secs(3)).expect("lock");
        lock.replace(SessionState {
            phase: Phase::Ready,
            session: Some(Session {
                access_token: HUMAN_JWT.to_string(),
                refresh_token: Some("human_rt_fixture".to_string()),
                expires_at: time::OffsetDateTime::now_utc().unix_timestamp() + 3600,
            }),
            bootstrap: None,
        })
        .expect("seed human session");
        unsafe {
            match prev_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            match prev_xdg_cfg {
                Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
            match prev_xdg_data {
                Some(v) => std::env::set_var("XDG_DATA_HOME", v),
                None => std::env::remove_var("XDG_DATA_HOME"),
            }
        }
    }

    // 3. `data db capabilities --delegated-grant-id grant-active` exposes live narrowed grants via CLI.
    let cli_caps = isolated_cmd(&worktree, "cloud-ops", Some(&agent_pw))
        .env("DECAPOD_TEST_MODE", "1")
        .env("DECAPOD_CLOUD_TEST_ORIGIN", &origin)
        .args([
            "data",
            "db",
            "capabilities",
            "--delegated-grant-id",
            "grant-active",
        ])
        .output()
        .expect("cli capabilities");
    assert!(
        cli_caps.status.success(),
        "cli capabilities failed: {}",
        String::from_utf8_lossy(&cli_caps.stderr)
    );
    let caps_json: Value = serde_json::from_slice(&cli_caps.stdout).expect("caps json");
    assert_eq!(caps_json["backend"], "cloud");
    assert_eq!(caps_json["database"], "cloud:propodus");
    assert_eq!(caps_json["cloud_authority"]["principal_verified"], true);
    assert_eq!(
        caps_json["cloud_authority"]["delegated_grant_resolved"],
        true
    );
    assert_eq!(
        caps_json["cloud_authority"]["delegated_grant_id"],
        "grant-active"
    );
    let cap_entries = caps_json["capabilities"].as_array().expect("entries");
    let verify_entry = cap_entries
        .iter()
        .find(|e| e["capability"] == "db.verify")
        .expect("db.verify entry");
    assert_eq!(verify_entry["decision"], "allowed");
    assert_eq!(verify_entry["reason_code"], "CLOUD_TIER_ALLOWED");
    let backup_entry = cap_entries
        .iter()
        .find(|e| e["capability"] == "db.backup")
        .expect("db.backup entry");
    assert_eq!(backup_entry["decision"], "denied");
    assert_eq!(
        backup_entry["reason_code"],
        "CLOUD_DELEGATED_OPERATION_NOT_GRANTED"
    );

    // 4. JSON-RPC `data.database.capabilities` enforces the exact same contract.
    let rpc_caps = isolated_cmd(&worktree, "cloud-ops", Some(&agent_pw))
        .env("DECAPOD_TEST_MODE", "1")
        .env("DECAPOD_CLOUD_TEST_ORIGIN", &origin)
        .args([
            "rpc",
            "--op",
            "data.database.capabilities",
            "--params",
            r#"{"delegated_grant_id":"grant-active"}"#,
        ])
        .output()
        .expect("rpc capabilities");
    assert!(
        rpc_caps.status.success(),
        "rpc capabilities failed: {}",
        String::from_utf8_lossy(&rpc_caps.stderr)
    );
    let rpc_caps_json: Value = serde_json::from_slice(&rpc_caps.stdout).expect("rpc caps json");
    assert_eq!(rpc_caps_json["success"], true);
    assert_eq!(
        rpc_caps_json["result"]["cloud_authority"]["delegated_grant_id"],
        "grant-active"
    );

    // 5. Granted capability passes admission and refuses local SQLite fallback (never creating decapod.db).
    let cli_verify_active = isolated_cmd(&worktree, "cloud-ops", Some(&agent_pw))
        .env("DECAPOD_TEST_MODE", "1")
        .env("DECAPOD_CLOUD_TEST_ORIGIN", &origin)
        .args([
            "data",
            "db",
            "maintain",
            "verify",
            "--delegated-grant-id",
            "grant-active",
        ])
        .output()
        .expect("verify active grant");
    assert!(!cli_verify_active.status.success());
    assert!(
        String::from_utf8_lossy(&cli_verify_active.stderr)
            .contains("CLOUD_LOCAL_SQLITE_UNSUPPORTED"),
        "stderr was: {}",
        String::from_utf8_lossy(&cli_verify_active.stderr)
    );

    // 6. Ungranted capability fails admission BEFORE execution via RPC.
    let rpc_backup_denied = isolated_cmd(&worktree, "cloud-ops", Some(&agent_pw))
        .env("DECAPOD_TEST_MODE", "1")
        .env("DECAPOD_CLOUD_TEST_ORIGIN", &origin)
        .args([
            "rpc",
            "--op",
            "data.database.backup",
            "--params",
            r#"{"destination":".decapod/backups/cloud-deny.db","delegated_grant_id":"grant-active"}"#,
        ])
        .output()
        .expect("rpc backup denied");
    assert!(!rpc_backup_denied.status.success());
    let rpc_backup_str = format!(
        "{}{}",
        String::from_utf8_lossy(&rpc_backup_denied.stdout),
        String::from_utf8_lossy(&rpc_backup_denied.stderr)
    );
    assert!(
        rpc_backup_str.contains("CLOUD_DELEGATED_OPERATION_NOT_GRANTED"),
        "output was: {rpc_backup_str}"
    );

    // 7. Revoked, expired, and missing delegated grants fail closed.
    let cli_revoked = isolated_cmd(&worktree, "cloud-ops", Some(&agent_pw))
        .env("DECAPOD_TEST_MODE", "1")
        .env("DECAPOD_CLOUD_TEST_ORIGIN", &origin)
        .args([
            "data",
            "db",
            "maintain",
            "verify",
            "--delegated-grant-id",
            "grant-revoked",
        ])
        .output()
        .expect("verify revoked grant");
    assert!(!cli_revoked.status.success());
    assert!(
        String::from_utf8_lossy(&cli_revoked.stderr).contains("CLOUD_DELEGATED_GRANT_REVOKED"),
        "stderr was: {}",
        String::from_utf8_lossy(&cli_revoked.stderr)
    );

    let rpc_expired = isolated_cmd(&worktree, "cloud-ops", Some(&agent_pw))
        .env("DECAPOD_TEST_MODE", "1")
        .env("DECAPOD_CLOUD_TEST_ORIGIN", &origin)
        .args([
            "rpc",
            "--op",
            "data.database.verify",
            "--params",
            r#"{"delegated_grant_id":"grant-expired"}"#,
        ])
        .output()
        .expect("rpc verify expired");
    assert!(!rpc_expired.status.success());
    let rpc_expired_str = format!(
        "{}{}",
        String::from_utf8_lossy(&rpc_expired.stdout),
        String::from_utf8_lossy(&rpc_expired.stderr)
    );
    assert!(
        rpc_expired_str.contains("CLOUD_DELEGATED_GRANT_EXPIRED"),
        "output was: {rpc_expired_str}"
    );

    let cli_no_grant = isolated_cmd(&worktree, "cloud-ops", Some(&agent_pw))
        .env("DECAPOD_TEST_MODE", "1")
        .env("DECAPOD_CLOUD_TEST_ORIGIN", &origin)
        .args(["data", "db", "maintain", "verify"])
        .output()
        .expect("verify missing grant");
    assert!(!cli_no_grant.status.success());
    assert!(
        String::from_utf8_lossy(&cli_no_grant.stderr).contains("CLOUD_DELEGATED_GRANT_REQUIRED"),
        "stderr was: {}",
        String::from_utf8_lossy(&cli_no_grant.stderr)
    );

    stop.store(true, Ordering::Relaxed);
    let _ = server_thread.join();

    let ops = observed_control_ops.lock().expect("lock");
    assert!(ops.contains(&"propodus_repository_access_v1".to_string()));
    assert!(ops.contains(&"propodus_delegated_grants_v1".to_string()));
    assert!(
        !repo.join(".decapod/data/decapod.db").exists(),
        "cloud db maintenance must never create main repo .decapod/data/decapod.db"
    );
    assert!(
        !worktree.join(".decapod/data/decapod.db").exists(),
        "cloud db maintenance must never create worktree .decapod/data/decapod.db"
    );
}
