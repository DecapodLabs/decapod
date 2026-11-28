//! P0 fault-detection suite for Decapod issue #1795:
//! Prove the governance test suite detects weakened enforcement rules across
//! critical admission boundaries without weakening shipped `src/`.
//!
//! This test suite:
//! 1. Inventories the seven critical admission boundaries and maps each to an
//!    existing rule, actual-binary/kernel test, and lawful acceptance control.
//! 2. Exercises an isolated temporary copy of the repository (`CARGO_TARGET_DIR`
//!    isolated inside `TempDir`, `CARGO_BUILD_JOBS=4`, `--offline`) and injects
//!    one bounded fault per rule across five consequential rules covering the
//!    five required attacks.
//! 3. Distinguishes `detected`, `survived`, `unexercised`, `build_invalid`, and
//!    `equivalent_fault`, never counting an unexecuted/aborted/uncompiled test
//!    as `detected`.
//! 4. Supplies the smallest legitimate negative + positive fixture for any
//!    meaningful surviving fault (expired-lease `todo renew` in `todo.rs`) and
//!    reruns the fault injection against the real `decapod` CLI binary to prove
//!    the remediation fixture catches it while positive controls continue to pass.
//! 5. Publishes and validates a compact, versioned JSON rule-to-falsifier
//!    report (`schema_version = "1.0.0"`), making no general formal-soundness
//!    claim.

use decapod::core::bounded_process::{BUILD_TIMEOUT, BoundedCommand};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;
use tempfile::TempDir;

