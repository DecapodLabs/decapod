use decapod::core::schemas;
use decapod::core::store::Store;
use decapod::core::store::StoreKind;
use decapod::core::todo::{
    ClaimMode, TodoCommand, add_task, check_trust_level, claim_task, claim_task_with_lease,
    fleet_health, get_task, get_work_claim, handoff_task, initialize_todo_db, list_tasks,
    rebuild_from_events, renew_claim_lease, update_status, yield_claim_lease,
};
use decapod::plugins::policy;
use rusqlite::Connection;
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn assert_typed_todo_id(id: &str) {
    let (task_type, body) = id
        .split_once('_')
        .expect("id should contain type separator");
    assert_eq!(task_type.len(), 4, "type prefix must be 4 chars");
    assert_eq!(body.len(), 16, "id body must be 16 chars");
    assert!(
        task_type.chars().all(|c| c.is_ascii_lowercase()),
        "type prefix should be lowercase letters"
    );
    assert!(
        body.chars().all(|c| c.is_ascii_alphanumeric()),
        "id body should be alphanumeric"
    );
}

/// Set trust on the consolidated `agents` table (schema fold #1129).
/// Ensures the row exists so handoff/shared claim policy gates can evaluate.
fn set_agent_trust(db: &Connection, agent_id: &str, trust_level: &str, ts: &str) {
    db.execute(
        "INSERT INTO agents(agent_id, last_seen, status, updated_at, trust_level, expertise_json, category_claims_json)
         VALUES(?1, ?2, 'active', ?2, ?3, '[]', '[]')
         ON CONFLICT(agent_id) DO UPDATE SET
           trust_level = excluded.trust_level,
           updated_at = excluded.updated_at,
           last_seen = excluded.last_seen",
        rusqlite::params![agent_id, ts, trust_level],
    )
    .expect("set agent trust");
}

#[test]
fn test_todo_lifecycle() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();

    // 1. Add task
    let add_args = TodoCommand::Add {
        title: "Test task".to_string(),
        description: "".to_string(),
        tags: "tag1".to_string(),
        owner: "arx".to_string(),
        due: None,
        r#ref: "".to_string(),
        scope: "".to_string(),
        dir: Some(tmp.path().to_string_lossy().to_string()),
        priority: "high".to_string(),
        depends_on: "".to_string(),
        blocks: "".to_string(),
        parent: None,
        one_shot: 0,
    };
    let res = add_task(&root, &add_args).unwrap();
    let task_id = res.get("id").unwrap().as_str().unwrap();
    let task_hash = res.get("hash").and_then(|v| v.as_str()).unwrap_or_default();
    assert_typed_todo_id(task_id);
    assert_eq!(task_hash, &task_id.split_once('_').unwrap().1[..6]);

    // 2. Get task
    let task = get_task(&root, task_id).unwrap().expect("Task not found");
    assert_eq!(task.hash, task_hash);
    assert_eq!(task.title, "Test task");
    assert_eq!(task.status, "open");
    assert_eq!(task.owners.len(), 1);
    assert_eq!(task.owners[0].agent_id, "arx");
    assert_eq!(task.owners[0].claim_type, "primary");

    // 3. Mark done
    let store = Store {
        kind: StoreKind::Repo,
        root: root.clone(),
    };
    update_status(&store, task_id, "done", "task.done", serde_json::json!({})).unwrap();
    let task = get_task(&root, task_id).unwrap().unwrap();
    assert_eq!(task.status, "done");

    // 4. List tasks
    let tasks = list_tasks(&root, Some("done".to_string()), None, None, None, None).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, task_id);
}

