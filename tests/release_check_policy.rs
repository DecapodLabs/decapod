use decapod::core::capsule_policy::CapsulePolicyBinding;
use decapod::core::context_capsule::{
    ContextCapsuleSnippet, ContextCapsuleSource, DeterministicContextCapsule,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dirs");
    }
    fs::write(path, content).expect("write file");
}

fn setup_release_fixture(changelog_unreleased: &str) -> (TempDir, PathBuf) {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().to_path_buf();

    let init = Command::new("git")
        .current_dir(&root)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    write(
        &root.join("CHANGELOG.md"),
        &format!("# Changelog\n\n## [Unreleased]\n{changelog_unreleased}\n"),
    );
    write(&root.join(".decapod/README.md"), "decapod fixture\n");
    write(&root.join(".decapod/data/.gitkeep"), "");
    write(
        &root.join("constitution/docs/MIGRATIONS.md"),
        "# Migrations\n\n- forward-only\n",
    );
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(&root.join("Cargo.lock"), "# lock\n");
    write(
        &root.join("tests/golden/rpc/v1/agent_init.request.json"),
        "{ \"op\": \"agent.init\" }\n",
    );
    write(
        &root.join("tests/golden/rpc/v1/agent_init.response.json"),
        "{ \"status\": \"ok\" }\n",
    );
    write(&root.join("README.md"), "fixture\n");
    write(
        &root.join("src/core/schemas.rs"),
        "pub fn schema_version() -> &'static str { \"1\" }\n",
    );

    let readme = fs::read(root.join("README.md")).expect("read readme");
    let readme_hash = sha256_hex(&readme);
    let policy_hash = sha256_hex(b"fixture-policy-v1");
    let capsule = DeterministicContextCapsule {
        schema_version: "1.1.0".to_string(),
        topic: "release fixture".to_string(),
        scope: "interfaces".to_string(),
        task_id: Some("R_FIXTURE".to_string()),
        workunit_id: None,
        sources: vec![ContextCapsuleSource {
            path: "interfaces/CONTROL_PLANE.md".to_string(),
            section: "Control Plane".to_string(),
        }],
        snippets: vec![ContextCapsuleSnippet {
            source_path: "interfaces/CONTROL_PLANE.md".to_string(),
            text: "fixture snippet".to_string(),
        }],
        policy: CapsulePolicyBinding::default(),
        capsule_hash: String::new(),
    }
    .with_recomputed_hash()
    .expect("compute capsule hash");
    let capsule_path = ".decapod/generated/context/R_FIXTURE.json";
    write(
        &root.join(capsule_path),
        &serde_json::to_string_pretty(&capsule).expect("serialize capsule"),
    );

    write(
        &root.join(".decapod/generated/artifacts/provenance/artifact_manifest.json"),
        &format!(
            "{{\n  \"schema_version\": \"1.0.0\",\n  \"kind\": \"artifact_manifest\",\n  \"policy_lineage\": {{\n    \"policy_hash\": \"{policy_hash}\",\n    \"policy_revision\": \"fixture-policy@1\",\n    \"risk_tier\": \"medium\",\n    \"capsule_path\": \"{capsule_path}\",\n    \"capsule_hash\": \"{capsule_hash}\"\n  }},\n  \"artifacts\": [{{\"path\": \"README.md\", \"sha256\": \"{readme_hash}\"}}]\n}}\n",
            capsule_hash = capsule.capsule_hash
        ),
    );
    write(
        &root.join(".decapod/generated/artifacts/provenance/proof_manifest.json"),
        &format!(
            "{{\n  \"schema_version\": \"1.0.0\",\n  \"kind\": \"proof_manifest\",\n  \"policy_lineage\": {{\n    \"policy_hash\": \"{policy_hash}\",\n    \"policy_revision\": \"fixture-policy@1\",\n    \"risk_tier\": \"medium\",\n    \"capsule_path\": \"{capsule_path}\",\n    \"capsule_hash\": \"{capsule_hash}\"\n  }},\n  \"proofs\": [{{\"command\": \"decapod validate\", \"result\": \"pass\"}}],\n  \"environment\": {{\"os\": \"linux\", \"rust\": \"stable\"}}\n}}\n",
            capsule_hash = capsule.capsule_hash
        ),
    );
    write(
        &root.join(".decapod/generated/artifacts/provenance/intent_convergence_checklist.json"),
        &format!(
            "{{\n  \"schema_version\": \"1.0.0\",\n  \"kind\": \"intent_convergence_checklist\",\n  \"policy_lineage\": {{\n    \"policy_hash\": \"{policy_hash}\",\n    \"policy_revision\": \"fixture-policy@1\",\n    \"risk_tier\": \"medium\",\n    \"capsule_path\": \"{capsule_path}\",\n    \"capsule_hash\": \"{capsule_hash}\"\n  }},\n  \"pr\": {{\"base\": \"master\", \"scope\": \"fixture\"}},\n  \"intent\": \"Keep proofs and intent converged\",\n  \"scope\": \"release\",\n  \"checklist\": [\n    {{\"name\": \"intent\", \"status\": \"pass\", \"evidence\": \"INTENT.md\"}}\n  ]\n}}\n",
            capsule_hash = capsule.capsule_hash
        ),
    );

    let add = Command::new("git")
        .current_dir(&root)
        .args(["add", "."])
        .output()
        .expect("git add");
    assert!(add.status.success(), "git add failed");
    let commit = Command::new("git")
        .current_dir(&root)
        .env("GIT_AUTHOR_NAME", "Alex H. Raber")
        .env("GIT_AUTHOR_EMAIL", "alex@example.com")
        .env("GIT_COMMITTER_NAME", "Alex H. Raber")
        .env("GIT_COMMITTER_EMAIL", "alex@example.com")
        .args(["commit", "-m", "fixture"])
        .output()
        .expect("git commit");
    assert!(
        commit.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&commit.stderr)
    );

    (tmp, root)
}

