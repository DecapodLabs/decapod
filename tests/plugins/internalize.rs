//! Tests for internalized context artifacts.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

fn decapod_bin() -> String {
    env!("CARGO_BIN_EXE_decapod").to_string()
}

fn setup_project() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    let output = Command::new(decapod_bin())
        .current_dir(&temp_path)
        .args(["init", "--force"])
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("run decapod init");
    assert!(output.status.success(), "decapod init failed");

    fs::write(
        temp_path.join("sample_doc.txt"),
        "This is a sample document for internalization testing.\nIt has multiple lines.\nAnd some content.",
    )
    .unwrap();

    (temp_dir, temp_path)
}

fn run_decapod(dir: &Path, args: &[&str]) -> (bool, String) {
    let output = Command::new(decapod_bin())
        .current_dir(dir)
        .args(args)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("execute decapod");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), format!("{}\n{}", stdout, stderr))
}

fn parse_json_from_output(output: &str) -> serde_json::Value {
    let start = output.find('{').expect("json start");
    let end = output.rfind('}').expect("json end");
    serde_json::from_str(&output[start..=end]).expect("json parse")
}

#[test]
fn test_internalization_manifest_schema_roundtrip() {
    use decapod::plugins::internalize::{
        CapabilitiesContract, DeterminismClass, InternalizationManifest, ProvenanceEntry,
        ReplayClass, ReplayRecipe, RiskTier, SCHEMA_VERSION,
    };
    use std::collections::BTreeMap;

    let manifest = InternalizationManifest {
        schema_version: SCHEMA_VERSION.to_string(),
        id: "int_0123456789abcdef01234567".to_string(),
        source_hash: "a".repeat(64),
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
            inputs_hash: "a".repeat(64),
        }],
        replay_recipe: ReplayRecipe {
            mode: ReplayClass::Replayable,
            command: "decapod".to_string(),
            args: vec!["internalize".to_string(), "create".to_string()],
            env: BTreeMap::new(),
            reason: "deterministic profile with pinned binary hash".to_string(),
        },
        adapter_hash: "b".repeat(64),
        adapter_path: "adapter.bin".to_string(),
        capabilities_contract: CapabilitiesContract {
            allowed_scopes: vec!["qa".to_string()],
            permitted_tools: vec!["decapod-cli".to_string()],
            allow_code_gen: false,
        },
        risk_tier: RiskTier::default(),
        determinism_class: DeterminismClass::Deterministic,
        binary_hash: "c".repeat(64),
        runtime_fingerprint: "os=linux arch=x86_64 executable=builtin:noop".to_string(),
    };

    let json = serde_json::to_string_pretty(&manifest).unwrap();
    let roundtrip: InternalizationManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(manifest, roundtrip);
}

#[test]
fn test_schema_files_exist_and_parse() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let schema_dir = repo_root.join("constitution/interfaces/jsonschema/internalization");
    let files = [
        "InternalizationManifest.schema.json",
        "InternalizationCreateResult.schema.json",
        "InternalizationAttachResult.schema.json",
        "InternalizationDetachResult.schema.json",
        "InternalizationInspectResult.schema.json",
    ];

    for file in files {
        let raw = fs::read_to_string(schema_dir.join(file)).expect("read schema fixture");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("parse schema fixture");
        assert!(
            parsed.get("$id").is_some(),
            "schema {} must declare $id",
            file
        );
    }
}

#[test]
fn test_manifest_deterministic_for_same_inputs() {
    use decapod::plugins::internalize::create_internalization;

    let temp_dir = TempDir::new().unwrap();
    let store_root = temp_dir.path().to_path_buf();
    let doc_path = temp_dir.path().join("doc.txt");
    fs::write(&doc_path, "deterministic content").unwrap();

    let r1 = create_internalization(
        &store_root,
        doc_path.to_str().unwrap(),
        "model-v1",
        "noop",
        0,
        &["qa".to_string()],
    )
    .unwrap();
    let r2 = create_internalization(
        &store_root,
        doc_path.to_str().unwrap(),
        "model-v1",
        "noop",
        0,
        &["qa".to_string()],
    )
    .unwrap();

    assert!(!r1.cache_hit);
    assert!(r2.cache_hit);
    assert_eq!(r1.artifact_id, r2.artifact_id);
}