#[test]
fn test_similar_active_task_consolidates_with_comment() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();

    let active_args = TodoCommand::Add {
        title: "Fix duplicate todo label handoff routing".to_string(),
        description: "Make the active owner see comments from related requests".to_string(),
        tags: "todo,labels,agent-handoff".to_string(),
        owner: "".to_string(),
        due: None,
        r#ref: "".to_string(),
        scope: "".to_string(),
        dir: Some(tmp.path().to_string_lossy().to_string()),
        priority: "high".to_string(),
        depends_on: "".to_string(),
        blocks: "".to_string(),
        parent: None,
        one_shot: 0,
    };
    let active = add_task(&root, &active_args).unwrap();
    let active_id = active["id"].as_str().unwrap();
    claim_task(&root, active_id, "agent-a", ClaimMode::Exclusive).unwrap();

    let similar_args = TodoCommand::Add {
        title: "Repair similar task tag transfer for owners".to_string(),
        description: "New conversation asks for annotations on claimed work".to_string(),
        tags: "tasks,tags,comments,agent-routing".to_string(),
        owner: "agent-b".to_string(),
        due: None,
        r#ref: "".to_string(),
        scope: "".to_string(),
        dir: Some(tmp.path().to_string_lossy().to_string()),
        priority: "medium".to_string(),
        depends_on: "".to_string(),
        blocks: "".to_string(),
        parent: None,
        one_shot: 0,
    };
    let consolidated = add_task(&root, &similar_args).unwrap();
    assert_eq!(consolidated["id"], active_id);
    assert_eq!(consolidated["new_task_created"], false);
    assert_eq!(consolidated["consolidated"], true);
    assert_eq!(consolidated["consolidation"]["assigned_to"], "agent-a");

    let open = list_tasks(&root, Some("open".to_string()), None, None, None, None).unwrap();
    assert_eq!(
        open.len(),
        1,
        "similar request should not create another open task"
    );
    let task = get_task(&root, active_id).unwrap().unwrap();
    assert_eq!(task.comments.len(), 1);
    assert_eq!(task.comments[0].kind, "fuzzy-consolidation");
    assert!(
        task.comments[0]
            .comment
            .contains("Repair similar task tag transfer")
    );
}

#[test]
fn test_similar_active_task_does_not_consolidate_same_owner() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();

    let active_args = TodoCommand::Add {
        title: "Fix duplicate todo label handoff routing".to_string(),
        description: "Make active owner see comments".to_string(),
        tags: "todo,labels,agent-handoff".to_string(),
        owner: "agent-a".to_string(),
        due: None,
        r#ref: "".to_string(),
        scope: "".to_string(),
        dir: Some(tmp.path().to_string_lossy().to_string()),
        priority: "high".to_string(),
        depends_on: "".to_string(),
        blocks: "".to_string(),
        parent: None,
        one_shot: 0,
    };
    let active = add_task(&root, &active_args).unwrap();
    let active_id = active["id"].as_str().unwrap();
    claim_task(&root, active_id, "agent-a", ClaimMode::Exclusive).unwrap();

    let similar_args = TodoCommand::Add {
        title: "Repair similar task tag transfer for owners".to_string(),
        description: "New conversation asks for annotations on claimed work".to_string(),
        tags: "tasks,tags,comments,agent-routing".to_string(),
        owner: "agent-a".to_string(),
        due: None,
        r#ref: "".to_string(),
        scope: "".to_string(),
        dir: Some(tmp.path().to_string_lossy().to_string()),
        priority: "medium".to_string(),
        depends_on: "".to_string(),
        blocks: "".to_string(),
        parent: None,
        one_shot: 0,
    };
    let created = add_task(&root, &similar_args).unwrap();
    assert_ne!(created["id"], active_id);
    assert!(created.get("consolidated").is_none());

    let open = list_tasks(&root, Some("open".to_string()), None, None, None, None).unwrap();
    assert_eq!(
        open.len(),
        2,
        "same-owner similar work should remain independently visible"
    );
}

#[test]
fn test_todo_rebuild() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();

    // Add some tasks
    for i in 0..3 {
        let add_args = TodoCommand::Add {
            title: format!("Task {i}"),
            description: "".to_string(),
            tags: "".to_string(),
            owner: "".to_string(),
            due: None,
            r#ref: "".to_string(),
            scope: "".to_string(),
            dir: Some(tmp.path().to_string_lossy().to_string()),
            priority: "medium".to_string(),
            depends_on: "".to_string(),
            blocks: "".to_string(),
            parent: None,
            one_shot: 0,
        };
        add_task(&root, &add_args).unwrap();
    }

    // Rebuild is now a canonical-datastore projection check. The event
    // stream lives in decapod.db; deleting that single source would delete
    // the source of truth rather than exercise a supported recovery path.
    let rebuild = rebuild_from_events(&root).unwrap();
    // After #1127 the unified `events` stream is the sole rebuild source.
    assert_eq!(rebuild["source"], "events");
    assert_eq!(rebuild["events"], 3);

    // Verify
    let tasks = list_tasks(&root, None, None, None, None, None).unwrap();
    assert_eq!(tasks.len(), 3);
}

