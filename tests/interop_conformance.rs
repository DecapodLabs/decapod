//! Black-box conformance suite for the Decapod governance interoperability
//! profile (issues #870/#871).
//!
//! Every scenario spawns the real `decapod` binary (via
//! `env!("CARGO_BIN_EXE_decapod")`) against an isolated temporary git fixture
//! with isolated XDG dirs. Reports are written to
//! `target/interop-conformance/<scenario>.json` (or
//! `$INTEROP_CONFORMANCE_REPORT_DIR`). Reports are diagnostic conformance
//! evidence only — never a Decapod validation receipt and never publication
//! authorization. Any failed case fails the test with a nonzero exit.

use serde_json::{Value, json};
use std::process::Output;

#[path = "support/interop_harness.rs"]
mod harness;

use harness::Harness;

fn isolated_command(root: &std::path::Path, program: &std::ffi::OsStr) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    command.current_dir(root);
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if ["DECAPOD_", "GIT_", "BUILDKITE", "GITHUB", "XDG_"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            command.env_remove(key);
        }
    }
    command
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    command
}

// A separate implementation/process checks the focused JSON interpretation.
// This is same-host differential parsing, not independent executor trust or a
// second implementation of Decapod's custody/publication policy.
fn python_json_reason(root: &std::path::Path, value: &str) -> String {
    use decapod::core::bounded_process::BoundedCommand;
    let output = isolated_command(root, std::ffi::OsStr::new("python3"))
        .args([
            "-I",
            "-S",
            "-B",
            "-c",
            include_str!("fixtures/interop_strict_json.py"),
            value,
        ])
        .bounded_output(std::time::Duration::from_secs(30))
        .unwrap();
    assert!(output.status.success(), "{}", combined(&output));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn portable_encoding_vectors_agree_with_independent_decoder() {
    use decapod::core::bounded_process::BoundedCommand;
    use sha2::{Digest, Sha256};
    use std::{ffi::OsStr, time::Duration};

    let corpus_bytes = include_bytes!("fixtures/interop_json_vectors.v1.json");
    let schema_bytes = include_bytes!("../assets/schemas/interop-json-vectors.schema.json");
    let corpus: Value = serde_json::from_slice(corpus_bytes).unwrap();
    let schema: Value = serde_json::from_slice(schema_bytes).unwrap();
    assert_eq!(corpus["profile"], "decapod.interop.encoding");
    assert_eq!(corpus["profile_version"], "1.0.0");
    assert_eq!(
        corpus["schema_version"],
        schema["properties"]["schema_version"]["const"]
    );
    let vectors = corpus["vectors"].as_array().unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for vector in vectors {
        assert!(ids.insert(vector["id"].as_str().unwrap()));
        let twin = vectors
            .iter()
            .find(|candidate| candidate["id"] == vector["positive_twin"])
            .unwrap();
        assert_eq!(twin["expected"], "accepted");
    }

    let fixture = tempfile::tempdir().unwrap();
    let binary = harness::resolve_decapod_bin();
    let command = |program: &OsStr| isolated_command(fixture.path(), program);
    assert!(
        command(OsStr::new("git"))
            .args(["init", "-q"])
            .bounded_output(Duration::from_secs(30))
            .unwrap()
            .status
            .success()
    );

    // Refusal must precede project/session startup. Do not accept an unrelated
    // startup failure as proof that malformed input was rejected.
    let mut results = Vec::new();
    for vector in vectors
        .iter()
        .filter(|vector| vector["expected"] == "invalid_json")
    {
        let raw = vector["raw"].as_str().unwrap();
        let independent = python_json_reason(fixture.path(), raw);
        assert_eq!(independent, "invalid_json", "{}", vector["id"]);
        let output = command(binary.as_os_str())
            .args(["rpc", "--op", "interop.project", "--params", raw])
            .bounded_output(Duration::from_secs(30))
            .unwrap();
        assert!(!output.status.success(), "{}", combined(&output));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("Invalid JSON parameters"),
            "{}: {}",
            vector["id"],
            combined(&output)
        );
        assert!(
            std::fs::read_dir(fixture.path())
                .unwrap()
                .all(|entry| entry.unwrap().file_name() == ".git"),
            "invalid vector {} initialized project/session state",
            vector["id"]
        );
        results.push(json!({
            "id": vector["id"], "raw_sha256": format!("{:x}", Sha256::digest(raw.as_bytes())),
            "expected": vector["expected"], "independent_reason": independent,
            "kernel_reason": "invalid_json", "kernel_exit": output.status.code(),
            "kernel_stdout": String::from_utf8_lossy(&output.stdout),
            "kernel_stderr": String::from_utf8_lossy(&output.stderr)
        }));
    }

    let initialized = command(binary.as_os_str())
        .args([
            "init",
            "--product-summary",
            "Portable JSON interpretation controls",
        ])
        .bounded_output(Duration::from_secs(60))
        .unwrap();
    assert!(initialized.status.success(), "{}", combined(&initialized));
    for vector in vectors
        .iter()
        .filter(|vector| vector["expected"] == "accepted")
    {
        let raw = vector["raw"].as_str().unwrap();
        let independent = python_json_reason(fixture.path(), raw);
        assert_eq!(independent, "accepted", "{}", vector["id"]);
        let output = command(binary.as_os_str())
            .args(["rpc", "--op", "interop.project", "--params", raw])
            .bounded_output(Duration::from_secs(30))
            .unwrap();
        assert!(
            output.status.success(),
            "{}: {}",
            vector["id"],
            combined(&output)
        );
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["success"], true);
        assert_eq!(response["result"]["canonical_state_verified"], false);
        results.push(json!({
            "id": vector["id"], "raw_sha256": format!("{:x}", Sha256::digest(raw.as_bytes())),
            "expected": vector["expected"], "independent_reason": independent,
            "kernel_reason": "accepted", "kernel_exit": output.status.code(),
            "kernel_stdout": String::from_utf8_lossy(&output.stdout),
            "kernel_stderr": String::from_utf8_lossy(&output.stderr)
        }));
    }
    assert_eq!(
        results.len(),
        vectors.len(),
        "every pinned vector must execute"
    );
    results.sort_by_key(|result| result["id"].as_str().unwrap().to_owned());
    let version = command(binary.as_os_str())
        .args(["system", "version"])
        .bounded_output(Duration::from_secs(30))
        .unwrap();
    assert!(version.status.success());
    let python_identity = command(OsStr::new("python3"))
        .args(["-I", "-S", "-B", "-c",
            "import hashlib,json,pathlib,sys; print(json.dumps({'version':sys.version,'executable_sha256':hashlib.sha256(pathlib.Path(sys.executable).read_bytes()).hexdigest()}))"])
        .bounded_output(Duration::from_secs(30))
        .unwrap();
    assert!(python_identity.status.success());
    let python_identity: Value = serde_json::from_slice(&python_identity.stdout).unwrap();
    let directory = std::env::var_os("INTEROP_CONFORMANCE_REPORT_DIR")
        .or_else(|| std::env::var_os("TEST_UNDECLARED_OUTPUTS_DIR"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/interop-conformance")
        });
    let report = json!({
        "schema": "decapod.interop.encoding-report.v1",
        "profile": corpus["profile"], "profile_version": corpus["profile_version"],
        "scope": "same_host_raw_json_decoding_only", "publication_ready": false,
        "binary_version": String::from_utf8_lossy(&version.stdout).trim(),
        "binary_sha256": format!("{:x}", Sha256::digest(std::fs::read(&binary).unwrap())),
        "independent_identity": python_identity,
        "independent_source_sha256": format!("{:x}", Sha256::digest(include_bytes!("fixtures/interop_strict_json.py"))),
        "vectors_sha256": format!("{:x}", Sha256::digest(corpus_bytes)),
        "vector_schema_sha256": format!("{:x}", Sha256::digest(schema_bytes)),
        "cases": results,
        "unsupported": ["canonical_numeric_hash", "replay_authority", "cross_tenant_authority", "independent_host", "authenticated_conformance_producer"]
    });
    let bytes = serde_json::to_vec_pretty(&report).unwrap();
    let write_report = |directory: &std::path::Path| {
        std::fs::create_dir_all(directory)?;
        std::fs::write(directory.join("portable-encoding-v1.json"), &bytes)
    };
    if write_report(&directory).is_err() {
        // Match the existing harness fallback for read-only Bazel runfiles.
        write_report(&std::env::temp_dir().join("interop-conformance")).unwrap();
    }
}

