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
fn test_rpc_context_bindings() {
    let request = serde_json::json!({
        "op": "context.bindings",
        "params": {}
    });

    let res = run_rpc(request);
    assert!(res["success"].as_bool().unwrap());
    assert!(res["result"]["ops"].get("workspace.ensure").is_some());
}

#[test]
fn test_rpc_trace_and_redaction() {
    let secret_id = format!("SECRET_{}", ulid::Ulid::new());
    let request = serde_json::json!({
        "op": "schema.get",
        "params": {
            "entity": "todo",
            "my_password": "supersecretpassword",
            "id": secret_id
        }
    });

    let _res = run_rpc(request);

    // Export traces to verify
    let mut child = Command::new("cargo")
        .args(["run", "--", "trace", "export", "--last", "1"])
        .env("DECAPOD_SESSION_PASSWORD", "test") // Dummy
        .env("DECAPOD_AGENT_ID", "test") // Dummy
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn cargo run for trace export");

    let output = child.wait_with_output().expect("Failed to read stdout");
    let trace_line = String::from_utf8_lossy(&output.stdout);
    
    assert!(trace_line.contains(&secret_id));
    assert!(trace_line.contains("[REDACTED]"));
    assert!(!trace_line.contains("supersecretpassword"));
}
