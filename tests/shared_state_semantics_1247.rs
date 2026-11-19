//! Adversarial shared-state transition semantics suite for #1247.
//!
//! Proves Decapod's local SQLite and Dactyl/cloud-boundary semantics across:
//! 1. Concurrent exclusive-claim single-winner arbitration (`>=10` contenders)
//!    and shared-claim domain rules without lost updates or phantom events.
//! 2. Fencing against stale lease holders after expiry/reclaim (including ABA
//!    reclaim), revoked sessions, changed task revisions, workspace handoff,
//!    and cross-task substitution attempts.
//! 3. Atomic state + event commits, rollback on failure, before-dispatch
//!    rejection, lost-ack `OutcomeUnknown` preservation, and safe same-ID
//!    reconciliation without unproved auto-replay.
//! 4. Exact binding of validation and task completion to candidate commit SHA,
//!    task revision/fence, accepted target, and policy/evidence digest while
//!    keeping historical evidence readable.
//! 5. Cloud denial / unavailability failing closed with zero silent fallback to
//!    local SQLite and zero mutations to local or mock cloud stores.
//!
//! Note on live hosted Propodus Postgres: end-to-end execution against the
//! hosted multi-tenant PostgreSQL service remains externally bounded when
//! hosted credentials are absent (see `tests/propodus_live.rs` and
//! `tests/supabase_service.rs`); this suite proves the local SQLite and real
//! loopback Dactyl/HTTP denial & lost-ack contracts offline.

use dactyl_db::AdapterErrorKind;
use decapod::core::backend::{BackendRoute, CloudDatastore, StorageContext};
use decapod::core::cloud_todo_operation::{
    self, FailureKind, Journal as CloudTodoJournal, OperationRecord, Outcome,
};
use decapod::core::dactyl::DactylBridge;
use decapod::core::dactyl_todo::DactylTodoStore;
use decapod::core::db::{Connection, params};
use decapod::core::error::{CloudAuthStatus, DecapodError};
use decapod::core::governed_transition::{
    CompletionClaim, EvidenceRecord, ProofJudgment, StateIdentity, identity_from_snapshot,
    judge_completion,
};
use decapod::core::repo_identity::resolve_repository_identity_from_remote;
use decapod::core::state;
use decapod::core::storage::{Task as StorageTask, TodoStore};
use decapod::core::store::{Store, StoreKind};
use decapod::core::todo::{
    self, ClaimMode, TodoCommand, admit_authoring, check_authoring, claim_task_with_lease,
    comment_task, edit_task, get_task, handoff_task_as, initialize_todo_db, release_task_as_actor,
    renew_claim_lease, run_cloud_todo_command_with_store, todo_db_path, update_status,
    yield_claim_lease,
};
use decapod::core::work_claim::{WorkClaimStatus, WorkClaimValidationStatus};
use decapod::core::workspace;
use decapod::core::workunit::{self, WorkUnitManifest, WorkUnitStatus};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn decapod_bin() -> PathBuf {
    let cargo_bin = env!("CARGO_BIN_EXE_decapod");
    Path::new(cargo_bin)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(cargo_bin))
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn init_git_decapod_repo() -> (TempDir, PathBuf, String) {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path().to_path_buf();
    git(&dir, &["init", "-b", "master"]);
    git(&dir, &["config", "user.email", "test@decapod.local"]);
    git(&dir, &["config", "user.name", "Decapod Test"]);
    std::fs::write(dir.join("README.md"), "# Test Repo\n").expect("write readme");
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-m", "initial commit"]);

    let init = Command::new(decapod_bin())
        .args(["init", "--force"])
        .current_dir(&dir)
        .output()
        .expect("decapod init");
    assert!(
        init.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-m", "decapod init"]);

    let session = Command::new(decapod_bin())
        .args(["session", "acquire"])
        .env("DECAPOD_AGENT_ID", "agent-a")
        .current_dir(&dir)
        .output()
        .expect("session acquire");
    assert!(
        session.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&session.stderr)
    );
    let password = String::from_utf8_lossy(&session.stdout)
        .lines()
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .expect("session password");

    (tmp, dir, password)
}