#[test]
fn unregistered_rpc_is_rejected_before_project_startup() {
    use decapod::core::bounded_process::BoundedCommand;
    use std::time::Duration;

    let fixture = tempfile::tempdir().unwrap();
    let command = |program: &std::ffi::OsStr| isolated_command(fixture.path(), program);
    let initialized = command(std::ffi::OsStr::new("git"))
        .args(["init", "-q"])
        .bounded_output(Duration::from_secs(30))
        .unwrap();
    assert!(initialized.status.success());
    let output = command(harness::resolve_decapod_bin().as_os_str())
        .args(["rpc", "--op", "interop.unregistered", "--params", "{}"])
        .bounded_output(Duration::from_secs(30))
        .unwrap();
    let response: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|_| panic!("expected typed RPC refusal: {}", combined(&output)));
    assert_eq!(response["success"], false);
    assert_eq!(response["error"]["code"], "unknown_op");
    assert!(
        std::fs::read_dir(fixture.path())
            .unwrap()
            .all(|entry| entry.unwrap().file_name() == ".git"),
        "an unknown RPC operation must not initialize a project or session"
    );
}

#[test]
fn malformed_rpc_parameters_cannot_drop_required_negotiation() {
    use decapod::core::bounded_process::BoundedCommand;
    use std::{ffi::OsStr, time::Duration};

    let fixture = tempfile::tempdir().unwrap();
    let binary = harness::resolve_decapod_bin();
    let command = |program: &OsStr| isolated_command(fixture.path(), program);
    let initialized = command(OsStr::new("git"))
        .args(["init", "-q"])
        .bounded_output(Duration::from_secs(30))
        .unwrap();
    assert!(initialized.status.success());
    for params in [
        "{",
        r#"{"required_extensions":["unsupported"],}"#,
        r#"{"required_extensions":[1e400]}"#,
        r#"{"required_extensions":["unsupported"],"required_extensions":[]}"#,
        r#"{"required_extensions":["unsupported"],"required_\u0065xtensions":[]}"#,
        r#"{"optional_metadata":{"grant":"narrow","grant":"wide"}}"#,
    ] {
        assert_eq!(python_json_reason(fixture.path(), params), "invalid_json");
        let output = command(binary.as_os_str())
            .args(["rpc", "--op", "interop.negotiate", "--params", params])
            .bounded_output(Duration::from_secs(30))
            .unwrap();
        assert!(!output.status.success(), "{}", combined(&output));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("Invalid JSON parameters"),
            "{}",
            combined(&output)
        );
        assert!(
            std::fs::read_dir(fixture.path())
                .unwrap()
                .all(|entry| entry.unwrap().file_name() == ".git"),
            "invalid parameters must be refused before project or session startup"
        );
    }

    // Establish a real project for ordinary compatibility discovery. No
    // session, custody, provider or gate-skip override is supplied.
    let initialized = command(binary.as_os_str())
        .args([
            "init",
            "--product-summary",
            "Isolated RPC negotiation controls",
        ])
        .bounded_output(Duration::from_secs(60))
        .unwrap();
    assert!(initialized.status.success(), "{}", combined(&initialized));
    // Replay the original false accept on an initialized project, then prove
    // ordinary negotiation still succeeds after that refusal.
    let malformed = command(binary.as_os_str())
        .args([
            "rpc",
            "--op",
            "interop.negotiate",
            "--params",
            r#"{"required_extensions":["unsupported"],}"#,
        ])
        .bounded_output(Duration::from_secs(30))
        .unwrap();
    assert!(!malformed.status.success(), "{}", combined(&malformed));
    assert!(
        String::from_utf8_lossy(&malformed.stderr).contains("Invalid JSON parameters"),
        "{}",
        combined(&malformed)
    );
    for params in [
        None,
        Some("{}"),
        Some(r#"{"required_extensions":["receipts"]}"#),
    ] {
        let mut invocation = command(binary.as_os_str());
        invocation.args(["rpc", "--op", "interop.negotiate"]);
        if let Some(params) = params {
            invocation.args(["--params", params]);
        }
        let output = invocation.bounded_output(Duration::from_secs(30)).unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["success"], true, "{response}");
        assert_eq!(response["result"]["outcome"]["negotiated"], true);
    }
    let output = command(binary.as_os_str())
        .args([
            "rpc",
            "--op",
            "interop.negotiate",
            "--params",
            r#"{"required_extensions":["unsupported"]}"#,
        ])
        .bounded_output(Duration::from_secs(30))
        .unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["success"], false, "{response}");
    assert_eq!(response["error"]["code"], "negotiation_refused");

    // Both transports must preserve the same meaning, including escaped keys
    // and nested duplicates. Refusal is followed by ordinary operation.
    for params in [
        r#"{"required_extensions":["unsupported"],"required_extensions":[]}"#,
        r#"{"required_extensions":["unsupported"],"required_\u0065xtensions":[]}"#,
        r#"{"optional_metadata":[{"grant":"narrow","grant":"wide"}]}"#,
    ] {
        assert_eq!(python_json_reason(fixture.path(), params), "invalid_json");
        for stdin in [false, true] {
            let mut invocation = command(binary.as_os_str());
            if stdin {
                use std::io::Write;
                invocation.args(["rpc", "--stdin"]);
                invocation.stdin(std::process::Stdio::piped());
                let mut child = invocation
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .unwrap();
                let request = format!(r#"{{"op":"interop.negotiate","params":{params}}}"#);
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(request.as_bytes())
                    .unwrap();
                let output = harness::wait_bounded(child);
                assert!(!output.status.success(), "{}", combined(&output));
                assert!(
                    String::from_utf8_lossy(&output.stderr).contains("duplicate JSON key"),
                    "{}",
                    combined(&output)
                );
            } else {
                let output = invocation
                    .args(["rpc", "--op", "interop.negotiate", "--params", params])
                    .bounded_output(Duration::from_secs(30))
                    .unwrap();
                assert!(!output.status.success(), "{}", combined(&output));
                assert!(
                    String::from_utf8_lossy(&output.stderr).contains("duplicate JSON key"),
                    "{}",
                    combined(&output)
                );
            }
        }
    }
    // Legal JSON is still subject to the existing strict negotiation schema.
    let unknown_field = r#"{"required_extensions":["receipts"],"optional_metadata":{"label":"é","ratio":1.25,"count":4,"enabled":true,"absent":null}}"#;
    assert_eq!(
        python_json_reason(fixture.path(), unknown_field),
        "accepted"
    );
    let output = command(binary.as_os_str())
        .args([
            "rpc",
            "--op",
            "interop.negotiate",
            "--params",
            unknown_field,
        ])
        .bounded_output(Duration::from_secs(30))
        .unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["success"], false, "{response}");
    assert_eq!(response["error"]["code"], "invalid_params");

    let ordinary = r#"{"required_extensions":["receipts"],"optional_ops":["interop.\u03bb"],"optional_extensions":["future"]}"#;
    assert_eq!(python_json_reason(fixture.path(), ordinary), "accepted");
    let output = command(binary.as_os_str())
        .args(["rpc", "--op", "interop.negotiate", "--params", ordinary])
        .bounded_output(Duration::from_secs(30))
        .unwrap();
    assert!(output.status.success(), "{}", combined(&output));
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["success"], true, "{response}");
    assert_eq!(response["result"]["outcome"]["negotiated"], true);
    assert_eq!(
        response["result"]["outcome"]["omitted_optional_ops"],
        json!(["interop.λ"])
    );
    assert_eq!(
        response["result"]["outcome"]["omitted_optional_extensions"],
        json!(["future"])
    );
}

