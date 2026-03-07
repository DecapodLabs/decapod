use decapod::core::context_capsule::{
    ContextCapsuleSnippet, ContextCapsuleSource, DeterministicContextCapsule,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(dir).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run decapod")
}

fn combined_output(output: &std::process::Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn setup_repo() -> (TempDir, PathBuf, String) {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path().to_path_buf();

    let init = Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let decapod_init = run_decapod(&dir, &["init", "--force"], &[]);
    assert!(
        decapod_init.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&decapod_init.stderr)
    );

    let acquire = run_decapod(
        &dir,
        &["session", "acquire"],
        &[("DECAPOD_AGENT_ID", "unknown")],
    );
    assert!(
        acquire.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );
    let stdout = String::from_utf8_lossy(&acquire.stdout);
    let password = stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .expect("password in session acquire output");

    (tmp, dir, password)
}

#[test]
fn validate_stubs_are_non_blocking_when_artifacts_absent() {
    let (_tmp, dir, password) = setup_repo();
    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        validate.status.success(),
        "validate should pass with no optional phase-0 artifacts; stderr:\n{}",
        String::from_utf8_lossy(&validate.stderr)
    );
}

#[test]
fn validate_fails_on_invalid_workunit_manifest_if_present() {
    let (_tmp, dir, password) = setup_repo();
    let workunits = dir.join(".decapod").join("governance").join("workunits");
    fs::create_dir_all(&workunits).expect("create workunits dir");
    fs::write(workunits.join("test_BAD.json"), "{not-json").expect("write malformed workunit");

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        !validate.status.success(),
        "validate should fail for malformed workunit"
    );
    let stderr = combined_output(&validate);
    assert!(
        stderr.contains("invalid workunit manifest"),
        "expected workunit parse failure in stderr, got:\n{}",
        stderr
    );
}

#[test]
fn validate_fails_on_verified_workunit_missing_passing_proofs() {
    let (_tmp, dir, password) = setup_repo();
    let workunits = dir.join(".decapod").join("governance").join("workunits");
    fs::create_dir_all(&workunits).expect("create workunits dir");
    fs::write(
        workunits.join("test_BAD_VERIFIED.json"),
        r#"{
  "task_id": "test_BAD_VERIFIED",
  "intent_ref": "intent://bad",
  "spec_refs": [],
  "state_refs": [],
  "proof_plan": ["validate_passes"],
  "proof_results": [],
  "status": "VERIFIED"
}"#,
    )
    .expect("write malformed verified workunit");

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        !validate.status.success(),
        "validate should fail for VERIFIED workunit without passing proof gates"
    );
    let stderr = combined_output(&validate);
    assert!(
        stderr.contains("invalid VERIFIED workunit manifest"),
        "expected VERIFIED workunit gate failure in stderr, got:\n{}",
        stderr
    );
}

#[test]
fn validate_fails_on_verified_workunit_missing_capsule_policy_lineage() {
    let (_tmp, dir, password) = setup_repo();
    let workunits = dir.join(".decapod").join("governance").join("workunits");
    fs::create_dir_all(&workunits).expect("create workunits dir");
    fs::write(
        workunits.join("test_BAD_NO_CAPSULE.json"),
        r#"{
  "task_id": "test_BAD_NO_CAPSULE",
  "intent_ref": "intent://missing-capsule",
  "spec_refs": [],
  "state_refs": [],
  "proof_plan": ["validate_passes"],
  "proof_results": [
    {"gate":"validate_passes","status":"pass","artifact_ref":null}
  ],
  "status": "VERIFIED"
}"#,
    )
    .expect("write verified workunit missing capsule");

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        !validate.status.success(),
        "validate should fail for VERIFIED workunit without capsule lineage"
    );
    let stderr = combined_output(&validate);
    assert!(
        stderr.contains("WORKUNIT_CAPSULE_POLICY_LI"),
        "expected missing capsule lineage marker in stderr, got:\n{}",
        stderr
    );
}

