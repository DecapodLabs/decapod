//! Tests for internalized context artifacts.
//!
//! Proves: manifest determinism, source hash binding, TTL expiry blocking,
//! schema stability, and the full create → attach → inspect lifecycle.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn decapod_bin() -> String {
    env!("CARGO_BIN_EXE_decapod").to_string()
}

fn setup_project() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    // Init decapod
    let output = Command::new(decapod_bin())
        .current_dir(&temp_path)
        .args(["init", "--force"])
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("Failed to run decapod init");
    assert!(output.status.success(), "decapod init failed");

    // Create a sample source document
    fs::write(
        temp_path.join("sample_doc.txt"),
        "This is a sample document for internalization testing.\nIt has multiple lines.\nAnd some content.",
    )
    .unwrap();

    (temp_dir, temp_path)
}

fn run_decapod(dir: &PathBuf, args: &[&str]) -> (bool, String) {
    let output = Command::new(decapod_bin())
        .current_dir(dir)
        .args(args)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("Failed to execute decapod");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), format!("{}\n{}", stdout, stderr))
}

// ── Schema Stability Tests ─────────────────────────────────────────────

#[test]
fn test_internalization_manifest_schema_roundtrip() {
    use decapod::plugins::internalize::{
        CapabilitiesContract, InternalizationManifest, ProvenanceEntry, ReplayRecipe, RiskTier,
    };
    use std::collections::BTreeMap;

    let manifest = InternalizationManifest {
        schema_version: "1.0.0".to_string(),
        id: "01TESTID000000000000000000".to_string(),
        source_hash: "abc123".to_string(),
        source_path: "/tmp/doc.txt".to_string(),
        extraction_method: "noop".to_string(),
        chunking_params: BTreeMap::new(),
        base_model_id: "test-model-v1".to_string(),
        internalizer_profile: "noop".to_string(),
        internalizer_version: "1.0.0".to_string(),
        adapter_format: "noop".to_string(),
        created_at: "2026-02-28T00:00:00Z".to_string(),
        ttl_seconds: 3600,
        expires_at: Some("2026-02-28T01:00:00Z".to_string()),
        provenance: vec![ProvenanceEntry {
            op: "internalize.create".to_string(),
            timestamp: "2026-02-28T00:00:00Z".to_string(),
            actor: "decapod-cli".to_string(),
            inputs_hash: "abc123".to_string(),
        }],
        replay_recipe: ReplayRecipe {
            command: "decapod".to_string(),
            args: vec!["internalize".to_string(), "create".to_string()],
            env: BTreeMap::new(),
        },
        adapter_hash: "def456".to_string(),
        adapter_path: "adapter.bin".to_string(),
        capabilities_contract: CapabilitiesContract {
            allowed_scopes: vec!["qa".to_string()],
            permitted_tools: vec!["*".to_string()],
            allow_code_gen: false,
        },
        risk_tier: RiskTier::default(),
    };

    // Serialize
    let json = serde_json::to_string_pretty(&manifest).unwrap();

    // Deserialize
    let roundtrip: InternalizationManifest = serde_json::from_str(&json).unwrap();

    assert_eq!(manifest, roundtrip, "Manifest must survive JSON roundtrip");
}

#[test]
fn test_create_result_schema_has_required_fields() {
    use decapod::plugins::internalize::InternalizationCreateResult;

    let json = r#"{
        "schema_version": "1.0.0",
        "success": true,
        "artifact_id": "test",
        "artifact_path": "/tmp/test",
        "manifest": {
            "schema_version": "1.0.0",
            "id": "test",
            "source_hash": "abc",
            "source_path": "/tmp/doc.txt",
            "extraction_method": "noop",
            "chunking_params": {},
            "base_model_id": "model",
            "internalizer_profile": "noop",
            "internalizer_version": "1.0.0",
            "adapter_format": "noop",
            "created_at": "2026-02-28T00:00:00Z",
            "ttl_seconds": 0,
            "provenance": [],
            "replay_recipe": {"command": "decapod", "args": [], "env": {}},
            "adapter_hash": "def",
            "adapter_path": "adapter.bin",
            "capabilities_contract": {"allowed_scopes": [], "permitted_tools": [], "allow_code_gen": false},
            "risk_tier": {"creation": "compute-risky", "attach": "behavior-changing", "inspect": "read-only"}
        },
        "source_hash": "abc",
        "adapter_hash": "def"
    }"#;

    let result: InternalizationCreateResult = serde_json::from_str(json).unwrap();
    assert!(result.success);
    assert_eq!(result.schema_version, "1.0.0");
}