#[test]
fn caller_supplied_reports_cannot_authenticate_current_proof() {
    use decapod::core::bounded_process::BoundedCommand;
    use std::{ffi::OsStr, time::Duration};

    let fixture = tempfile::tempdir().unwrap();
    let binary = harness::resolve_decapod_bin();
    let command = |program: &OsStr| isolated_command(fixture.path(), program);
    assert!(
        command(OsStr::new("git"))
            .args(["init", "-q"])
            .bounded_output(Duration::from_secs(30))
            .unwrap()
            .status
            .success()
    );
    let initialized = command(binary.as_os_str())
        .args([
            "init",
            "--product-summary",
            "Isolated report provenance controls",
        ])
        .bounded_output(Duration::from_secs(60))
        .unwrap();
    assert!(initialized.status.success(), "{}", combined(&initialized));

    for (judgment, report) in [
        (
            "not_proven",
            json!({
                "status": "pass", "head": "a".repeat(40),
                "publication_ready": true, "receipt_hash": "sha256:0123456789abcdef"
            }),
        ),
        (
            "not_proven",
            json!({
                "status": "pass", "head": "a".repeat(40),
                "receipt_hash": format!("sha256:{}", "b".repeat(64)),
                "rules_fingerprint": "caller-rule", "current_rules_fingerprint": "caller-rule",
                "verifier": "caller-tool", "current_verifier": "caller-tool"
            }),
        ),
        ("failed", json!({"status": "fail", "head": "a".repeat(40)})),
        ("stale", json!({"status": "stale", "head": "a".repeat(40)})),
    ] {
        let params = json!({"kind": "validation_report", "report": report}).to_string();
        let output = command(binary.as_os_str())
            .args(["rpc", "--op", "interop.project", "--params", &params])
            .bounded_output(Duration::from_secs(30))
            .unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["success"], true, "{response}");
        assert_eq!(response["result"]["input_origin"], "caller_supplied");
        assert_eq!(response["result"]["canonical_state_verified"], false);
        let projection = &response["result"]["projection"];
        assert_eq!(projection["proof_judgment"], judgment, "{response}");
        assert_eq!(projection["publication_ready"], false, "{response}");
        assert!(
            projection["proof_reason"]
                .as_str()
                .unwrap()
                .contains("Caller-supplied report"),
            "{response}"
        );
    }
    for (approved, status) in [
        (json!(true), "approved"),
        (json!(false), "denied"),
        (Value::Null, "pending"),
    ] {
        let params = json!({
            "kind": "approval", "action": "workspace.publish", "scope": "repo",
            "approved": approved,
            "input_origin": "independent_executor", "canonical_state_verified": true
        })
        .to_string();
        let output = command(binary.as_os_str())
            .args(["rpc", "--op", "interop.project", "--params", &params])
            .bounded_output(Duration::from_secs(30))
            .unwrap();
        assert!(output.status.success(), "{}", combined(&output));
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["success"], true, "{response}");
        assert_eq!(response["result"]["input_origin"], "caller_supplied");
        assert_eq!(response["result"]["canonical_state_verified"], false);
        let projection = &response["result"]["projection"];
        assert_eq!(projection["status"], status);
        if status != "pending" {
            assert!(
                projection["remediation"]
                    .as_str()
                    .unwrap()
                    .contains("does not authenticate")
            );
        }
    }
    // Unproven display data does not inhibit ordinary compatible operation.
    let output = command(binary.as_os_str())
        .args([
            "rpc",
            "--op",
            "interop.negotiate",
            "--params",
            r#"{"required_extensions":["receipts"]}"#,
        ])
        .bounded_output(Duration::from_secs(30))
        .unwrap();
    assert!(output.status.success(), "{}", combined(&output));
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["success"], true, "{response}");
    assert_eq!(response["result"]["outcome"]["negotiated"], true);
}

/// Read-only ops whose dispatch acceptance is proven against the real binary
/// (an advertised op that dispatch rejects as `unknown_op` fails the suite).
fn read_only_dispatch_probes() -> Vec<(&'static str, Value)> {
    vec![
        ("agent.init", json!({})),
        ("workspace.status", json!({})),
        (
            "context.resolve",
            json!({"intent": "interop discovery probe"}),
        ),
        (
            "context.scope",
            json!({"intent": "interop discovery probe"}),
        ),
        (
            "context.capsule.query",
            json!({"topic": "interop", "scope": "core"}),
        ),
        ("context.bindings", json!({})),
        ("constitution.get", json!({"section": "core/DECAPOD"})),
        ("schema.get", json!({"entity": "todo"})),
        ("intent.reconcile", json!({})),
        ("store.query", json!({"entity": "todo"})),
        ("standards.resolve", json!({})),
        ("mentor.obligations", json!({})),
        ("interop.manifest", json!({})),
        ("interop.negotiate", json!({})),
        (
            "interop.project",
            json!({"kind": "interlock_status", "status": "UNKNOWN"}),
        ),
    ]
}

