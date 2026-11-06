use decapod::core::rpc::{RpcRequest, RpcResponse};
use std::fs;
use std::path::PathBuf;

fn vector(name: &str) -> (RpcRequest, RpcResponse) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let req_raw = fs::read_to_string(root.join(format!("tests/golden/rpc/v1/{name}.request.json")))
        .expect("read request vector");
    let res_raw =
        fs::read_to_string(root.join(format!("tests/golden/rpc/v1/{name}.response.json")))
            .expect("read response vector");
    let req: RpcRequest = serde_json::from_str(&req_raw).expect("parse request vector");
    let res: RpcResponse = serde_json::from_str(&res_raw).expect("parse response vector");
    (req, res)
}

#[test]
fn rpc_agent_init_golden_vectors_are_parseable_and_stable() {
    let (req, res) = vector("agent_init");

    assert_eq!(req.op, "agent.init");
    assert_eq!(res.receipt.op, "agent.init");
    assert_eq!(res.id, req.id);
    assert!(
        res.allowed_next_ops
            .iter()
            .any(|op| op.op == "context.resolve")
    );
}

#[test]
fn interop_manifest_vector_declares_profile_and_unsupported_transports() {
    let (req, res) = vector("interop_manifest");

    assert_eq!(req.op, "interop.manifest");
    assert_eq!(res.receipt.op, "interop.manifest");
    assert_eq!(res.id, req.id);
    assert!(res.success);
    let manifest = &res.result.as_ref().expect("manifest result")["manifest"];
    assert_eq!(manifest["schema_version"], "1.0.0");
    assert_eq!(manifest["profile_id"], "decapod.interop");
    assert_eq!(manifest["profile_version"], "v1");
    assert!(
        manifest["authority_declaration"]
            .as_str()
            .expect("authority declaration")
            .contains("sole authority")
    );
    let capabilities = manifest["capabilities"].as_array().expect("capabilities");
    let rpc_entry = capabilities
        .iter()
        .find(|c| c["id"] == "rpc:agent.init")
        .expect("rpc:agent.init entry");
    assert_eq!(rpc_entry["implementation_status"], "implemented");
    assert_eq!(rpc_entry["evidence"]["state"], "verified");
    assert_eq!(
        rpc_entry["evidence"]["conformance_test"],
        "interop_conformance::discovery_and_negotiation"
    );
    let mcp = capabilities
        .iter()
        .find(|c| c["id"] == "transport:mcp")
        .expect("transport:mcp entry");
    assert_eq!(mcp["implementation_status"], "intentionally_not_used");
    assert!(mcp.get("implementation").is_none());
    // validate=true requested and the reviewed expectation is zero violations.
    assert_eq!(
        res.result.as_ref().unwrap()["validation"],
        serde_json::json!([])
    );
}

#[test]
fn interop_negotiate_ok_vector_is_sorted_and_omits_unknown_optionals() {
    let (req, res) = vector("interop_negotiate_ok");

    assert_eq!(req.op, "interop.negotiate");
    assert_eq!(res.id, req.id);
    assert!(res.success);
    let outcome = &res.result.as_ref().expect("outcome")["outcome"];
    assert_eq!(outcome["negotiated"], true);
    assert_eq!(outcome["profile_version"], "v1");
    // Independent reviewed expectation: required ops are sorted.
    assert_eq!(
        outcome["supported_required_ops"],
        serde_json::json!(["agent.init", "schema.get"])
    );
    // Unsupported optional op and unknown optional extension are reported omitted.
    assert_eq!(
        outcome["omitted_optional_ops"],
        serde_json::json!(["rpc:transport:mcp"])
    );
    assert_eq!(
        outcome["omitted_optional_extensions"],
        serde_json::json!(["zzz-unknown"])
    );
    assert!(
        outcome["authority_note"]
            .as_str()
            .expect("authority note")
            .contains("grants no authority")
    );
}

#[test]
fn interop_negotiate_refused_vector_fails_closed_with_typed_code() {
    let (req, res) = vector("interop_negotiate_refused");

    assert_eq!(req.op, "interop.negotiate");
    assert!(!res.success);
    let error = res.error.as_ref().expect("typed error");
    assert_eq!(error.code, "negotiation_refused");
    assert_eq!(
        error.details.as_ref().expect("details")["reason"],
        "unsupported_required_op"
    );
    assert_eq!(error.details.as_ref().unwrap()["op"], "rpc:transport:mcp");
}

#[test]
fn interop_project_vector_preserves_refusal_semantics() {
    let (req, res) = vector("interop_project");

    assert_eq!(req.op, "interop.project");
    assert!(res.success);
    let projection = &res.result.as_ref().expect("projection")["projection"];
    // A refused source response must project as refused, never success.
    assert_eq!(projection["status"], "refused");
    assert_eq!(projection["op"], "interop.negotiate");
    assert_eq!(
        projection["correlation_id"],
        "01HZINNERREQUEST000000000000000"
    );
    assert_eq!(projection["error"]["code"], "negotiation_refused");
    assert_eq!(projection["source"], "rpc_response");
}

#[test]
fn unknown_op_vector_is_typed_unknown_op() {
    let (req, res) = vector("unknown_op");

    assert_eq!(req.op, "interop.nonexistent");
    assert!(!res.success);
    assert_eq!(res.error.as_ref().expect("error").code, "unknown_op");
    assert_eq!(res.receipt.op, "interop.nonexistent");
}