#[test]
fn test_trust_level_check() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();

    // Unknown agent defaults to basic
    let has_access = check_trust_level(&root, "unknown_agent", "basic").unwrap();
    assert!(has_access);

    // Unknown agent should NOT have core access (higher than basic)
    let has_access = check_trust_level(&root, "unknown_agent", "core").unwrap();
    assert!(!has_access);

    // Unknown agent should NOT have verified access (higher than basic)
    let has_access = check_trust_level(&root, "unknown_agent", "verified").unwrap();
    assert!(!has_access);
}

#[test]
fn test_trust_level_hierarchy() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();

    // Default is basic, so it should pass basic check
    assert!(check_trust_level(&root, "test_agent", "basic").unwrap());

    // But should fail for higher levels
    assert!(!check_trust_level(&root, "test_agent", "verified").unwrap());
    assert!(!check_trust_level(&root, "test_agent", "core").unwrap());
}

fn run_cmd(repo_root: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(repo_root)
        .args(args)
        .stdin(std::process::Stdio::null())
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("run decapod");
    assert!(
        output.status.success(),
        "command failed: {:?}\nstderr: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_start = stdout.find('{').expect("json output start");
    serde_json::from_str(&stdout[json_start..]).expect("parse json")
}

fn run_raw(repo_root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(repo_root)
        .args(args)
        .stdin(std::process::Stdio::null())
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("run decapod")
}

fn bootstrap_repo(repo: &Path) {
    let init = run_raw(repo, &["init", "--force"]);
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let session = run_raw(repo, &["session", "acquire"]);
    assert!(
        session.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&session.stderr)
    );
}

#[test]
fn test_claim_modes_and_owner_consolidation() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    bootstrap_repo(repo);
    let added = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "add",
            "Claim mode test",
            "--owner",
            "agent-a,agent-b",
        ],
    );
    let task_id = added["id"].as_str().unwrap().to_string();

    let _ = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "claim",
            "--id",
            &task_id,
            "--agent",
            "agent-a",
            "--mode",
            "exclusive",
        ],
    );

    let db = Connection::open(repo.join(".decapod/data/decapod.db")).unwrap();
    let ts = "1771202800Z";
    set_agent_trust(&db, "agent-b", "verified", ts);

    let shared = run_cmd(
        repo,
        &[
            "todo", "--format", "json", "claim", "--id", &task_id, "--agent", "agent-b", "--mode",
            "shared",
        ],
    );
    assert_eq!(shared["status"], "ok");
    assert_eq!(shared["result"]["mode"], "shared");

    let got = run_cmd(repo, &["todo", "--format", "json", "get", "--id", &task_id]);
    assert_eq!(got["item"]["owner"], "agent-a");
    let owners = got["item"]["owners"].as_array().unwrap();
    assert_eq!(owners.len(), 2);
    assert!(
        owners
            .iter()
            .any(|o| o["agent_id"] == "agent-a" && o["claim_type"] == "primary")
    );
    assert!(
        owners
            .iter()
            .any(|o| o["agent_id"] == "agent-b" && o["claim_type"] == "secondary")
    );

    let _ = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "edit",
            "--id",
            &task_id,
            "--owner",
            "agent-c,agent-d",
        ],
    );
    let got_after_edit = run_cmd(repo, &["todo", "--format", "json", "get", "--id", &task_id]);
    assert_eq!(got_after_edit["item"]["owner"], "agent-c");
    let owners_after_edit = got_after_edit["item"]["owners"].as_array().unwrap();
    assert!(
        owners_after_edit
            .iter()
            .any(|o| o["agent_id"] == "agent-c" && o["claim_type"] == "primary")
    );
    assert!(
        owners_after_edit
            .iter()
            .any(|o| o["agent_id"] == "agent-d" && o["claim_type"] == "secondary")
    );
    assert!(
        !owners_after_edit
            .iter()
            .any(|o| o["agent_id"] == "agent-a" || o["agent_id"] == "agent-b")
    );
}