const REPORT_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FaultOutcome {
    Detected,
    Survived,
    Unexercised,
    BuildInvalid,
    EquivalentFault,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
struct TestRunCounts {
    passed: u64,
    failed: u64,
    ignored: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BoundaryInventoryEntry {
    boundary: String,
    rule_id: String,
    owner_refs: String,
    source_symbol: String,
    existing_test: String,
    lawful_acceptance_control: String,
}

#[derive(Debug, Clone)]
struct FaultSpec {
    rule_id: &'static str,
    attack_class: &'static str,
    boundary: &'static str,
    source_file: &'static str,
    source_symbol: &'static str,
    fault_operator: &'static str,
    target_snippet: &'static str,
    fault_replacement: &'static str,
    targeted_commands: &'static [&'static [&'static str]],
    targeted_existing_test: &'static str,
    positive_control: &'static str,
    fixture_id: &'static str,
    expected_refusal: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FaultResultRecord {
    rule_id: String,
    attack_class: String,
    boundary: String,
    source_file: String,
    source_symbol: String,
    fault_operator: String,
    targeted_existing_test: String,
    positive_control: String,
    fixture_id: String,
    expected_refusal: String,
    observed_refusal: String,
    baseline_passed: bool,
    initial_outcome: FaultOutcome,
    remediation_fixture_id: Option<String>,
    post_remediation_outcome: FaultOutcome,
    executed_tests: u64,
    failed_tests: u64,
    exit_codes: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BuildIdentity {
    package_name: String,
    package_version: String,
    manifest_dir: String,
    git_head: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GovernanceFaultReport {
    schema_version: String,
    kind: String,
    issue: String,
    scope_note: String,
    build_identity: BuildIdentity,
    boundary_inventory: Vec<BoundaryInventoryEntry>,
    fault_results: Vec<FaultResultRecord>,
}

fn boundary_inventory() -> Vec<BoundaryInventoryEntry> {
    vec![
        BoundaryInventoryEntry {
            boundary: "task_lease_custody".to_string(),
            rule_id: "RULE-CUSTODY-004".to_string(),
            owner_refs: "#1795 / #1776".to_string(),
            source_symbol: "src/decapod/core/todo.rs::renew_claim_lease".to_string(),
            existing_test: "tests/plugins/todo.rs::exclusive_claim_sets_lease_and_renew_extends_it + expired_lease_is_reclaimable_by_another_agent".to_string(),
            lawful_acceptance_control: "Active exclusive lease renewal advances lease_generation and extends lease_expires_at".to_string(),
        },
        BoundaryInventoryEntry {
            boundary: "accepted_policy_precedence".to_string(),
            rule_id: "RULE-POLICY-003".to_string(),
            owner_refs: "#1776 / #1763 / #1733".to_string(),
            source_symbol: "src/decapod/core/readiness_profile.rs::required_source_catalog_with_executor".to_string(),
            existing_test: "src/decapod/core/readiness_profile/tests.rs::accepted_executor_never_authorizes_changed_candidate_workflows_or_carriers".to_string(),
            lawful_acceptance_control: "Accepted executor binds exact reviewed workflows/carriers and admits reviewed candidate helper changes".to_string(),
        },
        BoundaryInventoryEntry {
            boundary: "required_proof_producer_binding".to_string(),
            rule_id: "RULE-PROOF-001".to_string(),
            owner_refs: "#1763 / #1733".to_string(),
            source_symbol: "src/decapod/core/workunit.rs::validate_verified_manifest".to_string(),
            existing_test: "tests/workunit_publish_gate.rs::publish_gate_rejects_manually_declared_verified_passes".to_string(),
            lawful_acceptance_control: "Observed execution checkpoint and trajectory-bound non-task branch pass publish gate".to_string(),
        },
        BoundaryInventoryEntry {
            boundary: "exact_candidate_base_freshness".to_string(),
            rule_id: "RULE-FRESHNESS-002".to_string(),
            owner_refs: "#1052 / #1763".to_string(),
            source_symbol: "src/decapod/core/workspace.rs::verify_validation_artifacts_for_publish".to_string(),
            existing_test: "tests/unit/core/workspace_publication_tests.rs (core::workspace::publication::tests::current_receipt_accepts_proof_commit_but_rejects_stale_code_epoch_task_and_revision)".to_string(),
            lawful_acceptance_control: "Current validation receipt bound to active epoch, HEAD ancestry, and plan passes".to_string(),
        },
        BoundaryInventoryEntry {
            boundary: "approval_scope".to_string(),
            rule_id: "RULE-APPROVAL-006".to_string(),
            owner_refs: "#1776 / #1733".to_string(),
            source_symbol: "src/decapod/core/governed_transition.rs::admit_attempt".to_string(),
            existing_test: "tests/policy_self_authorization_1776.rs + tests/unit/core/todo_tests.rs".to_string(),
            lawful_acceptance_control: "In-scope attempt matching current governing identity and non-forbidden paths is admissible".to_string(),
        },
        BoundaryInventoryEntry {
            boundary: "retry_on_unknown".to_string(),
            rule_id: "RULE-RETRY-005".to_string(),
            owner_refs: "#1660 / #1733".to_string(),
            source_symbol: "src/decapod/core/group_broker.rs::execute_request".to_string(),
            existing_test: "tests/unit/core/group_broker_tests.rs::durable_started_receipt_prevents_reexecution_and_binds_caller".to_string(),
            lawful_acceptance_control: "Pre-execution deadline expiration returns BROKER_DEADLINE_BEFORE_EXECUTION without leaving a started receipt".to_string(),
        },
        BoundaryInventoryEntry {
            boundary: "final_pr_eligibility".to_string(),
            rule_id: "RULE-PUBLISH-007".to_string(),
            owner_refs: "#1659 / #1052 / #1776".to_string(),
            source_symbol: "src/decapod/core/validation_runtime.rs::validate_workbranch_readiness".to_string(),
            existing_test: "tests/exact_candidate_governance_1052.rs + tests/gate_skip_override_readiness_1659.rs".to_string(),
            lawful_acceptance_control: "Clean candidate descending from accepted base with full readiness evidence publishes cleanly".to_string(),
        },
    ]
}

fn five_fault_specs() -> Vec<FaultSpec> {
    vec![
        FaultSpec {
            rule_id: "RULE-PROOF-001",
            attack_class: "fabricated_pass_without_execution_or_evidence",
            boundary: "required_proof_producer_binding",
            source_file: "src/decapod/core/workunit.rs",
            source_symbol: "workunit::validate_verified_manifest",
            fault_operator: "replace_observed_outcome_with_declaration",
            target_snippet: r#"    if manifest.status != WorkUnitStatus::Verified {
        return Err(journal_error(
            "WORKUNIT_PROOF_UNVERIFIABLE",
            "current status is not VERIFIED",
        ));
    }
    verify_resume_record(root, current)"#,
            fault_replacement: r#"    if manifest.status != WorkUnitStatus::Verified {
        return Err(journal_error(
            "WORKUNIT_PROOF_UNVERIFIABLE",
            "current status is not VERIFIED",
        ));
    }
    let _ = current;
    Ok(())"#,
            targeted_commands: &[&[
                "test",
                "--offline",
                "--test",
                "workunit_publish_gate",
                "publish_gate_",
            ]],
            targeted_existing_test: "cargo test --offline --test workunit_publish_gate publish_gate_",
            positive_control: "publish_gate_skips_when_branch_has_no_task_ids",
            fixture_id: "workunit_publish_gate::publish_gate_rejects_manually_declared_verified_passes",
            expected_refusal: "WORKUNIT_CHECKPOINT_REQUIRED",
        },
        FaultSpec {
            rule_id: "RULE-FRESHNESS-002",
            attack_class: "replayed_validation_receipt_on_changed_tree",
            boundary: "exact_candidate_base_freshness",
            source_file: "src/decapod/core/workspace.rs",
            source_symbol: "workspace::verify_validation_artifacts_for_publish",
            fault_operator: "ignore_stale_generation",
            target_snippet: r#"    if receipt.validation_epoch != active_epoch
        || receipt.repo_signal_fingerprint != project_specs::repo_signal_fingerprint(repo_root)?
        || receipt.decapod_release != entrypoint_integrity::RELEASE_VERSION
    {
        return Err(DecapodError::ValidationError(
            "STALE_PUBLICATION_VALIDATION: code, living specs, or evaluator changed since validation. Review the affected specs, refresh through `decapod rpc --op specs.refresh`, and rerun `decapod validate`.".into(),
        ));
    }"#,
            fault_replacement: r#"    let _ = (&active_epoch, &receipt.repo_signal_fingerprint, &receipt.decapod_release);"#,
            targeted_commands: &[&[
                "test",
                "--offline",
                "--lib",
                "core::workspace::publication::tests::current_receipt_accepts_proof_commit_but_rejects_stale_code_epoch_task_and_revision",
            ]],
            targeted_existing_test: "cargo test --offline --lib core::workspace::publication::tests::current_receipt_accepts_proof_commit_but_rejects_stale_code_epoch_task_and_revision",
            positive_control: "current_receipt_accepts_proof_commit_but_rejects_stale_code_epoch_task_and_revision (pre-mutation proof commit check)",
            fixture_id: "workspace_publication_tests::current_receipt_accepts_proof_commit_but_rejects_stale_code_epoch_task_and_revision",
            expected_refusal: "STALE_PUBLICATION_VALIDATION",
        },
        FaultSpec {
            rule_id: "RULE-POLICY-003",
            attack_class: "altered_eval_requirements_with_recomputed_digest",
            boundary: "accepted_policy_precedence",
            source_file: "src/decapod/core/readiness_profile.rs",
            source_symbol: "readiness_profile::required_source_catalog_with_executor",
            fault_operator: "relax_fence",
            target_snippet: r#"    for (path, expected) in reviewed_workflow_digests() {
        if !is_reviewed_workflow_digest(
            path,
            expected,
            discovered.files.get(*path).map(String::as_str),
        ) {
            return Err(PlanningFailure::ControllerContractUnsupported(
                (*path).into(),
            ));
        }
    }
    let carrier = carrier_files();
    for (path, compiled) in &carrier {
        if discovered.files.get(*path) != Some(&bytes_digest(compiled)) {
            return Err(PlanningFailure::ControllerContractUnsupported(
                (*path).into(),
            ));
        }
    }"#,
            fault_replacement: r#"    let carrier = carrier_files();"#,
            targeted_commands: &[&[
                "test",
                "--offline",
                "--lib",
                "core::readiness_profile::tests::accepted_executor_",
            ]],
            targeted_existing_test: "cargo test --offline --lib core::readiness_profile::tests::accepted_executor_",
            positive_control: "accepted_executor_binds_accepted_identity_and_candidate_source_to_every_obligation",
            fixture_id: "readiness_profile::tests::accepted_executor_never_authorizes_changed_candidate_workflows_or_carriers",
            expected_refusal: "PlanningFailure::ControllerContractUnsupported",
        },
        FaultSpec {
            rule_id: "RULE-CUSTODY-004",
            attack_class: "expired_lease_exclusive_transition",
            boundary: "task_lease_custody",
            source_file: "src/decapod/core/todo.rs",
            source_symbol: "todo::renew_claim_lease",
            fault_operator: "invert_allow_block",
            target_snippet: r#"                    if fleet_coord::lease_allows_reclaim(prev_lease.as_deref(), &ts) {"#,
            fault_replacement: r#"                    if false && fleet_coord::lease_allows_reclaim(prev_lease.as_deref(), &ts) {"#,
            targeted_commands: &[
                &[
                    "test",
                    "--offline",
                    "--test",
                    "plugins_todo_tests",
                    "exclusive_claim_sets_lease_and_renew_extends_it",
                ],
                &[
                    "test",
                    "--offline",
                    "--test",
                    "plugins_todo_tests",
                    "expired_lease_is_reclaimable_by_another_agent",
                ],
                &[
                    "test",
                    "--offline",
                    "--test",
                    "plugins_todo_tests",
                    "authoring_binding_one_to_one_uniqueness_renew_and_full_lifecycle_retirement",
                ],
            ],
            targeted_existing_test: "cargo test --offline --test plugins_todo_tests (exclusive_claim_sets_lease_and_renew_extends_it + expired_lease_is_reclaimable_by_another_agent + authoring_binding_one_to_one_uniqueness_renew_and_full_lifecycle_retirement)",
            positive_control: "exclusive_claim_sets_lease_and_renew_extends_it",
            fixture_id: "plugins_todo_tests (existing suite lacked expired-lease renew negative)",
            expected_refusal: "lease_state=expired conflict (reclaim with todo claim instead of renew)",
        },
        FaultSpec {
            rule_id: "RULE-RETRY-005",
            attack_class: "blind_retry_on_broker_unknown",
            boundary: "retry_on_unknown",
            source_file: "src/decapod/core/group_broker.rs",
            source_symbol: "group_broker::execute_request",
            fault_operator: "drop_required_check",
            target_snippet: r#"    if let Some(existing) = dedupe_lookup(broker_root, request)? {"#,
            fault_replacement: r#"    if let Some(existing) = { let _ = dedupe_lookup(broker_root, request)?; Option::<DedupeRecord>::None } {"#,
            targeted_commands: &[&[
                "test",
                "--offline",
                "--lib",
                "core::group_broker::tests::durable_started_receipt_prevents_reexecution_and_binds_caller",
            ]],
            targeted_existing_test: "cargo test --offline --lib core::group_broker::tests::durable_started_receipt_prevents_reexecution_and_binds_caller",
            positive_control: "queued_request_deadline_prevents_execution_without_a_receipt",
            fixture_id: "group_broker_tests::durable_started_receipt_prevents_reexecution_and_binds_caller",
            expected_refusal: "status=UNKNOWN / BROKER_UNKNOWN without re-execution",
        },
    ]
}

fn parse_test_counts(output: &str) -> TestRunCounts {
    let mut counts = TestRunCounts::default();
    for line in output.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("test result:") {
            continue;
        }
        for segment in trimmed.split(';') {
            let tokens: Vec<&str> = segment.split_whitespace().collect();
            for pair in tokens.windows(2) {
                if let Ok(num) = pair[0].parse::<u64>() {
                    match pair[1].trim_end_matches('.') {
                        "passed" => counts.passed += num,
                        "failed" => counts.failed += num,
                        "ignored" => counts.ignored += num,
                        _ => {}
                    }
                }
            }
        }
    }
    counts
}

fn classify_cargo_test_run(
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
    fault_applied: bool,
    patch_is_noop: bool,
) -> (FaultOutcome, TestRunCounts) {
    if !fault_applied || patch_is_noop {
        return (FaultOutcome::EquivalentFault, TestRunCounts::default());
    }
    let counts = parse_test_counts(stdout);
    let total_executed = counts.passed + counts.failed;
    let compile_failed = stderr.contains("could not compile") || stderr.contains("error[E");
    if compile_failed && total_executed == 0 {
        return (FaultOutcome::BuildInvalid, counts);
    }
    if total_executed == 0 {
        return (FaultOutcome::Unexercised, counts);
    }
    if counts.failed > 0 {
        return (FaultOutcome::Detected, counts);
    }
    if exit_code == Some(0) && counts.passed > 0 {
        return (FaultOutcome::Survived, counts);
    }
    (FaultOutcome::Unexercised, counts)
}

fn copy_dir_filtered(src: &Path, dst: &Path, is_root: bool) {
    fs::create_dir_all(dst).expect("create isolated dir");
    for entry in fs::read_dir(src).expect("read source dir") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if is_root
            && matches!(
                name_str.as_ref(),
                ".git" | ".decapod" | "target" | "dist" | "constitution_embed"
            )
        {
            continue;
        }
        if name_str.ends_with(".bak") {
            continue;
        }
        let src_path = entry.path();
        let dst_path = dst.join(&name);
        let meta = fs::symlink_metadata(&src_path).expect("metadata");
        if meta.file_type().is_symlink() {
            let target = fs::read_link(&src_path).expect("read symlink");
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, &dst_path).expect("create symlink");
            #[cfg(not(unix))]
            if src_path.is_dir() {
                copy_dir_filtered(&src_path, &dst_path, false);
            } else {
                fs::copy(&src_path, &dst_path).expect("copy symlink target");
            }
        } else if meta.is_dir() {
            copy_dir_filtered(&src_path, &dst_path, false);
        } else if meta.is_file() {
            fs::copy(&src_path, &dst_path).expect("copy file");
        }
    }
}