fn run_release_check(root: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(root)
        .args(["release", "check"])
        .output()
        .expect("run release check")
}

fn run_release_lineage_sync(root: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(root)
        .args(["release", "lineage-sync"])
        .output()
        .expect("run release lineage-sync")
}

#[test]
fn release_check_blocks_schema_changes_without_changelog_note() {
    let (_tmp, root) = setup_release_fixture("- housekeeping only");
    fs::write(
        root.join("src/core/schemas.rs"),
        "pub fn schema_version() -> &'static str { \"2\" }\n",
    )
    .expect("mutate schemas");

    let output = run_release_check(&root);
    assert!(!output.status.success(), "release check should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("schema/interface files changed"),
        "release check should explain schema/interface changelog policy; stderr:\n{}",
        stderr
    );
}

#[test]
fn release_check_allows_schema_changes_with_changelog_note() {
    let (_tmp, root) = setup_release_fixture("- schema: bump todo shape for v2");
    fs::write(
        root.join("src/core/schemas.rs"),
        "pub fn schema_version() -> &'static str { \"2\" }\n",
    )
    .expect("mutate schemas");

    let output = run_release_check(&root);
    assert!(
        output.status.success(),
        "release check should pass when changelog includes schema note.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn release_check_autostamps_missing_policy_lineage() {
    let (_tmp, root) = setup_release_fixture("- schema: bump todo shape for v2");
    write(
        &root.join(".decapod/generated/artifacts/provenance/proof_manifest.json"),
        "{\n  \"schema_version\": \"1.0.0\",\n  \"kind\": \"proof_manifest\",\n  \"proofs\": [{\"command\": \"decapod validate\", \"result\": \"pass\"}],\n  \"environment\": {\"os\": \"linux\", \"rust\": \"stable\"}\n}\n",
    );
    let output = run_release_check(&root);
    assert!(output.status.success(), "release check should pass");
    let proof_path = root.join(".decapod/generated/artifacts/provenance/proof_manifest.json");
    let proof: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&proof_path).expect("read proof manifest"))
            .expect("parse proof manifest");
    assert!(
        proof.get("policy_lineage").is_some(),
        "release check should auto-stamp missing lineage"
    );
}

