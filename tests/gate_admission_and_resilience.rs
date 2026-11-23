//! Phase-correct gate admission denials (#1763), multi-process cold-context
//! crash/handoff resilience (#1367), and constitution relevance/contribution
//! integrity negative controls (#1772, #1774).

use decapod::core::agent_interop::{
    AuthorityPlane, ClaimKind, EvidencePlane, GovernanceEnvelopeV1, IntentPlane, LocalTrustPolicy,
    NormalizedVerdict, OutcomePlane, PlaneVersions, ProducerTrustClass, RawEvidenceReceipt,
    ReasonClass, ReplayLedger, import_external_envelope,
};
use decapod::core::docs;
use serde_json::Value;
use std::collections::BTreeSet;
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
        .unwrap();
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
    cmd.output().unwrap()
}

fn acquire_session(xdg_root: &Path, dir: &Path, agent: &str) -> String {
    let out = decapod(xdg_root, dir, &["session", "acquire"], agent, None);
    assert!(
        out.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("Password:"))
        .map(str::trim)
        .expect("Password line in session acquire output")
        .to_string()
}

fn setup_isolated_worktree(agent: &str) -> (tempfile::TempDir, PathBuf, PathBuf, String, String) {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("repo");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("README.md"), "# Gate Admission Fixture\n").unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn val() -> u32 { 1 }\n").unwrap();
    git(&root, &["init", "-b", "master"]);
    git(&root, &["config", "user.email", "gate@decapod.local"]);
    git(&root, &["config", "user.name", "Gate Fixture"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "initial"]);

    let xdg = tmp.path();
    let init = decapod(
        xdg,
        &root,
        &[
            "init",
            "--force",
            "--no-ci",
            "--no-specs",
            "--no-container-workspaces",
        ],
        agent,
        None,
    );
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "decapod init"]);

    let pw = acquire_session(xdg, &root, agent);
    let todo = decapod(
        xdg,
        &root,
        &[
            "todo",
            "add",
            "Original human request: implement bounded retry with max_attempts=3",
        ],
        agent,
        Some(&pw),
    );
    assert!(todo.status.success());
    let task_id = serde_json::from_slice::<Value>(&todo.stdout).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let claim = decapod(
        xdg,
        &root,
        &["todo", "claim", "--id", &task_id],
        agent,
        Some(&pw),
    );
    assert!(claim.status.success());
    let ws = decapod(
        xdg,
        &root,
        &[
            "workspace",
            "ensure",
            "--branch",
            &format!("agent/{agent}/{task_id}-gate-test"),
        ],
        agent,
        Some(&pw),
    );
    assert!(
        ws.status.success(),
        "workspace ensure failed: {}",
        String::from_utf8_lossy(&ws.stderr)
    );
    let ws_json: Value =
        serde_json::from_str(String::from_utf8_lossy(&ws.stdout).lines().last().unwrap()).unwrap();
    let wt = PathBuf::from(ws_json["worktree_path"].as_str().unwrap());
    (tmp, root, wt, task_id, pw)
}