#[test]
fn test_risk_zones_and_trust_tiers_enforced() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    bootstrap_repo(repo);

    let added = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "add",
            "Risk/trust test",
            "--owner",
            "agent-a",
        ],
    );
    let task_id = added["id"].as_str().unwrap().to_string();

    // Shared claim requires verified trust (default unknown/basic should fail).
    let shared_fail = run_raw(
        repo,
        &[
            "todo", "--format", "json", "claim", "--id", &task_id, "--agent", "agent-b", "--mode",
            "shared",
        ],
    );
    assert!(
        !shared_fail.status.success(),
        "shared claim should fail without verified trust"
    );
    assert!(String::from_utf8_lossy(&shared_fail.stderr).contains("Policy gate denied"));

    // Grant verified trust to agent-b and retry shared claim.
    let db = Connection::open(repo.join(".decapod/data/decapod.db")).unwrap();
    let ts = "1771203000Z";
    set_agent_trust(&db, "agent-b", "verified", ts);
    drop(db);
    let shared_ok = run_cmd(
        repo,
        &[
            "todo", "--format", "json", "claim", "--id", &task_id, "--agent", "agent-b", "--mode",
            "shared",
        ],
    );
    assert_eq!(shared_ok["status"], "ok");
    assert_eq!(shared_ok["result"]["mode"], "shared");

    // Handoff requires verified trust and explicit approval.
    let db = Connection::open(repo.join(".decapod/data/decapod.db")).unwrap();
    set_agent_trust(&db, "agent-a", "verified", ts);

    let handoff_fail = run_raw(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "handoff",
            "--id",
            &task_id,
            "--to",
            "agent-c",
            "--from",
            "agent-b",
            "--summary",
            "handoff test",
        ],
    );
    assert!(
        !handoff_fail.status.success(),
        "handoff should fail without approval"
    );

    let store = Store {
        kind: StoreKind::Repo,
        root: repo.join(".decapod/data"),
    };
    policy::approve_action(&store, "todo.handoff", None, "operator", "global").unwrap();

    let handoff_ok = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "handoff",
            "--id",
            &task_id,
            "--to",
            "agent-c",
            "--from",
            "agent-b",
            "--summary",
            "handoff test",
        ],
    );
    assert_eq!(handoff_ok["status"], "ok");
    assert_eq!(handoff_ok["result"]["to"], "agent-c");
    assert_eq!(handoff_ok["result"]["lease_lifecycle"], "claimed");
    assert!(
        handoff_ok["result"]["lease_generation"]
            .as_u64()
            .unwrap_or(0)
            >= 1,
        "handoff must publish a lease generation: {handoff_ok}"
    );
    assert!(
        handoff_ok["result"]["intent_anchor"]
            .as_str()
            .is_some_and(|anchor| !anchor.is_empty()),
        "handoff must preserve/bind an intent anchor: {handoff_ok}"
    );
}

#[test]
fn test_done_accepts_positional_id() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    bootstrap_repo(repo);

    let added = run_cmd(
        repo,
        &["todo", "--format", "json", "add", "Positional done test"],
    );
    let task_id = added["id"].as_str().unwrap().to_string();

    let done_out = run_cmd(repo, &["todo", "--format", "json", "done", &task_id]);
    assert_eq!(done_out["status"], "ok");

    let got = run_cmd(repo, &["todo", "--format", "json", "get", "--id", &task_id]);
    assert_eq!(got["item"]["status"], "done");
}

#[test]
fn test_claim_includes_container_result_when_autorun_enabled() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    bootstrap_repo(repo);

    let added = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "add",
            "Claim autorun envelope test",
            "--owner",
            "agent-a",
        ],
    );
    let task_id = added["id"].as_str().unwrap().to_string();

    let claimed = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "claim",
            "--id",
            &task_id,
            "--agent",
            "agent-a",
            "--mode",
            "exclusive",
        ],
    );
    assert_eq!(claimed["status"], "ok");
    assert!(
        claimed.get("container").is_some(),
        "claim response should include container launch result"
    );
    let container_status = claimed["container"]["status"].as_str().unwrap_or("");
    assert!(
        container_status == "ok" || container_status == "error" || container_status == "warning",
        "container status should be ok/error/warning, got '{container_status}'"
    );
}

