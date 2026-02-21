use decapod::core::rpc::{RpcRequest, RpcResponse};
use std::fs;
use std::path::PathBuf;

#[test]
fn rpc_agent_init_golden_vectors_are_parseable_and_stable() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let req_path = root.join("tests/golden/rpc/v1/agent_init.request.json");
    let res_path = root.join("tests/golden/rpc/v1/agent_init.response.json");

    let req_raw = fs::read_to_string(req_path).expect("read request vector");
    let res_raw = fs::read_to_string(res_path).expect("read response vector");

    let req: RpcRequest = serde_json::from_str(&req_raw).expect("parse request vector");
    let res: RpcResponse = serde_json::from_str(&res_raw).expect("parse response vector");

    assert_eq!(req.op, "agent.init");
    assert_eq!(res.receipt.op, "agent.init");
    assert_eq!(res.id, req.id);
    assert!(
        res.allowed_next_ops
            .iter()
            .any(|op| op.op == "context.resolve")
    );
}