#[test]
fn issue_1763_toctou_head_and_base_movement_and_self_declared_proof_fail_closed() {
    let (tmp, root, wt, _task_id, pw) = setup_isolated_worktree("agent-a");
    let xdg = tmp.path();

    // 1. Record a checkpoint on tree X.
    let base_sha = git(&root, &["rev-parse", "master"]);
    fs::write(wt.join("src/lib.rs"), "pub fn val() -> u32 { 2 }\n").unwrap();
    git(&wt, &["add", "-A"]);
    let _ = Command::new("git")
        .current_dir(&wt)
        .args(["reset", "--", ".decapod/governance.json"])
        .output();

    let ckpt = decapod(
        xdg,
        &wt,
        &[
            "govern",
            "artifacts",
            "checkpoint",
            "--id",
            "ckpt-x",
            "--summary",
            "candidate tree X",
        ],
        "agent-a",
        Some(&pw),
    );
    assert!(
        ckpt.status.success(),
        "checkpoint failed: {}",
        String::from_utf8_lossy(&ckpt.stderr)
    );
    git(&wt, &["add", ".decapod/governance.json"]);
    git(&wt, &["commit", "-m", "tree X"]);
    let head_x = git(&wt, &["rev-parse", "HEAD"]);

    // 2. Advance candidate tree to Y WITHOUT re-checkpointing/re-proving:
    // verify-checkpoints --publication-material MUST refuse stale tree Y (#1763).
    fs::write(wt.join("src/lib.rs"), "pub fn val() -> u32 { 999 }\n").unwrap();
    git(&wt, &["add", "src/lib.rs"]);
    git(&wt, &["commit", "-m", "uncheckpointed tree Y"]);
    let head_y = git(&wt, &["rev-parse", "HEAD"]);
    assert_ne!(head_x, head_y);

    let verify_stale_head = decapod(
        xdg,
        &wt,
        &[
            "govern",
            "artifacts",
            "verify-checkpoints",
            "--publication-material",
            "--base-branch",
            "master",
            "--target-base",
            "master",
        ],
        "agent-a",
        Some(&pw),
    );
    assert!(
        !verify_stale_head.status.success(),
        "verify-checkpoints must refuse uncheckpointed mutation from tree X to tree Y"
    );

    // 3. Checkpoint tree Y cleanly, then advance target base `master` (TOCTOU base movement!).
    let ckpt_y = decapod(
        xdg,
        &wt,
        &[
            "govern",
            "artifacts",
            "checkpoint",
            "--id",
            "ckpt-y",
            "--summary",
            "candidate tree Y",
        ],
        "agent-a",
        Some(&pw),
    );
    assert!(ckpt_y.status.success());
    git(&wt, &["add", ".decapod/governance.json"]);
    git(&wt, &["commit", "-m", "checkpoint Y"]);
    let head_y_ckpt = git(&wt, &["rev-parse", "HEAD"]);

    // Advance master in main repo (simulating concurrent merge on target base).
    fs::write(
        root.join("README.md"),
        "# Master advanced after preflight\n",
    )
    .unwrap();
    git(&root, &["add", "README.md"]);
    git(&root, &["commit", "-m", "advance master base"]);
    let new_base_sha = git(&root, &["rev-parse", "master"]);
    assert_ne!(base_sha, new_base_sha);

    let verify_moved_base = decapod(
        xdg,
        &wt,
        &[
            "govern",
            "artifacts",
            "verify-checkpoints",
            "--publication-material",
            "--base-branch",
            &base_sha,
            "--head-ref",
            &head_y_ckpt,
            "--target-base",
            "master",
        ],
        "agent-a",
        Some(&pw),
    );
    assert!(
        !verify_moved_base.status.success(),
        "verify-checkpoints must refuse when target base master moves after checkpoint"
    );
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&verify_moved_base.stdout),
        String::from_utf8_lossy(&verify_moved_base.stderr)
    );
    assert!(
        combined.contains("PUBLICATION_MATERIAL_BASE_CHANGED")
            || combined.contains(&new_base_sha[..8])
            || combined.contains("stale"),
        "expected base-movement diagnostic, got: {combined}"
    );
}

