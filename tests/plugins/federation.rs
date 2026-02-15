use decapod::core::store::{Store, StoreKind};
use decapod::plugins::federation::{
    add_edge, add_node, add_source_to_node, edit_node, initialize_federation_db,
    rebuild_from_events, supersede_node, transition_node_status, validate_federation,
};
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
        "test-agent",
    )
    .unwrap();

    assert!(node.id.starts_with("F_"));
    assert_eq!(node.node_type, "lesson");
    assert_eq!(node.status, "active");
    assert_eq!(node.priority, "notable");
    assert_eq!(node.title, "Test lesson");
    assert_eq!(node.actor, "test-agent");
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
        "test",
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
        "test",
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
        "test",
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
        "test",
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
        "test",
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
        "test",
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
        "test",
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
        "test",
    )
    .unwrap();

    supersede_node(&store, &old.id, &new.id, "Requirements changed").unwrap();

    // Old node should now be superseded (verify via validate)
    let results = validate_federation(&store.root).unwrap();
    for (gate, passed, _msg) in &results {
        assert!(passed, "Gate {} failed", gate);
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
        "test",
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
        "test",
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
        "test",
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
        "test",
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
        "test",
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
        "test",
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
        "test",
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
    let results = validate_federation(&store.root).unwrap();
    for (gate, passed, msg) in &results {
        assert!(passed, "Gate {} failed: {}", gate, msg);
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
        "test",
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
        "test",
    )
    .unwrap();

    // Add a second source after creation
    add_source_to_node(&store, &node.id, "commit:deadbeef").unwrap();

    // Rebuild and validate — all gates including rebuild_determinism should pass
    let count = rebuild_from_events(&store.root).unwrap();
    assert!(count > 0);

    let results = validate_federation(&store.root).unwrap();
    for (gate, passed, msg) in &results {
        assert!(passed, "Gate {} failed: {}", gate, msg);
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
        "test",
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
        "test",
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
        "test",
    )
    .unwrap();

    add_edge(&store, &n1.id, &n2.id, "relates_to").unwrap();
    add_source_to_node(&store, &n2.id, "file:b.rs").unwrap();
    transition_node_status(&store, &n2.id, "deprecated", "node.deprecate", "stale").unwrap();

    // Validate — the rebuild_determinism gate should pass
    let results = validate_federation(&store.root).unwrap();
    let determinism_gate = results
        .iter()
        .find(|(name, _, _)| name == "federation.rebuild_determinism");
    assert!(
        determinism_gate.is_some(),
        "rebuild_determinism gate should exist"
    );
    let (_, passed, msg) = determinism_gate.unwrap();
    assert!(passed, "rebuild_determinism gate failed: {}", msg);
}

#[test]
fn test_validate_clean_store() {
    let (_tmp, store) = test_store();

    // Empty store should pass all gates
    let results = validate_federation(&store.root).unwrap();
    for (gate, passed, _msg) in &results {
        assert!(passed, "Gate {} failed on empty store", gate);
    }
}
