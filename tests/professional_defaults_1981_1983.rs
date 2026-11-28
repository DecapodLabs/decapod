//! End-to-end outcome-driven professional responsibility, behavior-relevant proof,
//! authorized delivery endpoint progression, and cold-continuation tests
//! (`#1981`, `#1982`, `#1983`, `#1367`).

use decapod::core::professional_defaults::{
    DeliveryEndpointKind, DeliveryPhaseStatus, DutyEvidenceKind,
};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

fn decapod_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_decapod"))
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git command");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn decapod(
    xdg_root: &Path,
    dir: &Path,
    args: &[&str],
    agent: &str,
    password: Option<&str>,
) -> std::process::Output {
    let mut cmd = Command::new(decapod_bin());
    cmd.current_dir(dir)
        .args(args)
        .env("XDG_CONFIG_HOME", xdg_root.join(".xdg-cfg"))
        .env("XDG_DATA_HOME", xdg_root.join(".xdg-data"))
        .env("DECAPOD_AGENT_ID", agent)
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env_remove("DECAPOD_SESSION_PASSWORD");
    if let Some(pw) = password {
        cmd.env("DECAPOD_SESSION_PASSWORD", pw);
    }
    cmd.output().expect("run decapod")
}

fn acquire_session(xdg_root: &Path, dir: &Path, agent: &str) -> String {
    let out = decapod(xdg_root, dir, &["session", "acquire"], agent, None);
    assert!(
        out.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("Password: "))
        .unwrap_or_else(|| panic!("missing Password line in session acquire stdout:\n{stdout}"))
        .trim()
        .to_string()
}

fn run_json(xdg_root: &Path, dir: &Path, args: &[&str], agent: &str, pw: Option<&str>) -> Value {
    let out = decapod(xdg_root, dir, args, agent, pw);
    assert!(
        out.status.success(),
        "decapod {:?} failed: stdout={} stderr={}",
        args,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("json stdout")
}

fn inject_governance_toml(cfg_path: &Path, kv_lines: &str) {
    let existing = fs::read_to_string(cfg_path).expect("read config.toml");
    let updated = if existing.contains("[governance]") {
        existing.replacen("[governance]", &format!("[governance]\n{kv_lines}"), 1)
    } else {
        format!("{existing}\n[governance]\n{kv_lines}\n")
    };
    fs::write(cfg_path, updated).expect("write config.toml");
}

fn setup_repo_with_worktree(
    governance_kv_toml: Option<&str>,
    workflows: bool,
    task_title: &str,
    task_desc: Option<&str>,
) -> (
    tempfile::TempDir,
    tempfile::TempDir,
    PathBuf,
    PathBuf,
    String,
    String,
) {
    let dir = tempdir().expect("repo dir");
    let xdg = tempdir().expect("xdg dir");
    let root = dir.path().to_path_buf();

    git(&root, &["init", "-b", "master"]);
    git(&root, &["config", "user.email", "test@example.com"]);
    git(&root, &["config", "user.name", "Test"]);
    fs::create_dir_all(root.join("src")).expect("src dir");
    fs::write(root.join("README.md"), "# Test Repo\n").expect("readme");
    fs::write(root.join("src/lib.rs"), "pub fn ok() -> bool { true }\n").expect("src");
    if workflows {
        fs::create_dir_all(root.join(".github/workflows")).expect("workflows dir");
        fs::write(
            root.join(".github/workflows/ci.yml"),
            "name: ci\non: [pull_request]\n",
        )
        .expect("workflow");
    }
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-m", "initial"]);

    let init_out = decapod(xdg.path(), &root, &["init", "--force"], "agent-main", None);
    assert!(
        init_out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&init_out.stderr)
    );
    if let Some(cfg) = governance_kv_toml {
        inject_governance_toml(&root.join(".decapod/config.toml"), cfg);
    }
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-m", "decapod init"]);

    let pw = acquire_session(xdg.path(), &root, "agent-main");
    let mut todo_args = vec!["todo", "add", task_title, "--format", "json"];
    if let Some(d) = task_desc {
        todo_args.extend(["--description", d]);
    }
    let added = run_json(xdg.path(), &root, &todo_args, "agent-main", Some(&pw));
    let task_id = added["id"].as_str().expect("todo id").to_string();

    let claim_out = decapod(
        xdg.path(),
        &root,
        &["todo", "claim", "--id", &task_id],
        "agent-main",
        Some(&pw),
    );
    assert!(
        claim_out.status.success(),
        "todo claim failed: {}",
        String::from_utf8_lossy(&claim_out.stderr)
    );

    let branch = format!("agent/agent-main/{task_id}-prof-defaults");
    let ws = run_json(
        xdg.path(),
        &root,
        &["workspace", "ensure", "--branch", &branch],
        "agent-main",
        Some(&pw),
    );
    let wt_str = ws["worktree_path"]
        .as_str()
        .or_else(|| ws["git"]["worktree_path"].as_str())
        .unwrap_or_else(|| panic!("missing worktree_path in workspace ensure output: {ws}"));
    let wt_path = PathBuf::from(wt_str);
    let wt_pw = acquire_session(xdg.path(), &wt_path, "agent-main");
    (dir, xdg, root, wt_path, task_id, wt_pw)
}

