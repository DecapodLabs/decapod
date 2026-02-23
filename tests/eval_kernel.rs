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
        &[("DECAPOD_AGENT_ID", "eval-agent")],
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

fn envs(password: &str) -> [(&str, &str); 2] {
    [
        ("DECAPOD_AGENT_ID", "eval-agent"),
        ("DECAPOD_SESSION_PASSWORD", password),
    ]
}

fn create_plan(dir: &Path, password: &str, prompt_hash: &str, runs: u32) -> String {
    let out = run_decapod(
        dir,
        &[
            "eval",
            "plan",
            "--task-set-id",
            "web-e2e-smoke",
            "--task-ref",
            "checkout-flow",
            "--task-ref",
            "profile-save",
            "--runs-per-variant",
            &runs.to_string(),
            "--model-id",
            "gpt-test",
            "--agent-version",
            "1.0.0",
            "--agent-id",
            "eval-agent",
            "--prompt-hash",
            prompt_hash,
            "--seed",
            "123",
            "--tool-version",
            "playwright=1.52.0",
            "--env",
            "browser=chromium-123",
            "--judge-model-id",
            "judge-1",
            "--judge-prompt-hash",
            "judge-hash-1",
            "--judge-timeout-ms",
            "2000",
        ],
        &envs(password),
    );
    assert!(
        out.status.success(),
        "eval plan failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: Value = serde_json::from_slice(&out.stdout).expect("parse plan payload");
    payload["plan_id"].as_str().expect("plan_id").to_string()
}

fn ingest_and_judge(
    dir: &Path,
    password: &str,
    plan_id: &str,
    run_id: &str,
    variant: &str,
    status: &str,
    failure_reason: Option<&str>,
) {
    let mut args = vec![
        "eval",
        "ingest-run",
        "--plan-id",
        plan_id,
        "--run-id",
        run_id,
        "--variant",
        variant,
        "--task-ref",
        "checkout-flow",
        "--attempt-index",
        "1",
        "--status",
        status,
        "--duration-ms",
        "1200",
    ];
    if let Some(reason) = failure_reason {
        args.extend(["--failure-reason", reason]);
    }
    let ingest = run_decapod(dir, &args, &envs(password));
    assert!(
        ingest.status.success(),
        "eval ingest-run failed: {}",
        String::from_utf8_lossy(&ingest.stderr)
    );

    let judge_json = if status == "pass" {
        r#"{"success":true,"explanation":"task completed","failure_reason":null,"reached_captcha":false,"impossible_task":false}"#
    } else {
        r#"{"success":false,"explanation":"selector timeout","failure_reason":"timeout","reached_captcha":false,"impossible_task":false}"#
    };
    let judge = run_decapod(
        dir,
        &[
            "eval",
            "judge",
            "--plan-id",
            plan_id,
            "--run-id",
            run_id,
            "--json",
            judge_json,
            "--timeout-ms",
            "2000",
        ],
        &envs(password),
    );
    assert!(
        judge.status.success(),
        "eval judge failed: {}",
        String::from_utf8_lossy(&judge.stderr)
    );
}

#[test]
fn eval_golden_vector_is_deterministic_and_gate_decision_stable() {
    let (_tmp, dir, password) = setup_repo();
    let plan_id = create_plan(&dir, &password, "prompt-hash-a", 5);

    for (run_id, variant, status) in [
        ("R_B1", "baseline", "pass"),
        ("R_B2", "baseline", "pass"),
        ("R_B3", "baseline", "fail"),
        ("R_B4", "baseline", "fail"),
        ("R_B5", "baseline", "pass"),
        ("R_C1", "candidate", "pass"),
        ("R_C2", "candidate", "pass"),
        ("R_C3", "candidate", "pass"),
        ("R_C4", "candidate", "fail"),
        ("R_C5", "candidate", "pass"),
    ] {
        ingest_and_judge(
            &dir,
            &password,
            &plan_id,
            run_id,
            variant,
            status,
            if status == "fail" {
                Some("timeout")
            } else {
                None
            },
        );
    }

    let a1 = run_decapod(
        &dir,
        &[
            "eval",
            "aggregate",
            "--plan-id",
            &plan_id,
            "--baseline-variant",
            "baseline",
            "--candidate-variant",
            "candidate",
            "--iterations",
            "300",
            "--aggregate-id",
            "A_GOLDEN_1",
        ],
        &envs(&password),
    );
    assert!(a1.status.success(), "aggregate #1 failed");
    let p1: Value = serde_json::from_slice(&a1.stdout).expect("aggregate payload #1");

    let a2 = run_decapod(
        &dir,
        &[
            "eval",
            "aggregate",
            "--plan-id",
            &plan_id,
            "--baseline-variant",
            "baseline",
            "--candidate-variant",
            "candidate",
            "--iterations",
            "300",
            "--aggregate-id",
            "A_GOLDEN_2",
        ],
        &envs(&password),
    );
    assert!(a2.status.success(), "aggregate #2 failed");
    let p2: Value = serde_json::from_slice(&a2.stdout).expect("aggregate payload #2");

    assert_eq!(p1["delta_success_rate"], p2["delta_success_rate"]);
    assert_eq!(p1["ci"], p2["ci"]);

    let gate = run_decapod(
        &dir,
        &[
            "eval",
            "gate",
            "--aggregate-id",
            "A_GOLDEN_1",
            "--min-runs",
            "5",
            "--max-regression",
            "0.0",
        ],
        &envs(&password),
    );
    assert!(
        gate.status.success(),
        "eval gate should pass for golden vector; stderr: {}",
        String::from_utf8_lossy(&gate.stderr)
    );
}

#[test]
fn eval_judge_contract_rejects_malformed_json() {
    let (_tmp, dir, password) = setup_repo();
    let plan_id = create_plan(&dir, &password, "prompt-hash-a", 1);

    let ingest = run_decapod(
        &dir,
        &[
            "eval",
            "ingest-run",
            "--plan-id",
            &plan_id,
            "--run-id",
            "R_BAD_JUDGE",
            "--variant",
            "baseline",
            "--task-ref",
            "checkout-flow",
            "--attempt-index",
            "1",
            "--status",
            "pass",
        ],
        &envs(&password),
    );
    assert!(ingest.status.success());

    let judge = run_decapod(
        &dir,
        &[
            "eval",
            "judge",
            "--plan-id",
            &plan_id,
            "--run-id",
            "R_BAD_JUDGE",
            "--json",
            r#"{"success":true}"#,
        ],
        &envs(&password),
    );
    assert!(
        !judge.status.success(),
        "judge should fail malformed contract"
    );
    let stderr = String::from_utf8_lossy(&judge.stderr);
    assert!(
        stderr.contains("EVAL_JUDGE_JSON_CONTRACT_ERROR"),
        "expected contract marker, got: {}",
        stderr
    );
}

#[test]
fn eval_judge_timeout_is_typed_and_gate_blocks_progress() {
    let (_tmp, dir, password) = setup_repo();
    let plan_id = create_plan(&dir, &password, "prompt-hash-a", 1);

    let ingest = run_decapod(
        &dir,
        &[
            "eval",
            "ingest-run",
            "--plan-id",
            &plan_id,
            "--run-id",
            "R_TIMEOUT",
            "--variant",
            "baseline",
            "--task-ref",
            "checkout-flow",
            "--attempt-index",
            "1",
            "--status",
            "fail",
            "--failure-reason",
            "timeout",
        ],
        &envs(&password),
    );
    assert!(ingest.status.success());

    let judge = run_decapod(
        &dir,
        &[
            "eval",
            "judge",
            "--plan-id",
            &plan_id,
            "--run-id",
            "R_TIMEOUT",
            "--json",
            r#"{"success":false,"explanation":"timed","failure_reason":"timeout","reached_captcha":false,"impossible_task":false}"#,
            "--timeout-ms",
            "10",
            "--simulate-delay-ms",
            "50",
        ],
        &envs(&password),
    );
    assert!(!judge.status.success(), "judge should timeout");
    let stderr = String::from_utf8_lossy(&judge.stderr);
    assert!(stderr.contains("EVAL_JUDGE_TIMEOUT"), "stderr: {}", stderr);

    let aggregate = run_decapod(
        &dir,
        &[
            "eval",
            "aggregate",
            "--plan-id",
            &plan_id,
            "--baseline-variant",
            "baseline",
            "--candidate-variant",
            "baseline",
            "--iterations",
            "100",
            "--aggregate-id",
            "A_TIMEOUT",
        ],
        &envs(&password),
    );
    assert!(
        !aggregate.status.success(),
        "aggregate should fail without judged runs"
    );
}

#[test]
fn eval_plan_hash_changes_on_settings_change_and_cross_plan_compare_requires_ack() {
    let (_tmp, dir, password) = setup_repo();
    let plan_a = create_plan(&dir, &password, "prompt-hash-a", 1);
    let plan_b = create_plan(&dir, &password, "prompt-hash-b", 1);
    assert_ne!(plan_a, plan_b, "plan ids must differ when settings differ");

    ingest_and_judge(
        &dir, &password, &plan_a, "R_A_BASE", "baseline", "pass", None,
    );
    ingest_and_judge(
        &dir,
        &password,
        &plan_a,
        "R_A_CAND",
        "candidate",
        "pass",
        None,
    );

    let agg_a = run_decapod(
        &dir,
        &[
            "eval",
            "aggregate",
            "--plan-id",
            &plan_a,
            "--baseline-variant",
            "baseline",
            "--candidate-variant",
            "candidate",
            "--iterations",
            "100",
            "--aggregate-id",
            "A_PLAN_A",
        ],
        &envs(&password),
    );
    assert!(agg_a.status.success(), "aggregate plan A failed");

    ingest_and_judge(
        &dir, &password, &plan_b, "R_B_BASE", "baseline", "pass", None,
    );
    ingest_and_judge(
        &dir,
        &password,
        &plan_b,
        "R_B_CAND",
        "candidate",
        "pass",
        None,
    );

    let mismatch = run_decapod(
        &dir,
        &[
            "eval",
            "aggregate",
            "--plan-id",
            &plan_b,
            "--baseline-variant",
            "baseline",
            "--candidate-variant",
            "candidate",
            "--iterations",
            "100",
            "--aggregate-id",
            "A_PLAN_B_NOACK",
            "--baseline-aggregate-id",
            "A_PLAN_A",
        ],
        &envs(&password),
    );
    assert!(
        !mismatch.status.success(),
        "cross-plan compare should fail without acknowledge"
    );
    assert!(
        String::from_utf8_lossy(&mismatch.stderr).contains("EVAL_SETTINGS_MISMATCH"),
        "expected settings mismatch marker"
    );

    let acked = run_decapod(
        &dir,
        &[
            "eval",
            "aggregate",
            "--plan-id",
            &plan_b,
            "--baseline-variant",
            "baseline",
            "--candidate-variant",
            "candidate",
            "--iterations",
            "100",
            "--aggregate-id",
            "A_PLAN_B_ACK",
            "--baseline-aggregate-id",
            "A_PLAN_A",
            "--acknowledge-setting-drift",
        ],
        &envs(&password),
    );
    assert!(acked.status.success(), "acknowledged compare should pass");
}