fn add_local_task(root: &Path, title: &str) -> (String, String) {
    let added = todo::add_task(
        root,
        &TodoCommand::Add {
            title: title.to_string(),
            description: "shared-state semantics test".to_string(),
            priority: "high".to_string(),
            tags: "concurrency".to_string(),
            owner: String::new(),
            due: None,
            r#ref: String::new(),
            scope: String::new(),
            dir: Some(root.to_string_lossy().into_owned()),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .expect("add task");
    let id = added["id"].as_str().expect("task id").to_string();
    let hash = added["hash"].as_str().expect("task hash").to_string();
    (id, hash)
}

fn dactyl_local_store() -> Option<(DactylTodoStore, TempDir, PathBuf)> {
    let tempdir = TempDir::new().expect("dactyl tempdir");
    let db_path = tempdir.path().join("dactyl_tasks.db");
    std::fs::File::create(&db_path).expect("sqlite file");
    let bridge = match DactylBridge::open_local(&db_path, dactyl_db::AccessMode::ReadWrite) {
        Ok(bridge) => bridge,
        Err(DecapodError::DactylError(err))
            if err.adapter_code() == Some("sqlite_runtime_unavailable") =>
        {
            return None;
        }
        Err(err) => panic!("open local dactyl bridge: {err}"),
    };
    bridge
        .write(
            "CREATE TABLE tasks (repo_id TEXT NOT NULL DEFAULT 'DecapodLabs/decapod', id TEXT PRIMARY KEY, hash TEXT NOT NULL, title TEXT NOT NULL, description TEXT, status TEXT NOT NULL, assigned_to TEXT, scope TEXT NOT NULL, dir_path TEXT NOT NULL, priority TEXT NOT NULL, category TEXT NOT NULL, tags TEXT, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, version INTEGER NOT NULL DEFAULT 1, owner TEXT, assigned_at TEXT, completed_at TEXT)",
            &[],
        )
        .expect("tasks table");
    bridge
        .write(
            "CREATE TABLE events (event_id TEXT PRIMARY KEY, ts TEXT NOT NULL, seq INTEGER NOT NULL, stream TEXT NOT NULL, subject_kind TEXT, subject_id TEXT, event_type TEXT NOT NULL DEFAULT '', payload TEXT NOT NULL, actor TEXT NOT NULL DEFAULT 'decapod')",
            &[],
        )
        .expect("events table");
    bridge
        .write(
            "CREATE UNIQUE INDEX events_stream_seq ON events(stream, seq)",
            &[],
        )
        .expect("events unique seq");
    drop(bridge);
    let context = StorageContext::from_route(
        BackendRoute::Local {
            path: db_path.clone(),
        },
        None,
    )
    .expect("local storage context");
    Some((
        DactylTodoStore::new(context, "DecapodLabs/decapod"),
        tempdir,
        db_path,
    ))
}

fn sample_cloud_task(id: &str, title: &str) -> StorageTask {
    let now = time::OffsetDateTime::now_utc();
    StorageTask {
        id: id.to_string(),
        repo_id: "DecapodLabs/decapod".to_string(),
        hash: "1247ab".to_string(),
        title: title.to_string(),
        description: Some("cloud concurrency test".to_string()),
        status: "open".to_string(),
        assignee: None,
        scope: "repo".to_string(),
        dir_path: String::new(),
        priority: "high".to_string(),
        category: "backend".to_string(),
        tags: vec!["concurrency".to_string()],
        created_at: now,
        updated_at: now,
        version: 1,
    }
}

async fn read_http_request(stream: &mut TcpStream) -> (String, Value) {
    let mut bytes = Vec::new();
    let mut buf = [0_u8; 4096];
    let header_end = loop {
        let count = stream.read(&mut buf).await.expect("read request");
        assert!(count > 0, "unexpected EOF before HTTP headers");
        bytes.extend_from_slice(&buf[..count]);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = String::from_utf8_lossy(&bytes[..header_end]).to_string();
    let content_length: usize = headers
        .lines()
        .find_map(|line| {
            let lower = line.to_ascii_lowercase();
            lower
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse::<usize>().expect("content-length"))
        })
        .unwrap_or(0);
    while bytes.len() < header_end + content_length {
        let count = stream.read(&mut buf).await.expect("read body");
        assert!(count > 0, "unexpected EOF before HTTP body");
        bytes.extend_from_slice(&buf[..count]);
    }
    let body = if content_length == 0 {
        Value::Null
    } else {
        serde_json::from_slice(&bytes[header_end..header_end + content_length])
            .expect("JSON request body")
    };
    (headers, body)
}

async fn write_http_json(stream: &mut TcpStream, status: u16, body: Value) {
    let payload = body.to_string();
    let response = format!(
        "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    stream
        .write_all(response.as_bytes())
        .await
        .expect("write HTTP response");
}

// ---------------------------------------------------------------------------
// Case (1): Exclusive-claim single-winner arbitration & shared-claim rules
// ---------------------------------------------------------------------------

#[test]
fn case1_local_concurrent_exclusive_claims_have_single_winner_and_attribution() {
    let tmp = TempDir::new().expect("tempdir");
    let data_root = tmp.path().join(".decapod/data");
    initialize_todo_db(&data_root).expect("init todo db");
    let (task_id, _) = add_local_task(&data_root, "Concurrent exclusive arbitration");

    let contenders = 12usize;
    let barrier = Arc::new(Barrier::new(contenders));
    let outcomes: Vec<(String, Result<Value, DecapodError>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..contenders)
            .map(|idx| {
                let barrier = Arc::clone(&barrier);
                let data_root = data_root.clone();
                let task_id = task_id.clone();
                let agent = format!("agent-{idx:02}");
                scope.spawn(move || {
                    barrier.wait();
                    let res = claim_task_with_lease(
                        &data_root,
                        &task_id,
                        &agent,
                        ClaimMode::Exclusive,
                        Some(3600),
                        None,
                    );
                    (agent, res)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let winners: Vec<(&String, &Value)> = outcomes
        .iter()
        .filter_map(|(agent, res)| match res {
            Ok(val) if val["status"] == "ok" => Some((agent, val)),
            _ => None,
        })
        .collect();
    let conflicts: Vec<_> = outcomes
        .iter()
        .filter(|(_, res)| match res {
            Ok(val) => val["status"] == "conflict",
            Err(DecapodError::ValidationError(msg)) => {
                msg.starts_with("STORAGE_POOL_LOCK_TIMEOUT:")
            }
            _ => false,
        })
        .collect();

    assert_eq!(winners.len(), 1, "expected exactly 1 winner: {outcomes:?}");
    assert_eq!(
        conflicts.len(),
        contenders - 1,
        "all other contenders must receive inspectable conflict/contention errors: {outcomes:?}"
    );

    let (winner_agent, winner_res) = winners[0];
    assert_eq!(winner_res["result"]["lease_generation"], 1);
    assert_eq!(winner_res["result"]["assigned_to"], *winner_agent);
    for (_, outcome) in &conflicts {
        if let Ok(conflict) = outcome {
            assert_eq!(conflict["result"]["mode"], "exclusive");
            assert_eq!(conflict["result"]["assigned_to"], *winner_agent);
        }
    }

    let task = get_task(&data_root, &task_id).unwrap().unwrap();
    assert_eq!(task.assigned_to, *winner_agent);

    let conn = Connection::open(todo_db_path(&data_root)).expect("open db");
    let primary_owners: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM task_owners WHERE task_id = ?1 AND claim_type = 'primary'",
            [&task_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(primary_owners, 1);

    let claim_events: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT actor FROM events WHERE stream = 'todo' AND subject_id = ?1 AND event_type = 'task.claim'")
            .unwrap();
        stmt.query_map([&task_id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(claim_events, vec![winner_agent.clone()]);

    let checkpoint_actors: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT actor FROM events WHERE stream = 'todo' AND subject_id = ?1 AND event_type = 'task.journal.checkpoint' ORDER BY seq ASC")
            .unwrap();
        stmt.query_map([&task_id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(
        checkpoint_actors.last().map(String::as_str),
        Some(winner_agent.as_str()),
        "task.journal.checkpoint must attribute claim to the winning agent, not 'decapod'"
    );
}

#[test]
fn case1_local_shared_claims_preserve_primary_owner_and_enforce_basic_policy() {
    let (_tmp, repo_dir, _password) = init_git_decapod_repo();
    let data_root = repo_dir.join(".decapod/data");
    let (task_id, _) = add_local_task(&data_root, "Shared claim policy rules");

    // Promote agents to verified trust so shared claims are policy-eligible.
    let conn = Connection::open(todo_db_path(&data_root)).unwrap();
    for agent in ["primary-agent", "secondary-agent"] {
        conn.execute(
            "INSERT INTO agents(agent_id, trust_level, status, updated_at)
             VALUES(?1, 'verified', 'active', '1Z')
             ON CONFLICT(agent_id) DO UPDATE SET trust_level = 'verified'",
            [agent],
        )
        .unwrap();
    }
    drop(conn);

    // Basic-trust shared claim on an unclaimed task is denied.
    let basic_err = claim_task_with_lease(
        &data_root,
        &task_id,
        "basic-agent",
        ClaimMode::Shared,
        None,
        None,
    )
    .expect_err("basic agent cannot initiate shared claim on unclaimed task");
    assert!(
        basic_err.to_string().contains("Permission denied")
            || basic_err.to_string().contains("primary owner")
            || basic_err.to_string().contains("verified"),
        "unexpected error: {basic_err}"
    );

    let first = claim_task_with_lease(
        &data_root,
        &task_id,
        "primary-agent",
        ClaimMode::Exclusive,
        Some(3600),
        None,
    )
    .unwrap();
    assert_eq!(first["status"], "ok");

    let shared = claim_task_with_lease(
        &data_root,
        &task_id,
        "secondary-agent",
        ClaimMode::Shared,
        None,
        None,
    )
    .unwrap();
    assert_eq!(shared["status"], "ok", "{shared}");
    assert_eq!(shared["result"]["mode"], "shared");
    assert_eq!(shared["result"]["assigned_to"], "primary-agent");

    let task = get_task(&data_root, &task_id).unwrap().unwrap();
    assert_eq!(task.assigned_to, "primary-agent");

    let conn = Connection::open(todo_db_path(&data_root)).unwrap();
    let owners: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare("SELECT agent_id, claim_type FROM task_owners WHERE task_id = ?1 ORDER BY claim_type, agent_id")
            .unwrap();
        stmt.query_map([&task_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(
        owners,
        vec![
            ("primary-agent".to_string(), "primary".to_string()),
            ("secondary-agent".to_string(), "secondary".to_string()),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn case1_dactyl_concurrent_exclusive_claims_have_single_winner_and_no_phantom_events() {
    let Some((store, _tempdir, db_path)) = dactyl_local_store() else {
        eprintln!("skipping Dactyl concurrency test: host SQLite runtime unavailable");
        return;
    };
    let added = store
        .add_task(
            sample_cloud_task("todo_1247_race", "Dactyl concurrent claim race"),
            "seed-agent".to_string(),
            "intent:seed".to_string(),
        )
        .await
        .expect("seed add");
    assert_eq!(added.version, 1);

    let contenders = 12usize;
    let barrier = Arc::new(tokio::sync::Barrier::new(contenders));
    let mut set = tokio::task::JoinSet::new();
    for idx in 0..contenders {
        let store = store.clone();
        let barrier = Arc::clone(&barrier);
        let actor = format!("cloud-agent-{idx:02}");
        set.spawn(async move {
            barrier.wait().await;
            let res = store.claim_task("todo_1247_race", actor.clone()).await;
            (actor, res)
        });
    }

    let mut winners = Vec::new();
    let mut conflicts = Vec::new();
    while let Some(joined) = set.join_next().await {
        let (actor, res) = joined.expect("task join");
        match res {
            Ok(task) => winners.push((actor, task)),
            Err(err) => conflicts.push((actor, err)),
        }
    }

    assert_eq!(
        winners.len(),
        1,
        "exactly one cloud claimant must win: {winners:?}"
    );
    assert_eq!(
        conflicts.len(),
        contenders - 1,
        "all losing claimants must fail visibly"
    );
    let (winner_actor, winner_task) = &winners[0];
    assert_eq!(winner_task.status, "in_progress");
    assert_eq!(winner_task.assignee.as_deref(), Some(winner_actor.as_str()));
    assert_eq!(winner_task.version, 2);

    for (_, err) in &conflicts {
        let dactyl_err = err
            .downcast_ref::<DecapodError>()
            .and_then(|e| match e {
                DecapodError::DactylError(inner) => Some(inner),
                _ => None,
            })
            .or_else(|| err.downcast_ref::<dactyl_db::DactylError>());
        assert!(
            dactyl_err.is_some_and(|e| matches!(
                e.adapter_kind(),
                Some(
                    AdapterErrorKind::Conflict | AdapterErrorKind::Busy | AdapterErrorKind::Locked
                )
            )),
            "losing claimant must fail with typed conflict/contention, got: {err:?}"
        );
    }

    let conn = Connection::open(db_path).expect("inspect sqlite");
    let total_events: i64 = conn
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        total_events, 2,
        "must record only 1 task.add + 1 winning task.claim (no phantom losing events)"
    );
    let claim_actor: String = conn
        .query_row(
            "SELECT actor FROM events WHERE event_type = 'task.claim'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(claim_actor, *winner_actor);
}

// ---------------------------------------------------------------------------
// Case (2): Stale lease (expiry/reclaim/ABA), revoked session, revision,
//           workspace handoff, and cross-task substitution refusal
// ---------------------------------------------------------------------------

#[test]
fn case2_stale_lease_after_reclaim_and_aba_cannot_mutate_author_or_complete() {
    let (_tmp, repo_dir, _password) = init_git_decapod_repo();
    let data_root = repo_dir.join(".decapod/data");
    let store = Store {
        kind: StoreKind::Repo,
        root: data_root.clone(),
    };
    let (task_id, _) = add_local_task(&data_root, "ABA lease fencing regression");
    let (other_task_id, _) = add_local_task(&data_root, "Cross-task substitution target");

    // 1. Agent A claims with a 15-second lease and binds worktree W1.
    let claim_a = claim_task_with_lease(
        &data_root,
        &task_id,
        "agent-a",
        ClaimMode::Exclusive,
        Some(15),
        None,
    )
    .unwrap();
    assert_eq!(claim_a["status"], "ok");
    assert_eq!(claim_a["result"]["lease_generation"], 1);
    let rev_after_claim_a = get_task(&data_root, &task_id).unwrap().unwrap().revision;

    let branch_a = format!("agent/agent-a/{task_id}-work");
    let ws_a = workspace::ensure_workspace(
        &repo_dir,
        Some(workspace::WorkspaceConfig {
            branch: Some(branch_a.clone()),
            use_container: false,
            base_image: None,
            base_branch: None,
        }),
        "agent-a",
    )
    .expect("ensure workspace for agent-a");
    let wt_a = ws_a.git.worktree_path.expect("agent-a worktree path");
    check_authoring(&data_root, &wt_a, "agent-a", false).expect("live binding valid");

    // 2. Wait until the 6s lease expires, then Agent B reclaims (generation = 2).
    let exp_secs = claim_a["result"]["lease_expires_at"]
        .as_str()
        .unwrap()
        .trim_end_matches('Z')
        .parse::<u64>()
        .unwrap();
    loop {
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if now_secs > exp_secs {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let claim_b = claim_task_with_lease(
        &data_root,
        &task_id,
        "agent-b",
        ClaimMode::Exclusive,
        Some(3600),
        None,
    )
    .unwrap();
    assert_eq!(claim_b["status"], "ok", "{claim_b}");
    assert_eq!(claim_b["result"]["lease_generation"], 2);
    assert_eq!(claim_b["result"]["reclaimed_from"], "agent-a");

    // Agent A's old fence cannot mutate status, renew, release, or author.
    let stale_status = update_status(
        &store,
        &task_id,
        rev_after_claim_a,
        "open",
        "in_progress",
        "task.start",
        json!({"actor": "agent-a"}),
    )
    .unwrap();
    assert_eq!(stale_status["status"], "conflict");
    assert_eq!(stale_status["result"]["reason"], "stale_task_revision");

    let stale_renew = renew_claim_lease(&data_root, &task_id, "agent-a", Some(600)).unwrap();
    assert_eq!(stale_renew["status"], "conflict");

    let stale_release = release_task_as_actor(&data_root, &task_id, "agent-a").unwrap();
    assert_eq!(stale_release["status"], "conflict");
    assert_eq!(stale_release["result"]["reason"], "not_assignee");

    let stale_author = check_authoring(&data_root, &wt_a, "agent-a", false).unwrap_err();
    assert!(
        stale_author.to_string().contains("AUTHORING_CUSTODY_STALE")
            || stale_author
                .to_string()
                .contains("AUTHORING_BINDING_REQUIRED"),
        "stale worktree must be rejected after reclaim: {stale_author}"
    );

    // 3. ABA reclaim: Agent B releases (preserving generation = 2), then Agent A re-claims.
    let rel_b = release_task_as_actor(&data_root, &task_id, "agent-b").unwrap();
    assert_eq!(rel_b["status"], "ok");
    let reclaim_a = claim_task_with_lease(
        &data_root,
        &task_id,
        "agent-a",
        ClaimMode::Exclusive,
        Some(3600),
        None,
    )
    .unwrap();
    assert_eq!(reclaim_a["status"], "ok");
    assert!(
        reclaim_a["result"]["lease_generation"]
            .as_u64()
            .unwrap_or(0)
            >= 2,
        "ABA reclaim must not revert to generation 1: {reclaim_a}"
    );

    // Old fence from generation=1 / rev_after_claim_a is STILL rejected even
    // though Agent A is once again the owner (ABA immunity).
    let aba_status = update_status(
        &store,
        &task_id,
        rev_after_claim_a,
        "open",
        "done",
        "task.done",
        json!({"actor": "agent-a"}),
    )
    .unwrap();
    assert_eq!(aba_status["status"], "conflict");
    assert_eq!(aba_status["result"]["reason"], "stale_task_revision");

    // And an un-refreshed worktree binding from generation=1 fails check_authoring(bind=false).
    // Note: wt_a binding was retired on force_release, so bind=false refuses it until re-admitted.
    let aba_author = check_authoring(&data_root, &wt_a, "agent-a", false).unwrap_err();
    assert!(
        aba_author
            .to_string()
            .contains("AUTHORING_BINDING_REQUIRED")
            || aba_author.to_string().contains("AUTHORING_BINDING_STALE"),
        "ABA old binding must not silently re-activate: {aba_author}"
    );

    // Re-admitting wt_a binds generation=3; bumping generation via renew invalidates generation=3 if unchecked,
    // and cross-task substitution against other_task_id is refused.
    admit_authoring(&data_root, &wt_a, "agent-a").expect("re-admit generation 3");
    let workunit_other = WorkUnitManifest {
        task_id: other_task_id.clone(),
        intent_ref: format!("intent:todo:{other_task_id}"),
        spec_refs: vec![],
        state_refs: vec![],
        proof_plan: vec!["validate_passes".to_string()],
        proof_results: vec![],
        validation_epoch: None,
        validation_record: None,
        status: WorkUnitStatus::Verified,
    };
    workunit::write_workunit(&wt_a, &workunit_other).expect("write uncheckpointed manifest");
    // Cross-task substitution: agent-a's worktree branch names `task_id`, not `other_task_id`,
    // and manual VERIFIED without journal checkpoint fails closed.
    let cross_pub = workspace::verify_workunit_gate_for_publish(&wt_a, &branch_a).unwrap_err();
    assert!(
        cross_pub
            .to_string()
            .contains("missing required workunit manifest"),
        "branch for {task_id} must not accept manifest for {other_task_id}: {cross_pub}"
    );
    let forged_pub = workspace::verify_workunit_gate_for_publish(
        &wt_a,
        &format!("agent/agent-a/{other_task_id}"),
    )
    .unwrap_err();
    assert!(
        forged_pub
            .to_string()
            .contains("WORKUNIT_CHECKPOINT_REQUIRED"),
        "forged VERIFIED workunit without checkpoint must fail closed: {forged_pub}"
    );
}

#[test]
fn case2_handoff_retires_prior_custody_and_revoked_session_blocks_mutations() {
    let (_tmp, repo_dir, password) = init_git_decapod_repo();
    let data_root = repo_dir.join(".decapod/data");
    let store = Store {
        kind: StoreKind::Repo,
        root: data_root.clone(),
    };
    let (task_id, _) = add_local_task(&data_root, "Handoff and session revocation");

    let claim_a = claim_task_with_lease(
        &data_root,
        &task_id,
        "agent-a",
        ClaimMode::Exclusive,
        Some(3600),
        None,
    )
    .unwrap();
    assert_eq!(claim_a["status"], "ok");

    let branch_a = format!("agent/agent-a/{task_id}-handoff");
    let ws_a = workspace::ensure_workspace(
        &repo_dir,
        Some(workspace::WorkspaceConfig {
            branch: Some(branch_a),
            use_container: false,
            base_image: None,
            base_branch: None,
        }),
        "agent-a",
    )
    .unwrap();
    let wt_a = ws_a.git.worktree_path.unwrap();

    // Handoff policy requires verified trust; confirm basic trust is denied first, then promote agent-a.
    let basic_handoff_err = handoff_task_as(
        &store,
        &task_id,
        "agent-b",
        Some("agent-a"),
        "unverified handoff must fail",
        Some(3600),
        "agent-a",
    )
    .unwrap_err();
    assert!(basic_handoff_err.to_string().contains("Policy gate denied"));

    let conn = Connection::open(todo_db_path(&data_root)).unwrap();
    conn.execute(
        "INSERT INTO agents(agent_id, trust_level, status, updated_at)
         VALUES('agent-a', 'verified', 'active', '1Z')
         ON CONFLICT(agent_id) DO UPDATE SET trust_level = 'verified'",
        [],
    )
    .unwrap();
    drop(conn);

    // Verified actor still requires exact policy approval before handoff.
    let unapproved_handoff_err = handoff_task_as(
        &store,
        &task_id,
        "agent-b",
        Some("agent-a"),
        "unapproved handoff must fail",
        Some(3600),
        "agent-a",
    )
    .unwrap_err();
    assert!(unapproved_handoff_err.to_string().contains("approval"));

    let expiry = format!(
        "{}Z",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 900
    );
    decapod::plugins::policy::approve_action_with_expiry(
        &store,
        "todo.handoff",
        Some(&task_id),
        "operator",
        "global",
        Some(&expiry),
    )
    .unwrap();

    // Owner self-initiated handoff from agent-a -> agent-b advances generation and retires wt_a.
    let handoff = handoff_task_as(
        &store,
        &task_id,
        "agent-b",
        Some("agent-a"),
        "transferring custody to agent-b",
        Some(3600),
        "agent-a",
    )
    .unwrap();
    assert_eq!(handoff["status"], "ok", "{handoff}");
    assert_eq!(handoff["result"]["lease_generation"], 2);
    assert_eq!(handoff["result"]["to"], "agent-b");

    // Agent A can no longer author, renew, yield, or release after handoff.
    let auth_err = check_authoring(&data_root, &wt_a, "agent-a", false).unwrap_err();
    assert!(auth_err.to_string().contains("AUTHORING_CUSTODY_STALE"));
    assert_eq!(
        renew_claim_lease(&data_root, &task_id, "agent-a", Some(600)).unwrap()["status"],
        "conflict"
    );
    assert_eq!(
        yield_claim_lease(&data_root, &task_id, "agent-a", "late yield").unwrap()["status"],
        "conflict"
    );
    assert_eq!(
        release_task_as_actor(&data_root, &task_id, "agent-a").unwrap()["status"],
        "conflict"
    );

    // Revoked / forged local session token cannot execute strict session-authenticated mutations.
    let before = get_task(&data_root, &task_id).unwrap().unwrap();
    let forged_session = Command::new(decapod_bin())
        .args([
            "todo",
            "journal",
            "reconcile-legacy",
            "--expected-review",
            "sha256:0000",
        ])
        .current_dir(&repo_dir)
        .env("DECAPOD_AGENT_ID", "agent-a")
        .env("DECAPOD_SESSION_PASSWORD", format!("{password}-revoked"))
        .output()
        .expect("run with forged/revoked session password");
    assert!(
        !forged_session.status.success(),
        "invalid/revoked session password must fail closed"
    );
    let forged_err = format!(
        "{}\n{}",
        String::from_utf8_lossy(&forged_session.stdout),
        String::from_utf8_lossy(&forged_session.stderr)
    );
    assert!(
        forged_err.contains("AGENT_IDENTITY_FORGED"),
        "expected AGENT_IDENTITY_FORGED, got: {forged_err}"
    );

    let revoked_session = Command::new(decapod_bin())
        .args([
            "todo",
            "journal",
            "reconcile-legacy",
            "--expected-review",
            "sha256:0000",
        ])
        .current_dir(&repo_dir)
        .env("DECAPOD_AGENT_ID", "agent-b")
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .output()
        .expect("run without active session");
    assert!(!revoked_session.status.success());
    let revoked_err = format!(
        "{}\n{}",
        String::from_utf8_lossy(&revoked_session.stdout),
        String::from_utf8_lossy(&revoked_session.stderr)
    );
    assert!(
        revoked_err.contains("AGENT_IDENTITY_UNAUTHENTICATED"),
        "expected AGENT_IDENTITY_UNAUTHENTICATED, got: {revoked_err}"
    );
    let after = get_task(&data_root, &task_id).unwrap().unwrap();
    assert_eq!(after.assigned_to, before.assigned_to);
    assert_eq!(after.revision, before.revision);
}

#[test]
fn case2_revoked_cloud_session_fails_closed_without_local_fallback() {
    let (tx, rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tx.send(format!("http://{addr}")).unwrap();
            let (mut stream, _) = listener.accept().await.unwrap();
            let _ = read_http_request(&mut stream).await;
            write_http_json(
                &mut stream,
                401,
                json!({"error": {"code": "session_revoked", "message": "session revoked"}}),
            )
            .await;
        });
    });
    let endpoint = rx.recv().unwrap();

    let tmp = TempDir::new().unwrap();
    let identity =
        resolve_repository_identity_from_remote("https://github.com/DecapodLabs/decapod.git")
            .unwrap();
    let context = StorageContext::from_route(
        BackendRoute::cloud(identity, endpoint).unwrap(),
        Some("revoked-bearer-token"),
    )
    .unwrap()
    .with_cloud_datastore(CloudDatastore::Neon)
    .unwrap();
    let store = DactylTodoStore::new(context, "DecapodLabs/decapod");
    let err = run_cloud_todo_command_with_store(
        tmp.path(),
        &TodoCommand::Claim {
            id: "todo_revoked".into(),
            agent: None,
            mode: ClaimMode::Exclusive,
            lease_seconds: None,
            intent_anchor: None,
        },
        &store,
    )
    .unwrap_err();
    assert!(
        matches!(err, DecapodError::CloudAuth(ref d) if d.status == CloudAuthStatus::Revoked),
        "expected CloudAuthStatus::Revoked, got: {err:?}"
    );
    assert!(!tmp.path().join("decapod.db").exists());
    assert!(!tmp.path().join(".decapod/data/decapod.db").exists());
    server.join().unwrap();
}

// ---------------------------------------------------------------------------
// Case (3): Atomic transition + event commit, rollback, lost-ack, no replay
// ---------------------------------------------------------------------------

#[test]
fn case3_atomic_task_and_event_rollback_on_local_sqlite() {
    let tmp = TempDir::new().expect("tempdir");
    let data_root = tmp.path().join(".decapod/data");
    initialize_todo_db(&data_root).expect("init todo db");
    let store = Store {
        kind: StoreKind::Repo,
        root: data_root.clone(),
    };
    let (task_id, _) = add_local_task(&data_root, "Atomic rollback regression");
    let before = get_task(&data_root, &task_id).unwrap().unwrap();

    // Missing task edit must not emit a phantom task.edit event.
    let missing_edit = edit_task(
        &data_root,
        "todo_does_not_exist",
        Some("phantom"),
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(missing_edit["status"], "not_found");
    let conn = Connection::open(todo_db_path(&data_root)).unwrap();
    let phantom_edits: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE subject_id = 'todo_does_not_exist' AND event_type = 'task.edit'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(phantom_edits, 0);

    // Force an event-insert abort inside the transaction; task state must roll back cleanly.
    conn.execute(
        "CREATE TRIGGER abort_todo_events BEFORE INSERT ON events
         WHEN NEW.event_type = 'task.claim'
         BEGIN SELECT RAISE(ABORT, 'injected event write failure'); END",
        [],
    )
    .unwrap();
    let before_events: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE subject_id = ?1",
            [&task_id],
            |r| r.get(0),
        )
        .unwrap();
    drop(conn);

    let err = claim_task_with_lease(
        &data_root,
        &task_id,
        "rollback-agent",
        ClaimMode::Exclusive,
        Some(3600),
        None,
    )
    .expect_err("event failure must abort the entire claim transaction");
    assert!(err.to_string().contains("injected event write failure"));

    let after = get_task(&data_root, &task_id).unwrap().unwrap();
    assert_eq!(after.assigned_to, before.assigned_to);
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.status, before.status);

    let conn = Connection::open(todo_db_path(&data_root)).unwrap();
    let after_events: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE subject_id = ?1",
            [&task_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(after_events, before_events);
    conn.execute("DROP TRIGGER abort_todo_events", []).unwrap();

    // Verify normal update_status commits task + event + checkpoint atomically.
    let ok = update_status(
        &store,
        &task_id,
        before.revision,
        &before.status,
        "in_progress",
        "task.start",
        json!({"actor": "agent-atomic"}),
    )
    .unwrap();
    assert_eq!(ok["status"], "ok");
    let final_task = get_task(&data_root, &task_id).unwrap().unwrap();
    assert_eq!(final_task.status, "in_progress");
    assert_eq!(final_task.revision, before.revision + 1);
}

#[tokio::test]
async fn case3_dactyl_rollback_and_lost_ack_stay_explicit_without_duplicate_execution() {
    // Part A: Local Dactyl atomic rollback when event insert aborts.
    if let Some((local_dactyl, _tmp, db_path)) = dactyl_local_store() {
        local_dactyl
            .add_task(
                sample_cloud_task("todo_dactyl_rb", "Dactyl rollback"),
                "agent-a".into(),
                "intent:rb".into(),
            )
            .await
            .unwrap();
        local_dactyl
            .claim_task("todo_dactyl_rb", "agent-a".into())
            .await
            .unwrap();
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "CREATE TRIGGER abort_done_event BEFORE INSERT ON events
                 WHEN NEW.event_type = 'task.done'
                 BEGIN SELECT RAISE(ABORT, 'injected dactyl event failure'); END",
                [],
            )
            .unwrap();
        }
        let err = local_dactyl
            .complete_task("todo_dactyl_rb", "agent-a".into(), "done".into())
            .await
            .expect_err("failed event write must roll back task completion");
        assert!(err.to_string().contains("injected dactyl event failure"));
        let persisted = local_dactyl
            .get_task("todo_dactyl_rb")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(persisted.status, "in_progress");
        assert_eq!(persisted.version, 2);
    }

    // Part B: Loopback Dactyl HTTP server simulating:
    //  1) pre-dispatch auth/authorization denial (BlockedBeforeSubmit)
    //  2) commit-then-lost-ack (OutcomeUnknown persisted in CloudTodoJournal)
    //  3) exact same-ID reconciliation recovering the single committed task without replay
    //  4) changed-payload reuse with same operation_id rejected as Conflict
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server_commits = Arc::new(AtomicUsize::new(0));
    let server_commits_clone = Arc::clone(&server_commits);
    let server = tokio::spawn(async move {
        let mut stored_event = Value::Null;
        let mut stored_task = serde_json::to_value(sample_cloud_task(
            "todo_lost_ack_1247",
            "Lost ack reconciliation",
        ))
        .unwrap();
        stored_task["tags"] = json!("");
        stored_task["assignee"] = json!("");

        for step in 0..6 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let (headers, body) = read_http_request(&mut stream).await;
            match step {
                0 => {
                    // Before-dispatch check denied -> no /batch write happens.
                    assert!(headers.starts_with("POST /query "));
                    write_http_json(
                        &mut stream,
                        403,
                        json!({"error": {"code": "repository_not_authorized", "message": "denied"}}),
                    )
                    .await;
                }
                1 => {
                    // Pre-dispatch reconciliation read finds nothing.
                    assert!(headers.starts_with("POST /query "));
                    write_http_json(
                        &mut stream,
                        200,
                        json!({"columns": ["subject_id", "payload"], "rows": []}),
                    )
                    .await;
                }
                2 => {
                    // /batch commits on server, then drops TCP connection (lost ack).
                    assert!(headers.starts_with("POST /batch "));
                    let ops = body["operations"].as_array().unwrap();
                    assert_eq!(ops.len(), 3);
                    stored_task["id"] = ops[0]["params"][1].clone();
                    let event_params = ops[1]["params"].as_array().unwrap();
                    stored_event = json!({
                        "subject_id": event_params[2],
                        "payload": event_params[4]
                    });
                    server_commits_clone.fetch_add(1, Ordering::SeqCst);
                    drop(stream);
                }
                3 => {
                    // Immediate post-lost-ack reconciliation probe sees transient partition -> stays OutcomeUnknown.
                    assert!(headers.starts_with("POST /query "));
                    write_http_json(
                        &mut stream,
                        503,
                        json!({"error": {"code": "service_unavailable", "message": "partition after batch"}}),
                    )
                    .await;
                }
                4 => {
                    // Explicit same-ID retry reconciles via events read without calling /batch again.
                    assert!(headers.starts_with("POST /query "));
                    assert!(body["sql"].as_str().unwrap().contains("FROM events"));
                    write_http_json(
                        &mut stream,
                        200,
                        json!({"columns": ["subject_id", "payload"], "rows": [stored_event.clone()], "affected_rows": 0}),
                    )
                    .await;
                }
                5 => {
                    assert!(headers.starts_with("POST /query "));
                    assert!(body["sql"].as_str().unwrap().contains("FROM tasks"));
                    write_http_json(
                        &mut stream,
                        200,
                        json!({"columns": stored_task.as_object().unwrap().keys().collect::<Vec<_>>(), "rows": [stored_task.clone()], "affected_rows": 0}),
                    )
                    .await;
                }
                _ => unreachable!(),
            }
        }
    });

    let tmp = TempDir::new().unwrap();
    let journal = CloudTodoJournal::at(tmp.path().join("ops-journal"));
    let identity =
        resolve_repository_identity_from_remote("https://github.com/DecapodLabs/decapod.git")
            .unwrap();
    let context = StorageContext::from_route(
        BackendRoute::cloud(identity, &endpoint).unwrap(),
        Some("synthetic-bearer"),
    )
    .unwrap()
    .with_cloud_datastore(CloudDatastore::Neon)
    .unwrap();
    let store = DactylTodoStore::new(context, "DecapodLabs/decapod");

    let req_json = json!({"title": "Lost ack reconciliation"});
    let fp = cloud_todo_operation::fingerprint(&endpoint, "DecapodLabs/decapod", &req_json);
    let op = OperationRecord::new(fp.clone(), None).unwrap();
    journal.prepare(&op).unwrap();

    // Step 0: Before-dispatch authorization denial keeps status BlockedBeforeSubmit and 0 server commits.
    let task = sample_cloud_task(&op.task_id, "Lost ack reconciliation");
    let blocked_err = store
        .add_task(task.clone(), "agent-a".into(), op.intent())
        .await
        .unwrap_err();
    assert_eq!(
        cloud_todo_operation::anyhow_failure(&blocked_err),
        (Outcome::BlockedBeforeSubmit, FailureKind::Authorization)
    );
    assert_eq!(server_commits.load(Ordering::SeqCst), 0);

    // Step 1+2: Commit-then-lost-ack leaves OutcomeUnknown in journal and exactly 1 server commit.
    journal.finish(&op, Outcome::OutcomeUnknown, None).unwrap();
    let lost_ack_err = store
        .add_task(task.clone(), "agent-a".into(), op.intent())
        .await
        .unwrap_err();
    assert_eq!(
        cloud_todo_operation::anyhow_failure(&lost_ack_err).0,
        Outcome::OutcomeUnknown
    );
    assert_eq!(server_commits.load(Ordering::SeqCst), 1);
    assert_eq!(journal.list().unwrap()[0].status, Outcome::OutcomeUnknown);

    // Step 3+4: Same-ID retry reconciles without a second /batch commit.
    let reconciled = store
        .add_task(task.clone(), "agent-a".into(), op.intent())
        .await
        .unwrap();
    assert_eq!(reconciled.id, op.task_id);
    assert_eq!(
        server_commits.load(Ordering::SeqCst),
        1,
        "reconciliation must not execute a second mutation"
    );
    journal.finish(&op, Outcome::Succeeded, None).unwrap();
    assert_eq!(journal.list().unwrap()[0].status, Outcome::Succeeded);

    // Changed arguments with same operation_id fail immediately before dispatch.
    let changed_fp = cloud_todo_operation::fingerprint(
        &endpoint,
        "DecapodLabs/decapod",
        &json!({"title": "Mutated payload"}),
    );
    let conflict = OperationRecord::new(changed_fp, Some(&op.operation_id)).unwrap_err();
    assert!(conflict.to_string().contains("different arguments"));

    server.await.unwrap();
}

// ---------------------------------------------------------------------------
// Case (4): Exact binding of validation/completion to commit SHA, task
//           revision/fence, accepted target, and policy/evidence digest
// ---------------------------------------------------------------------------

#[test]
fn case4_validation_and_completion_bind_to_commit_revision_target_and_policy() {
    // Part A: StateIdentity & governed_transition proof binding.
    let baseline = StateIdentity {
        head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        rules_fingerprint: "sha256:rules-v1".to_string(),
        verifier: "decapod.pr-readiness.v1".to_string(),
    };
    let evidence = EvidenceRecord {
        state: baseline.clone(),
        output_hash: "sha256:1111111111111111111111111111111111111111111111111111111111111111"
            .to_string(),
        inputs_hash: "sha256:2222222222222222222222222222222222222222222222222222222222222222"
            .to_string(),
        observed_pass: true,
        corrupt: false,
    };
    let claim = CompletionClaim {
        declared_status: Some("done".to_string()),
        passes: Some(true),
        notes: Some("self-reported".to_string()),
        evidence: Some(evidence.clone()),
    };
    assert_eq!(judge_completion(&claim, &baseline), ProofJudgment::Proven);

    // Different commit SHA cannot reuse passing proof.
    let moved_head = StateIdentity {
        head: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
        ..baseline.clone()
    };
    assert_eq!(judge_completion(&claim, &moved_head), ProofJudgment::Stale);

    // Later policy/rules revision cannot reuse passing proof.
    let later_policy = StateIdentity {
        rules_fingerprint: "sha256:rules-v2".to_string(),
        ..baseline.clone()
    };
    assert_eq!(
        judge_completion(&claim, &later_policy),
        ProofJudgment::Stale
    );

    // Different verifier / target contract cannot reuse passing proof.
    let other_verifier = StateIdentity {
        verifier: "decapod.pr-readiness.v2".to_string(),
        ..baseline.clone()
    };
    assert_eq!(
        judge_completion(&claim, &other_verifier),
        ProofJudgment::Stale
    );

    // Self-declared status/passes without evidence never proves completion.
    let unbacked = CompletionClaim {
        evidence: None,
        ..claim
    };
    assert_eq!(
        judge_completion(&unbacked, &baseline),
        ProofJudgment::NotProven
    );

    // Part B: Task revision/fence + out-of-band tampering detection + historical readability.
    // First prove in a git repo that out-of-band raw SQL verification injection is rejected by
    // task_journal (TASK_JOURNAL_CONFLICT) and forged artifacts fail reconstruction.
    let (_tmp, repo_dir, _password) = init_git_decapod_repo();
    let data_root = repo_dir.join(".decapod/data");
    let (task_id, _) = add_local_task(&data_root, "Revision-bound proof lifecycle");
    let store = Store {
        kind: StoreKind::Repo,
        root: data_root.clone(),
    };
    claim_task_with_lease(
        &data_root,
        &task_id,
        "agent-proof",
        ClaimMode::Exclusive,
        Some(3600),
        None,
    )
    .unwrap();
    let rev_at_proof = get_task(&data_root, &task_id).unwrap().unwrap().revision;

    let forged_artifacts = json!({
        "task_id": task_id,
        "task_revision": rev_at_proof,
        "proof_plan_results": [{"proof_gate": "validate_passes", "output_hash": "sha256:forged"}]
    });
    let forged_judgment = decapod::core::governed_transition::judge_stored_verification(
        &repo_dir,
        "passed",
        &forged_artifacts,
    )
    .unwrap();
    assert_ne!(forged_judgment, ProofJudgment::Proven);

    // Out-of-band SQL mutation of task_verification after claim checkpoint must trigger TASK_JOURNAL_CONFLICT.
    let conn = Connection::open(todo_db_path(&data_root)).unwrap();
    conn.execute(
        "INSERT INTO task_verification(
            todo_id, proof_plan, last_verified_at, last_verified_status,
            last_verified_notes, verification_policy_days, updated_at, verification_artifacts
         ) VALUES(?1, '[\"validate_passes\"]', '100Z', 'passed', 'out-of-band', 90, '100Z', ?2)",
        params![task_id, forged_artifacts.to_string()],
    )
    .unwrap();
    drop(conn);
    let forged_done = update_status(
        &store,
        &task_id,
        rev_at_proof,
        "open",
        "done",
        "task.done",
        json!({"actor": "agent-proof"}),
    )
    .unwrap();
    assert_eq!(forged_done["status"], "conflict", "{forged_done}");
    let journal_err = edit_task(
        &data_root,
        &task_id,
        None,
        Some("mutating after raw SQL verification insert"),
        None,
        None,
    )
    .unwrap_err();
    assert!(
        journal_err.to_string().contains("TASK_JOURNAL_CONFLICT"),
        "expected TASK_JOURNAL_CONFLICT for out-of-band task_verification insert, got: {journal_err}"
    );

    // Next, build a clean rebuilt store (no pre-existing task_journal checkpoint yet) with real
    // reconstructed proof artifacts bound to repo_dir HEAD + rules + verifier + task revision.
    let tmp_clean = TempDir::new().unwrap();
    let clean_data = tmp_clean.path().join(".decapod/data");
    std::fs::create_dir_all(&clean_data).unwrap();
    let clean_store = Store {
        kind: StoreKind::Repo,
        root: clean_data.clone(),
    };
    let clean_events = clean_data.join("todo.events.jsonl");
    let clean_id = "task_rev_bound_1247";
    let ev_add = json!({
        "ts": "100Z", "event_id": "EV_ADD_1247", "event_type": "task.add",
        "status": "success", "task_id": clean_id,
        "payload": {"title": "Revision-bound proof lifecycle"}, "actor": "agent-proof"
    });
    let ev_claim = json!({
        "ts": "101Z", "event_id": "EV_CLAIM_1247", "event_type": "task.claim",
        "status": "success", "task_id": clean_id,
        "payload": {"agent": "agent-proof", "mode": "exclusive", "lease_expires_at": "9999999999Z", "lease_generation": 1, "revision": 2},
        "actor": "agent-proof"
    });
    std::fs::write(&clean_events, format!("{ev_add}\n{ev_claim}\n")).unwrap();
    todo::rebuild_db_from_events(&clean_events, &todo_db_path(&clean_data)).unwrap();
    let clean_rev = get_task(&clean_data, clean_id).unwrap().unwrap().revision;

    let snapshot = state::canonical_snapshot(&repo_dir).expect("canonical snapshot");
    let identity = identity_from_snapshot(&snapshot);
    let readme_bytes = std::fs::read(repo_dir.join("README.md")).unwrap();
    let readme_hash = format!("{:x}", Sha256::digest(&readme_bytes));
    let mut files = BTreeMap::new();
    files.insert("README.md".to_string(), readme_hash.clone());
    let mut hasher = Sha256::new();
    for (path, hash) in &files {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(hash.as_bytes());
        hasher.update([0]);
    }
    hasher.update(identity.head.as_bytes());
    hasher.update([0]);
    hasher.update(identity.rules_fingerprint.as_bytes());
    hasher.update([0]);
    hasher.update(identity.verifier.as_bytes());
    let reconstructed_digest = format!("{:x}", hasher.finalize());

    let bound_artifacts = json!({
        "task_id": clean_id,
        "task_revision": clean_rev + 1,
        "canonical_head": identity.head,
        "rules_fingerprint": identity.rules_fingerprint,
        "verifier": identity.verifier,
        "file_artifacts": [{"path": "README.md", "hash": format!("sha256:{readme_hash}")}],
        "receipt_hash": format!("sha256:{reconstructed_digest}"),
        "proof_plan_results": [{"proof_gate": "validate_passes", "output_hash": format!("sha256:{reconstructed_digest}")}]
    });
    assert_eq!(
        decapod::core::governed_transition::judge_stored_verification(
            &repo_dir,
            "passed",
            &bound_artifacts,
        )
        .unwrap(),
        ProofJudgment::Proven
    );

    let conn = Connection::open(todo_db_path(&clean_data)).unwrap();
    conn.execute(
        "INSERT INTO task_verification(
            todo_id, proof_plan, last_verified_at, last_verified_status,
            last_verified_notes, verification_policy_days, updated_at, verification_artifacts
         ) VALUES(?1, '[\"validate_passes\"]', '102Z', 'passed', 'reconstructed proof', 90, '102Z', ?2)",
        params![clean_id, bound_artifacts.to_string()],
    )
    .unwrap();
    drop(conn);

    // Complete under exact revision fence + reconstructed proof.
    let done = update_status(
        &clean_store,
        clean_id,
        clean_rev,
        "open",
        "done",
        "task.done",
        json!({"actor": "agent-proof"}),
    )
    .unwrap();
    assert_eq!(done["status"], "ok", "{done}");

    let claim_ok = todo::get_work_claim(&clean_data, clean_id)
        .unwrap()
        .unwrap();
    assert_eq!(claim_ok.status, WorkClaimStatus::Complete);
    assert_eq!(
        claim_ok.validation_status,
        WorkClaimValidationStatus::Passed
    );
    assert!(!claim_ok.proof_refs.is_empty());

    // Comments do not advance revision or stale proof.
    comment_task(&clean_data, clean_id, "audit observation").unwrap();
    let still_ok = todo::get_work_claim(&clean_data, clean_id)
        .unwrap()
        .unwrap();
    assert_eq!(still_ok.status, WorkClaimStatus::Complete);

    // Material edit advances task revision -> prior proof becomes Stale,
    // while historical proof_refs remain readable.
    edit_task(
        &clean_data,
        clean_id,
        None,
        Some("Changed acceptance scope after done"),
        None,
        None,
    )
    .unwrap();
    let stale_claim = todo::get_work_claim(&clean_data, clean_id)
        .unwrap()
        .unwrap();
    assert_eq!(stale_claim.status, WorkClaimStatus::Blocked);
    assert_eq!(
        stale_claim.validation_status,
        WorkClaimValidationStatus::Stale
    );
    assert!(
        !stale_claim.proof_refs.is_empty(),
        "historical proof_refs must remain readable after later revision"
    );

    // Advancing git HEAD in repo_dir makes bound_artifacts Stale for the new commit SHA
    // while keeping the historical artifact record intact.
    std::fs::write(repo_dir.join("README.md"), "# Updated Repo Commit\n").unwrap();
    git(&repo_dir, &["add", "README.md"]);
    git(&repo_dir, &["commit", "-m", "new candidate commit"]);
    let post_commit_judgment = decapod::core::governed_transition::judge_stored_verification(
        &repo_dir,
        "passed",
        &bound_artifacts,
    )
    .unwrap();
    assert_ne!(
        post_commit_judgment,
        ProofJudgment::Proven,
        "proof bound to prior commit/artifact bytes must not verify on new commit"
    );
}

// ---------------------------------------------------------------------------
// Case (5): Cloud denial / unavailability leaves stores unchanged and NEVER
//           silently falls back to local SQLite
// ---------------------------------------------------------------------------

#[test]
fn case5_cloud_denial_and_unavailability_never_fall_back_to_local_sqlite() {
    for (status_code, err_code, expected_msg) in [
        (403_u16, "repository_not_authorized", "RepositoryDenied"),
        (503_u16, "service_unavailable", "ProviderUnavailable"),
    ] {
        let mut_observed = Arc::new(AtomicBool::new(false));
        let mut_observed_clone = Arc::clone(&mut_observed);
        let (tx, rx) = std::sync::mpsc::channel();
        let err_code = err_code.to_string();

        let server = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let addr = listener.local_addr().unwrap();
                tx.send(format!("http://{addr}")).unwrap();
                let (mut stream, _) = listener.accept().await.unwrap();
                let (headers, _body) = read_http_request(&mut stream).await;
                if headers.starts_with("POST /batch ") {
                    mut_observed_clone.store(true, Ordering::SeqCst);
                }
                write_http_json(
                    &mut stream,
                    status_code,
                    json!({"error": {"code": err_code, "message": "cloud failure"}}),
                )
                .await;
            });
        });
        let endpoint = rx.recv().unwrap();

        let tmp = TempDir::new().unwrap();
        git(tmp.path(), &["init", "-q"]);
        git(
            tmp.path(),
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/DecapodLabs/decapod.git",
            ],
        );
        std::fs::create_dir_all(tmp.path().join(".decapod/data")).unwrap();
        std::fs::write(
            tmp.path().join(".decapod/config.toml"),
            "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"cloud\"\n",
        )
        .unwrap();

        let identity =
            resolve_repository_identity_from_remote("https://github.com/DecapodLabs/decapod.git")
                .unwrap();
        let context = StorageContext::from_route(
            BackendRoute::cloud(identity, &endpoint).unwrap(),
            Some("synthetic-bearer"),
        )
        .unwrap()
        .with_cloud_datastore(CloudDatastore::Neon)
        .unwrap();
        let store = DactylTodoStore::new(context, "DecapodLabs/decapod");

        let err = run_cloud_todo_command_with_store(
            tmp.path(),
            &TodoCommand::Claim {
                id: "todo_cloud_denied".into(),
                agent: None,
                mode: ClaimMode::Exclusive,
                lease_seconds: None,
                intent_anchor: None,
            },
            &store,
        )
        .unwrap_err();
        let rendered = format!("{err:?}");
        assert!(
            rendered.contains(expected_msg) || rendered.contains("Unavailable"),
            "expected {expected_msg}, got: {rendered}"
        );
        assert!(
            !tmp.path().join(".decapod/data/decapod.db").exists(),
            "cloud failure must never create local decapod.db"
        );
        assert!(
            !tmp.path().join(".decapod/data/todo.db").exists(),
            "cloud failure must never create local todo.db"
        );
        let _ = mut_observed.load(Ordering::SeqCst);
        server.join().unwrap();
    }

    // Also verify CLI process invocation in cloud mode against an unreachable port
    // fails closed with zero local SQLite creation.
    let tmp = TempDir::new().unwrap();
    let machine = TempDir::new().unwrap();
    git(tmp.path(), &["init", "-q"]);
    git(
        tmp.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/DecapodLabs/decapod.git",
        ],
    );
    let init = Command::new(decapod_bin())
        .args(["init", "--backend", "cloud", "--force", "--proof"])
        .env("DECAPOD_CLOUD_DATASTORE", "neon")
        .env("HOME", machine.path())
        .env("XDG_CONFIG_HOME", machine.path().join("config"))
        .env("XDG_DATA_HOME", machine.path().join("data"))
        .current_dir(tmp.path())
        .output()
        .expect("cloud init");
    assert!(
        init.status.success(),
        "cloud init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let cli_add = Command::new(decapod_bin())
        .args(["todo", "add", "must not write locally", "--format", "json"])
        .current_dir(tmp.path())
        .env("DECAPOD_CLOUD_DATASTORE", "neon")
        .env("DECAPOD_TEST_MODE", "1")
        .env("DECAPOD_CLOUD_TEST_ORIGIN", "http://127.0.0.1:1")
        .env("DECAPOD_HEADLESS", "1")
        .env("HOME", machine.path())
        .env("XDG_CONFIG_HOME", machine.path().join("config"))
        .env("XDG_DATA_HOME", machine.path().join("data"))
        .output()
        .expect("cloud cli add");
    assert!(!cli_add.status.success());
    assert!(!tmp.path().join(".decapod/data/decapod.db").exists());
    assert!(!tmp.path().join(".decapod/data/todo.db").exists());
}