#[test]
fn test_source_hash_binding_is_enforced_on_attach() {
    use decapod::plugins::internalize::{attach_internalization, create_internalization};

    let temp_dir = TempDir::new().unwrap();
    let store_root = temp_dir.path().to_path_buf();
    let doc_path = temp_dir.path().join("doc.txt");
    fs::write(&doc_path, "version 1").unwrap();

    let created = create_internalization(
        &store_root,
        doc_path.to_str().unwrap(),
        "model-v1",
        "noop",
        0,
        &["qa".to_string()],
    )
    .unwrap();

    fs::write(&doc_path, "version 2").unwrap();

    let err = attach_internalization(
        &store_root,
        &created.artifact_id,
        "session-1",
        "decapod-cli",
        1800,
    )
    .unwrap_err();
    assert!(format!("{}", err).contains("Source integrity check failed"));
}

#[test]
fn test_ttl_blocks_attach_after_expiry() {
    use decapod::plugins::internalize::{attach_internalization, create_internalization};

    let temp_dir = TempDir::new().unwrap();
    let store_root = temp_dir.path().to_path_buf();
    let doc_path = temp_dir.path().join("doc.txt");
    fs::write(&doc_path, "content").unwrap();

    let result = create_internalization(
        &store_root,
        doc_path.to_str().unwrap(),
        "model-v1",
        "noop",
        1,
        &["qa".to_string()],
    )
    .unwrap();

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

    let err = attach_internalization(
        &store_root,
        &result.artifact_id,
        "test-session",
        "decapod-cli",
        1800,
    );
    assert!(err.is_err());
}

#[test]
fn test_full_lifecycle_create_attach_detach_inspect() {
    use decapod::plugins::internalize::{
        DeterminismClass, ReplayClass, attach_internalization, create_internalization,
        detach_internalization, inspect_internalization,
    };

    let temp_dir = TempDir::new().unwrap();
    let store_root = temp_dir.path().to_path_buf();
    let doc_path = temp_dir.path().join("doc.txt");
    fs::write(&doc_path, "lifecycle test document").unwrap();

    let create_result = create_internalization(
        &store_root,
        doc_path.to_str().unwrap(),
        "claude-sonnet-4-6",
        "noop",
        0,
        &["qa".to_string()],
    )
    .unwrap();
    assert_eq!(
        create_result.manifest.determinism_class,
        DeterminismClass::Deterministic
    );
    assert_eq!(
        create_result.manifest.replay_recipe.mode,
        ReplayClass::Replayable
    );

    let inspect_result = inspect_internalization(&store_root, &create_result.artifact_id).unwrap();
    assert_eq!(inspect_result.status, "valid");
    assert!(inspect_result.integrity.replayable_claim_valid);

    let attach_result = attach_internalization(
        &store_root,
        &create_result.artifact_id,
        "session-001",
        "decapod-cli",
        900,
    )
    .unwrap();
    assert_eq!(attach_result.lease_seconds, 900);

    let mount_path = store_root
        .join("generated")
        .join("sessions")
        .join("session-001")
        .join("internalize_mounts")
        .join(format!("mount_{}.json", create_result.artifact_id));
    assert!(mount_path.exists());

    let detach_result =
        detach_internalization(&store_root, &create_result.artifact_id, "session-001").unwrap();
    assert!(detach_result.detached);
    assert!(!mount_path.exists());
}

#[test]
fn test_cli_create_attach_detach_inspect() {
    let (_temp_dir, temp_path) = setup_project();

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
    assert!(success, "create should succeed:\n{}", output);
    let created = parse_json_from_output(&output);
    let artifact_id = created["artifact_id"].as_str().unwrap();

    let (success, output) = run_decapod(
        &temp_path,
        &[
            "internalize",
            "attach",
            "--id",
            artifact_id,
            "--session",
            "session-123",
            "--tool",
            "decapod-cli",
            "--lease-seconds",
            "600",
            "--format",
            "json",
        ],
    );
    assert!(success, "attach should succeed:\n{}", output);

    let (success, output) = run_decapod(
        &temp_path,
        &[
            "internalize",
            "detach",
            "--id",
            artifact_id,
            "--session",
            "session-123",
            "--format",
            "json",
        ],
    );
    assert!(success, "detach should succeed:\n{}", output);

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
    assert!(success, "inspect should succeed:\n{}", output);
}
