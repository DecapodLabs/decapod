use std::process::{Command, Stdio};
use std::io::Write;
use serde_json::Value;

fn run_rpc(request: Value) -> Value {
    let mut child = Command::new("cargo")
        .args(["run", "--", "rpc", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn cargo run");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    stdin.write_all(serde_json::to_string(&request).unwrap().as_bytes()).expect("Failed to write to stdin");
    drop(stdin);

    let output = child.wait_with_output().expect("Failed to read stdout");
    serde_json::from_slice(&output.stdout).expect("Failed to parse JSON response")
}

#[test]
fn test_rpc_context_resolve_determinism() {
    let request = serde_json::json!({
        "op": "context.resolve",
        "params": {
            "op": "workspace.ensure",
            "touched_paths": ["src/core/rpc.rs"],
            "intent_tags": ["security"],
            "limit": 5
        }
    });

    let res1 = run_rpc(request.clone());
    let res2 = run_rpc(request.clone());

    assert_eq!(res1["result"], res2["result"]);
    assert!(res1["success"].as_bool().unwrap());
    
    let fragments = res1["result"]["fragments"].as_array().unwrap();
    assert!(!fragments.is_empty());
}

#[test]
fn test_rpc_schema_get() {
    let request = serde_json::json!({
        "op": "schema.get",
        "params": {
            "entity": "todo"
        }
    });

    let res = run_rpc(request);
    assert!(res["success"].as_bool().unwrap());
    assert_eq!(res["result"]["schema_version"], "v1");
}

#[test]
fn test_rpc_store_upsert_knowledge() {
    let id = format!("K_TEST_{}", ulid::Ulid::new());
    let request = serde_json::json!({
        "op": "store.upsert",
        "params": {
            "entity": "knowledge",
            "payload": {
                "id": id,
                "title": "RPC Test Knowledge",
                "text": "This is a test entry from RPC",
                "provenance": "cmd:cargo-test"
            }
        }
    });

    let res = run_rpc(request);
    assert!(res["success"].as_bool().unwrap());
    assert_eq!(res["result"]["stored"], true);
    assert_eq!(res["result"]["id"], id);
}

#[test]
fn test_rpc_store_query_knowledge() {
    let id = format!("K_QUERY_TEST_{}", ulid::Ulid::new());
    let title = "Unique Query Test Knowledge";
    
    // Upsert first
    run_rpc(serde_json::json!({
        "op": "store.upsert",
        "params": {
            "entity": "knowledge",
            "payload": {
                "id": id,
                "title": title,
                "text": "This is a test entry for query",
                "provenance": "cmd:cargo-test"
            }
        }
    }));

    let request = serde_json::json!({
        "op": "store.query",
        "params": {
            "entity": "knowledge",
            "query": {
                "text": title
            }
        }
    });

    let res = run_rpc(request);
    assert!(res["success"].as_bool().unwrap());
    let items = res["result"]["items"].as_array().unwrap();
    assert!(!items.is_empty(), "Items should not be empty for query: {}", title);
    
    let found = items.iter().any(|item| item["title"] == title);
    assert!(found, "Should find the upserted item");
}
