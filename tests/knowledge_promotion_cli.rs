use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(dir).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run decapod")
}

fn setup_repo() -> (TempDir, PathBuf, String) {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path().to_path_buf();

    let git_init = Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(git_init.status.success(), "git init failed");

    let init = run_decapod(&dir, &["init", "--force"], &[]);
    assert!(
        init.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let acquire = run_decapod(
        &dir,
        &["session", "acquire"],
        &[("DECAPOD_AGENT_ID", "unknown")],
    );
    assert!(
        acquire.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );
    let password = String::from_utf8_lossy(&acquire.stdout)
        .lines()
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .expect("password in session acquire output");

    (tmp, dir, password)
}

#[test]
fn knowledge_promote_writes_append_only_database_event() {
    let (_tmp, dir, password) = setup_repo();

    let out = run_decapod(
        &dir,
        &[
            "data",
            "knowledge",
            "promote",
            "--source-entry-id",
            "K_001",
            "--evidence-ref",
            "commit:abc123",
            "--evidence-ref",
            "file:docs/spec.md#L10",
            "--approved-by",
            "human/reviewer-1",
            "--reason",
            "convert episodic finding into procedural norm",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        out.status.success(),
        "knowledge promote failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let payload: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(payload["source_entry_id"], "K_001");
    assert_eq!(payload["target_class"], "procedural");
    assert_eq!(payload["approved_by"], "human/reviewer-1");

    let db_path = dir.join(".decapod").join("data").join("decapod.db");
    let conn = decapod::core::db::Connection::open(db_path).expect("open canonical datastore");
    let raw: String = conn
        .query_row(
            "SELECT payload FROM events WHERE stream = 'knowledge' AND event_id = ?1",
            [payload["event_id"].as_str().expect("event id")],
            |row| row.get(0),
        )
        .expect("promotion event in canonical datastore");
    let event: Value = serde_json::from_str(&raw).expect("valid event payload");
    assert_eq!(event["source_entry_id"], "K_001");
    assert_eq!(event["target_class"], "procedural");
}

#[test]
fn knowledge_promote_rejects_missing_evidence_refs() {
    let (_tmp, dir, password) = setup_repo();

    let out = run_decapod(
        &dir,
        &[
            "data",
            "knowledge",
            "promote",
            "--source-entry-id",
            "K_002",
            "--approved-by",
            "human/reviewer-2",
            "--reason",
            "insufficient evidence should fail",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        !out.status.success(),
        "promote should fail without evidence refs"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("at least one --evidence-ref is required"),
        "unexpected error: {stderr}"
    );
}

#[test]
fn procedural_knowledge_add_requires_promotion_event_provenance() {
    let (_tmp, dir, password) = setup_repo();

    let out = run_decapod(
        &dir,
        &[
            "data",
            "knowledge",
            "add",
            "--id",
            "procedural/commit_norms/no-event",
            "--title",
            "Commit norms",
            "--text",
            "Must include tests",
            "--provenance",
            "commit:abc123",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        !out.status.success(),
        "procedural add should fail without event-backed provenance"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("procedural knowledge entries require provenance"),
        "unexpected error: {stderr}"
    );
}

#[test]
fn procedural_knowledge_add_accepts_valid_promotion_event_provenance() {
    let (_tmp, dir, password) = setup_repo();

    let promote = run_decapod(
        &dir,
        &[
            "data",
            "knowledge",
            "promote",
            "--source-entry-id",
            "K_source",
            "--evidence-ref",
            "commit:abc123",
            "--approved-by",
            "human/reviewer-3",
            "--reason",
            "promote proven workflow guidance",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        promote.status.success(),
        "knowledge promote failed: {}",
        String::from_utf8_lossy(&promote.stderr)
    );
    let promote_payload: Value = serde_json::from_slice(&promote.stdout).expect("promotion json");
    let event_id = promote_payload["event_id"]
        .as_str()
        .expect("event_id from promotion output");
    let provenance = format!("event:{event_id}");

    let add = run_decapod(
        &dir,
        &[
            "data",
            "knowledge",
            "add",
            "--id",
            "procedural/commit_norms/with-event",
            "--title",
            "Commit norms",
            "--text",
            "Run tests before publish",
            "--provenance",
            &provenance,
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        add.status.success(),
        "procedural add should succeed with valid event-backed provenance: {}",
        String::from_utf8_lossy(&add.stderr)
    );
}

#[test]
fn knowledge_write_attestation_supersession_and_outcome_feedback_end_to_end() {
    let temp = TempDir::new().unwrap();
    let repo_root = temp.path();
    let data_dir = repo_root.join(".decapod/data");
    std::fs::create_dir_all(&data_dir).unwrap();
    decapod::core::todo::initialize_todo_db(&data_dir).unwrap();
    decapod::core::db::initialize_knowledge_db(&data_dir).unwrap();
    let store = decapod::core::store::Store {
        kind: decapod::core::store::StoreKind::Repo,
        root: data_dir,
    };

    let guide = repo_root.join("ARCHITECTURE_NOTE.md");
    std::fs::write(&guide, "Use option A for storage transactions.\n").unwrap();

    // 1. Seed rejected choice B repeated 20 times, then supersede with attested A.
    let repeated_b = "Prefer legacy option B. ".repeat(20);
    decapod::plugins::knowledge::add_attested_knowledge(
        &store,
        repo_root,
        decapod::plugins::knowledge::AddKnowledgeParams {
            id: "K_B",
            title: "Storage choice B",
            content: &repeated_b,
            provenance: "file:ARCHITECTURE_NOTE.md#L1",
            claim_id: None,
            merge_key: Some("storage-tx-choice"),
            conflict_policy: decapod::plugins::knowledge::KnowledgeConflictPolicy::Supersede,
            status: "active",
            ttl_policy: "persistent",
            expires_ts: None,
        },
        "root",
    )
    .unwrap();

    let res = decapod::plugins::knowledge::add_attested_knowledge(
        &store,
        repo_root,
        decapod::plugins::knowledge::AddKnowledgeParams {
            id: "K_A",
            title: "Storage choice A",
            content: "Human accepted option A over B for transactional safety.",
            provenance: "file:ARCHITECTURE_NOTE.md#L1",
            claim_id: None,
            merge_key: Some("storage-tx-choice"),
            conflict_policy: decapod::plugins::knowledge::KnowledgeConflictPolicy::Supersede,
            status: "active",
            ttl_policy: "persistent",
            expires_ts: None,
        },
        "root",
    )
    .unwrap();
    assert_eq!(res.action, "superseded");
    assert_eq!(res.superseded_ids, vec!["K_B".to_string()]);

    // Fresh operative packet must contain ONLY A, with write-time attestation & supersedes_id = K_B.
    let pkt1 = decapod::plugins::knowledge::resolve_context(
        &store,
        repo_root,
        Some("storage option"),
        None,
        &[],
        None,
    )
    .unwrap();
    assert_eq!(pkt1.status, "advisory_knowledge_selected");
    assert_eq!(pkt1.records.len(), 1);
    let rec = &pkt1.records[0];
    assert_eq!(rec.id, "K_A");
    assert_eq!(rec.supersedes_id.as_deref(), Some("K_B"));
    assert_eq!(rec.provenance_class, "write_time_source_attested");
    assert_eq!(rec.freshness, "source_attested_current");
    assert!(!rec.excerpt.contains("Prefer legacy option B"));

    // Historical B remains explicitly auditable by ID and history event.
    let hist_b = decapod::plugins::knowledge::get_knowledge(&store, "K_B")
        .unwrap()
        .expect("superseded B retained for audit");
    assert_eq!(hist_b.status, "superseded");
    assert!(hist_b.content.contains("Prefer legacy option B"));
    let audit = decapod::plugins::knowledge::knowledge_history(&store, "K_B").unwrap();
    assert_eq!(audit.len(), 1);

    // 2. Mutating the attested source file must surface `source_changed_since_write`.
    std::fs::write(&guide, "Updated note after write.\n").unwrap();
    let pkt2 = decapod::plugins::knowledge::resolve_context(
        &store,
        repo_root,
        Some("storage option"),
        None,
        &[],
        None,
    )
    .unwrap();
    assert_eq!(pkt2.records[0].freshness, "source_changed_since_write");
    assert!(
        pkt2.records[0]
            .reasons
            .iter()
            .any(|r| r == "source_file_modified_since_attestation")
    );

    // 3. Add a second independent entry K_C, mark K_A with `hurt` outcome feedback:
    // K_A must carry visible caution and sort behind K_C without self-promoting K_C.
    decapod::plugins::knowledge::add_attested_knowledge(
        &store,
        repo_root,
        decapod::plugins::knowledge::AddKnowledgeParams {
            id: "K_C",
            title: "Storage checklist",
            content: "Always check storage option invariants.",
            provenance: "file:ARCHITECTURE_NOTE.md#L1",
            claim_id: None,
            merge_key: Some("storage-checklist"),
            conflict_policy: decapod::plugins::knowledge::KnowledgeConflictPolicy::Supersede,
            status: "active",
            ttl_policy: "persistent",
            expires_ts: None,
        },
        "root",
    )
    .unwrap();
    decapod::plugins::knowledge::log_retrieval_feedback(
        &store,
        "storage option",
        &["K_A".to_string(), "K_C".to_string()],
        &["K_A".to_string()],
        "hurt",
        "manual_feedback",
    )
    .unwrap();

    let pkt3 = decapod::plugins::knowledge::resolve_context(
        &store,
        repo_root,
        Some("storage option"),
        None,
        &[],
        None,
    )
    .unwrap();
    assert_eq!(pkt3.records.len(), 2);
    assert_eq!(pkt3.records[0].id, "K_C");
    assert_eq!(pkt3.records[1].id, "K_A");
    assert_eq!(
        pkt3.records[1].feedback_summary.as_ref().map(|f| f.hurt),
        Some(1)
    );
    assert!(
        pkt3.records[1]
            .reasons
            .iter()
            .any(|r| r == "prior_retrieval_outcome_hurt")
    );
    assert_eq!(
        pkt3.records[0].authority,
        "advisory_below_current_intent_and_policy"
    );
}