#[test]
fn issue_1981_terse_request_parity_and_lossless_request_preservation() {
    let (_d1, xdg1, _root1, wt1, task1, pw1) = setup_repo_with_worktree(
        None,
        true,
        "roll the dice on the agy tui ux",
        Some("Improve terminal onboarding UX across cold starts"),
    );
    let (_d2, xdg2, _root2, wt2, task2, pw2) = setup_repo_with_worktree(
        None,
        true,
        "Audit the current AGY terminal onboarding flow from a new user's perspective and leave it meaningfully clearer",
        Some("Improve terminal onboarding UX across cold starts"),
    );

    let pkt1 = run_json(
        xdg1.path(),
        &wt1,
        &[
            "infer",
            "orientation",
            "--task-id",
            &task1,
            "--intent",
            "roll the dice on the agy tui ux",
            "--clarification",
            "Focus on keyboard discovery and clean terminal state",
            "--format",
            "json",
        ],
        "agent-main",
        Some(&pw1),
    );
    let pkt2 = run_json(
        xdg2.path(),
        &wt2,
        &[
            "infer",
            "orientation",
            "--task-id",
            &task2,
            "--intent",
            "Audit the current AGY terminal onboarding flow from a new user's perspective and leave it meaningfully clearer",
            "--clarification",
            "Focus on keyboard discovery and clean terminal state",
            "--format",
            "json",
        ],
        "agent-main",
        Some(&pw2),
    );

    let duties1 = pkt1["derived_duties"].as_array().expect("duties1");
    let duties2 = pkt2["derived_duties"].as_array().expect("duties2");
    assert!(
        duties1.len() > 5,
        "expected >5 duties for terse request, got {}",
        duties1.len()
    );
    assert!(
        duties2.len() > 5,
        "expected >5 duties for keywordless phrasing, got {}",
        duties2.len()
    );

    // Neither phrasing mentions "fix", "bug", or "test", yet both infer Engineering/QA + baseline control + Design/UX + Docs.
    let funcs1: Vec<&str> = duties1
        .iter()
        .filter_map(|d| d["function"].as_str())
        .collect();
    let funcs2: Vec<&str> = duties2
        .iter()
        .filter_map(|d| d["function"].as_str())
        .collect();
    assert!(funcs1.contains(&"engineering_qa"));
    assert!(funcs1.contains(&"design_ux"));
    assert!(funcs1.contains(&"docs_support"));
    assert_eq!(
        funcs1, funcs2,
        "keywordless equivalent request must infer parity duties"
    );

    // Lossless composition retains intent + description + clarification.
    let orig = pkt1["original_request"].as_str().expect("original_request");
    assert!(orig.contains("roll the dice on the agy tui ux"));
    assert!(orig.contains("Improve terminal onboarding UX across cold starts"));
    assert!(orig.contains("Focus on keyboard discovery and clean terminal state"));
    assert_eq!(
        pkt1["delivery_endpoint"]["endpoint"],
        serde_json::to_value(DeliveryEndpointKind::PullRequest).unwrap()
    );

    // Initialize governed plan with --derive-duties and verify >5 duties survive context.resolve & plan status without truncation.
    let plan = run_json(
        xdg1.path(),
        &wt1,
        &[
            "govern",
            "plan",
            "init",
            "--title",
            "AGY TUI UX upgrade",
            "--intent",
            "roll the dice on the agy tui ux",
            "--todo-id",
            &task1,
            "--original-request",
            "roll the dice on the agy tui ux",
            "--clarification",
            "Focus on keyboard discovery and clean terminal state",
            "--derive-duties",
        ],
        "agent-main",
        Some(&pw1),
    );
    let plan_duties = plan["derived_duties"].as_array().expect("plan duties");
    assert_eq!(plan_duties.len(), duties1.len());
    assert!(plan_duties.len() > 5);

    let rpc_out = decapod(
        xdg1.path(),
        &wt1,
        &[
            "rpc",
            "--op",
            "context.resolve",
            "--params",
            &format!("{{\"task_id\":\"{task1}\",\"limit\":2}}"),
        ],
        "agent-main",
        Some(&pw1),
    );
    assert!(
        rpc_out.status.success(),
        "context.resolve failed: {}",
        String::from_utf8_lossy(&rpc_out.stderr)
    );
    let rpc_json: Value = serde_json::from_slice(&rpc_out.stdout).expect("rpc json");
    let ctx_duties = rpc_json["result"]["governed_plan"]["derived_duties"]
        .as_array()
        .expect("context.resolve derived_duties");
    assert_eq!(
        ctx_duties.len(),
        plan_duties.len(),
        "context.resolve must not truncate >5 derived duties"
    );
}