#[test]
fn release_check_requires_consistent_policy_lineage_across_manifests() {
    let (_tmp, root) = setup_release_fixture("- schema: bump todo shape for v2");
    let proof_path = root.join(".decapod/generated/artifacts/provenance/proof_manifest.json");
    let mut proof: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&proof_path).expect("read proof manifest"))
            .expect("parse proof manifest");
    proof["policy_lineage"]["risk_tier"] = serde_json::Value::String("high".to_string());
    write(
        &proof_path,
        &serde_json::to_string_pretty(&proof).expect("serialize proof manifest"),
    );

    let output = run_release_check(&root);
    assert!(output.status.success(), "release check should pass");
    let proof_after: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            root.join(".decapod/generated/artifacts/provenance/proof_manifest.json"),
        )
        .expect("read stamped proof"),
    )
    .expect("parse stamped proof");
    let artifact_after: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            root.join(".decapod/generated/artifacts/provenance/artifact_manifest.json"),
        )
        .expect("read stamped artifact"),
    )
    .expect("parse stamped artifact");
    assert!(
        proof_after["policy_lineage"] == artifact_after["policy_lineage"],
        "release check should normalize lineage consistency across manifests"
    );
}

#[test]
fn release_check_repairs_lineage_capsule_drift() {
    let (_tmp, root) = setup_release_fixture("- schema: bump todo shape for v2");
    let capsule_path = root.join(".decapod/generated/context/R_FIXTURE.json");
    let mut capsule: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&capsule_path).expect("read capsule"))
            .expect("parse capsule");
    capsule["topic"] = serde_json::Value::String("tampered release fixture".to_string());
    write(
        &capsule_path,
        &serde_json::to_string_pretty(&capsule).expect("serialize capsule"),
    );

    let output = run_release_check(&root);
    assert!(output.status.success(), "release check should pass");
    let proof_after: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            root.join(".decapod/generated/artifacts/provenance/proof_manifest.json"),
        )
        .expect("read stamped proof"),
    )
    .expect("parse stamped proof");
    let lineage_capsule_path = proof_after["policy_lineage"]["capsule_path"]
        .as_str()
        .expect("lineage capsule path");
    let capsule_after: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(lineage_capsule_path)).expect("read stamped capsule"),
    )
    .expect("parse stamped capsule");
    assert!(
        proof_after["policy_lineage"]["capsule_hash"] == capsule_after["capsule_hash"],
        "release check should repair lineage to the deterministic capsule hash"
    );
}

#[test]
fn release_check_fails_closed_for_invalid_release_risk_tier_env() {
    let (_tmp, root) = setup_release_fixture("- schema: bump todo shape for v2");
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(&root)
        .env("DECAPOD_RELEASE_RISK_TIER", "invalid")
        .args(["release", "check"])
        .output()
        .expect("run release check");
    assert!(!output.status.success(), "release check should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid DECAPOD_RELEASE_RISK_TIER"),
        "release check should fail closed with typed error for invalid risk tier env; stderr:\n{}",
        stderr
    );
}

#[test]
fn release_lineage_sync_stamps_all_provenance_manifests() {
    let (_tmp, root) = setup_release_fixture("- schema: bump todo shape for v2");
    let proof_path = root.join(".decapod/generated/artifacts/provenance/proof_manifest.json");
    write(
        &proof_path,
        "{\n  \"schema_version\": \"1.0.0\",\n  \"kind\": \"proof_manifest\",\n  \"proofs\": [{\"command\": \"decapod validate\", \"result\": \"pass\"}],\n  \"environment\": {\"os\": \"linux\", \"rust\": \"stable\"}\n}\n",
    );
    let output = run_release_lineage_sync(&root);
    assert!(
        output.status.success(),
        "lineage sync should pass.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"cmd\":\"release.lineage_sync\""),
        "lineage sync should emit envelope"
    );

    let artifact: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            root.join(".decapod/generated/artifacts/provenance/artifact_manifest.json"),
        )
        .expect("read artifact manifest"),
    )
    .expect("parse artifact");
    let proof: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&proof_path).expect("read proof manifest"))
            .expect("parse proof");
    let intent: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            root.join(".decapod/generated/artifacts/provenance/intent_convergence_checklist.json"),
        )
        .expect("read intent manifest"),
    )
    .expect("parse intent");

    assert!(
        proof.get("policy_lineage").is_some(),
        "proof should be stamped"
    );
    assert_eq!(
        artifact["policy_lineage"], proof["policy_lineage"],
        "artifact/proof lineage should match"
    );
    assert_eq!(
        artifact["policy_lineage"], intent["policy_lineage"],
        "artifact/intent lineage should match"
    );
}