// ── Manifest Determinism ───────────────────────────────────────────────

#[test]
fn test_manifest_deterministic_for_same_inputs() {
    use decapod::plugins::internalize::create_internalization;

    let temp_dir = TempDir::new().unwrap();
    let store_root = temp_dir.path().to_path_buf();

    // Create source doc
    let doc_path = temp_dir.path().join("doc.txt");
    fs::write(&doc_path, "deterministic content").unwrap();

    let r1 = create_internalization(
        &store_root,
        doc_path.to_str().unwrap(),
        "model-v1",
        "noop",
        0,
        &[],
    )
    .unwrap();

    let r2 = create_internalization(
        &store_root,
        doc_path.to_str().unwrap(),
        "model-v1",
        "noop",
        0,
        &[],
    )
    .unwrap();

    // Source hashes must be identical for same input
    assert_eq!(r1.source_hash, r2.source_hash);
    // Adapter hashes must be identical for noop (empty adapter)
    assert_eq!(r1.adapter_hash, r2.adapter_hash);
    // But artifact IDs must differ (ULIDs)
    assert_ne!(r1.artifact_id, r2.artifact_id);
}

// ── Source Hash Binding ────────────────────────────────────────────────

#[test]
fn test_source_hash_changes_when_document_changes() {
    use decapod::plugins::internalize::create_internalization;

    let temp_dir = TempDir::new().unwrap();
    let store_root = temp_dir.path().to_path_buf();

    let doc_path = temp_dir.path().join("doc.txt");

    // First version
    fs::write(&doc_path, "version 1").unwrap();
    let r1 = create_internalization(
        &store_root,
        doc_path.to_str().unwrap(),
        "model-v1",
        "noop",
        0,
        &[],
    )
    .unwrap();

    // Modify document
    fs::write(&doc_path, "version 2").unwrap();
    let r2 = create_internalization(
        &store_root,
        doc_path.to_str().unwrap(),
        "model-v1",
        "noop",
        0,
        &[],
    )
    .unwrap();

    assert_ne!(
        r1.source_hash, r2.source_hash,
        "Source hash must change when document changes"
    );
}

// ── TTL Enforcement ────────────────────────────────────────────────────

#[test]
fn test_ttl_blocks_attach_after_expiry() {
    use decapod::plugins::internalize::{attach_internalization, create_internalization};

    let temp_dir = TempDir::new().unwrap();
    let store_root = temp_dir.path().to_path_buf();

    let doc_path = temp_dir.path().join("doc.txt");
    fs::write(&doc_path, "content").unwrap();

    // Create with TTL=1 second
    let result = create_internalization(
        &store_root,
        doc_path.to_str().unwrap(),
        "model-v1",
        "noop",
        1,
        &[],
    )
    .unwrap();

    // Manually set expires_at to the past
    let art_dir = store_root
        .join("generated")
        .join("artifacts")
        .join("internalizations")
        .join(&result.artifact_id);
    let manifest_path = art_dir.join("manifest.json");
    let raw = fs::read_to_string(&manifest_path).unwrap();
    let mut manifest: serde_json::Value = serde_json::from_str(&raw).unwrap();
    manifest["expires_at"] = serde_json::Value::String("2020-01-01T00:00:00Z".to_string());
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    // Attempt attach — should fail with Expired
    let err = attach_internalization(&store_root, &result.artifact_id, "test-session");
    assert!(err.is_err(), "Attach must fail on expired artifact");
    let err_msg = format!("{}", err.unwrap_err());
    assert!(
        err_msg.contains("expired"),
        "Error must mention expiry: {}",
        err_msg
    );
}

// ── Full Lifecycle: Create → Inspect → Attach ─────────────────────────