#[test]
fn issue_1981_non_code_commercial_and_proportionate_scoping() {
    let (_d_comm, xdg_comm, _r_comm, wt_comm, task_comm, pw_comm) = setup_repo_with_worktree(
        None,
        false,
        "make paid team signup usable",
        Some("Checkout and seat activation fail silently"),
    );
    let comm_pkt = run_json(
        xdg_comm.path(),
        &wt_comm,
        &[
            "infer",
            "orientation",
            "--task-id",
            &task_comm,
            "--format",
            "json",
        ],
        "agent-main",
        Some(&pw_comm),
    );
    let comm_funcs: Vec<&str> = comm_pkt["derived_duties"]
        .as_array()
        .expect("comm duties")
        .iter()
        .filter_map(|d| d["function"].as_str())
        .collect();
    assert!(comm_funcs.contains(&"commercial_org"));
    assert!(comm_funcs.contains(&"security_privacy_data_ops"));
    assert!(comm_funcs.contains(&"engineering_qa"));

    let (_d_biz, xdg_biz, _r_biz, wt_biz, task_biz, pw_biz) = setup_repo_with_worktree(
        None,
        false,
        "Update investor FAQ pricing summary for board deck",
        None,
    );
    let biz_pkt = run_json(
        xdg_biz.path(),
        &wt_biz,
        &[
            "infer",
            "orientation",
            "--task-id",
            &task_biz,
            "--affected-path",
            "docs/investor-faq.md",
            "--format",
            "json",
        ],
        "agent-main",
        Some(&pw_biz),
    );
    let biz_duties = biz_pkt["derived_duties"].as_array().expect("biz duties");
    assert!(!biz_duties.is_empty());
    assert!(
        biz_duties.iter().all(|d| {
            let ek: DutyEvidenceKind =
                serde_json::from_value(d["evidence_kind"].clone()).expect("evidence kind");
            !matches!(ek, DutyEvidenceKind::FailingBaselineControl)
        }),
        "non-code investor FAQ deliverable must not demand software baseline test controls"
    );

    let (_d_prose, xdg_prose, _r_prose, wt_prose, task_prose, pw_prose) =
        setup_repo_with_worktree(None, false, "Fix spelling typo in README.md", None);
    let prose_pkt = run_json(
        xdg_prose.path(),
        &wt_prose,
        &[
            "infer",
            "orientation",
            "--task-id",
            &task_prose,
            "--affected-path",
            "README.md",
            "--format",
            "json",
        ],
        "agent-main",
        Some(&pw_prose),
    );
    let prose_duties = prose_pkt["derived_duties"]
        .as_array()
        .expect("prose duties");
    assert!(
        prose_duties.len() <= 3,
        "spelling typo fix should stay proportionate (got {})",
        prose_duties.len()
    );
}