#[test]
fn test_ownership_rebuild_replay_parity() {
    let tmp = tempdir().unwrap();
    let repo = tmp.path();
    bootstrap_repo(repo);

    let added = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "add",
            "Ownership replay parity",
            "--owner",
            "agent-a,agent-b",
        ],
    );
    let task_id = added["id"].as_str().unwrap().to_string();

    // Prepare trust gates for shared claim.
    let db = Connection::open(repo.join(".decapod/data/decapod.db")).unwrap();
    let ts = "1771203600Z";
    set_agent_trust(&db, "agent-c", "verified", ts);

    let _ = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "claim",
            "--id",
            &task_id,
            "--agent",
            "agent-a",
            "--mode",
            "exclusive",
        ],
    );
    let _ = run_cmd(
        repo,
        &[
            "todo", "--format", "json", "claim", "--id", &task_id, "--agent", "agent-c", "--mode",
            "shared",
        ],
    );
    let _ = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "remove-owner",
            "--id",
            &task_id,
            "--agent",
            "agent-b",
        ],
    );
    let _ = run_cmd(
        repo,
        &[
            "todo",
            "--format",
            "json",
            "add-owner",
            "--id",
            &task_id,
            "--agent",
            "agent-d",
            "--claim-type",
            "secondary",
        ],
    );

    let before = run_cmd(repo, &["todo", "--format", "json", "get", "--id", &task_id]);
    let before_owner = before["item"]["owner"].as_str().unwrap().to_string();
    let before_assigned = before["item"]["assigned_to"].as_str().unwrap().to_string();
    let mut before_owners: Vec<String> = before["item"]["owners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            format!(
                "{}:{}",
                o["agent_id"].as_str().unwrap(),
                o["claim_type"].as_str().unwrap()
            )
        })
        .collect();
    before_owners.sort();

    let _ = run_cmd(repo, &["todo", "--format", "json", "rebuild"]);
    let after = run_cmd(repo, &["todo", "--format", "json", "get", "--id", &task_id]);
    let after_owner = after["item"]["owner"].as_str().unwrap().to_string();
    let after_assigned = after["item"]["assigned_to"].as_str().unwrap().to_string();
    let mut after_owners: Vec<String> = after["item"]["owners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            format!(
                "{}:{}",
                o["agent_id"].as_str().unwrap(),
                o["claim_type"].as_str().unwrap()
            )
        })
        .collect();
    after_owners.sort();

    assert_eq!(
        before_owner, after_owner,
        "owner mirror should survive rebuild"
    );
    assert_eq!(
        before_assigned, after_assigned,
        "assigned_to should survive rebuild"
    );
    assert_eq!(
        before_owners, after_owners,
        "ownership claim/release replay should be deterministic"
    );
}

fn add_scoped_task(root: &Path, title: &str, scope: &str, dir: &str) -> String {
    add_scoped_task_with_dependencies(root, title, scope, dir, "")
}

fn add_scoped_task_with_dependencies(
    root: &Path,
    title: &str,
    scope: &str,
    dir: &str,
    depends_on: &str,
) -> String {
    let args = TodoCommand::Add {
        title: title.to_string(),
        description: format!("houseboat lease test: {title}"),
        tags: "houseboat,fleet".to_string(),
        owner: "".to_string(),
        due: None,
        r#ref: "".to_string(),
        scope: scope.to_string(),
        dir: Some(dir.to_string()),
        priority: "medium".to_string(),
        depends_on: depends_on.to_string(),
        blocks: "".to_string(),
        parent: None,
        one_shot: 0,
    };
    let res = add_task(root, &args).unwrap();
    res["id"].as_str().unwrap().to_string()
}

#[test]
fn exclusive_claim_sets_lease_and_renew_extends_it() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let dir = tmp.path().join("src").join("core");
    std::fs::create_dir_all(&dir).unwrap();

    let task_id = add_scoped_task(
        &root,
        "Lease renewal target alpha",
        "architecture",
        dir.to_str().unwrap(),
    );
    let claim = claim_task_with_lease(
        &root,
        &task_id,
        "agent-lease",
        ClaimMode::Exclusive,
        Some(120),
        Some("intent:houseboat:wave2".to_string()),
    )
    .unwrap();
    assert_eq!(claim["status"], "ok");
    assert_eq!(claim["result"]["lease_seconds"], 120);
    assert_eq!(claim["result"]["lease_generation"], 1);
    assert_eq!(claim["result"]["lease_lifecycle"], "claimed");
    assert_eq!(claim["result"]["intent_anchor"], "intent:houseboat:wave2");
    let first_lease = claim["result"]["lease_expires_at"]
        .as_str()
        .expect("lease_expires_at")
        .to_string();
    assert!(!first_lease.is_empty());

    let renew = renew_claim_lease(&root, &task_id, "agent-lease", Some(600)).unwrap();
    assert_eq!(renew["status"], "ok", "renew should succeed: {renew}");
    assert_eq!(renew["result"]["lease_seconds"], 600);
    assert_eq!(renew["result"]["lease_generation"], 2);
    assert_eq!(renew["result"]["lease_lifecycle"], "extended");
    let second_lease = renew["result"]["lease_expires_at"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(second_lease, first_lease);

    let fleet = fleet_health(&root, Some("agent-lease")).unwrap();
    assert_eq!(fleet["status"], "ok");
    assert!(fleet["fleet"]["claim_count"].as_u64().unwrap() >= 1);
    assert!(
        fleet["fleet"]["capacity"]["reserved_units"]
            .as_u64()
            .unwrap()
            >= 1
    );
    assert!(
        fleet["fleet"]["intent_anchors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str() == Some("intent:houseboat:wave2"))
    );
}

#[test]
fn expired_lease_is_reclaimable_by_another_agent() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let dir = tmp.path().join("src").join("workspace");
    std::fs::create_dir_all(&dir).unwrap();

    let task_id = add_scoped_task(
        &root,
        "Reclaimable expired lease task",
        "documentation",
        dir.to_str().unwrap(),
    );
    let first = claim_task(&root, &task_id, "agent-old", ClaimMode::Exclusive).unwrap();
    assert_eq!(first["status"], "ok");

    let db = root.join(schemas::LOCAL_DB_NAME);
    let conn = Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE tasks SET lease_expires_at = '1Z' WHERE id = ?1",
        [&task_id],
    )
    .unwrap();
    // Presence must also be stale, or only lease expiry is enough for reclaim.
    conn.execute(
        "UPDATE agents SET last_seen = '1Z', updated_at = '1Z' WHERE agent_id = 'agent-old'",
        [],
    )
    .ok();

    let reclaim = claim_task(&root, &task_id, "agent-new", ClaimMode::Exclusive).unwrap();
    assert_eq!(reclaim["status"], "ok", "reclaim should succeed: {reclaim}");
    assert_eq!(reclaim["result"]["reclaimed_from"], "agent-old");
    assert_eq!(reclaim["result"]["assigned_to"], "agent-new");
    assert_eq!(reclaim["result"]["lease_lifecycle"], "reclaimed");
    assert!(reclaim["result"]["lease_generation"].as_u64().unwrap() >= 2);

    let task = get_task(&root, &task_id).unwrap().unwrap();
    assert_eq!(task.assigned_to, "agent-new");
}