#[test]
fn validate_fails_on_verified_workunit_capsule_without_state_ref_binding() {
    let (_tmp, dir, password) = setup_repo();
    let workunits = dir.join(".decapod").join("governance").join("workunits");
    let capsules = dir.join(".decapod").join("generated").join("context");
    fs::create_dir_all(&workunits).expect("create workunits dir");
    fs::create_dir_all(&capsules).expect("create context dir");

    let mut capsule = DeterministicContextCapsule {
        schema_version: "1.1.0".to_string(),
        topic: "lineage".to_string(),
        scope: "interfaces".to_string(),
        task_id: Some("test_BAD_STATE_REF".to_string()),
        workunit_id: None,
        sources: vec![ContextCapsuleSource {
            path: "interfaces/PLAN_GOVERNED_EXECUTION.md".to_string(),
            section: "Contract".to_string(),
        }],
        snippets: vec![ContextCapsuleSnippet {
            source_path: "interfaces/PLAN_GOVERNED_EXECUTION.md".to_string(),
            text: "promotion path is proof-gated".to_string(),
        }],
        policy: Default::default(),
        capsule_hash: String::new(),
    };
    capsule = capsule
        .with_recomputed_hash()
        .expect("recompute capsule hash");
    fs::write(
        capsules.join("test_BAD_STATE_REF.json"),
        serde_json::to_vec_pretty(&capsule).expect("serialize capsule"),
    )
    .expect("write capsule");

    fs::write(
        workunits.join("test_BAD_STATE_REF.json"),
        r#"{
  "task_id": "test_BAD_STATE_REF",
  "intent_ref": "intent://missing-state-ref",
  "spec_refs": [],
  "state_refs": [],
  "proof_plan": ["validate_passes"],
  "proof_results": [
    {"gate":"validate_passes","status":"pass","artifact_ref":null}
  ],
  "status": "VERIFIED"
}"#,
    )
    .expect("write verified workunit missing state_ref binding");

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        !validate.status.success(),
        "validate should fail for VERIFIED workunit without capsule state_ref binding"
    );
    let stderr = combined_output(&validate);
    assert!(
        stderr.contains("WORKUNIT_CAPSULE_POLICY_LI"),
        "expected missing capsule state_ref marker in stderr, got:\n{}",
        stderr
    );
}

#[test]
fn validate_fails_on_context_capsule_hash_mismatch_if_present() {
    let (_tmp, dir, password) = setup_repo();
    let capsules = dir.join(".decapod").join("generated").join("context");
    fs::create_dir_all(&capsules).expect("create capsules dir");

    let mut capsule = DeterministicContextCapsule {
        schema_version: "1.1.0".to_string(),
        topic: "phase0".to_string(),
        scope: "interfaces".to_string(),
        task_id: Some("test_1".to_string()),
        workunit_id: Some("test_1".to_string()),
        sources: vec![ContextCapsuleSource {
            path: "interfaces/CLAIMS.md".to_string(),
            section: "2. Claims".to_string(),
        }],
        snippets: vec![ContextCapsuleSnippet {
            source_path: "interfaces/CLAIMS.md".to_string(),
            text: "claim.context.capsule.deterministic".to_string(),
        }],
        policy: Default::default(),
        capsule_hash: String::new(),
    };
    capsule.capsule_hash = "wrong_hash".to_string();
    fs::write(
        capsules.join("test_1.json"),
        serde_json::to_vec_pretty(&capsule).expect("serialize capsule"),
    )
    .expect("write capsule");

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        !validate.status.success(),
        "validate should fail for capsule hash mismatch"
    );
    let stderr = combined_output(&validate);
    assert!(
        stderr.contains("Context capsule hash mismatch"),
        "expected context capsule hash mismatch failure in stderr, got:\n{}",
        stderr
    );
}

#[test]
fn validate_fails_on_invalid_knowledge_promotion_ledger_if_present() {
    let (_tmp, dir, password) = setup_repo();
    let data_dir = dir.join(".decapod").join("data");
    fs::create_dir_all(&data_dir).expect("create data dir");
    fs::write(
        data_dir.join("knowledge.promotions.jsonl"),
        "{\"event_id\":\"evt_1\"}\n",
    )
    .expect("write promotions ledger");

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        !validate.status.success(),
        "validate should fail for incomplete promotion ledger entries"
    );
    let stderr = combined_output(&validate);
    assert!(
        stderr.contains("Knowledge promotion ledger missing"),
        "expected promotion ledger schema failure in stderr, got:\n{}",
        stderr
    );
}

#[test]
fn validate_fails_on_non_procedural_target_class_in_promotion_ledger() {
    let (_tmp, dir, password) = setup_repo();
    let data_dir = dir.join(".decapod").join("data");
    fs::create_dir_all(&data_dir).expect("create data dir");
    fs::write(
        data_dir.join("knowledge.promotions.jsonl"),
        r#"{"event_id":"evt_2","ts":"1Z","source_entry_id":"K_1","target_class":"semantic","evidence_refs":["commit:abc123"],"approved_by":"human/reviewer","actor":"agent/test","reason":"bad class"}
"#,
    )
    .expect("write promotions ledger with invalid target class");

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        !validate.status.success(),
        "validate should fail for non-procedural target_class in promotion ledger"
    );
    let stderr = combined_output(&validate);
    assert!(
        stderr.contains("target_class='procedural'"),
        "expected target_class guard failure in stderr, got:\n{}",
        stderr
    );
}

