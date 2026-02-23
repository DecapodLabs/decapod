use serde_json::Value;
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

fn setup_repo() -> (TempDir, PathBuf, String) {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path().to_path_buf();

    let git_init = Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(git_init.status.success(), "git init failed");

    let init = run_decapod(&dir, &["init", "--force"], &[]);
    assert!(
        init.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&init.stderr)
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
    let password = String::from_utf8_lossy(&acquire.stdout)
        .lines()
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .expect("password in session acquire output");

    (tmp, dir, password)
}

#[test]
fn phase4_kernel_surfaces_work_together() {
    let (_tmp, dir, password) = setup_repo();
    let auth = [
        ("DECAPOD_AGENT_ID", "unknown"),
        ("DECAPOD_SESSION_PASSWORD", password.as_str()),
        ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
    ];

    let init_workunit = run_decapod(
        &dir,
        &[
            "govern",
            "workunit",
            "init",
            "--task-id",
            "R_PHASE4",
            "--intent-ref",
            "intent://phase4",
        ],
        &auth,
    );
    assert!(
        init_workunit.status.success(),
        "workunit init failed: {}",
        String::from_utf8_lossy(&init_workunit.stderr)
    );

    let proof_plan = run_decapod(
        &dir,
        &[
            "govern",
            "workunit",
            "set-proof-plan",
            "--task-id",
            "R_PHASE4",
            "--gate",
            "validate_passes",
        ],
        &auth,
    );
    assert!(
        proof_plan.status.success(),
        "set-proof-plan failed: {}",
        String::from_utf8_lossy(&proof_plan.stderr)
    );

    for (gate, status) in [("validate_passes", "pass"), ("validate_passes", "pass")] {
        let out = run_decapod(
            &dir,
            &[
                "govern",
                "workunit",
                "record-proof",
                "--task-id",
                "R_PHASE4",
                "--gate",
                gate,
                "--status",
                status,
            ],
            &auth,
        );
        assert!(
            out.status.success(),
            "record-proof failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    for status in ["executing", "claimed", "verified"] {
        let out = run_decapod(
            &dir,
            &[
                "govern",
                "workunit",
                "transition",
                "--task-id",
                "R_PHASE4",
                "--to",
                status,
            ],
            &auth,
        );
        assert!(
            out.status.success(),
            "workunit transition {} failed: {}",
            status,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let capsule = run_decapod(
        &dir,
        &[
            "govern",
            "capsule",
            "query",
            "--topic",
            "proof gates",
            "--scope",
            "interfaces",
            "--task-id",
            "R_PHASE4",
            "--write",
        ],
        &auth,
    );
    assert!(
        capsule.status.success(),
        "capsule query failed: {}",
        String::from_utf8_lossy(&capsule.stderr)
    );
    let capsule_payload: Value = serde_json::from_slice(&capsule.stdout).expect("capsule payload");
    let capsule_path = capsule_payload["path"].as_str().expect("capsule path");
    assert!(Path::new(capsule_path).exists(), "capsule artifact missing");

    let promote = run_decapod(
        &dir,
        &[
            "data",
            "knowledge",
            "promote",
            "--source-entry-id",
            "K_PHASE4",
            "--evidence-ref",
            "commit:abc123",
            "--approved-by",
            "human/reviewer",
            "--reason",
            "phase4 regression",
        ],
        &auth,
    );
    assert!(
        promote.status.success(),
        "knowledge promote failed: {}",
        String::from_utf8_lossy(&promote.stderr)
    );
    let promote_payload: Value = serde_json::from_slice(&promote.stdout).expect("promote payload");
    let event_id = promote_payload["event_id"].as_str().expect("event id");

    let add_proc = run_decapod(
        &dir,
        &[
            "data",
            "knowledge",
            "add",
            "--id",
            "procedural/commit_norms/phase4",
            "--title",
            "Phase 4 Norm",
            "--text",
            "always prove before publish",
            "--provenance",
            &format!("event:{}", event_id),
        ],
        &auth,
    );
    assert!(
        add_proc.status.success(),
        "procedural knowledge add failed: {}",
        String::from_utf8_lossy(&add_proc.stderr)
    );

    let validate = run_decapod(&dir, &["validate"], &auth);
    assert!(
        validate.status.success(),
        "phase4 regression validate failed: {}",
        String::from_utf8_lossy(&validate.stderr)
    );
}