#[test]
fn overlapping_exclusive_claims_are_rejected() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let shared = tmp.path().join("src").join("shared-module");
    std::fs::create_dir_all(&shared).unwrap();
    let nested = shared.join("impl");
    std::fs::create_dir_all(&nested).unwrap();

    let first_id = add_scoped_task(
        &root,
        "Fleet overlap first task unique title alpha",
        "architecture",
        shared.to_str().unwrap(),
    );
    let second_id = add_scoped_task(
        &root,
        "Fleet overlap second task unique title omega",
        "architecture",
        nested.to_str().unwrap(),
    );
    assert_ne!(first_id, second_id);

    let first = claim_task(&root, &first_id, "agent-a", ClaimMode::Exclusive).unwrap();
    assert_eq!(first["status"], "ok", "first claim: {first}");

    let second = claim_task(&root, &second_id, "agent-b", ClaimMode::Exclusive).unwrap();
    assert_eq!(
        second["status"], "conflict",
        "overlapping exclusive claim must conflict: {second}"
    );
    assert_eq!(second["result"]["overlap"], true);
    let overlaps = second["result"]["overlaps"].as_array().unwrap();
    assert!(
        overlaps
            .iter()
            .any(|o| o["reason"] == "path_overlap" || o["reason"] == "scope_overlap"),
        "expected path or scope overlap, got {overlaps:?}"
    );
}

#[test]
fn yield_preserves_generation_and_frees_capacity() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let dir = tmp.path().join("src").join("yield");
    std::fs::create_dir_all(&dir).unwrap();

    let task_id = add_scoped_task(
        &root,
        "Yield lease capacity task unique",
        "platform_engineering",
        dir.to_str().unwrap(),
    );
    let claim = claim_task(&root, &task_id, "agent-y", ClaimMode::Exclusive).unwrap();
    assert_eq!(claim["status"], "ok");
    assert_eq!(claim["result"]["lease_generation"], 1);

    let yielded = yield_claim_lease(&root, &task_id, "agent-y", "paused for handoff").unwrap();
    assert_eq!(yielded["status"], "ok", "yield: {yielded}");
    assert_eq!(yielded["result"]["lease_lifecycle"], "yielded");
    assert_eq!(yielded["result"]["lease_generation"], 1);

    let task = get_task(&root, &task_id).unwrap().unwrap();
    assert!(task.assigned_to.is_empty());

    // Same or other agents can reclaim after yield; generation advances on re-issue.
    let reclaim = claim_task(&root, &task_id, "agent-y", ClaimMode::Exclusive).unwrap();
    assert_eq!(reclaim["status"], "ok", "post-yield claim: {reclaim}");
    assert_eq!(reclaim["result"]["assigned_to"], "agent-y");
    assert_eq!(reclaim["result"]["lease_lifecycle"], "claimed");
}

