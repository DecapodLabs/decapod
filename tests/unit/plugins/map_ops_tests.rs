// Moved from src/decapod/plugins/map_ops.rs
use super::*;
use tempfile::tempdir;

fn test_store() -> (tempfile::TempDir, Store) {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    fs::create_dir_all(&root).unwrap();
    let store = Store {
        kind: crate::core::store::StoreKind::Repo,
        root,
    };
    (tmp, store)
}

#[test]
fn test_map_llm_rejects_empty_items() {
    let (_tmp, store) = test_store();
    let result = map_llm(&store, "[]", "prompt", "{}", "agent");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("must not be empty"));
}

#[test]
fn test_map_agentic_rejects_empty_retain() {
    let (_tmp, store) = test_store();
    let result = map_agentic(&store, "[\"item1\"]", "delegate prompt", "", "agent");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("scope-reduction"));
}

#[test]
fn test_map_agentic_logs_delegation() {
    let (_tmp, store) = test_store();
    let result = map_agentic(
        &store,
        "[\"item1\", \"item2\"]",
        "do the thing",
        "orchestration",
        "agent",
    )
    .unwrap();
    assert_eq!(result["item_count"].as_u64().unwrap(), 2);
    assert_eq!(result["retain"].as_str().unwrap(), "orchestration");

    let events = read_map_events(&store.root).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].op, "map.agentic");
}

#[test]
fn test_map_llm_produces_result() {
    let (_tmp, store) = test_store();
    let result = map_llm(
        &store,
        "[\"a\", \"b\", \"c\"]",
        "summarize: {{item}}",
        "{\"type\": \"object\"}",
        "agent",
    )
    .unwrap();
    assert_eq!(result["item_count"].as_u64().unwrap(), 3);
    assert!(result["result_hash"].as_str().is_some());
}