#[test]
fn issue_1982_flagship_green_tests_broken_ux_vacuous_and_stale_falsifiers() {
    let (_d, xdg, _root, wt, task_id, pw) = setup_repo_with_worktree(
        None,
        true,
        "roll the dice on the agy tui ux",
        Some("Fix empty-state onboarding and keyboard hint regression"),
    );

    let init_plan = run_json(
        xdg.path(),
        &wt,
        &[
            "govern",
            "plan",
            "init",
            "--title",
            "AGY TUI UX upgrade",
            "--intent",
            "roll the dice on the agy tui ux",
            "--todo-id",
            &task_id,
            "--original-request",
            "roll the dice on the agy tui ux",
            "--derive-duties",
        ],
        "agent-main",
        Some(&pw),
    );
    let duties = init_plan["derived_duties"].as_array().expect("duties");
    assert!(duties.len() > 5);

    // Reject dropping explicit request or relabeling required -> optional or empty N/A.
    let explicit_id = duties[0]["duty_id"].as_str().unwrap();
    let req_drop = decapod(
        xdg.path(),
        &wt,
        &[
            "govern",
            "plan",
            "refine-duty",
            "--duty-id",
            explicit_id,
            "--disposition",
            "non-applicable-with-rationale",
            "--exclusion-rationale",
            "ignore",
            "--source-backed-rationale",
        ],
        "agent-main",
        Some(&pw),
    );
    assert!(!req_drop.status.success());

    let qa_duty_id = duties
        .iter()
        .find(|d| {
            d["function"] == "engineering_qa" && d["evidence_kind"] == "failing_baseline_control"
        })
        .expect("baseline control duty")["duty_id"]
        .as_str()
        .unwrap()
        .to_string();
    let opt_drop = decapod(
        xdg.path(),
        &wt,
        &[
            "govern",
            "plan",
            "refine-duty",
            "--duty-id",
            &qa_duty_id,
            "--disposition",
            "planned",
            "--requested-required-flag",
            "false",
        ],
        "agent-main",
        Some(&pw),
    );
    assert!(
        !opt_drop.status.success(),
        "relabeling required duty as optional must fail"
    );

    // Add a single delivery phase, then approve plan and enter phase.
    run_json(
        xdg.path(),
        &wt,
        &[
            "govern",
            "plan",
            "phase-add",
            "--id",
            "deliver",
            "--name",
            "Deliver",
            "--description",
            "Execute and verify outcome",
        ],
        "agent-main",
        Some(&pw),
    );
    run_json(
        xdg.path(),
        &wt,
        &["govern", "plan", "approve"],
        "agent-main",
        Some(&pw),
    );
    run_json(
        xdg.path(),
        &wt,
        &["govern", "plan", "enter-phase", "--phase-id", "deliver"],
        "agent-main",
        Some(&pw),
    );

    let head1 = git(&wt, &["rev-parse", "HEAD"]);
    let target = git(&wt, &["rev-parse", "master"]);

    // Refine all duties EXCEPT mark qa_duty_id as "existing suite stayed green while requested UX stayed uncovered".
    for d in duties {
        let did = d["duty_id"].as_str().unwrap();
        let covered = if did == qa_duty_id { "false" } else { "true" };
        let mut args = vec![
            "govern",
            "plan",
            "refine-duty",
            "--duty-id",
            did,
            "--disposition",
            "satisfied",
            "--check-executed",
            "--check-passed",
            "--observed-execution",
            "--negative-control-verified",
            "--evidence-ref",
            "artifacts/ux-test-report.json",
            "--collected-test-count",
            "3",
            "--assertion-count",
            "7",
            "--subject-commit",
            &head1,
            "--target-commit",
            &target,
        ];
        if covered == "true" {
            args.push("--requested-behavior-covered");
        }
        run_json(xdg.path(), &wt, &args, "agent-main", Some(&pw));
    }

    // "Green tests, broken UX": verify-coverage and exit-phase MUST fail with REQUESTED_BEHAVIOR_UNCOVERED!
    let cov_fail = decapod(
        xdg.path(),
        &wt,
        &["govern", "plan", "verify-coverage"],
        "agent-main",
        Some(&pw),
    );
    assert!(!cov_fail.status.success());
    assert!(
        String::from_utf8_lossy(&cov_fail.stderr).contains("REQUESTED_BEHAVIOR_UNCOVERED")
            || String::from_utf8_lossy(&cov_fail.stdout).contains("REQUESTED_BEHAVIOR_UNCOVERED")
    );
    let exit_fail = decapod(
        xdg.path(),
        &wt,
        &["govern", "plan", "exit-phase", "--phase-id", "deliver"],
        "agent-main",
        Some(&pw),
    );
    assert!(
        !exit_fail.status.success(),
        "exit-phase must block DONE when requested behavior is uncovered"
    );

    // Vacuous 0-collected-test falsifier:
    run_json(
        xdg.path(),
        &wt,
        &[
            "govern",
            "plan",
            "refine-duty",
            "--duty-id",
            &qa_duty_id,
            "--disposition",
            "satisfied",
            "--check-executed",
            "--check-passed",
            "--requested-behavior-covered",
            "--observed-execution",
            "--negative-control-verified",
            "--evidence-ref",
            "artifacts/empty-run.json",
            "--collected-test-count",
            "0",
            "--assertion-count",
            "0",
            "--subject-commit",
            &head1,
            "--target-commit",
            &target,
        ],
        "agent-main",
        Some(&pw),
    );
    let vac_fail = decapod(
        xdg.path(),
        &wt,
        &["govern", "plan", "verify-coverage"],
        "agent-main",
        Some(&pw),
    );
    assert!(!vac_fail.status.success());
    assert!(String::from_utf8_lossy(&vac_fail.stderr).contains("VACUOUS_OR_EMPTY_CHECK"));

    // Satisfy qa_duty_id properly against head1, then advance git HEAD to commit2 to prove STALE_DUTY_EVIDENCE!
    run_json(
        xdg.path(),
        &wt,
        &[
            "govern",
            "plan",
            "refine-duty",
            "--duty-id",
            &qa_duty_id,
            "--disposition",
            "satisfied",
            "--check-executed",
            "--check-passed",
            "--requested-behavior-covered",
            "--observed-execution",
            "--negative-control-verified",
            "--evidence-ref",
            "artifacts/ux-real-run.json",
            "--collected-test-count",
            "2",
            "--assertion-count",
            "5",
            "--subject-commit",
            &head1,
            "--target-commit",
            &target,
        ],
        "agent-main",
        Some(&pw),
    );
    fs::write(
        wt.join("src/lib.rs"),
        "pub fn ok() -> bool { true }\npub fn tui_ux() -> &'static str { \"clear\" }\n",
    )
    .unwrap();
    git(&wt, &["add", "src/lib.rs"]);
    git(&wt, &["commit", "-m", "impl tui ux"]);
    let head2 = git(&wt, &["rev-parse", "HEAD"]);
    assert_ne!(head1, head2);

    let stale_fail = decapod(
        xdg.path(),
        &wt,
        &["govern", "plan", "verify-coverage"],
        "agent-main",
        Some(&pw),
    );
    assert!(!stale_fail.status.success());
    assert!(
        String::from_utf8_lossy(&stale_fail.stderr).contains("STALE_DUTY_EVIDENCE")
            || String::from_utf8_lossy(&stale_fail.stdout).contains("STALE_DUTY_EVIDENCE")
    );

    // Refresh all duties for head2 -> verify-coverage passes and exit-phase reaches DONE.
    for d in duties {
        let did = d["duty_id"].as_str().unwrap();
        run_json(
            xdg.path(),
            &wt,
            &[
                "govern",
                "plan",
                "refine-duty",
                "--duty-id",
                did,
                "--disposition",
                "satisfied",
                "--check-executed",
                "--check-passed",
                "--requested-behavior-covered",
                "--observed-execution",
                "--negative-control-verified",
                "--evidence-ref",
                "artifacts/ux-head2-run.json",
                "--collected-test-count",
                "2",
                "--assertion-count",
                "5",
                "--subject-commit",
                &head2,
                "--target-commit",
                &target,
            ],
            "agent-main",
            Some(&pw),
        );
    }
    let ok_cov = run_json(
        xdg.path(),
        &wt,
        &["govern", "plan", "verify-coverage"],
        "agent-main",
        Some(&pw),
    );
    assert_eq!(ok_cov["ready"], Value::Bool(true));

    let done_plan = run_json(
        xdg.path(),
        &wt,
        &["govern", "plan", "exit-phase", "--phase-id", "deliver"],
        "agent-main",
        Some(&pw),
    );
    assert_eq!(done_plan["state"], "DONE");
}