fn run_isolated_cargo(repo: &Path, target_dir: &Path, args: &[&str]) -> Output {
    let cargo_bin = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut cmd = Command::new(cargo_bin);
    cmd.current_dir(repo).args(args);
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if ["DECAPOD_", "GIT_", "BUILDKITE", "GITHUB"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            cmd.env_remove(key);
        }
    }
    for key in [
        "CARGO_MANIFEST_DIR",
        "CARGO_PKG_NAME",
        "CARGO_PKG_VERSION",
        "OUT_DIR",
        "CARGO_BIN_EXE_decapod",
    ] {
        cmd.env_remove(key);
    }
    cmd.env("CARGO_TARGET_DIR", target_dir);
    if std::env::var_os("CARGO_BUILD_JOBS").is_none() {
        cmd.env("CARGO_BUILD_JOBS", "4");
    }
    cmd.bounded_output(BUILD_TIMEOUT)
        .expect("run isolated cargo")
}

fn resolve_shipped_decapod_bin() -> PathBuf {
    let cargo_bin = env!("CARGO_BIN_EXE_decapod");
    Path::new(cargo_bin)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(cargo_bin))
}

fn run_decapod_cli(bin: &Path, dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(bin);
    cmd.current_dir(dir).args(args);
    for key in [
        "DECAPOD_AGENT_ID",
        "DECAPOD_SESSION_PASSWORD",
        "DECAPOD_VALIDATE_SKIP_GIT_GATES",
        "DECAPOD_VALIDATE_SKIP_TOOLING_GATES",
        "DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES",
        "DECAPOD_CLAIM_AUTORUN",
    ] {
        cmd.env_remove(key);
    }
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.bounded_output(BUILD_TIMEOUT).expect("run decapod cli")
}

