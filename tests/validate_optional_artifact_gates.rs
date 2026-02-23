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
    fs::write(workunits.join("R_BAD.json"), "{not-json").expect("write malformed workunit");

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
fn validate_fails_on_context_capsule_hash_mismatch_if_present() {
    let (_tmp, dir, password) = setup_repo();
    let capsules = dir.join(".decapod").join("generated").join("context");
    fs::create_dir_all(&capsules).expect("create capsules dir");

    let mut capsule = DeterministicContextCapsule {
        topic: "phase0".to_string(),
        scope: "interfaces".to_string(),
        task_id: Some("R_1".to_string()),
        workunit_id: Some("R_1".to_string()),
        sources: vec![ContextCapsuleSource {
            path: "interfaces/CLAIMS.md".to_string(),
            section: "2. Claims".to_string(),
        }],
        snippets: vec![ContextCapsuleSnippet {
            source_path: "interfaces/CLAIMS.md".to_string(),
            text: "claim.context.capsule.deterministic".to_string(),
        }],
        capsule_hash: String::new(),
    };
    capsule.capsule_hash = "wrong_hash".to_string();
    fs::write(
        capsules.join("R_1.json"),
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