#[test]
fn issue_1983_and_1367_standing_delivery_elevation_rejection_and_cold_continuation() {
    // 1) Repo with PR workflow + accepted standing_pr_merge = true delegates PullRequestThenMerge autonomously.
    let (_d_pr, xdg_pr, _r_pr, wt_pr, task_pr, pw_pr) = setup_repo_with_worktree(
        Some("standing_pr_merge = true"),
        true,
        "roll the dice on the agy tui ux",
        Some("Deliver cohesive onboarding improvement"),
    );

    let plan_pr = run_json(
        xdg_pr.path(),
        &wt_pr,
        &[
            "govern",
            "plan",
            "init",
            "--title",
            "AGY TUI UX",
            "--intent",
            "roll the dice on the agy tui ux",
            "--todo-id",
            &task_pr,
            "--original-request",
            "roll the dice on the agy tui ux",
            "--derive-duties",
        ],
        "agent-main",
        Some(&pw_pr),
    );
    assert_eq!(
        plan_pr["delivery_endpoint"]["endpoint"],
        serde_json::to_value(DeliveryEndpointKind::PullRequestThenMerge).unwrap()
    );
    let duties = plan_pr["derived_duties"].as_array().expect("duties");
    assert!(duties.len() > 5);

    let head = git(&wt_pr, &["rev-parse", "HEAD"]);
    let target = git(&wt_pr, &["rev-parse", "master"]);
    for d in duties {
        let did = d["duty_id"].as_str().unwrap();
        run_json(
            xdg_pr.path(),
            &wt_pr,
            &[
                "govern",
                "plan",
                "refine-duty",
                "--duty-id",
                did,
                "--disposition",
                "satisfied",
                "--check-executed",
                "--check-passed",
                "--requested-behavior-covered",
                "--observed-execution",
                "--negative-control-verified",
                "--evidence-ref",
                "artifacts/pr-verified.json",
                "--collected-test-count",
                "3",
                "--assertion-count",
                "6",
                "--subject-commit",
                &head,
                "--target-commit",
                &target,
            ],
            "agent-main",
            Some(&pw_pr),
        );
    }

    // Advance through PrOpened -> UnknownPendingReadback -> Merged without redundant follow-up prompt.
    let d_open = run_json(
        xdg_pr.path(),
        &wt_pr,
        &[
            "govern",
            "plan",
            "advance-delivery",
            "--phase-status",
            "pr-opened",
            "--pr-number",
            "42",
            "--pr-checks-green",
            "--pr-reviews-satisfied",
            "--operation-id",
            "op-pr-42",
            "--full-validation-passed",
        ],
        "agent-main",
        Some(&pw_pr),
    );
    assert_eq!(d_open["requires_human_followup_prompt"], Value::Bool(false));
    assert_eq!(d_open["permitted_to_advance"], Value::Bool(true));

    run_json(
        xdg_pr.path(),
        &wt_pr,
        &["govern", "plan", "approve"],
        "agent-main",
        Some(&pw_pr),
    );

    // Create a workunit checkpoint while interrupted at PrOpened (#1367 cold continuation).
    let gov_doc: Value = serde_json::from_str(
        &fs::read_to_string(wt_pr.join(".decapod/governance.json")).expect("read gov doc"),
    )
    .expect("parse gov doc");
    let traj_intent_id = gov_doc["sections"]["trajectory"]["intent_id"]
        .as_str()
        .expect("trajectory intent_id")
        .to_string();
    run_json(
        xdg_pr.path(),
        &wt_pr,
        &[
            "govern",
            "workunit",
            "init",
            "--task-id",
            &task_pr,
            "--intent-ref",
            &traj_intent_id,
        ],
        "agent-main",
        Some(&pw_pr),
    );
    run_json(
        xdg_pr.path(),
        &wt_pr,
        &[
            "govern",
            "workunit",
            "checkpoint",
            "--task-id",
            &task_pr,
            "--operation-id",
            "ckpt-cold-1",
        ],
        "agent-main",
        Some(&pw_pr),
    );

    // Cold second run (brand-new empty XDG dir, zero prior session state) acquires a session
    // in the same worktree and inspects workunit status: must recover >5 derived duties AND PullRequestThenMerge + PrOpened!
    let xdg_cold = tempdir().expect("cold xdg dir");
    let cold_pw = acquire_session(xdg_cold.path(), &wt_pr, "agent-main");
    let wu_status = run_json(
        xdg_cold.path(),
        &wt_pr,
        &["govern", "workunit", "status", "--task-id", &task_pr],
        "agent-main",
        Some(&cold_pw),
    );
    assert!(
        wu_status["derived_duties"]
            .as_array()
            .expect("wu duties")
            .len()
            > 5,
        "cold workunit status must preserve all >5 derived duties"
    );
    assert_eq!(
        wu_status["delivery_endpoint"]["endpoint"],
        serde_json::to_value(DeliveryEndpointKind::PullRequestThenMerge).unwrap()
    );
    assert_eq!(
        wu_status["delivery_phase"],
        serde_json::to_value(DeliveryPhaseStatus::PrOpened).unwrap()
    );

    let d_unknown = run_json(
        xdg_cold.path(),
        &wt_pr,
        &[
            "govern",
            "plan",
            "advance-delivery",
            "--phase-status",
            "unknown-pending-readback",
            "--pr-number",
            "42",
            "--pr-checks-green",
            "--pr-reviews-satisfied",
            "--operation-id",
            "op-pr-42",
            "--full-validation-passed",
        ],
        "agent-main",
        Some(&cold_pw),
    );
    assert_eq!(d_unknown["must_query_remote_first"], Value::Bool(true));
    assert_eq!(d_unknown["reuse_operation_id"], "op-pr-42");
    assert_eq!(d_unknown["reuse_pr_number"], 42);

    let d_merged = run_json(
        xdg_cold.path(),
        &wt_pr,
        &[
            "govern",
            "plan",
            "advance-delivery",
            "--phase-status",
            "merged",
            "--pr-number",
            "42",
            "--pr-checks-green",
            "--pr-reviews-satisfied",
            "--operation-id",
            "op-pr-42",
            "--forge-merge-observed",
            "true",
            "--full-validation-passed",
        ],
        "agent-main",
        Some(&cold_pw),
    );
    assert_eq!(d_merged["terminal_reached"], Value::Bool(true));
    assert_eq!(
        d_merged["status"],
        serde_json::to_value(DeliveryPhaseStatus::Merged).unwrap()
    );

    // 2) Candidate branch attempting to self-authorize main_merge / standing_pr_merge fails closed!
    let (_d_cand, xdg_cand, _r_cand, wt_cand, task_cand, pw_cand) =
        setup_repo_with_worktree(None, true, "roll the dice", None);
    inject_governance_toml(
        &wt_cand.join(".decapod/config.toml"),
        "standing_pr_merge = true\nmain_merge = true",
    );

    let pkt_tampered = run_json(
        xdg_cand.path(),
        &wt_cand,
        &[
            "infer",
            "orientation",
            "--task-id",
            &task_cand,
            "--format",
            "json",
        ],
        "agent-main",
        Some(&pw_cand),
    );
    assert_eq!(
        pkt_tampered["delivery_endpoint"]["candidate_attempted_elevation"],
        Value::Bool(true)
    );
    assert_ne!(
        pkt_tampered["delivery_endpoint"]["endpoint"],
        serde_json::to_value(DeliveryEndpointKind::PullRequestThenMerge).unwrap()
    );
    assert_ne!(
        pkt_tampered["delivery_endpoint"]["endpoint"],
        serde_json::to_value(DeliveryEndpointKind::DirectMerge).unwrap()
    );

    let val_fail = decapod(
        xdg_cand.path(),
        &wt_cand,
        &["validate"],
        "agent-main",
        Some(&pw_cand),
    );
    assert!(!val_fail.status.success());
    let val_msg = format!(
        "{}{}",
        String::from_utf8_lossy(&val_fail.stdout),
        String::from_utf8_lossy(&val_fail.stderr)
    );
    assert!(
        val_msg.contains("POLICY_SELF_AUTHORIZATION_REJECTED"),
        "expected POLICY_SELF_AUTHORIZATION_REJECTED, got: {val_msg}"
    );
}