#[test]
fn issue_1367_multiprocess_cold_context_handoff_recovers_intent_refuses_stale_and_dedupes() {
    let (tmp, root, wt, task_id, pw_a) = setup_isolated_worktree("agent-alpha");
    let xdg = tmp.path();

    // Process A commits candidate tree X and records checkpoint.
    fs::write(
        wt.join("src/lib.rs"),
        "pub fn max_attempts() -> u32 { 3 }\n",
    )
    .unwrap();
    git(&wt, &["add", "-A"]);
    let _ = Command::new("git")
        .current_dir(&wt)
        .args(["reset", "--", ".decapod/governance.json"])
        .output();
    let ckpt_a = decapod(
        xdg,
        &wt,
        &[
            "govern",
            "artifacts",
            "checkpoint",
            "--id",
            "ckpt-alpha-x",
            "--summary",
            "alpha passed on tree X",
        ],
        "agent-alpha",
        Some(&pw_a),
    );
    assert!(
        ckpt_a.status.success(),
        "ckpt_a failed: {}",
        String::from_utf8_lossy(&ckpt_a.stderr)
    );
    git(&wt, &["add", ".decapod/governance.json"]);
    git(&wt, &["commit", "-m", "alpha tree X"]);
    let head_x = git(&wt, &["rev-parse", "HEAD"]);
    let tree_x = git(&wt, &["rev-parse", "HEAD^{tree}"]);
    let base_sha = git(&root, &["rev-parse", "master"]);

    // Process A mutates to tree Y and exits (simulating crash mid-work with stale receipt for X!).
    fs::write(
        wt.join("src/lib.rs"),
        "pub fn max_attempts() -> u32 { 99 }\n",
    )
    .unwrap();
    git(&wt, &["add", "src/lib.rs"]);
    git(&wt, &["commit", "-m", "alpha uncommitted-proof tree Y"]);
    let head_y = git(&wt, &["rev-parse", "HEAD"]);
    let tree_y = git(&wt, &["rev-parse", "HEAD^{tree}"]);
    assert_ne!(head_x, head_y);

    // Cold-context Process B starts fresh with ZERO shared memory, new agent ID + fresh session.
    // 1. Attempting mutating/custody-scoped orientation without custody MUST fail closed (no impersonation).
    let pw_b = acquire_session(xdg, &root, "agent-beta");
    let orient_b = decapod(
        xdg,
        &wt,
        &["infer", "orientation", "--task-id", &task_id],
        "agent-beta",
        Some(&pw_b),
    );
    assert!(
        !orient_b.status.success(),
        "Process B must not silently assume agent-alpha's custody"
    );
    assert!(
        String::from_utf8_lossy(&orient_b.stderr).contains("AUTHORING_CUSTODY_STALE"),
        "expected AUTHORING_CUSTODY_STALE, got: {}",
        String::from_utf8_lossy(&orient_b.stderr)
    );

    // 2. Cold Process B inspects canonical intent read-only via `decapod observe specs`
    // without transcript replay or impersonating Process A.
    let observe_b = decapod(
        xdg,
        &wt,
        &["observe", "specs", "--format", "json"],
        "agent-beta",
        Some(&pw_b),
    );
    assert!(
        observe_b.status.success(),
        "cold Process B read-only observe specs failed: {}",
        String::from_utf8_lossy(&observe_b.stderr)
    );
    let obs_json: Value = serde_json::from_slice(&observe_b.stdout).unwrap();
    assert!(
        obs_json.to_string().contains("max_attempts=3"),
        "cold Process B must recover original canonical intent without transcript replay: {obs_json}"
    );

    // Process B evaluates Process A's inherited PASS envelope for tree X against current tree Y:
    // MUST reject as StaleEvidence, NEVER import authority, and dedupe unknown-outcome op_id!
    let policy = LocalTrustPolicy {
        repository_id: "repo:handoff-fixture".to_string(),
        tenant_id: "tenant:local".to_string(),
        expected_recipient_id: "agent-beta".to_string(),
        expected_head_commit_sha: head_y,
        expected_base_commit_sha: base_sha.clone(),
        expected_source_tree_sha: tree_y,
        min_lease_generation: 1,
        current_proof_plan_epoch: 1,
        accepted_producers: BTreeSet::from(["independent-ci".to_string()]),
        require_human_approval_for_ops: BTreeSet::from(["workspace.publish".to_string()]),
    };
    let raw_rcpt = RawEvidenceReceipt::from_raw("ci-run-1", r#"{"status":"passed"}"#);
    let stale_env_from_a = GovernanceEnvelopeV1 {
        profile_id: "decapod.governance-envelope".to_string(),
        profile_version: "v1".to_string(),
        plane_versions: PlaneVersions::default(),
        intent: IntentPlane {
            repository_id: "repo:handoff-fixture".to_string(),
            tenant_id: "tenant:local".to_string(),
            task_id: task_id.clone(),
            run_id: "run-alpha".to_string(),
            op_id: "op-external-write-42".to_string(),
            actor_id: "agent-alpha".to_string(),
            recipient_id: "agent-beta".to_string(),
            replay_domain: "repo:handoff-fixture".to_string(),
            lease_generation: 1,
            original_human_request_sha256: "sha256:orig".to_string(),
            agent_interpretation_summary: "alpha summary".to_string(),
            human_request_independently_observed: true,
        },
        authority: AuthorityPlane {
            requested_operation: "workspace.publish".to_string(),
            actual_parameters_canonical: "{\"branch\":\"agent/alpha\"}".to_string(),
            claimed_approved_operation: None,
            claimed_approved_parameters_canonical: None,
            authenticated_human_approval: true,
            plan_approved_by_agent: true,
            issued_at_epoch: 1_700_000_000,
            expires_at_epoch: 1_700_003_600,
            delegation: None,
            mandatory_extensions: vec![],
            optional_extensions: vec![],
        },
        evidence: EvidencePlane {
            producer_id: "independent-ci".to_string(),
            producer_trust_class: ProducerTrustClass::IndependentExecutor,
            source_tree_sha: tree_x,
            head_commit_sha: head_x,
            base_commit_sha: base_sha,
            proof_plan_epoch: 1,
            claimed_receipt_digests: vec![raw_rcpt.claimed_sha256.clone()],
            raw_receipts: vec![raw_rcpt],
            self_generated_checksum_only: false,
            independent_effect_observed: true,
            newer_suite_failed: false,
            environment_tamper_signals: vec![],
            history_events: vec![],
            expected_terminal_seq: None,
        },
        outcome: OutcomePlane {
            claim_kind: ClaimKind::Effect,
            declared_status: "passed".to_string(),
            observed_status: "passed".to_string(),
            observed_assertions: Some(5),
            observed_http_status: None,
            process_path_authorized: true,
            behavioral_passed: true,
            target_uri_or_path: None,
        },
        envelope_sha256: String::new(),
    }
    .with_computed_digest();

    let mut replay_ledger = ReplayLedger::new();
    let stale_report = import_external_envelope(
        &stale_env_from_a,
        &policy,
        &mut replay_ledger,
        1_700_000_100,
    );
    assert_eq!(stale_report.verdict, NormalizedVerdict::Rejected);
    assert_eq!(stale_report.reason_class, ReasonClass::StaleEvidence);
    assert!(!stale_report.imported_authority_granted);

    // Dedupe check for unknown-outcome external mutation op_id across crash/restart:
    // op-external-write-42 was already recorded during envelope evaluation above.
    let dup_err = replay_ledger
        .check_and_record(
            "repo:handoff-fixture",
            "repo:handoff-fixture",
            "tenant:local",
            "run-alpha",
            "op-external-write-42",
            "sha256:payload-mutated",
        )
        .expect_err("conflicting replay of unknown-outcome op_id must be refused");
    assert!(dup_err.contains("duplicate op_id"));
}

#[test]
fn issue_1772_and_1774_constitution_relevance_resists_repetition_bias_and_verifies_binding_controls()
 {
    // #1772: Irrelevant gibberish query must NOT fall back to arbitrary alphabetic docs.
    let tmp = tempdir().unwrap();
    let no_match = docs::resolve_scoped_fragments(
        tmp.path(),
        Some("zzzz_nonexistent_unicorn_quasar_999"),
        None,
        &[],
        &[],
        5,
    );
    assert!(
        no_match.is_empty(),
        "irrelevant query must return empty projection rather than alphabetic fallback: {:?}",
        no_match.iter().map(|f| &f.r#ref).collect::<Vec<_>>()
    );

    // #1772: Repeated keyword stuffing of a non-existent/rejected token must not invent binding rules.
    let stuffed_query = "zephyr_florb_quux ".repeat(25);
    let stuffed =
        docs::resolve_scoped_fragments(tmp.path(), Some(&stuffed_query), None, &[], &[], 5);
    assert!(
        stuffed.is_empty(),
        "keyword-stuffed rejected phrase must not trigger false-positive constitution matches"
    );

    // #1774: Verify core/SPECS.sections.apply is present in the embedded binary corpus,
    // binding, and explicitly gated under authority_review.control_id == "core/SPECS".
    let out = Command::new(decapod_bin())
        .args(["constitution", "get", "core/SPECS"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let specs_node: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(specs_node["binding"], "binding");
    assert_eq!(specs_node["authority_review"]["control_id"], "core/SPECS");
    assert!(
        specs_node["sections"].get("apply").is_some(),
        "core/SPECS.sections.apply must be present in embedded constitution"
    );
}