fn combined(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn knowledge_items(h: &Harness, text: &str) -> Vec<Value> {
    let res = h.rpc(
        "store.query",
        json!({"entity": "knowledge", "query": {"text": text}}),
    );
    assert_eq!(res["success"], true, "store.query failed: {res}");
    res["result"]["items"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn todo_items(h: &Harness) -> Vec<Value> {
    let res = h.rpc("store.query", json!({"entity": "todo"}));
    assert_eq!(res["success"], true, "store.query todo failed: {res}");
    res["result"]["items"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn add_todo_cli(h: &Harness, title: &str) -> String {
    let out = h.raw(&["todo", "add", title], &[], true);
    let text = combined(&out);
    let parsed: Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("todo add JSON output");
    let id = parsed["id"].as_str().expect("todo id").to_string();
    assert!(
        text.contains("\"ok\"") || out.status.success(),
        "todo add failed: {text}"
    );
    id
}

fn parse_session_password(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("Password: ").map(|v| v.trim().to_string()))
        .expect("session acquire must print Password: <pw>")
}

fn parse_fingerprint(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| {
            l.strip_prefix("Fingerprint: ")
                .map(|v| v.trim().to_string())
        })
        .expect("policy eval must print Fingerprint: <hex>")
}

fn parse_approval_id(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| {
            l.strip_prefix("Action Approved (ID: ")
                .and_then(|rest| rest.strip_suffix(")"))
                .map(|v| v.trim().to_string())
        })
        .expect("policy approve must print Action Approved (ID: <ulid>)")
}

//# 1 — Discovery and negotiation
#[test]
fn discovery_and_negotiation() {
    let mut h = Harness::new("discovery_and_negotiation");

    // Manifest discovery through the real read-only RPC boundary.
    let res = h.rpc("interop.manifest", json!({"validate": true}));
    let ok = res["success"] == true;
    h.check(
        "manifest.discovery",
        "rpc-stdin:interop.manifest",
        "success with machine-readable manifest",
        format!("success={}", res["success"]),
        ok,
        None,
        vec![format!("envelope={}", &res["id"])],
    );
    let manifest = &res["result"]["manifest"];
    h.check(
        "manifest.schema_version",
        "rpc-stdin:interop.manifest",
        "1.0.0",
        manifest["schema_version"].as_str().unwrap_or("?"),
        manifest["schema_version"] == "1.0.0",
        None,
        vec![],
    );
    h.check(
        "manifest.self_validation_clean",
        "rpc-stdin:interop.manifest(validate=true)",
        "validation: []",
        serde_json::to_string(&res["result"]["validation"]).unwrap(),
        res["result"]["validation"] == json!([]),
        None,
        vec![],
    );
    let caps = manifest["capabilities"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    h.check(
        "manifest.session_requirements",
        "rpc-stdin:interop.manifest",
        "bundle export/import, mentor and assurance advertise their actual session gate",
        serde_json::to_string(&caps).unwrap(),
        [
            "context.bundle.export", "context.bundle.import",
            "mentor.obligations", "assurance.evaluate",
        ].iter().all(|op| caps.iter().any(|cap| {
            cap["id"] == format!("rpc:{op}")
                && cap["implementation"]["session_required"] == true
        })),
        None,
        vec!["session gates remain authoritative; automatic acquisition is not session-free execution".into()],
    );
    h.check(
        "manifest.current_dispatch_entries",
        "rpc-stdin:interop.manifest",
        "intent.reconcile, state.resolve and state.diff are implemented and advertised",
        serde_json::to_string(&caps.iter().map(|cap| &cap["id"]).collect::<Vec<_>>()).unwrap(),
        ["intent.reconcile", "state.resolve", "state.diff"]
            .iter()
            .all(|op| {
                caps.iter().any(|cap| {
                    cap["id"] == format!("rpc:{op}")
                        && cap["implementation_status"] == "implemented"
                })
            }),
        None,
        vec!["registered names and manifest metadata share the existing catalog".into()],
    );
    h.check(
        "manifest.source_coverage_is_not_execution",
        "rpc-stdin:interop.manifest",
        "all capabilities not_run; source coverage locations retained",
        serde_json::to_string(
            &caps
                .iter()
                .map(|cap| (&cap["id"], &cap["evidence"]))
                .collect::<Vec<_>>(),
        )
        .unwrap(),
        caps.iter().all(|cap| cap["evidence"]["state"] == "not_run")
            && caps.iter().any(|cap| {
                cap["evidence"]["conformance_test"].is_string()
                    && cap["evidence"]["evidence_ref"].is_string()
            }),
        None,
        vec!["test registration describes source coverage, not an observed execution".into()],
    );
    let mcp = caps.iter().find(|c| c["id"] == "transport:mcp");
    let a2a = caps.iter().find(|c| c["id"] == "transport:a2a");
    let acp = caps.iter().find(|c| c["id"] == "transport:acp");
    h.check(
        "manifest.bounded_transports_truthful",
        "rpc-stdin:interop.manifest",
        "transport:mcp and transport:a2a implemented (evidence=not_run); transport:acp intentionally_not_used",
        format!(
            "mcp={} a2a={} acp={}",
            serde_json::to_string(&mcp).unwrap_or_else(|_| "missing".to_string()),
            serde_json::to_string(&a2a).unwrap_or_else(|_| "missing".to_string()),
            serde_json::to_string(&acp).unwrap_or_else(|_| "missing".to_string()),
        ),
        mcp.is_some_and(|c| {
            c["implementation_status"] == "implemented" && c["evidence"]["state"] == "not_run"
        }) && a2a.is_some_and(|c| {
            c["implementation_status"] == "implemented" && c["evidence"]["state"] == "not_run"
        }) && acp.is_some_and(|c| c["implementation_status"] == "intentionally_not_used"),
        None,
        vec![],
    );
    h.check(
        "manifest.authority_declaration",
        "rpc-stdin:interop.manifest",
        "manifest declares Decapod state as sole authority",
        manifest["authority_declaration"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(80)
            .collect::<String>(),
        manifest["authority_declaration"]
            .as_str()
            .is_some_and(|s| s.contains("sole authority")),
        None,
        vec![],
    );

    // Every advertised read-only op is actually accepted by the real dispatcher.
    let probes = read_only_dispatch_probes();
    let mut all_accepted = true;
    let mut rejected = Vec::new();
    for (op, params) in &probes {
        let (res, out) = h.rpc_raw(json!({"op": op, "params": params}));
        let unknown = res["success"] == false && res["error"]["code"] == "unknown_op";
        if unknown || (out.status.success() && res.is_null()) {
            all_accepted = false;
            rejected.push(format!(
                "{op}: {}",
                combined(&out).chars().take(120).collect::<String>()
            ));
        }
    }
    h.check(
        "manifest.ops_dispatch_accepted",
        "rpc-stdin:<read-only probes>",
        "no advertised op is rejected as unknown_op by the real dispatcher",
        if rejected.is_empty() {
            "all dispatch-accepted".to_string()
        } else {
            rejected.join("; ")
        },
        all_accepted,
        if all_accepted {
            None
        } else {
            Some("advertised op not dispatchable".into())
        },
        vec![format!("probed {} ops", probes.len())],
    );

    // Successful negotiation: deterministic, sorted, order-invariant.
    let ok1 = h.rpc(
        "interop.negotiate",
        json!({
            "required_ops": ["schema.get", "agent.init"],
            "optional_ops": ["rpc:transport:mcp"],
            "optional_extensions": ["zzz-unknown", "receipts"],
        }),
    );
    let ok2 = h.rpc(
        "interop.negotiate",
        json!({
            "required_ops": ["agent.init", "schema.get"],
            "optional_extensions": ["receipts", "zzz-unknown"],
            "optional_ops": ["rpc:transport:mcp"],
        }),
    );
    let o1 = &ok1["result"]["outcome"];
    let o2 = &ok2["result"]["outcome"];
    h.check(
        "negotiate.supported_required_ops",
        "rpc-stdin:interop.negotiate",
        "negotiated=true with sorted supported_required_ops=[agent.init,schema.get]",
        serde_json::to_string(o1).unwrap_or_default(),
        ok1["success"] == true
            && o1["negotiated"] == true
            && o1["supported_required_ops"] == json!(["agent.init", "schema.get"]),
        None,
        vec![],
    );
    h.check(
        "negotiate.order_invariant",
        "rpc-stdin:interop.negotiate",
        "reordering independent optional inputs yields the identical outcome",
        format!("o1==o2: {}", o1 == o2),
        o1 == o2,
        None,
        vec![],
    );
    h.check(
        "negotiate.omits_unknown_optional_safely",
        "rpc-stdin:interop.negotiate",
        "unknown optional extension and unsupported optional op reported omitted",
        serde_json::to_string(o1).unwrap_or_default(),
        o1["omitted_optional_extensions"] == json!(["zzz-unknown"])
            && o1["omitted_optional_ops"] == json!(["rpc:transport:mcp"]),
        None,
        vec![],
    );

    // Fail-closed refusals.
    let refused = h.rpc(
        "interop.negotiate",
        json!({"required_ops": ["rpc:transport:mcp"]}),
    );
    h.check(
        "negotiate.refuses_unsupported_mandatory",
        "rpc-stdin:interop.negotiate",
        "success=false code=negotiation_refused for mandatory rpc:transport:mcp",
        format!(
            "success={} code={}",
            refused["success"], refused["error"]["code"]
        ),
        refused["success"] == false && refused["error"]["code"] == "negotiation_refused",
        None,
        vec![],
    );
    let bad_version = h.rpc("interop.negotiate", json!({"profile_version": "v99"}));
    h.check(
        "negotiate.refuses_incompatible_version",
        "rpc-stdin:interop.negotiate",
        "success=false code=negotiation_refused for profile_version v99",
        format!(
            "success={} code={}",
            bad_version["success"], bad_version["error"]["code"]
        ),
        bad_version["success"] == false && bad_version["error"]["code"] == "negotiation_refused",
        None,
        vec![],
    );
    let profile = h.rpc(
        "interop.negotiate",
        json!({"requested_profile": "read-only-orientation"}),
    );
    h.check(
        "negotiate.profile_read_only_orientation",
        "rpc-stdin:interop.negotiate",
        "profile read-only-orientation negotiated=true, status implemented",
        serde_json::to_string(&profile["result"]["outcome"]).unwrap_or_default(),
        profile["success"] == true
            && profile["result"]["outcome"]["negotiated"] == true
            && profile["result"]["outcome"]["profile_status"] == "implemented",
        None,
        vec![],
    );
    let fleet = h.rpc(
        "interop.negotiate",
        json!({"requested_profile": "fleet-observation"}),
    );
    h.check(
        "negotiate.fleet_observation_partial",
        "rpc-stdin:interop.negotiate",
        "fleet-observation reported partial with missing fleet.observe, not negotiated",
        serde_json::to_string(&fleet["result"]["outcome"]).unwrap_or_default(),
        fleet["result"]["outcome"]["profile_status"] == "partial"
            && fleet["result"]["outcome"]["negotiated"] == false
            && fleet["result"]["outcome"]["missing_profile_ops"] == json!(["fleet.observe"]),
        None,
        vec![],
    );

    // Negotiation is read-only: canonical governed state is unchanged.
    let before = todo_items(&h);
    let _ = h.rpc(
        "interop.negotiate",
        json!({"required_ops": ["store.upsert"]}),
    );
    let after = todo_items(&h);
    h.check(
        "negotiate.does_not_mutate_state",
        "rpc-stdin:interop.negotiate + store.query",
        "todo store identical before/after negotiation",
        format!("before={} after={}", before.len(), after.len()),
        before == after,
        None,
        vec![],
    );

    h.finish();
}

// # 2 — Authorized governed mutation with state read-back
#[test]
fn authorized_mutation() {
    let mut h = Harness::new("authorized_mutation");

    let kid = format!(
        "K_INTEROP_{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let payload = json!({
        "id": kid,
        "title": "Interop Authorized Mutation Probe",
        "text": "interop authorized mutation probe body",
        "provenance": "cmd:interop-conformance-authorized",
    });
    let upsert = h.rpc(
        "store.upsert",
        json!({"entity": "knowledge", "payload": payload}),
    );
    h.check(
        "mutation.upsert_success",
        "rpc-stdin:store.upsert",
        "success=true, result.stored=true, receipt.op=store.upsert",
        format!(
            "success={} stored={} receipt.op={}",
            upsert["success"], upsert["result"]["stored"], upsert["receipt"]["op"]
        ),
        upsert["success"] == true
            && upsert["result"]["stored"] == true
            && upsert["result"]["id"] == kid
            && upsert["receipt"]["op"] == "store.upsert",
        None,
        vec![format!(
            "receipt.inputs_hash={}",
            upsert["receipt"]["inputs_hash"]
        )],
    );

    // Read back canonical governed state through a public interface.
    let items = knowledge_items(&h, "interop authorized mutation probe");
    let found = items.iter().any(|i| i["id"] == kid);
    h.check(
        "mutation.state_read_back",
        "rpc-stdin:store.query",
        "upserted knowledge entity is present in canonical store",
        format!("items={} found={}", items.len(), found),
        found,
        Some("stored state does not match receipt".into()).filter(|_| !found),
        vec![format!("id={kid}")],
    );

    // CLI mutation + read-back through RPC (cross-surface state agreement).
    let tid = add_todo_cli(&h, "interop authorized mutation todo");
    let todos = todo_items(&h);
    let todo_found = todos.iter().any(|t| t["id"] == tid);
    let get = h.raw(
        &["todo", "get", "--id", &tid, "--format", "json"],
        &[],
        true,
    );
    let get_json: Value = serde_json::from_slice(&get.stdout).unwrap_or(Value::Null);
    h.check(
        "mutation.cli_todo_visible_via_rpc",
        "cli:todo.add + rpc-stdin:store.query + cli:todo.get",
        "CLI-created todo visible in canonical store and via CLI get",
        format!(
            "rpc_found={} cli_item_id={} cli_item_status={}",
            todo_found, get_json["item"]["id"], get_json["item"]["status"]
        ),
        todo_found && get_json["item"]["id"] == tid,
        None,
        vec![format!("todo_id={tid}")],
    );

    // Projection of the real mutation response preserves success semantics.
    let proj = h.rpc(
        "interop.project",
        json!({"kind": "rpc_response", "response": upsert}),
    );
    let p = &proj["result"]["projection"];
    h.check(
        "projection.of_authorized_mutation",
        "rpc-stdin:interop.project",
        "reported status=succeeded with correlation id preserved, caller input not canonically verified",
        serde_json::to_string(p).unwrap_or_default(),
        proj["success"] == true
            && proj["result"]["input_origin"] == "caller_supplied"
            && proj["result"]["canonical_state_verified"] == false
            && p["status"] == "succeeded"
            && p["op"] == "store.upsert"
            && p["correlation_id"] == upsert["id"]
            && p["receipt"]["inputs_hash"] == upsert["receipt"]["inputs_hash"],
        None,
        vec![],
    );

    h.finish();
}

// # 3 — Denied mutation leaves canonical state unchanged
#[test]
fn denied_mutation() {
    let mut h = Harness::new("denied_mutation");

    // Baseline canonical state: one todo created via the authenticated CLI,
    // plus the fixture's git HEAD as the worktree immutability marker.
    let tid = add_todo_cli(&h, "interop denied mutation probe");
    let head_before = git_head(&h);

    // Note on sessions: the CLI entrypoint funnel auto-acquires a session when
    // no valid password is supplied, so "no password" is not a denial mode.
    // Denials come from the trust/policy gates and protected-branch
    // interlocks — those are what this scenario exercises.

    // 1. High-risk mutation attempted by a basic-trust agent is denied by the
    //    trust gate (task.archive requires verified trust).
    let intruder_acquire = h.raw_as("intruder", None, &["session", "acquire"], &[]);
    let intruder_pw = parse_session_password(&intruder_acquire);
    let denied = h.raw_as(
        "intruder",
        Some(&intruder_pw),
        &["todo", "archive", "--id", &tid],
        &[],
    );
    let denied_text = combined(&denied);
    let denied_ok =
        !denied.status.success() && denied_text.contains("Policy gate denied for 'task.archive'");
    h.check(
        "denied.trust_gate_blocks_high_risk_mutation",
        "cli:todo.archive as basic-trust agent 'intruder'",
        "nonzero exit, typed 'Policy gate denied for task.archive' (trust < required)",
        denied_text.chars().take(200).collect::<String>(),
        denied_ok,
        Some("high-risk mutation was not denied".into()).filter(|_| !denied_ok),
        vec![format!("todo_id={tid}")],
    );

    // 2. A mutating RPC op (workspace.publish) is refused for the active
    //    session: the constitutional-awareness gate is bound to the session
    //    token, and the `unknown` entrypoint funnel rotates the session every
    //    process, so the mutation fails closed with a typed refusal and no
    //    success envelope.
    let (pub_resp, pub_out) = h.rpc_raw(json!({"op": "workspace.publish", "params": {}}));
    let pub_text = combined(&pub_out);
    let pub_refused = pub_resp["success"] != true && pub_text.contains("Constitutional awareness");
    h.check(
        "denied.mutating_rpc_refused",
        "rpc-stdin:workspace.publish",
        "no success envelope; typed constitutional-awareness refusal",
        pub_text.chars().take(200).collect::<String>(),
        pub_refused,
        Some("mutating op produced a success envelope".into()).filter(|_| !pub_refused),
        vec![format!("head_before={head_before}")],
    );

    // 3. Successful negotiation of a mutating op grants no authority: the
    //    same publish is still refused afterwards.
    let neg = h.rpc(
        "interop.negotiate",
        json!({"required_ops": ["workspace.publish"]}),
    );
    let (pub2_resp, _pub2_out) = h.rpc_raw(json!({"op": "workspace.publish", "params": {}}));
    h.check(
        "denied.negotiation_grants_no_authority",
        "rpc-stdin:interop.negotiate + rpc-stdin:workspace.publish",
        "negotiated=true yet the publish is still refused",
        format!(
            "negotiated={} publish_success={}",
            neg["result"]["outcome"]["negotiated"], pub2_resp["success"]
        ),
        neg["result"]["outcome"]["negotiated"] == true && pub2_resp["success"] != true,
        None,
        vec![
            neg["result"]["outcome"]["authority_note"]
                .as_str()
                .unwrap_or("")
                .to_string(),
        ],
    );

    // 4. Canonical state is unchanged by every denied attempt: the todo is
    //    still open and the git HEAD has not moved.
    let get = h.raw(
        &["todo", "get", "--id", &tid, "--format", "json"],
        &[],
        true,
    );
    let get_json: Value = serde_json::from_slice(&get.stdout).unwrap_or(Value::Null);
    let head_after = git_head(&h);
    let todos = todo_items(&h);
    let todo_state_ok = get_json["item"]["status"] == "open"
        && head_after == head_before
        && todos
            .iter()
            .any(|t| t["id"] == tid && t["status"] == "open");
    h.check(
        "denied.state_unchanged",
        "cli:todo.get + rpc-stdin:store.query + git rev-parse HEAD",
        "task still open, todo store intact, git HEAD unmoved",
        format!(
            "item.status={} head_before={} head_after={} todos={}",
            get_json["item"]["status"],
            head_before,
            head_after,
            todos.len()
        ),
        todo_state_ok,
        None,
        vec![format!("todo_id={tid}")],
    );

    h.finish();
}

/// Current git HEAD of the fixture worktree (worktree immutability marker).
fn git_head(h: &Harness) -> String {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&h.root)
        .output()
        .expect("git rev-parse");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// # 4 — Approval roundtrip through the real policy path
//
// Runs as the built-in operator identity (core trust) so the trust gate
// passes and the *approval* gate is what actually blocks — proving approval
// is enforced independently of trust.
#[test]
fn approval_roundtrip() {
    let mut h = Harness::new("approval_roundtrip");

    let tid = add_todo_cli(&h, "interop approval roundtrip probe");

    // Operator identity acquires its own session through the real CLI.
    let op_acquire = h.raw_as("operator", None, &["session", "acquire"], &[]);
    let op_pw = parse_session_password(&op_acquire);

    // High-risk mutation is denied before approval; the denial names the real
    // policy path.
    let denied1 = h.raw_as("operator", Some(&op_pw), &["todo", "archive", &tid], &[]);
    let denied_text = combined(&denied1);
    // The broker policy gate denies with a typed message naming the op and the
    // approval requirement (message text differs by enforcement path).
    let denied_ok = !denied1.status.success()
        && denied_text.contains("Policy gate denied for 'task.archive'")
        && (denied_text.contains("requires human approval")
            || denied_text.contains("lacks approval"));
    h.check(
        "approval.initially_denied",
        "cli:todo.archive (agent=operator)",
        "nonzero exit, typed policy-gate denial naming task.archive and approval",
        denied_text.chars().take(200).collect::<String>(),
        denied_ok,
        None,
        vec![format!("todo_id={tid}")],
    );

    // Obtain the real approval fingerprint from the policy evaluator.
    let eval = h.raw_as(
        "operator",
        Some(&op_pw),
        &[
            "govern",
            "policy",
            "eval",
            "--command",
            "task.archive",
            "--path",
            &tid,
        ],
        &[],
    );
    let fp = parse_fingerprint(&eval);
    h.check(
        "approval.fingerprint_from_policy_eval",
        "cli:govern policy eval",
        "64-hex fingerprint printed",
        fp.clone(),
        fp.len() == 64 && fp.bytes().all(|b| b.is_ascii_hexdigit()),
        None,
        vec![],
    );

    // Still denied before the approval is granted.
    let denied2 = h.raw_as("operator", Some(&op_pw), &["todo", "archive", &tid], &[]);
    h.check(
        "approval.denied_before_grant",
        "cli:todo.archive (pre-approval retry)",
        "still denied",
        combined(&denied2).chars().take(120).collect::<String>(),
        !denied2.status.success(),
        None,
        vec![],
    );

    // Grant through the legitimate authorization path, scoped globally.
    let approve = h.raw_as(
        "operator",
        Some(&op_pw),
        &[
            "govern", "policy", "approve", "--id", &fp, "--scope", "global",
        ],
        &[],
    );
    h.check(
        "approval.grant_via_policy_cli",
        "cli:govern policy approve",
        "approval granted for the exact fingerprint",
        combined(&approve).chars().take(120).collect::<String>(),
        approve.status.success(),
        None,
        vec![format!("fingerprint={fp}")],
    );
    // `policy revoke` addresses the approval record ID, not the fingerprint.
    let approval_id = parse_approval_id(&approve);

    // Now the same operation proceeds.
    let allowed = h.raw_as("operator", Some(&op_pw), &["todo", "archive", &tid], &[]);
    h.check(
        "approval.proceeds_after_grant",
        "cli:todo.archive (post-approval)",
        "mutation succeeds after scoped approval",
        combined(&allowed).chars().take(120).collect::<String>(),
        allowed.status.success(),
        None,
        vec![],
    );

    // Read back canonical state through a public interface.
    let get = h.raw(
        &["todo", "get", "--id", &tid, "--format", "json"],
        &[],
        true,
    );
    let get_json: Value = serde_json::from_slice(&get.stdout).unwrap_or(Value::Null);
    h.check(
        "approval.state_read_back",
        "cli:todo.get",
        "task item.status is archived in canonical state",
        format!(
            "status={} item_status={}",
            get_json["status"], get_json["item"]["status"]
        ),
        get_json["item"]["status"] == "archived",
        None,
        vec![format!("todo_id={tid}")],
    );

    // The approval is scoped: a different target id is not covered.
    let tid2 = add_todo_cli(&h, "interop approval scope probe");
    let denied3 = h.raw_as("operator", Some(&op_pw), &["todo", "archive", &tid2], &[]);
    h.check(
        "approval.scope_limited",
        "cli:todo.archive (different todo id)",
        "denied: approval fingerprint does not cover another target",
        combined(&denied3).chars().take(160).collect::<String>(),
        !denied3.status.success(),
        None,
        vec![format!("other_todo_id={tid2}")],
    );

    // Revoke re-blocks the exact operation (approval check runs before any
    // status check, so the retry is denied again).
    let revoke = h.raw_as(
        "operator",
        Some(&op_pw),
        &["govern", "policy", "revoke", "--id", &approval_id],
        &[],
    );
    let denied4 = h.raw_as("operator", Some(&op_pw), &["todo", "archive", &tid], &[]);
    h.check(
        "approval.revoked_reblocks",
        "cli:govern policy revoke + cli:todo.archive retry",
        "revocation succeeds and the same op is denied again",
        format!(
            "revoke_exit={} retry_denied={}",
            revoke.status,
            !denied4.status.success()
        ),
        revoke.status.success() && !denied4.status.success(),
        None,
        vec![
            format!("fingerprint={fp}"),
            format!("approval_id={approval_id}"),
        ],
    );

    // Projection of the real decision: denied stays denied.
    let proj = h.rpc(
        "interop.project",
        json!({
            "kind": "approval",
            "action": "task.archive",
            "correlation_id": tid,
            "risk_level": "high",
            "scope": "global",
            "approved": false,
            "reason": "lacks approval",
        }),
    );
    let p = &proj["result"]["projection"];
    h.check(
        "projection.denied_approval_stays_denied",
        "rpc-stdin:interop.project",
        "approval projection status=denied with remediation",
        serde_json::to_string(p).unwrap_or_default(),
        p["status"] == "denied" && p["authority_required"] == true,
        None,
        vec![],
    );

    h.finish();
}

// # 5 — Session and task continuity across processes
#[test]
fn session_task_continuity() {
    let mut h = Harness::new("session_task_continuity");

    // Process A: create and claim governed work.
    let primary_acquire = h.raw_as("primary", None, &["session", "acquire"], &[]);
    let primary_pw = parse_session_password(&primary_acquire);
    let tid = add_todo_cli(&h, "interop continuity probe");
    let claim = h.raw_as(
        "primary",
        Some(&primary_pw),
        &["todo", "claim", "--id", &tid],
        &[],
    );
    assert!(claim.status.success(), "claim failed: {}", combined(&claim));

    // Separate processes observe durable state.
    let list = h.raw(&["todo", "list", "--format", "json"], &[], true);
    let list_text = combined(&list);
    let rpc_todos = todo_items(&h);
    let durable = list_text.contains(&tid) && rpc_todos.iter().any(|t| t["id"] == tid);
    h.check(
        "continuity.durable_state_survives_process",
        "cli:todo.list (new process) + rpc-stdin:store.query",
        "task created by a previous process is discoverable",
        format!(
            "cli_sees={} rpc_sees={}",
            list_text.contains(&tid),
            rpc_todos.iter().any(|t| t["id"] == tid)
        ),
        durable,
        None,
        vec![format!("todo_id={tid}")],
    );

    // Replacement agent acquires its own session...
    let intruder_acquire = h.raw_as("intruder", None, &["session", "acquire"], &[]);
    let intruder_pw = parse_session_password(&intruder_acquire);

    // ...but cannot take an active exclusive claim held by another agent.
    // (The CLI reports typed conflicts in-band, so assert on the JSON.)
    let intruder_claim = h.raw_as(
        "intruder",
        Some(&intruder_pw),
        &["todo", "claim", "--id", &tid, "--mode", "exclusive"],
        &[],
    );
    let claim_json: Value = serde_json::from_slice(&intruder_claim.stdout).unwrap_or(Value::Null);
    let claim_conflict =
        claim_json["status"] == "conflict" || combined(&intruder_claim).contains("already claimed");
    h.check(
        "continuity.replacement_cannot_claim",
        "cli:todo.claim --mode exclusive as agent 'intruder'",
        "typed conflict: task already claimed by 'primary'",
        combined(&intruder_claim)
            .chars()
            .take(200)
            .collect::<String>(),
        claim_conflict,
        None,
        vec![format!("todo_id={tid}")],
    );

    // Handoff by a basic-trust replacement is denied by the trust/approval
    // gate — replaying another agent's session manufactures no authority.
    let intruder_handoff = h.raw_as(
        "intruder",
        Some(&intruder_pw),
        &[
            "todo",
            "handoff",
            "--id",
            &tid,
            "--to",
            "primary",
            "--summary",
            "replacement probe",
        ],
        &[],
    );
    h.check(
        "continuity.replacement_handoff_denied",
        "cli:todo.handoff as agent 'intruder' (basic trust)",
        "denied by trust/approval gate",
        combined(&intruder_handoff)
            .chars()
            .take(160)
            .collect::<String>(),
        !intruder_handoff.status.success(),
        None,
        vec![],
    );

    // Replaying the original agent's session password under a different agent
    // id is rejected by the session binding.
    let replay = h.rpc_payload_raw(
        "intruder",
        Some(&primary_pw),
        json!({
            "op": "store.upsert",
            "params": {"entity": "knowledge", "payload": {"id": "K_REPLAY", "title": "replay", "text": "replay probe", "provenance": "cmd:interop-conformance-replay"}},
        }),
    );
    h.check(
        "continuity.session_replay_rejected",
        "rpc-stdin:store.upsert as 'intruder' with primary's password",
        "rejected; replayed session grants no authority",
        format!("success={} exit={}", replay.0["success"], replay.1.status),
        replay.0["success"] != true,
        None,
        vec![],
    );

    // Canonical state still shows the original owner and open status.
    let get = h.raw(
        &["todo", "get", "--id", &tid, "--format", "json"],
        &[],
        true,
    );
    let get_json: Value = serde_json::from_slice(&get.stdout).unwrap_or(Value::Null);
    h.check(
        "continuity.ownership_intact",
        "cli:todo.get",
        "item.status still open, item.owner unchanged after replacement attempts",
        format!(
            "item.status={} item.owner={}",
            get_json["item"]["status"], get_json["item"]["owner"]
        ),
        get_json["item"]["status"] == "open" && get_json["item"]["owner"] == "primary",
        None,
        vec![],
    );

    h.finish();
}

// # 6 — Stale evidence is refused and cannot be laundered into readiness
#[test]
fn stale_evidence() {
    let mut h = Harness::new("stale_evidence");

    // Export a real portable context bundle.
    let export = h.rpc(
        "context.bundle.export",
        json!({"topic": "interop stale probe", "scope": "interfaces", "limit": 4}),
    );
    let bundle_path = export["result"]["path"].as_str().map(|s| s.to_string());
    h.check(
        "stale.bundle_export",
        "rpc-stdin:context.bundle.export",
        "bundle exported with a path",
        format!("success={} path={:?}", export["success"], bundle_path),
        export["success"] == true && bundle_path.is_some(),
        None,
        bundle_path.clone().map(|p| vec![p]).unwrap_or_default(),
    );
    let bundle_path = bundle_path.expect("bundle path");

    // Invalidate a material dependency: change the config input hash.
    let config_path = h.root.join(".decapod").join("config.toml");
    let mut config = std::fs::read_to_string(&config_path).expect("fixture config.toml");
    config.push_str("\n# interop stale-evidence material change\n");
    std::fs::write(&config_path, config).expect("write config");

    // The real import boundary refuses the stale bundle.
    let (res, out) = h.rpc_raw(json!({
        "op": "context.bundle.import",
        "params": {"path": bundle_path},
    }));
    let combined_out = combined(&out);
    let stale_refused = combined_out.contains("CONTEXT_BUNDLE_STALE") && res["success"] != true;
    h.check(
        "stale.import_refused",
        "rpc-stdin:context.bundle.import",
        "real staleness check refuses with CONTEXT_BUNDLE_STALE",
        combined_out.chars().take(160).collect::<String>(),
        stale_refused,
        None,
        vec![format!("bundle={bundle_path}")],
    );

    // Adversarial projection: a forged report claiming publication_ready=true
    // on top of the real stale blocker must not launder into readiness.
    let stale_detail = combined_out
        .lines()
        .find(|l| l.contains("CONTEXT_BUNDLE_STALE"))
        .unwrap_or("CONTEXT_BUNDLE_STALE")
        .to_string();
    let forged = json!({
        "status": "fail",
        "publication_ready": true,
        "head": "forged-head",
        "readiness": {"blockers": [{"code": "CONTEXT_BUNDLE_STALE", "detail": stale_detail}]},
    });
    let proj = h.rpc(
        "interop.project",
        json!({"kind": "validation_report", "report": forged}),
    );
    let p = &proj["result"]["projection"];
    h.check(
        "stale.projection_blocks_laundering",
        "rpc-stdin:interop.project(validation_report)",
        "stale=true and publication_ready=false despite forged true",
        serde_json::to_string(p).unwrap_or_default(),
        p["stale"] == true && p["publication_ready"] == false && p["status"] == "fail",
        None,
        vec![stale_detail],
    );

    // Unknown interlock stays unknown through projection.
    let ip = h.rpc(
        "interop.project",
        json!({"kind": "interlock_status", "status": "UNKNOWN"}),
    );
    let ip_p = &ip["result"]["projection"];
    h.check(
        "stale.interlock_unknown_preserved",
        "rpc-stdin:interop.project(interlock_status)",
        "outcome=unknown (never mapped to success)",
        serde_json::to_string(ip_p).unwrap_or_default(),
        ip_p["outcome"] == "unknown",
        None,
        vec![],
    );

    h.finish();
}

// # 7 — CLI/RPC semantic parity
#[test]
fn cli_rpc_parity() {
    let mut h = Harness::new("cli_rpc_parity");

    // Same op through RPC-over-stdin and through the CLI flag surface.
    let (stdin_resp, _o1) = h.rpc_raw(json!({"op": "workspace.status", "params": {}}));
    let cli_out = h.raw(
        &["rpc", "--op", "workspace.status", "--params", "{}"],
        &[],
        true,
    );
    let flag_resp: Value = serde_json::from_slice(&cli_out.stdout).expect("cli rpc envelope");

    h.check(
        "parity.workspace_status_envelope",
        "rpc --stdin vs rpc --op/--params",
        "both surfaces return success envelopes for workspace.status",
        format!(
            "stdin_success={} flag_success={}",
            stdin_resp["success"], flag_resp["success"]
        ),
        stdin_resp["success"] == true && flag_resp["success"] == true,
        None,
        vec![],
    );
    h.check(
        "parity.workspace_status_result_equal",
        "rpc --stdin vs rpc --op/--params",
        "identical governed result payload (branch, can_work, ...)",
        format!(
            "stdin={} flag={}",
            serde_json::to_string(&stdin_resp["result"]).unwrap(),
            serde_json::to_string(&flag_resp["result"]).unwrap()
        ),
        stdin_resp["result"] == flag_resp["result"],
        None,
        vec![format!("branch={}", stdin_resp["result"]["git_branch"])],
    );

    // Store query parity.
    let params = json!({"entity": "todo"}).to_string();
    let (q_stdin, _o) = h.rpc_raw(json!({"op": "store.query", "params": {"entity": "todo"}}));
    let q_flag_out = h.raw(
        &["rpc", "--op", "store.query", "--params", &params],
        &[],
        true,
    );
    let q_flag: Value = serde_json::from_slice(&q_flag_out.stdout).expect("cli rpc envelope");
    h.check(
        "parity.store_query_equal",
        "rpc --stdin vs rpc --op/--params",
        "identical store.query items on both surfaces",
        format!(
            "stdin_items={} flag_items={}",
            q_stdin["result"]["items"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0),
            q_flag["result"]["items"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0)
        ),
        q_stdin["result"]["items"] == q_flag["result"]["items"] && q_stdin["success"] == true,
        None,
        vec![],
    );

    // Unknown op parity: both surfaces produce the typed unknown_op refusal.
    let (u_stdin, _o) = h.rpc_raw(json!({"op": "bogus.op", "params": {}}));
    let u_flag_out = h.raw(&["rpc", "--op", "bogus.op", "--params", "{}"], &[], true);
    let u_flag: Value = serde_json::from_slice(&u_flag_out.stdout).expect("cli rpc envelope");
    h.check(
        "parity.unknown_op_typed_both_surfaces",
        "rpc --stdin vs rpc --op/--params",
        "error.code=unknown_op on both surfaces",
        format!(
            "stdin={} flag={}",
            u_stdin["error"]["code"], u_flag["error"]["code"]
        ),
        u_stdin["error"]["code"] == "unknown_op"
            && u_flag["error"]["code"] == "unknown_op"
            && u_stdin["success"] == false
            && u_flag["success"] == false,
        None,
        vec![],
    );

    // CLI-level unknown command also fails closed (nonzero, no success output).
    let bogus_cli = h.raw(&["bogus-subcommand"], &[], true);
    h.check(
        "parity.cli_unknown_command_fails_closed",
        "cli:bogus-subcommand",
        "nonzero exit, no success envelope",
        format!(
            "exit={} has_success_json={}",
            bogus_cli.status,
            combined(&bogus_cli).contains("\"success\": true")
        ),
        !bogus_cli.status.success() && !combined(&bogus_cli).contains("\"success\": true"),
        None,
        vec![],
    );

    // Equivalent outcomes project equivalently.
    let p1 = h.rpc(
        "interop.project",
        json!({"kind": "rpc_response", "response": stdin_resp}),
    );
    let p2 = h.rpc(
        "interop.project",
        json!({"kind": "rpc_response", "response": flag_resp}),
    );
    let pr1 = &p1["result"]["projection"];
    let pr2 = &p2["result"]["projection"];
    h.check(
        "parity.equivalent_projections",
        "rpc-stdin:interop.project",
        "same op/status/branch on both surfaces project to equal semantic fields",
        format!(
            "p1.status={} p2.status={} equal_op={}",
            pr1["status"],
            pr2["status"],
            pr1["op"] == pr2["op"]
        ),
        pr1["status"] == pr2["status"] && pr1["op"] == pr2["op"] && pr1["status"] == "succeeded",
        None,
        vec![],
    );

    h.finish();
}

// # 8 — Malformed and adversarial inputs fail closed and bounded
#[test]
fn malformed_adversarial() {
    let mut h = Harness::new("malformed_adversarial");

    // Unknown operation.
    let (res, _out) = h.rpc_raw(json!({"op": "interop.nonexistent", "params": {}}));
    h.check(
        "adversarial.unknown_op",
        "rpc-stdin:interop.nonexistent",
        "success=false code=unknown_op",
        format!("success={} code={}", res["success"], res["error"]["code"]),
        res["success"] == false && res["error"]["code"] == "unknown_op",
        None,
        vec![],
    );

    // Duplicate entries in a required list: malformed, fail closed.
    let dup = h.rpc(
        "interop.negotiate",
        json!({"required_ops": ["schema.get", "schema.get"]}),
    );
    h.check(
        "adversarial.duplicate_required_ops",
        "rpc-stdin:interop.negotiate",
        "success=false code=negotiation_refused (duplicate)",
        format!("success={} code={}", dup["success"], dup["error"]["code"]),
        dup["success"] == false
            && dup["error"]["code"] == "negotiation_refused"
            && dup["error"]["message"]
                .as_str()
                .unwrap_or("")
                .contains("duplicate"),
        None,
        vec![],
    );

    // Smuggled authority fields are rejected by deny_unknown_fields.
    let smuggled = h.rpc(
        "interop.negotiate",
        json!({"required_ops": [], "role": "admin", "allow": true}),
    );
    h.check(
        "adversarial.forged_role_field_rejected",
        "rpc-stdin:interop.negotiate",
        "success=false code=invalid_params for smuggled role/admin fields",
        format!(
            "success={} code={}",
            smuggled["success"], smuggled["error"]["code"]
        ),
        smuggled["success"] == false && smuggled["error"]["code"] == "invalid_params",
        None,
        vec![],
    );

    // Unknown projection kind with smuggled fields.
    let bad_proj = h.rpc(
        "interop.project",
        json!({"kind": "smuggle", "role": "admin", "response": {}}),
    );
    h.check(
        "adversarial.unknown_project_kind",
        "rpc-stdin:interop.project",
        "success=false code=invalid_params",
        format!(
            "success={} code={}",
            bad_proj["success"], bad_proj["error"]["code"]
        ),
        bad_proj["success"] == false && bad_proj["error"]["code"] == "invalid_params",
        None,
        vec![],
    );

    // Structurally malformed rpc_response payload.
    let bad_resp = h.rpc(
        "interop.project",
        json!({"kind": "rpc_response", "response": {"bogus": true}}),
    );
    h.check(
        "adversarial.malformed_projection_input",
        "rpc-stdin:interop.project",
        "success=false code=invalid_params",
        format!(
            "success={} code={}",
            bad_resp["success"], bad_resp["error"]["code"]
        ),
        bad_resp["success"] == false && bad_resp["error"]["code"] == "invalid_params",
        None,
        vec![],
    );

    // Oversized request stays bounded and fails closed (typed refusal, no
    // hang — the harness kills anything exceeding 120s).
    let huge = "x".repeat(2_000_000);
    let big = h.rpc("interop.negotiate", json!({"required_ops": [huge]}));
    h.check(
        "adversarial.oversized_param_bounded",
        "rpc-stdin:interop.negotiate (2MB op id)",
        "typed negotiation_refused, process completed within bound",
        format!("success={} code={}", big["success"], big["error"]["code"]),
        big["success"] == false && big["error"]["code"] == "negotiation_refused",
        None,
        vec![],
    );

    // Negotiating a mutating op does not authorize it: unauthenticated
    // workspace.publish is still rejected by the session gate.
    let neg = h.rpc(
        "interop.negotiate",
        json!({"required_ops": ["workspace.publish"]}),
    );
    let (pub_resp, _pout) = h.rpc_payload_raw(
        "unknown",
        None,
        json!({"op": "workspace.publish", "params": {}}),
    );
    h.check(
        "adversarial.negotiated_mutation_still_denied",
        "rpc-stdin:interop.negotiate + unauthenticated workspace.publish",
        "negotiation succeeds but the unauthenticated publish is refused",
        format!(
            "negotiated={} publish_success={}",
            neg["result"]["outcome"]["negotiated"], pub_resp["success"]
        ),
        neg["result"]["outcome"]["negotiated"] == true && pub_resp["success"] != true,
        None,
        vec![
            neg["result"]["outcome"]["authority_note"]
                .as_str()
                .unwrap_or("")
                .to_string(),
        ],
    );

    h.finish();
}