#[test]
fn test_full_lifecycle_create_inspect_attach() {
    use decapod::plugins::internalize::{
        attach_internalization, create_internalization, inspect_internalization,
    };

    let temp_dir = TempDir::new().unwrap();
    let store_root = temp_dir.path().to_path_buf();

    let doc_path = temp_dir.path().join("doc.txt");
    fs::write(&doc_path, "lifecycle test document").unwrap();

    // CREATE
    let create_result = create_internalization(
        &store_root,
        doc_path.to_str().unwrap(),
        "claude-sonnet-4-6",
        "noop",
        0,
        &["qa".to_string(), "summarization".to_string()],
    )
    .unwrap();

    assert!(create_result.success);
    assert_eq!(create_result.manifest.base_model_id, "claude-sonnet-4-6");
    assert_eq!(create_result.manifest.internalizer_profile, "noop");
    assert!(!create_result.source_hash.is_empty());

    // INSPECT
    let inspect_result = inspect_internalization(&store_root, &create_result.artifact_id).unwrap();

    assert_eq!(inspect_result.status, "valid");
    assert!(inspect_result.integrity.adapter_hash_valid);
    assert!(!inspect_result.integrity.expired);
    assert_eq!(
        inspect_result.manifest.source_hash,
        create_result.source_hash
    );

    // ATTACH
    let attach_result =
        attach_internalization(&store_root, &create_result.artifact_id, "session-001").unwrap();

    assert!(attach_result.success);
    assert_eq!(attach_result.session_id, "session-001");
    assert_eq!(attach_result.risk_classification, "behavior-changing");
    assert_eq!(attach_result.provenance_entry.op, "internalize.attach");

    // Verify provenance was logged to session dir
    let session_dir = store_root
        .join("generated")
        .join("sessions")
        .join("session-001");
    assert!(
        session_dir.exists(),
        "Session provenance directory must be created"
    );
}

// ── Noop Profile Tests ─────────────────────────────────────────────────

#[test]
fn test_noop_profile_produces_empty_adapter() {
    use decapod::plugins::internalize::InternalizerProfile;

    let temp_dir = TempDir::new().unwrap();
    let output_dir = temp_dir.path().to_path_buf();
    let doc_path = temp_dir.path().join("doc.txt");
    fs::write(&doc_path, "test").unwrap();

    let profile = InternalizerProfile::noop();
    assert_eq!(profile.name, "noop");
    assert_eq!(profile.adapter_format, "noop");

    let (adapter_path, _params) = profile.execute(&doc_path, "model", &output_dir).unwrap();
    assert!(adapter_path.exists());

    let content = fs::read(&adapter_path).unwrap();
    assert!(content.is_empty(), "Noop adapter must produce empty file");
}

// ── Risk Tier Tests ────────────────────────────────────────────────────

#[test]
fn test_risk_tier_defaults() {
    use decapod::plugins::internalize::RiskTier;

    let tier = RiskTier::default();
    assert_eq!(tier.creation, "compute-risky");
    assert_eq!(tier.attach, "behavior-changing");
    assert_eq!(tier.inspect, "read-only");
}

// ── CLI Integration (end-to-end via binary) ────────────────────────────

#[test]
fn test_cli_create_and_inspect() {
    let (_temp_dir, temp_path) = setup_project();

    // Create internalization
    let (success, output) = run_decapod(
        &temp_path,
        &[
            "internalize",
            "create",
            "--source",
            "sample_doc.txt",
            "--model",
            "test-model",
            "--profile",
            "noop",
            "--format",
            "json",
        ],
    );
    assert!(
        success,
        "internalize create should succeed. Output:\n{}",
        output
    );

    // Parse the JSON output to get artifact ID
    let stdout_lines: Vec<&str> = output.lines().collect();
    let json_start = stdout_lines
        .iter()
        .position(|l| l.trim_start().starts_with('{'));
    assert!(json_start.is_some(), "Output should contain JSON");

    // Find matching closing brace
    let json_str = &output[output.find('{').unwrap()..];
    let result: serde_json::Value =
        serde_json::from_str(&json_str[..json_str.rfind('}').unwrap() + 1])
            .expect("Should parse create result JSON");

    let artifact_id = result["artifact_id"].as_str().unwrap();
    assert!(!artifact_id.is_empty());

    // Inspect the artifact
    let (success, output) = run_decapod(
        &temp_path,
        &[
            "internalize",
            "inspect",
            "--id",
            artifact_id,
            "--format",
            "json",
        ],
    );
    assert!(
        success,
        "internalize inspect should succeed. Output:\n{}",
        output
    );

    let inspect_json_str = &output[output.find('{').unwrap()..];
    let inspect_result: serde_json::Value =
        serde_json::from_str(&inspect_json_str[..inspect_json_str.rfind('}').unwrap() + 1])
            .expect("Should parse inspect result JSON");

    assert_eq!(inspect_result["status"].as_str().unwrap(), "valid");
    assert_eq!(inspect_result["artifact_id"].as_str().unwrap(), artifact_id);
}

#[test]
fn test_cli_create_with_missing_source_fails() {
    let (_temp_dir, temp_path) = setup_project();

    let (success, _output) = run_decapod(
        &temp_path,
        &[
            "internalize",
            "create",
            "--source",
            "nonexistent.txt",
            "--model",
            "test-model",
        ],
    );
    assert!(
        !success,
        "internalize create with missing source should fail"
    );
}