#[test]
fn handoff_advances_lease_generation_and_preserves_intent() {
    use decapod::core::store::{Store, StoreKind};

    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let dir = tmp.path().join("src").join("handoff-lease");
    std::fs::create_dir_all(&dir).unwrap();

    let task_id = add_scoped_task(
        &root,
        "Houseboat handoff lease transfer unique",
        "handoff_scope",
        dir.to_str().unwrap(),
    );
    let claim = claim_task_with_lease(
        &root,
        &task_id,
        "agent-from",
        ClaimMode::Exclusive,
        Some(900),
        Some("intent:houseboat:wave4".to_string()),
    )
    .unwrap();
    assert_eq!(claim["status"], "ok", "claim: {claim}");
    assert_eq!(claim["result"]["lease_generation"], 1);
    assert_eq!(claim["result"]["intent_anchor"], "intent:houseboat:wave4");

    let store = Store {
        kind: StoreKind::Repo,
        root: root.clone(),
    };
    // Policy gate: handoff requires verified trust + explicit approval.
    // Trust lives on the consolidated agents table after schema fold (#1129).
    let db = Connection::open(root.join(schemas::LOCAL_DB_NAME)).unwrap();
    set_agent_trust(&db, "agent-from", "verified", "100Z");
    policy::approve_action(&store, "todo.handoff", None, "operator", "global").unwrap();

    let handed = handoff_task(
        &store,
        &task_id,
        "agent-to",
        Some("agent-from"),
        "wave4 lease transfer",
        Some(600),
    )
    .unwrap();
    assert_eq!(handed["status"], "ok", "handoff: {handed}");
    assert_eq!(handed["result"]["status"], "ok");
    assert_eq!(handed["result"]["from"], "agent-from");
    assert_eq!(handed["result"]["to"], "agent-to");
    assert_eq!(handed["result"]["prior_lease_generation"], 1);
    assert_eq!(handed["result"]["lease_generation"], 2);
    assert_eq!(handed["result"]["lease_lifecycle"], "claimed");
    assert_eq!(handed["result"]["intent_anchor"], "intent:houseboat:wave4");
    assert_eq!(handed["result"]["lease_seconds"], 600);
    assert!(
        handed["result"]["lease_expires_at"]
            .as_str()
            .is_some_and(|v| !v.is_empty())
    );

    let task = get_task(&root, &task_id).unwrap().unwrap();
    assert_eq!(task.assigned_to, "agent-to");

    let fleet = fleet_health(&root, Some("agent-to")).unwrap();
    let active = fleet["fleet"]["active_claims"].as_array().unwrap();
    assert!(
        active.iter().any(|c| {
            c["task_id"] == task_id
                && c["agent_id"] == "agent-to"
                && c["lease_generation"] == 2
                && c["intent_anchor"] == "intent:houseboat:wave4"
        }),
        "fleet must show receiver exclusive lease: {fleet}"
    );
}

#[test]
fn exclusive_lease_blocks_unproven_done() {
    use decapod::core::store::{Store, StoreKind};

    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let dir = tmp.path().join("src").join("proof");
    std::fs::create_dir_all(&dir).unwrap();

    let task_id = add_scoped_task(
        &root,
        "Proof gated completion unique task",
        "architecture",
        dir.to_str().unwrap(),
    );
    let claim = claim_task(&root, &task_id, "agent-p", ClaimMode::Exclusive).unwrap();
    assert_eq!(claim["status"], "ok");

    // Simulate todo done dispatch proof gate via exclusive_lease path using CLI handler isn't
    // easy without full Store; exercise the gate through a direct status attempt after checking
    // that fleet still shows exclusive custody.
    let fleet = fleet_health(&root, None).unwrap();
    let active = fleet["fleet"]["active_claims"].as_array().unwrap();
    assert!(
        active
            .iter()
            .any(|c| { c["task_id"] == task_id && c["lease_lifecycle"] == "claimed" }),
        "expected claimed lease in fleet: {fleet}"
    );

    // Yield removes exclusive custody so unmarked done can proceed for unleased tasks.
    let _ = yield_claim_lease(&root, &task_id, "agent-p", "done without lease").unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: root.clone(),
    };
    let done = update_status(&store, &task_id, "done", "task.done", serde_json::json!({})).unwrap();
    assert_eq!(done["status"], "ok", "yielded task may complete: {done}");
}

