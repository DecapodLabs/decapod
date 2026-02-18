use decapod::core::store::Store;
use decapod::core::store::StoreKind;
use decapod::core::todo::{
    TodoCommand, add_task, check_trust_level, get_task, initialize_todo_db, list_tasks,
    rebuild_from_events, todo_db_path, update_status,
};
use decapod::plugins::policy;
use rusqlite::Connection;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

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
        dir: Some(tmp.path().to_string_lossy().to_string()),
        priority: "high".to_string(),
        depends_on: "".to_string(),
        blocks: "".to_string(),
        parent: None,
    };
    let res = add_task(&root, &add_args).unwrap();
    let task_id = res.get("id").unwrap().as_str().unwrap();
    assert!(task_id.contains("_"));

    // 2. Get task
    let task = get_task(&root, task_id).unwrap().expect("Task not found");
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
fn test_todo_rebuild() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_todo_db(&root).unwrap();

    // Add some tasks
    for i in 0..3 {
        let add_args = TodoCommand::Add {
            title: format!("Task {}", i),
            description: "".to_string(),
            tags: "".to_string(),
            owner: "".to_string(),
            due: None,
            r#ref: "".to_string(),
            dir: Some(tmp.path().to_string_lossy().to_string()),
            priority: "medium".to_string(),
            depends_on: "".to_string(),
            blocks: "".to_string(),
            parent: None,
        };
        add_task(&root, &add_args).unwrap();
    }

    // Corrupt/Delete DB
    let db_path = todo_db_path(&root);
    fs::remove_file(&db_path).unwrap();

    // Rebuild
    rebuild_from_events(&root).unwrap();

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

    let db = Connection::open(repo.join(".decapod/data/todo.db")).unwrap();
    let ts = "1771202800Z";
    db.execute(
        "INSERT INTO agent_trust(agent_id, trust_level, granted_at, updated_at, granted_by)
         VALUES(?1, 'verified', ?2, ?2, 'test')
         ON CONFLICT(agent_id) DO UPDATE SET trust_level='verified', updated_at=?2, granted_by='test'",
        rusqlite::params!["agent-b", ts],
    )
    .unwrap();

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
    let db = Connection::open(repo.join(".decapod/data/todo.db")).unwrap();
    let ts = "1771203000Z";
    db.execute(
        "INSERT INTO agent_trust(agent_id, trust_level, granted_at, updated_at, granted_by)
         VALUES(?1, 'verified', ?2, ?2, 'test')
         ON CONFLICT(agent_id) DO UPDATE SET trust_level='verified', updated_at=?2, granted_by='test'",
        rusqlite::params!["agent-b", ts],
    )
    .unwrap();
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
    db.execute(
        "INSERT INTO agent_trust(agent_id, trust_level, granted_at, updated_at, granted_by)
         VALUES(?1, 'verified', ?2, ?2, 'test')
         ON CONFLICT(agent_id) DO UPDATE SET trust_level='verified', updated_at=?2, granted_by='test'",
        rusqlite::params!["agent-a", ts],
    )
    .unwrap();

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
        container_status == "ok" || container_status == "error",
        "container status should be ok/error, got '{}'",
        container_status
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
    let db = Connection::open(repo.join(".decapod/data/todo.db")).unwrap();
    let ts = "1771203600Z";
    db.execute(
        "INSERT INTO agent_trust(agent_id, trust_level, granted_at, updated_at, granted_by)
         VALUES(?1, 'verified', ?2, ?2, 'test')
         ON CONFLICT(agent_id) DO UPDATE SET trust_level='verified', updated_at=?2, granted_by='test'",
        rusqlite::params!["agent-c", ts],
    )
    .unwrap();

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