fn setup_cli_fixture_repo() -> (TempDir, PathBuf, String) {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path().to_path_buf();
    let git_init = Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .bounded_output(BUILD_TIMEOUT)
        .expect("git init");
    assert!(git_init.status.success(), "git init failed");

    let shipped = resolve_shipped_decapod_bin();
    let init = run_decapod_cli(&shipped, &dir, &["init", "--force"], &[]);
    assert!(
        init.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let acquire = run_decapod_cli(
        &shipped,
        &dir,
        &["session", "acquire"],
        &[("DECAPOD_AGENT_ID", "agent-1795")],
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
        .expect("session password");
    (tmp, dir, password)
}

/// Legitimate negative + positive fixture for Rule 4 (`todo::renew_claim_lease`):
/// - Positive: active exclusive claim renews cleanly (`status == "ok"`,
///   `lease_lifecycle == "extended"`, `lease_generation == 2`).
/// - Negative: once a 1s exclusive claim lease expires, `todo renew` MUST be
///   rejected (`status == "conflict"`, `lease_state == "expired"`), leaving
///   `lease_generation == 1`.
fn exercise_expired_lease_renew_fixture(bin: &Path) -> Result<(), String> {
    let (_tmp, dir, password) = setup_cli_fixture_repo();
    let base_envs = [
        ("DECAPOD_AGENT_ID", "agent-1795"),
        ("DECAPOD_SESSION_PASSWORD", password.as_str()),
        ("DECAPOD_CLAIM_AUTORUN", "0"),
    ];

    let add_out = run_decapod_cli(
        bin,
        &dir,
        &[
            "todo",
            "add",
            "Lease expiry positive and negative control",
            "--format",
            "json",
        ],
        &base_envs,
    );
    if !add_out.status.success() {
        return Err(format!(
            "todo add failed: {}",
            String::from_utf8_lossy(&add_out.stderr)
        ));
    }
    let add_json: Value =
        serde_json::from_slice(&add_out.stdout).map_err(|e| format!("parse todo add json: {e}"))?;
    let active_id = add_json["id"]
        .as_str()
        .ok_or_else(|| "missing active todo id".to_string())?
        .to_string();

    // Positive control: active lease renewal succeeds.
    let claim_active = run_decapod_cli(
        bin,
        &dir,
        &[
            "todo",
            "claim",
            "--id",
            &active_id,
            "--lease-seconds",
            "120",
            "--format",
            "json",
        ],
        &base_envs,
    );
    if !claim_active.status.success() {
        return Err(format!(
            "positive claim failed: {}",
            String::from_utf8_lossy(&claim_active.stderr)
        ));
    }
    let renew_active = run_decapod_cli(
        bin,
        &dir,
        &[
            "todo",
            "renew",
            "--id",
            &active_id,
            "--lease-seconds",
            "300",
            "--format",
            "json",
        ],
        &base_envs,
    );
    if !renew_active.status.success() {
        return Err(format!(
            "positive renew failed: {}",
            String::from_utf8_lossy(&renew_active.stderr)
        ));
    }
    let renew_active_json: Value = serde_json::from_slice(&renew_active.stdout)
        .map_err(|e| format!("parse positive renew json: {e}"))?;
    if renew_active_json["status"] != "ok"
        || renew_active_json["result"]["lease_lifecycle"] != "extended"
        || renew_active_json["result"]["lease_generation"] != 2
    {
        return Err(format!(
            "positive active renew did not advance lease as expected: {renew_active_json}"
        ));
    }

    // Negative control: 1s lease expires -> renew MUST reject as conflict/expired.
    let add_exp = run_decapod_cli(
        bin,
        &dir,
        &[
            "todo",
            "add",
            "Expired lease renew must reject",
            "--format",
            "json",
        ],
        &base_envs,
    );
    if !add_exp.status.success() {
        return Err(format!(
            "negative todo add failed: {}",
            String::from_utf8_lossy(&add_exp.stderr)
        ));
    }
    let add_exp_json: Value = serde_json::from_slice(&add_exp.stdout)
        .map_err(|e| format!("parse negative todo add json: {e}"))?;
    let expired_id = add_exp_json["id"]
        .as_str()
        .ok_or_else(|| "missing expired todo id".to_string())?
        .to_string();

    let claim_exp = run_decapod_cli(
        bin,
        &dir,
        &[
            "todo",
            "claim",
            "--id",
            &expired_id,
            "--lease-seconds",
            "1",
            "--format",
            "json",
        ],
        &base_envs,
    );
    if !claim_exp.status.success() {
        return Err(format!(
            "negative claim failed: {}",
            String::from_utf8_lossy(&claim_exp.stderr)
        ));
    }

    // Wait past the 1s exclusive lease boundary.
    std::thread::sleep(Duration::from_millis(1500));

    let renew_exp = run_decapod_cli(
        bin,
        &dir,
        &[
            "todo",
            "renew",
            "--id",
            &expired_id,
            "--lease-seconds",
            "300",
            "--format",
            "json",
        ],
        &base_envs,
    );
    let renew_exp_stdout = String::from_utf8_lossy(&renew_exp.stdout);
    let renew_exp_json: Value = serde_json::from_str(&renew_exp_stdout).map_err(|e| {
        format!(
            "parse negative renew json ({e}): stdout={renew_exp_stdout} stderr={}",
            String::from_utf8_lossy(&renew_exp.stderr)
        )
    })?;
    let rejected = renew_exp_json["status"] == "conflict"
        && renew_exp_json["result"]["status"] == "conflict"
        && renew_exp_json["result"]["lease_state"] == "expired"
        && renew_exp_json["result"]["message"]
            .as_str()
            .is_some_and(|msg| msg.contains("reclaim with todo claim instead of renew"));
    if !rejected {
        return Err(format!(
            "expired lease renew was NOT rejected (status={}, lease_state={}): {renew_exp_json}",
            renew_exp_json["status"], renew_exp_json["result"]["lease_state"]
        ));
    }
    Ok(())
}

fn summarize_failure_excerpt(stdout: &str, stderr: &str) -> String {
    for line in stdout.lines().chain(stderr.lines()) {
        if line.contains("FAILED")
            || line.contains("panicked at")
            || line.contains("WORKUNIT_CHECKPOINT_REQUIRED")
            || line.contains("STALE_PUBLICATION_VALIDATION")
            || line.contains("ControllerContractUnsupported")
            || line.contains("BROKER_UNKNOWN")
            || line.contains("assert")
        {
            return line.trim().to_string();
        }
    }
    "none".to_string()
}

fn extract_failed_test_names(stdout: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_failures = false;
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed == "failures:" {
            in_failures = true;
            continue;
        }
        if in_failures {
            if trimmed.is_empty() {
                in_failures = false;
                continue;
            }
            if !trimmed.starts_with("---- ")
                && !trimmed.starts_with("test result:")
                && !trimmed.contains("panicked at")
            {
                names.push(trimmed.to_string());
            }
        }
    }
    names
}

#[test]
fn classifier_never_counts_unexercised_build_invalid_or_equivalent_as_detected() {
    let (outcome, counts) = classify_cargo_test_run(
        Some(0),
        "running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
        "",
        true,
        false,
    );
    assert_eq!(outcome, FaultOutcome::Unexercised);
    assert_eq!(counts.passed + counts.failed, 0);

    let (aborted, _) = classify_cargo_test_run(Some(101), "", "signal killed", true, false);
    assert_eq!(aborted, FaultOutcome::Unexercised);

    let (build_invalid, _) = classify_cargo_test_run(
        Some(101),
        "",
        "error[E0308]: mismatched types\nerror: could not compile `decapod` (lib test)\n",
        true,
        false,
    );
    assert_eq!(build_invalid, FaultOutcome::BuildInvalid);

    let (equiv, _) = classify_cargo_test_run(
        Some(0),
        "test result: ok. 1 passed; 0 failed; 0 ignored\n",
        "",
        false,
        true,
    );
    assert_eq!(equiv, FaultOutcome::EquivalentFault);

    let (survived, counts) = classify_cargo_test_run(
        Some(0),
        "test result: ok. 3 passed; 0 failed; 0 ignored\n",
        "",
        true,
        false,
    );
    assert_eq!(survived, FaultOutcome::Survived);
    assert_eq!(counts.passed, 3);

    let (detected, counts) = classify_cargo_test_run(
        Some(101),
        "test result: FAILED. 1 passed; 1 failed; 0 ignored\n",
        "",
        true,
        false,
    );
    assert_eq!(detected, FaultOutcome::Detected);
    assert_eq!(counts.failed, 1);
}

#[test]
fn boundary_inventory_maps_all_seven_admission_boundaries() {
    let inventory = boundary_inventory();
    assert_eq!(inventory.len(), 7);
    for expected in [
        "task_lease_custody",
        "accepted_policy_precedence",
        "required_proof_producer_binding",
        "exact_candidate_base_freshness",
        "approval_scope",
        "retry_on_unknown",
        "final_pr_eligibility",
    ] {
        assert!(
            inventory.iter().any(|entry| entry.boundary == expected),
            "missing inventory entry for boundary {expected}"
        );
    }
    for entry in &inventory {
        assert!(!entry.rule_id.is_empty());
        assert!(!entry.source_symbol.is_empty());
        assert!(!entry.existing_test.is_empty());
        assert!(!entry.lawful_acceptance_control.is_empty());
    }
}

#[test]
fn expired_lease_renew_negative_and_positive_fixture_against_shipped_binary() {
    let shipped = resolve_shipped_decapod_bin();
    exercise_expired_lease_renew_fixture(&shipped)
        .expect("shipped binary must allow active renew and reject expired renew");
}

#[test]
fn isolated_fault_injection_detects_all_five_weakened_rules_and_publishes_report() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sandbox = TempDir::new().expect("sandbox tempdir");
    let iso_repo = sandbox.path().join("repo");
    let iso_target = sandbox.path().join("target");
    copy_dir_filtered(&manifest_dir, &iso_repo, true);
    let src_config = manifest_dir.join(".decapod").join("config.toml");
    if src_config.exists() {
        let dst_config = iso_repo.join(".decapod").join("config.toml");
        fs::create_dir_all(dst_config.parent().expect("config parent")).expect("create .decapod");
        fs::copy(&src_config, &dst_config).expect("copy .decapod/config.toml");
    }

    let specs = five_fault_specs();
    assert_eq!(specs.len(), 5);

    // 1. Run all positive baseline controls once on the unfaulted isolated build
    //    before injecting faults, avoiding redundant baseline recompiles.
    for spec in &specs {
        for cmd_args in spec.targeted_commands {
            let out = run_isolated_cargo(&iso_repo, &iso_target, cmd_args);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let counts = parse_test_counts(&stdout);
            assert!(
                out.status.success() && counts.failed == 0 && counts.passed > 0,
                "baseline positive control failed for {} ({:?}):\nstdout:\n{}\nstderr:\n{}",
                spec.rule_id,
                cmd_args,
                stdout,
                stderr
            );
        }
    }

    let mut fault_results = Vec::new();
    for spec in &specs {
        let target_file = iso_repo.join(spec.source_file);
        let original_source = fs::read_to_string(&target_file)
            .unwrap_or_else(|e| panic!("read {}: {e}", target_file.display()));
        assert!(
            original_source.contains(spec.target_snippet),
            "target snippet not found in {} for {}",
            spec.source_file,
            spec.rule_id
        );
        let baseline_passed = true;

        // 2. Inject single bounded fault into isolated copy (never shipped src/).
        let faulted_source =
            original_source.replacen(spec.target_snippet, spec.fault_replacement, 1);
        let patch_is_noop = faulted_source == original_source;
        fs::write(&target_file, &faulted_source).expect("write faulted source");

        let mut aggregated_outcome = FaultOutcome::Survived;
        let mut total_executed = 0u64;
        let mut total_failed = 0u64;
        let mut exit_codes = Vec::new();
        let mut observed_excerpt = String::new();
        let mut failed_test_names = Vec::new();

        for cmd_args in spec.targeted_commands {
            let out = run_isolated_cargo(&iso_repo, &iso_target, cmd_args);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let code = out.status.code().unwrap_or(-1);
            exit_codes.push(code);

            let (outcome, counts) =
                classify_cargo_test_run(out.status.code(), &stdout, &stderr, true, patch_is_noop);
            total_executed += counts.passed + counts.failed;
            total_failed += counts.failed;

            if matches!(
                outcome,
                FaultOutcome::BuildInvalid
                    | FaultOutcome::Unexercised
                    | FaultOutcome::EquivalentFault
            ) {
                aggregated_outcome = outcome;
                observed_excerpt = summarize_failure_excerpt(&stdout, &stderr);
                break;
            }
            if outcome == FaultOutcome::Detected {
                aggregated_outcome = FaultOutcome::Detected;
                failed_test_names.extend(extract_failed_test_names(&stdout));
                if observed_excerpt.is_empty() {
                    observed_excerpt = summarize_failure_excerpt(&stdout, &stderr);
                }
            }
        }

        if aggregated_outcome == FaultOutcome::Survived {
            observed_excerpt = "unchanged existing tests exited 0 (fault survived)".to_string();
        }

        // 3. For any surviving meaningful fault, verify our new negative+positive
        //    remediation fixture catches the faulted binary (`Detected`).
        let (remediation_fixture_id, post_remediation_outcome) =
            if aggregated_outcome == FaultOutcome::Survived && spec.rule_id == "RULE-CUSTODY-004" {
                let build_bin = run_isolated_cargo(
                    &iso_repo,
                    &iso_target,
                    &["build", "--offline", "--bin", "decapod"],
                );
                assert!(
                    build_bin.status.success(),
                    "failed to build faulted decapod binary: {}",
                    String::from_utf8_lossy(&build_bin.stderr)
                );
                let faulted_bin = iso_target.join("debug").join("decapod");
                let err = exercise_expired_lease_renew_fixture(&faulted_bin).expect_err(
                    "faulted RULE-CUSTODY-004 binary must fail the new expired-lease renew fixture",
                );
                assert!(
                    err.contains("expired lease renew was NOT rejected"),
                    "unexpected remediation check error: {err}"
                );
                observed_excerpt = format!("{observed_excerpt}; remediation_caught: {err}");
                (
                    Some("expired_lease_renew_negative_and_positive_fixture".to_string()),
                    FaultOutcome::Detected,
                )
            } else {
                (None, aggregated_outcome)
            };

        // 4. Restore original source bytes in the isolated copy before next fault.
        fs::write(&target_file, &original_source).expect("restore isolated source");

        assert!(
            total_executed > 0,
            "rule {} had 0 executed tests",
            spec.rule_id
        );
        assert_eq!(
            post_remediation_outcome,
            FaultOutcome::Detected,
            "rule {} did not reach post-remediation Detected state (got {:?})",
            spec.rule_id,
            post_remediation_outcome
        );

        fault_results.push(FaultResultRecord {
            rule_id: spec.rule_id.to_string(),
            attack_class: spec.attack_class.to_string(),
            boundary: spec.boundary.to_string(),
            source_file: spec.source_file.to_string(),
            source_symbol: spec.source_symbol.to_string(),
            fault_operator: spec.fault_operator.to_string(),
            targeted_existing_test: spec.targeted_existing_test.to_string(),
            positive_control: spec.positive_control.to_string(),
            fixture_id: if failed_test_names.is_empty() {
                spec.fixture_id.to_string()
            } else {
                format!(
                    "{} [failed: {}]",
                    spec.fixture_id,
                    failed_test_names.join(", ")
                )
            },
            expected_refusal: spec.expected_refusal.to_string(),
            observed_refusal: observed_excerpt,
            baseline_passed,
            initial_outcome: aggregated_outcome,
            remediation_fixture_id,
            post_remediation_outcome,
            executed_tests: total_executed,
            failed_tests: total_failed,
            exit_codes,
        });
    }

    // Verify expected classification: RULE-CUSTODY-004 survived unchanged suite and
    // was caught by our added remediation fixture; all 5 end at Detected.
    assert_eq!(fault_results.len(), 5);
    assert!(
        fault_results
            .iter()
            .any(|r| r.rule_id == "RULE-CUSTODY-004" && r.initial_outcome == FaultOutcome::Survived),
        "expected RULE-CUSTODY-004 to survive unchanged existing suite before remediation fixture"
    );
    assert!(
        fault_results
            .iter()
            .all(|r| r.post_remediation_outcome == FaultOutcome::Detected),
        "every rule must be Detected after remediation"
    );

    let git_head = Command::new("git")
        .current_dir(&manifest_dir)
        .args(["rev-parse", "HEAD"])
        .bounded_output(BUILD_TIMEOUT)
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string());

    let report = GovernanceFaultReport {
        schema_version: REPORT_SCHEMA_VERSION.to_string(),
        kind: "governance_fault_detection_report".to_string(),
        issue: "https://github.com/DecapodLabs/decapod/issues/1795".to_string(),
        scope_note: "Measures whether existing and remediation tests force existing Decapod rules to reject injected rule defects. Makes no general formal-soundness claim and never infers assurance from test names, vector counts, or unexecuted jobs.".to_string(),
        build_identity: BuildIdentity {
            package_name: env!("CARGO_PKG_NAME").to_string(),
            package_version: env!("CARGO_PKG_VERSION").to_string(),
            manifest_dir: manifest_dir.display().to_string(),
            git_head,
        },
        boundary_inventory: boundary_inventory(),
        fault_results,
    };

    let report_path = std::env::var_os("GOVERNANCE_FAULT_REPORT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir
                .join("target")
                .join("governance-fault-detection")
                .join("report-1795.json")
        });
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).expect("create report parent");
    }
    let serialized = serde_json::to_string_pretty(&report).expect("serialize report");
    fs::write(&report_path, &serialized).expect("write report");
    let parsed: GovernanceFaultReport =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read report"))
            .expect("round-trip parse report");
    assert_eq!(parsed.schema_version, "1.0.0");
    assert_eq!(parsed.fault_results.len(), 5);
    assert_eq!(parsed.boundary_inventory.len(), 7);
}