#[test]
fn validate_fails_when_gitignore_missing_generated_whitelist_rules() {
    let (_tmp, dir, password) = setup_repo();
    let gitignore_path = dir.join(".gitignore");
    let content = fs::read_to_string(&gitignore_path).expect("read .gitignore");
    let content = content
        .lines()
        .filter(|line| line.trim() != "!.decapod/generated/context/*.json")
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&gitignore_path, format!("{}\n", content)).expect("rewrite .gitignore");

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        !validate.status.success(),
        "validate should fail when generated whitelist .gitignore rules are missing"
    );
    let stderr = combined_output(&validate);
    assert!(
        stderr.contains("Missing .gitignore rule '!.decapod/generated/context/*.json'"),
        "expected generated whitelist .gitignore failure, got:\n{}",
        stderr
    );
}

#[test]
fn validate_fails_when_non_whitelisted_generated_file_is_tracked() {
    let (_tmp, dir, password) = setup_repo();
    let rogue = dir.join(".decapod/generated/rogue.json");
    fs::create_dir_all(rogue.parent().expect("rogue parent")).expect("mkdir generated");
    fs::write(&rogue, "{}\n").expect("write rogue generated file");

    let add = Command::new("git")
        .current_dir(&dir)
        .args(["add", "-f", ".decapod/generated/rogue.json"])
        .output()
        .expect("git add rogue generated");
    assert!(
        add.status.success(),
        "forced git add should succeed: {}",
        String::from_utf8_lossy(&add.stderr)
    );

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        !validate.status.success(),
        "validate should fail when non-whitelisted generated file is tracked"
    );
    let stderr = combined_output(&validate);
    assert!(
        stderr.contains("Tracked non-whitelisted generated artifacts found"),
        "expected generated whitelist tracked-file failure, got:\n{}",
        stderr
    );
}

#[test]
fn validate_fails_on_invalid_context_capsule_policy_contract_if_present() {
    let (_tmp, dir, password) = setup_repo();
    let policy_path = dir
        .join(".decapod")
        .join("generated")
        .join("policy")
        .join("context_capsule_policy.json");
    let invalid = serde_json::json!({
        "schema_version": "1.0.0",
        "policy_version": "jit-capsule-policy-v1",
        "repo_revision_binding": "HEAD",
        "default_risk_tier": "medium",
        "tiers": {
            "medium": {
                "allowed_scopes": [],
                "max_limit": 6,
                "allow_write": true
            }
        }
    });
    fs::write(
        &policy_path,
        serde_json::to_vec_pretty(&invalid).expect("serialize invalid policy"),
    )
    .expect("write invalid policy");

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        !validate.status.success(),
        "validate should fail for invalid capsule policy contract"
    );
    let stderr = combined_output(&validate);
    assert!(
        stderr.contains("has no allowed_scopes"),
        "expected context capsule policy gate failure, got:\n{}",
        stderr
    );
}

#[test]
fn validate_fails_on_internalization_source_hash_drift_if_present() {
    let (_tmp, dir, password) = setup_repo();
    let doc_path = dir.join("doc.txt");
    fs::write(&doc_path, "version 1").expect("write source doc");

    let create = run_decapod(
        &dir,
        &[
            "internalize",
            "create",
            "--source",
            "doc.txt",
            "--model",
            "test-model",
            "--profile",
            "noop",
            "--format",
            "json",
        ],
        &[("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")],
    );
    assert!(
        create.status.success(),
        "create failed: {}",
        combined_output(&create)
    );

    fs::write(&doc_path, "version 2").expect("mutate source doc");

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(!validate.status.success());
    let stderr = combined_output(&validate);
    assert!(stderr.contains("Internalization source hash mismatch"));
}

#[test]
fn validate_fails_on_best_effort_internalization_claiming_replayable() {
    let (_tmp, dir, password) = setup_repo();
    let doc_path = dir.join("doc.txt");
    fs::write(&doc_path, "version 1").expect("write source doc");

    let create = run_decapod(
        &dir,
        &[
            "internalize",
            "create",
            "--source",
            "doc.txt",
            "--model",
            "test-model",
            "--profile",
            "noop",
            "--format",
            "json",
        ],
        &[("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")],
    );
    assert!(
        create.status.success(),
        "create failed: {}",
        combined_output(&create)
    );
    let created: serde_json::Value = serde_json::from_slice(&create.stdout).expect("create json");
    let artifact_id = created["artifact_id"].as_str().expect("artifact id");
    let manifest_path = dir
        .join(".decapod")
        .join("generated")
        .join("artifacts")
        .join("internalizations")
        .join(artifact_id)
        .join("manifest.json");
    let raw = fs::read_to_string(&manifest_path).expect("read manifest");
    let mut manifest: serde_json::Value = serde_json::from_str(&raw).expect("parse manifest");
    manifest["determinism_class"] = serde_json::Value::String("best_effort".to_string());
    manifest["replay_recipe"]["mode"] = serde_json::Value::String("replayable".to_string());
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("serialize manifest"),
    )
    .expect("write manifest");

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(!validate.status.success());
    let stderr = combined_output(&validate);
    assert!(
        stderr.contains("claims replayable despite non-deterministic profile")
            || stderr.contains("replay metadata is inconsistent")
    );
}
