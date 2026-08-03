use decapod::core::store::{Store, StoreKind};
use decapod::plugins::federation::{
    FederationCli, FederationCommand, OutputFormat, add_edge, add_node, add_source_to_node,
    edit_node, find_node_by_source, initialize_federation_db, rebuild_from_events,
    run_federation_cli, supersede_node, transition_node_status, validate_federation,
};
use std::fs;
use tempfile::tempdir;

fn test_store() -> (tempfile::TempDir, Store) {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_federation_db(&root).unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root,
    };
    (tmp, store)
}

fn build_derived(store: &Store) {
    run_federation_cli(
        store,
        FederationCli {
            format: OutputFormat::Json,
            command: FederationCommand::IndexBuild,
        },
    )
    .unwrap();
    run_federation_cli(
        store,
        FederationCli {
            format: OutputFormat::Json,
            command: FederationCommand::GraphExport,
        },
    )
    .unwrap();
}

#[test]
fn test_add_and_list_node() {
    let (_tmp, store) = test_store();

    let node = add_node(
        &store,
        "Test lesson",
        "lesson",
        "notable",
        "agent_inferred",
        "Learned something important.",
        "",
        "ops",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    assert!(node.id.starts_with("F_"));
    assert_eq!(node.node_type, "lesson");
    assert_eq!(node.status, "active");
    assert_eq!(node.priority, "notable");
    assert_eq!(node.title, "Test lesson");
    assert_eq!(node.actor, "decapod");
}

#[test]
fn test_provenance_required_for_critical() {
    let (_tmp, store) = test_store();

    // Decision without sources should fail
    let result = add_node(
        &store,
        "Bad decision",
        "decision",
        "critical",
        "agent_inferred",
        "",
        "", // no sources
        "",
        "repo",
        None,
        "decapod",
    );
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Provenance required"));

    // Commitment without sources should also fail
    let result = add_node(
        &store,
        "Bad commitment",
        "commitment",
        "notable",
        "agent_inferred",
        "",
        "",
        "",
        "repo",
        None,
        "decapod",
    );
    assert!(result.is_err());

    // Decision with valid sources should succeed
    let node = add_node(
        &store,
        "Good decision",
        "decision",
        "critical",
        "agent_inferred",
        "Chose X over Y",
        "commit:abcdef01",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();
    assert_eq!(node.node_type, "decision");
    assert_eq!(node.sources.as_ref().unwrap().len(), 1);
}

#[test]
fn test_invalid_provenance_rejected() {
    let (_tmp, store) = test_store();

    let result = add_node(
        &store,
        "Bad source",
        "lesson",
        "critical",
        "agent_inferred",
        "",
        "not-a-valid-source",
        "",
        "repo",
        None,
        "decapod",
    );
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Invalid provenance")
    );
}

#[test]
fn test_edit_non_critical_node() {
    let (_tmp, store) = test_store();

    let node = add_node(
        &store,
        "Original title",
        "lesson",
        "notable",
        "agent_inferred",
        "Original body",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    // Edit should succeed for non-critical type
    edit_node(&store, &node.id, Some("New title"), None, None, None).unwrap();
}

#[test]
fn test_edit_critical_node_rejected() {
    let (_tmp, store) = test_store();

    let node = add_node(
        &store,
        "A decision",
        "decision",
        "critical",
        "agent_inferred",
        "",
        "file:README.md",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    // Edit should fail for critical type
    let result = edit_node(&store, &node.id, Some("Changed"), None, None, None);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Cannot edit critical")
    );
}

#[test]
fn test_supersede_lifecycle() {
    let (_tmp, store) = test_store();

    let old = add_node(
        &store,
        "Old decision",
        "decision",
        "critical",
        "agent_inferred",
        "",
        "file:old.rs",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    let new = add_node(
        &store,
        "New decision",
        "decision",
        "critical",
        "agent_inferred",
        "",
        "file:new.rs",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    supersede_node(&store, &old.id, &new.id, "Requirements changed").unwrap();

    // Old node should now be superseded (verify via validate)
    build_derived(&store);
    let results = validate_federation(&store.root).unwrap();
    for (gate, passed, _msg) in &results {
        assert!(passed, "Gate {gate} failed");
    }
}

#[test]
fn test_status_transition_only_from_active() {
    let (_tmp, store) = test_store();

    let node = add_node(
        &store,
        "A lesson",
        "lesson",
        "notable",
        "agent_inferred",
        "",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    // Deprecate should work from active
    transition_node_status(&store, &node.id, "deprecated", "node.deprecate", "outdated").unwrap();

    // Can't deprecate again (already deprecated)
    let result = transition_node_status(&store, &node.id, "disputed", "node.dispute", "also bad");
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Only active nodes")
    );
}

#[test]
fn test_edge_operations() {
    let (_tmp, store) = test_store();

    let a = add_node(
        &store,
        "Node A",
        "project",
        "notable",
        "agent_inferred",
        "",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    let b = add_node(
        &store,
        "Node B",
        "lesson",
        "notable",
        "agent_inferred",
        "",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    let edge_id = add_edge(&store, &a.id, &b.id, "relates_to").unwrap();
    assert!(edge_id.starts_with("FE_"));
}

#[test]
fn test_invalid_edge_type_rejected() {
    let (_tmp, store) = test_store();

    let a = add_node(
        &store,
        "Node A",
        "project",
        "notable",
        "agent_inferred",
        "",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    let b = add_node(
        &store,
        "Node B",
        "lesson",
        "notable",
        "agent_inferred",
        "",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    let result = add_edge(&store, &a.id, &b.id, "bogus_type");
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Invalid edge_type")
    );
}

#[test]
fn test_rebuild_determinism() {
    let (_tmp, store) = test_store();

    // Create several nodes
    let n1 = add_node(
        &store,
        "Decision 1",
        "decision",
        "critical",
        "human_confirmed",
        "Body 1",
        "file:a.rs",
        "tag1",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    let n2 = add_node(
        &store,
        "Lesson 1",
        "lesson",
        "notable",
        "agent_inferred",
        "Body 2",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    // Link them
    add_edge(&store, &n1.id, &n2.id, "relates_to").unwrap();

    // Deprecate n2
    transition_node_status(&store, &n2.id, "deprecated", "node.deprecate", "old").unwrap();

    // Rebuild
    let count = rebuild_from_events(&store.root).unwrap();
    assert!(count > 0);

    // Validate after rebuild — all gates should pass
    build_derived(&store);
    let results = validate_federation(&store.root).unwrap();
    for (gate, passed, msg) in &results {
        assert!(passed, "Gate {gate} failed: {msg}");
    }
}

#[test]
fn test_add_source_to_node() {
    let (_tmp, store) = test_store();

    let node = add_node(
        &store,
        "A lesson",
        "lesson",
        "notable",
        "agent_inferred",
        "",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    // Add a valid source
    let src_id = add_source_to_node(&store, &node.id, "file:README.md").unwrap();
    assert!(src_id.starts_with("FS_"));

    // Add another source
    let src_id2 = add_source_to_node(&store, &node.id, "commit:abc123").unwrap();
    assert!(src_id2.starts_with("FS_"));

    // Invalid source should be rejected
    let result = add_source_to_node(&store, &node.id, "not-valid");
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Invalid provenance")
    );

    // Non-existent node should fail
    let result = add_source_to_node(&store, "F_nonexistent", "file:foo.rs");
    assert!(result.is_err());
}

#[test]
fn test_add_source_survives_rebuild() {
    let (_tmp, store) = test_store();

    let node = add_node(
        &store,
        "Decision X",
        "decision",
        "critical",
        "agent_inferred",
        "body",
        "file:initial.rs",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    // Add a second source after creation
    add_source_to_node(&store, &node.id, "commit:deadbeef").unwrap();

    // Rebuild and validate — all gates including rebuild_determinism should pass
    let count = rebuild_from_events(&store.root).unwrap();
    assert!(count > 0);

    build_derived(&store);
    let results = validate_federation(&store.root).unwrap();
    for (gate, passed, msg) in &results {
        assert!(passed, "Gate {gate} failed: {msg}");
    }
}

#[test]
fn test_init_idempotent() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();

    // First init
    initialize_federation_db(&root).unwrap();

    // Second init should succeed (idempotent)
    initialize_federation_db(&root).unwrap();

    // Store should be usable after double-init
    let store = Store {
        kind: StoreKind::Repo,
        root,
    };
    let node = add_node(
        &store,
        "After re-init",
        "lesson",
        "notable",
        "agent_inferred",
        "",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();
    assert!(node.id.starts_with("F_"));
}

#[test]
fn test_rebuild_determinism_gate_passes() {
    let (_tmp, store) = test_store();

    // Build up some state: nodes, edges, sources, status transitions
    let n1 = add_node(
        &store,
        "Decision A",
        "decision",
        "critical",
        "human_confirmed",
        "Body",
        "file:a.rs",
        "arch",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    let n2 = add_node(
        &store,
        "Lesson B",
        "lesson",
        "notable",
        "agent_inferred",
        "Body 2",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    add_edge(&store, &n1.id, &n2.id, "relates_to").unwrap();
    add_source_to_node(&store, &n2.id, "file:b.rs").unwrap();
    transition_node_status(&store, &n2.id, "deprecated", "node.deprecate", "stale").unwrap();

    // Validate — the rebuild_determinism gate should pass
    build_derived(&store);
    let results = validate_federation(&store.root).unwrap();
    let determinism_gate = results
        .iter()
        .find(|(name, _, _)| name == "federation.rebuild_determinism");
    assert!(
        determinism_gate.is_some(),
        "rebuild_determinism gate should exist"
    );
    let (_, passed, msg) = determinism_gate.unwrap();
    assert!(passed, "rebuild_determinism gate failed: {msg}");
}

#[test]
fn test_derived_artifacts_build_and_validate() {
    let (_tmp, store) = test_store();

    let a = add_node(
        &store,
        "Project A",
        "project",
        "notable",
        "agent_inferred",
        "body",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();
    let b = add_node(
        &store,
        "Lesson B",
        "lesson",
        "notable",
        "agent_inferred",
        "body",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();
    add_edge(&store, &a.id, &b.id, "relates_to").unwrap();

    build_derived(&store);

    let index_path = store.root.join("federation/_index.md");
    let graph_path = store.root.join("federation/_graph.json");
    assert!(index_path.exists());
    assert!(graph_path.exists());
    assert!(
        fs::read_to_string(index_path)
            .unwrap()
            .contains("Federation Vault Index")
    );

    let results = validate_federation(&store.root).unwrap();
    let index_gate = results
        .iter()
        .find(|(name, _, _)| name == "federation.derived_index_fresh")
        .unwrap();
    let graph_gate = results
        .iter()
        .find(|(name, _, _)| name == "federation.derived_graph_fresh")
        .unwrap();
    assert!(index_gate.1, "{}", index_gate.2);
    assert!(graph_gate.1, "{}", graph_gate.2);
}

#[test]
fn test_derived_freshness_detects_drift_after_write() {
    let (_tmp, store) = test_store();

    add_node(
        &store,
        "Before drift",
        "lesson",
        "notable",
        "agent_inferred",
        "body",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    build_derived(&store);

    // Mutation after build should make derived artifacts stale.
    add_node(
        &store,
        "After drift",
        "lesson",
        "notable",
        "agent_inferred",
        "body2",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    let results = validate_federation(&store.root).unwrap();
    let index_gate = results
        .iter()
        .find(|(name, _, _)| name == "federation.derived_index_fresh")
        .unwrap();
    let graph_gate = results
        .iter()
        .find(|(name, _, _)| name == "federation.derived_graph_fresh")
        .unwrap();

    assert!(!index_gate.1, "index freshness should fail after mutation");
    assert!(!graph_gate.1, "graph freshness should fail after mutation");
}

#[test]
fn test_validate_clean_store() {
    let (_tmp, store) = test_store();

    // Empty store should pass all gates
    let results = validate_federation(&store.root).unwrap();
    for (gate, passed, _msg) in &results {
        assert!(passed, "Gate {gate} failed on empty store");
    }
}

#[test]
fn test_find_node_by_source() {
    let (_tmp, store) = test_store();

    // Add node with source - using valid format: event: followed by uppercase alphanumeric
    let _node = add_node(
        &store,
        "Task Intent",
        "commitment",
        "notable",
        "agent_inferred",
        "Test task created",
        "event:R01KHG4QFQ6ZQAN2F3SR6XC5NAZ",
        "todo",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    // Find by exact source
    let found = find_node_by_source(&store, "event:R01KHG4QFQ6ZQAN2F3SR6XC5NAZ").unwrap();
    assert!(found.is_some());
    assert!(found.unwrap().starts_with("F_"));

    // Not found
    let found = find_node_by_source(&store, "event:NONEXISTENT").unwrap();
    assert!(found.is_none());
}

#[test]
fn test_intent_proof_chain() {
    let (_tmp, store) = test_store();

    // Create intent node (task.add event)
    let intent = add_node(
        &store,
        "Task: Fix bug",
        "commitment",
        "notable",
        "agent_inferred",
        "Task created with priority high",
        "event:R01KHG4QFQ6ZQAN2F3SR6XC5NA",
        "bugfix",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    // Create proof node (task.done event) - using lesson type which doesn't require provenance
    let proof = add_node(
        &store,
        "Proof: Task completed",
        "lesson",
        "notable",
        "agent_inferred",
        "Task marked as done",
        "",
        "proof,completion",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    // Link intent to proof
    add_edge(&store, &intent.id, &proof.id, "depends_on").unwrap();

    // Verify the chain exists - just verify both nodes exist
    let found_intent = find_node_by_source(&store, "event:R01KHG4QFQ6ZQAN2F3SR6XC5NA").unwrap();
    assert!(found_intent.is_some());
}

/// Seed a mixed store: some events already double-wrapped (legacy import shape),
/// some canonical. Projection rows start correct (as after a healthy import
/// that wrote nodes alongside envelope payloads).
fn seed_mixed_legacy_store(store: &Store) -> (String, String, String) {
    use decapod::core::db;
    use decapod::core::schemas;
    use rusqlite::params;

    let commitment = add_node(
        store,
        "Task: legacy commitment",
        "commitment",
        "notable",
        "agent_inferred",
        "body commitment",
        "event:code_01legacytask",
        "todo",
        "repo",
        None,
        "decapod",
    )
    .unwrap();
    let lesson = add_node(
        store,
        "Native lesson",
        "lesson",
        "notable",
        "agent_inferred",
        "body lesson",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();
    let edge_id = add_edge(store, &commitment.id, &lesson.id, "depends_on").unwrap();
    let _src_id = add_source_to_node(store, &lesson.id, "file:legacy.rs").unwrap();

    // Rewrite selected event payloads to the historical double-wrapped shape
    // while leaving the live projection intact (the pre-repair failure mode).
    let db_path = store.root.join(schemas::LOCAL_DB_NAME);
    let conn = db::db_connect(&db_path.to_string_lossy()).unwrap();

    let mut stmt = conn
        .prepare(
            "SELECT event_id, event_type, subject_id, payload, actor, ts
             FROM events WHERE stream = 'federation' AND event_type IN ('node.create', 'edge.add', 'source.add')
             ORDER BY seq",
        )
        .unwrap();
    let rows: Vec<(String, String, Option<String>, String, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    // Double-wrap the first node.create and the edge.add only; leave one create
    // and source.add canonical so the store is mixed.
    let mut wrapped = 0;
    for (event_id, event_type, subject_id, payload_raw, actor, ts) in rows {
        if event_type == "source.add" {
            continue;
        }
        if event_type == "node.create" && subject_id.as_deref() == Some(lesson.id.as_str()) {
            continue; // keep lesson canonical
        }
        let inner: serde_json::Value = serde_json::from_str(&payload_raw).unwrap();
        if inner.get("event_type").is_some() {
            continue;
        }
        let envelope = serde_json::json!({
            "actor": actor,
            "event_id": event_id,
            "event_type": event_type,
            "node_id": subject_id,
            "payload": inner,
            "status": "success",
            "ts": ts,
        });
        conn.execute(
            "UPDATE events SET payload = ?1 WHERE event_id = ?2 AND stream = 'federation'",
            params![serde_json::to_string(&envelope).unwrap(), event_id],
        )
        .unwrap();
        wrapped += 1;
    }
    assert!(
        wrapped >= 2,
        "expected to double-wrap commitment create + edge, got {wrapped}"
    );

    (commitment.id, lesson.id, edge_id)
}

#[test]
fn test_legacy_wrapped_rebuild_preserves_fields_sources_and_is_idempotent() {
    use decapod::core::db;
    use decapod::core::schemas;
    use rusqlite::params;

    let (_tmp, store) = test_store();
    let (commitment_id, lesson_id, _edge_id) = seed_mixed_legacy_store(&store);

    // Pre-rebuild projection is healthy.
    let db_path = store.root.join(schemas::LOCAL_DB_NAME);
    let conn = db::db_connect(&db_path.to_string_lossy()).unwrap();
    let node_type: String = conn
        .query_row(
            "SELECT node_type FROM nodes WHERE id = ?1",
            params![commitment_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(node_type, "commitment");
    drop(conn);

    // First rebuild must recover from wrapped payloads without wiping fields.
    let count1 = rebuild_from_events(&store.root).unwrap();
    assert!(count1 > 0);

    let conn = db::db_connect(&db_path.to_string_lossy()).unwrap();
    let (nt, title): (String, String) = conn
        .query_row(
            "SELECT node_type, title FROM nodes WHERE id = ?1",
            params![commitment_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(nt, "commitment");
    assert_eq!(title, "Task: legacy commitment");

    let (nt2, title2): (String, String) = conn
        .query_row(
            "SELECT node_type, title FROM nodes WHERE id = ?1",
            params![lesson_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(nt2, "lesson");
    assert_eq!(title2, "Native lesson");

    // No empty-endpoint edges.
    let bad_edges: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM node_edges
             WHERE trim(COALESCE(source_id,'')) = '' OR trim(COALESCE(target_id,'')) = ''",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(bad_edges, 0);

    // depends_on edge restored with real endpoints.
    let depends: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM node_edges
             WHERE edge_type = 'depends_on' AND source_id = ?1 AND target_id = ?2",
            params![commitment_id, lesson_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(depends, 1);

    // source.add provenance survives.
    let sources: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM node_edges
             WHERE edge_type = 'source' AND source_id = ?1
               AND json_extract(metadata, '$.source') = 'file:legacy.rs'",
            params![lesson_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(sources >= 1, "source.add provenance missing after rebuild");

    // Stored payloads are canonical after rebuild re-insert.
    let wrapped_left: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events
             WHERE stream = 'federation'
               AND json_extract(payload, '$.event_type') IS NOT NULL
               AND json_extract(payload, '$.payload') IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(wrapped_left, 0);
    drop(conn);

    build_derived(&store);
    let results = validate_federation(&store.root).unwrap();
    for (gate, passed, msg) in &results {
        assert!(passed, "Gate {gate} failed after rebuild: {msg}");
    }

    // Second rebuild is a semantic no-op (determinism holds).
    let count2 = rebuild_from_events(&store.root).unwrap();
    assert_eq!(count1, count2);
    build_derived(&store);
    let results2 = validate_federation(&store.root).unwrap();
    for (gate, passed, msg) in &results2 {
        assert!(passed, "Gate {gate} failed after second rebuild: {msg}");
    }
}

#[test]
fn test_rebuild_fails_closed_on_malformed_edge_without_replacing_projection() {
    use decapod::core::db;
    use decapod::core::schemas;
    use rusqlite::params;

    let (_tmp, store) = test_store();
    let node = add_node(
        &store,
        "Keep me",
        "lesson",
        "notable",
        "agent_inferred",
        "body",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    let db_path = store.root.join(schemas::LOCAL_DB_NAME);
    let conn = db::db_connect(&db_path.to_string_lossy()).unwrap();
    // Inject a malformed edge.add that lacks endpoints.
    conn.execute(
        "INSERT INTO events(event_id, ts, seq, stream, subject_kind, subject_id, event_type, payload, actor)
         VALUES(
           'bad-edge-1', '1779199999Z',
           (SELECT COALESCE(MAX(seq),0)+1 FROM events WHERE stream = 'federation'),
           'federation', 'node', ?1, 'edge.add', ?2, 'decapod'
         )",
        params![
            node.id,
            serde_json::json!({"edge_id": "FE_bad", "edge_type": "depends_on"}).to_string()
        ],
    )
    .unwrap();
    let nodes_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
        .unwrap();
    let title_before: String = conn
        .query_row(
            "SELECT title FROM nodes WHERE id = ?1",
            params![node.id],
            |r| r.get(0),
        )
        .unwrap();
    drop(conn);

    let err = rebuild_from_events(&store.root).expect_err("malformed edge must fail rebuild");
    let msg = err.to_string();
    assert!(
        msg.contains("rolled back") || msg.contains("missing required field"),
        "unexpected error: {msg}"
    );

    // Pre-rebuild projection preserved.
    let conn = db::db_connect(&db_path.to_string_lossy()).unwrap();
    let nodes_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
        .unwrap();
    let title_after: String = conn
        .query_row(
            "SELECT title FROM nodes WHERE id = ?1",
            params![node.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(nodes_before, nodes_after);
    assert_eq!(title_before, title_after);
    assert_eq!(title_after, "Keep me");
}

#[test]
fn test_rebuild_fails_closed_on_contradictory_wrapped_payload() {
    use decapod::core::db;
    use decapod::core::schemas;
    use rusqlite::params;

    let (_tmp, store) = test_store();
    let node = add_node(
        &store,
        "Keep me",
        "lesson",
        "notable",
        "agent_inferred",
        "body",
        "",
        "",
        "repo",
        None,
        "decapod",
    )
    .unwrap();

    let db_path = store.root.join(schemas::LOCAL_DB_NAME);
    let conn = db::db_connect(&db_path.to_string_lossy()).unwrap();
    // Contradictory wrapper: row is node.create, envelope claims edge.add.
    let (event_id, actor, ts): (String, String, String) = conn
        .query_row(
            "SELECT event_id, actor, ts FROM events
             WHERE stream = 'federation' AND event_type = 'node.create' AND subject_id = ?1",
            params![node.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    // Contradict only event_type; other duplicated fields match the row.
    let bad = serde_json::json!({
        "event_id": event_id,
        "event_type": "edge.add",
        "node_id": node.id,
        "payload": {"edge_id": "x", "source_id": "a", "target_id": "b", "edge_type": "relates_to"},
        "actor": actor,
        "ts": ts
    });
    conn.execute(
        "UPDATE events SET payload = ?1 WHERE event_id = ?2",
        params![bad.to_string(), event_id],
    )
    .unwrap();
    drop(conn);

    let err = rebuild_from_events(&store.root).expect_err("contradictory envelope must fail");
    assert!(
        err.to_string().contains("LEGACY_EVENT_PAYLOAD") || err.to_string().contains("rolled back"),
        "unexpected: {err}"
    );

    let conn = db::db_connect(&db_path.to_string_lossy()).unwrap();
    let title: String = conn
        .query_row(
            "SELECT title FROM nodes WHERE id = ?1",
            params![node.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(title, "Keep me");
}