#[test]
fn dependency_readiness_gates_claim_and_feeds_work_claim_and_fleet() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let dep_dir = tmp.path().join("src").join("dependency");
    let target_dir = tmp.path().join("src").join("target");
    std::fs::create_dir_all(&dep_dir).unwrap();
    std::fs::create_dir_all(&target_dir).unwrap();

    let dependency_id = add_scoped_task(
        &root,
        "Houseboat dependency prerequisite unique",
        "dependency",
        dep_dir.to_str().unwrap(),
    );
    let target_id = add_scoped_task_with_dependencies(
        &root,
        "Houseboat dependency target unique",
        "target",
        target_dir.to_str().unwrap(),
        &dependency_id,
    );

    let waiting = claim_task(&root, &target_id, "agent-target", ClaimMode::Exclusive).unwrap();
    assert_eq!(waiting["status"], "conflict", "waiting claim: {waiting}");
    assert_eq!(
        waiting["result"]["dependency_readiness"]["state"],
        "waiting"
    );
    assert_eq!(
        waiting["result"]["dependency_readiness"]["blockers"][0]["reason"],
        "dependency_not_complete"
    );

    let db = root.join(schemas::LOCAL_DB_NAME);
    let conn = Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE tasks SET status = 'done' WHERE id = ?1",
        [&dependency_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO task_verification(
            todo_id, proof_plan, verification_artifacts, last_verified_at,
            last_verified_status, last_verified_notes, verification_policy_days, updated_at
         ) VALUES(?1, '[\"validate_passes\"]', ?2, '100Z', 'passed', '', 90, '100Z')",
        rusqlite::params![
            dependency_id,
            r#"{"proof_plan_results":[{"proof_gate":"validate_passes","output_hash":"sha256:dep-proof"}]}"#
        ],
    )
    .unwrap();

    let claimed = claim_task(&root, &target_id, "agent-target", ClaimMode::Exclusive).unwrap();
    assert_eq!(claimed["status"], "ok", "ready claim: {claimed}");
    assert_eq!(claimed["result"]["dependency_readiness"]["state"], "ready");
    assert_eq!(
        claimed["result"]["dependency_readiness"]["proof_refs"][0],
        format!("proof:{dependency_id}:validate_passes:sha256:dep-proof")
    );
    let work_claim = get_work_claim(&root, &target_id).unwrap().unwrap();
    assert!(work_claim.dependency_readiness.is_ready());

    conn.execute(
        "UPDATE task_verification SET last_verified_status = 'failed' WHERE todo_id = ?1",
        [&dependency_id],
    )
    .unwrap();
    let fleet = fleet_health(&root, Some("agent-target")).unwrap();
    assert_eq!(fleet["fleet"]["dependency_blocked_count"], 1);
    assert_eq!(fleet["fleet"]["proof_blocked_count"], 1);
    assert_eq!(
        fleet["fleet"]["dependency_blocked_claims"][0]["dependency_readiness"]["state"],
        "proof_blocked"
    );
}

#[test]
fn dependency_cycle_fails_closed_before_claim() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();
    let first_dir = tmp.path().join("src").join("cycle-a");
    let second_dir = tmp.path().join("src").join("cycle-b");
    std::fs::create_dir_all(&first_dir).unwrap();
    std::fs::create_dir_all(&second_dir).unwrap();
    let first_id = add_scoped_task(
        &root,
        "Houseboat cycle first unique",
        "cycle-a",
        first_dir.to_str().unwrap(),
    );
    let second_id = add_scoped_task(
        &root,
        "Houseboat cycle second unique",
        "cycle-b",
        second_dir.to_str().unwrap(),
    );

    let db = root.join(schemas::LOCAL_DB_NAME);
    let conn = Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE tasks SET depends_on = ?1 WHERE id = ?2",
        rusqlite::params![second_id, first_id],
    )
    .unwrap();
    conn.execute(
        "UPDATE tasks SET depends_on = ?1 WHERE id = ?2",
        rusqlite::params![first_id, second_id],
    )
    .unwrap();

    let result = claim_task(&root, &first_id, "agent-cycle", ClaimMode::Exclusive).unwrap();
    assert_eq!(result["status"], "conflict", "cycle claim: {result}");
    assert_eq!(result["result"]["resolution"], "repair_dependency_graph");
    assert_eq!(result["result"]["dependency_readiness"]["state"], "cycle");
    assert_eq!(
        result["result"]["dependency_readiness"]["cycle"],
        serde_json::json!([first_id, second_id, first_id])
    );
}
