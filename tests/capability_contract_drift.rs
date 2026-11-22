use decapod::core::capability_contract::{
    CAPABILITY_CONTRACT_REL_PATH, CompositeConsequenceState, ContractStatus, DriftKind,
    PreservedOutcomeClass, canonical_cloud_task_contract,
};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_decapod"))
}

fn run_cli_with_env(dir: &Path, args: &[&str], extra_envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(bin());
    cmd.current_dir(dir)
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .env_remove("DECAPOD_VALIDATE_SKIP_TOOLING_GATES")
        .env_remove("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES")
        .env_remove("GITHUB_ACTIONS")
        .env_remove("BUILDKITE")
        .env("DECAPOD_AGENT_ID", "contract-tester")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .args(args);
    for (k, v) in extra_envs {
        cmd.env(k, v);
    }
    cmd.output().expect("execute decapod")
}

fn run_cli(dir: &Path, args: &[&str]) -> Output {
    run_cli_with_env(dir, args, &[])
}

fn init_local_repo(dir: &Path) {
    let git_init = Command::new("git")
        .current_dir(dir)
        .args(["init", "-q", "-b", "main"])
        .output()
        .unwrap();
    assert!(git_init.status.success());
    let init = run_cli(
        dir,
        &[
            "init",
            "--force",
            "--no-ci",
            "--no-specs",
            "--no-container-workspaces",
        ],
    );
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
}

#[test]
fn cli_capabilities_contracts_fixture_verifies_all_falsifiers_and_traces() {
    let tmp = TempDir::new().unwrap();
    init_local_repo(tmp.path());

    let out = run_cli(
        tmp.path(),
        &["capabilities", "contracts", "--fixture", "--format", "json"],
    );
    assert!(
        out.status.success(),
        "cli failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: Value = serde_json::from_slice(&out.stdout).expect("parse JSON output");
    assert_eq!(parsed["local_offline_usable"], true);
    let fixture = &parsed["fixture"];
    assert_eq!(fixture["failed_scenarios"], 0);
    let total = fixture["total_scenarios"].as_u64().unwrap();
    assert!(total >= 16);
    assert_eq!(fixture["passed_scenarios"], total);

    let scenarios = fixture["scenarios"].as_array().unwrap();
    // Verify specific falsifiers & consequence states end-to-end.
    let severed = scenarios
        .iter()
        .find(|s| s["scenario_id"] == "two_step_fault_severed_ack_blind_replay_held")
        .unwrap();
    assert_eq!(severed["report"]["status"], "incompatible");
    assert_eq!(severed["report"]["consequence"]["state"], "held");
    assert_eq!(severed["report"]["consequence"]["goal_completed"], false);
    assert_eq!(
        severed["report"]["consequence"]["blind_replay_rejected"],
        true
    );

    let compensated = scenarios
        .iter()
        .find(|s| s["scenario_id"] == "two_step_fault_compensated_with_retained_fault")
        .unwrap();
    assert_eq!(compensated["report"]["consequence"]["state"], "compensated");
    assert_eq!(
        compensated["report"]["consequence"]["goal_completed"],
        false
    );
    assert_eq!(
        compensated["report"]["consequence"]["original_fault_retained"]["code"],
        "proof_record_rejected"
    );

    let local_offline = scenarios
        .iter()
        .find(|s| s["scenario_id"] == "unrelated_local_offline_repository_usable")
        .unwrap();
    assert_eq!(local_offline["report"]["status"], "currently_verified");
    assert_eq!(local_offline["report"]["local_offline_usable"], true);
}

#[test]
fn rpc_interop_capability_contracts_rejects_tampered_and_self_certified_inputs() {
    let tmp = TempDir::new().unwrap();
    init_local_repo(tmp.path());

    let mut tampered = canonical_cloud_task_contract();
    tampered.declared_status = ContractStatus::CurrentlyVerified;
    tampered.negative_cases.clear();
    tampered.operation_name = "task.renamed_unauthorized".to_string();

    let params = json!({
        "fixture": false,
        "contract": tampered
    })
    .to_string();
    let out = run_cli(
        tmp.path(),
        &[
            "rpc",
            "--op",
            "interop.capability_contracts",
            "--params",
            &params,
        ],
    );
    assert!(
        out.status.success(),
        "rpc failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rpc_resp: Value = serde_json::from_slice(&out.stdout).expect("parse RPC output");
    assert_eq!(rpc_resp["success"], true);
    let result = &rpc_resp["result"];
    assert_eq!(result["input_origin"], "caller_supplied");
    assert_eq!(result["canonical_state_verified"], false);
    assert_eq!(result["inspection"]["report"]["status"], "incompatible");
    let findings = result["inspection"]["report"]["findings"]
        .as_array()
        .unwrap();
    assert!(
        findings.iter().any(
            |f| f["kind"] == serde_json::to_value(DriftKind::SelfVerificationRejected).unwrap()
        )
    );
    assert!(
        findings
            .iter()
            .any(|f| f["kind"] == serde_json::to_value(DriftKind::DroppedNegativeCase).unwrap())
    );
    assert!(
        findings
            .iter()
            .any(|f| f["kind"] == serde_json::to_value(DriftKind::RenamedOperation).unwrap())
    );
}

#[test]
fn cli_contract_file_check_and_local_offline_repo_usability() {
    let tmp = TempDir::new().unwrap();
    init_local_repo(tmp.path());

    // An unrelated local repository with no Cloud binding stays usable offline
    // without touching any Cloud service.
    let inspect_local = run_cli(
        tmp.path(),
        &["capabilities", "contracts", "--format", "json"],
    );
    assert!(inspect_local.status.success());
    let local_json: Value = serde_json::from_slice(&inspect_local.stdout).unwrap();
    assert_eq!(local_json["repository_backend"], "local");
    assert_eq!(local_json["local_offline_usable"], true);
    assert_eq!(local_json["report"]["status"], "declared");

    // Local todo add remains usable offline.
    let add_out = run_cli(tmp.path(), &["todo", "add", "offline local work"]);
    assert!(
        add_out.status.success(),
        "local offline todo add failed: {}",
        String::from_utf8_lossy(&add_out.stderr)
    );

    // Inspecting a pinned untampered Cloud contract inside a local-only repo
    // reports Unsupported for Cloud while preserving local_offline_usable=true.
    let cloud_contract = canonical_cloud_task_contract();
    let contract_file = tmp.path().join("cloud-contract.json");
    fs::write(
        &contract_file,
        serde_json::to_vec_pretty(&cloud_contract).unwrap(),
    )
    .unwrap();
    let check_cloud_local = run_cli(
        tmp.path(),
        &[
            "capabilities",
            "contracts",
            "--contract",
            contract_file.to_str().unwrap(),
            "--format",
            "json",
        ],
    );
    assert!(check_cloud_local.status.success());
    let cloud_local_json: Value = serde_json::from_slice(&check_cloud_local.stdout).unwrap();
    assert_eq!(cloud_local_json["report"]["status"], "unsupported");
    assert_eq!(cloud_local_json["local_offline_usable"], true);

    // Dropping a negative case or altering policy in .decapod/contracts/capability_contract.json
    // causes the validation gate to fail closed against accepted policy.
    let mut drifted = cloud_contract;
    drifted.negative_cases.clear();
    let repo_contract_path = tmp.path().join(CAPABILITY_CONTRACT_REL_PATH);
    fs::create_dir_all(repo_contract_path.parent().unwrap()).unwrap();
    fs::write(
        &repo_contract_path,
        serde_json::to_vec_pretty(&drifted).unwrap(),
    )
    .unwrap();
    let val_err =
        decapod::core::capability_contract::validate_repo_contract_boundary(tmp.path(), tmp.path())
            .unwrap_err();
    assert!(
        val_err.to_string().contains("CAPABILITY_CONTRACT_DRIFT"),
        "expected CAPABILITY_CONTRACT_DRIFT, got {val_err}"
    );

    // Real CLI validation (`govern project-check` / `validate`) owns required gate
    // failure when a drifted capability contract is present in the repository.
    let val_cli = run_cli_with_env(
        tmp.path(),
        &["govern", "project-check", "--format", "json"],
        &[("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")],
    );
    assert!(
        !val_cli.status.success(),
        "expected govern project-check to fail on drifted capability contract"
    );
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&val_cli.stdout),
        String::from_utf8_lossy(&val_cli.stderr)
    );
    assert!(
        combined.contains("CAPABILITY_CONTRACT_DRIFT"),
        "expected CAPABILITY_CONTRACT_DRIFT in validation output: {combined}"
    );

    // Ensure preserved outcome & consequence enum variants are wired publicly.
    let _ = (
        CompositeConsequenceState::Prepared,
        PreservedOutcomeClass::Pending,
    );
}
