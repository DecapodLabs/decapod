//! Real consumer runtime contracts. Shared support contains no test cases.
#![cfg(any(target_os = "linux", target_os = "macos"))]

use decapod::core::bounded_process::Cancellation;
use decapod::core::readiness_evidence::{Conclusion, Observation, Provider, SuiteEvidence};
use decapod::core::readiness_profile;
use decapod::core::repository_profile;
use decapod::core::validation_runtime::{self as runtime, ValidationReport};
use std::fs;

#[path = "common/consumer_readiness_fixture.rs"]
mod consumer_readiness_fixture;
use consumer_readiness_fixture::{Fixture, LocalHost, WORKFLOW, git};

impl Fixture {
    fn with_accepted_profile(id: &str) -> Self {
        Self::with_accepted_config(Some(&format!(
            "[proof.required_profile]\nid = {id:?}\nreviewed_sources_digest = {:?}\n",
            format!("sha256:{}", "0".repeat(64)),
        )))
    }
}

fn blocked(report: &ValidationReport, expected: &str) {
    assert!(!report.ready, "unexpected readiness: {report:?}");
    assert_eq!(report.exit_code(), 1);
    assert!(
        report
            .blockers
            .iter()
            .any(|blocker| blocker.detail.contains(expected) || blocker.code.contains(expected)),
        "expected {expected}: {report:?}"
    );
}

#[test]
fn fresh_non_cargo_consumer_executes_every_required_source_check_in_snapshot() {
    let fixture = Fixture::new();
    assert!(!fixture.options.root.join("Cargo.toml").exists());
    let head = git(&fixture.options.root, &["rev-parse", "HEAD"]);
    let upstream = git(&fixture.upstream, &["rev-parse", "refs/heads/main"]);
    let mut host = fixture.host();
    let report = fixture.validate(&mut host);
    assert!(report.ready, "{report:?}");
    assert_eq!(report.exit_code(), 0);
    assert_eq!(report.scope, "repository_local_readiness");
    assert_eq!(host.execution_attempts, 1);
    let directory = report.evidence_path.unwrap();
    let evidence: SuiteEvidence =
        serde_json::from_slice(&fs::read(directory.join("observations.json")).unwrap()).unwrap();
    assert_eq!(evidence.results.len(), 2);
    let mut receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(directory.join("completion/native-readiness-receipt.json")).unwrap(),
    )
    .unwrap();
    let verdict: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("completion/verdict.json")).unwrap())
            .unwrap();
    assert_eq!(verdict["ready"], true);
    assert!(!directory.join("verdict.json").exists());
    assert_eq!(receipt["kind"], "native_readiness_receipt");
    assert_eq!(receipt["status"], "ok");
    assert_eq!(receipt["scope"], report.scope);
    assert_eq!(receipt["material"]["head_revision"], head);
    assert_eq!(receipt["material"]["accepted_base_revision"], upstream);
    assert_eq!(
        receipt["evidence_digest"],
        decapod::core::readiness_evidence::digest(&evidence).unwrap()
    );
    let governance = fs::read(fixture.options.root.join(".decapod/governance.json")).unwrap();
    use sha2::{Digest, Sha256};
    assert_eq!(
        receipt["governance_digest"],
        format!("sha256:{:x}", Sha256::digest(&governance))
    );
    let document: serde_json::Value = serde_json::from_slice(&governance).unwrap();
    assert_eq!(
        receipt["trajectory_digest"],
        decapod::core::readiness_evidence::digest(&document["sections"]["trajectory"]).unwrap()
    );
    let receipt_hash = receipt
        .as_object_mut()
        .unwrap()
        .remove("receipt_hash")
        .unwrap();
    assert_eq!(
        receipt_hash,
        decapod::core::readiness_evidence::digest(&receipt).unwrap()
    );
    assert!(!directory.starts_with(&fixture.options.root));
    for result in &evidence.results {
        assert_eq!(result.executor, Provider::Native);
        assert_eq!(result.key.profile, repository_profile::REQUIRED_PROFILE);
        assert!(matches!(
            result.observation,
            Observation::Executed {
                conclusion: Conclusion::Success,
                exit_code: Some(0),
                ..
            }
        ));
    }
    for (index, result) in evidence.results.iter().enumerate() {
        let diagnostic: serde_json::Value = serde_json::from_slice(
            &fs::read(directory.join(format!("native-{index}.diagnostic.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(diagnostic["schema_version"], "1.0.0");
        assert_eq!(
            diagnostic["check"],
            serde_json::to_value(&result.key).unwrap()
        );
        assert_eq!(diagnostic["termination"], "exited");
        assert!(diagnostic["error"].is_null());
        for extension in ["stdout", "stderr", "steps"] {
            let file = diagnostic[extension].as_str().unwrap();
            assert!(directory.join(file).is_file());
        }
    }
    let stdout = [0, 1]
        .map(|index| fs::read_to_string(directory.join(format!("native-{index}.stdout"))).unwrap())
        .join("\n");
    assert!(stdout.contains("consumer-source-inspected"));
    assert!(stdout.contains("second-source-inspected"));
    assert!(
        report
            .stage_dispositions
            .iter()
            .any(|stage| stage.evidence_scope == "no_hosted_ci_or_release_assurance")
    );
    assert_eq!(git(&fixture.options.root, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        git(&fixture.upstream, &["rev-parse", "refs/heads/main"]),
        upstream
    );
    assert!(git(&fixture.options.root, &["status", "--porcelain"]).is_empty());
}

#[test]
fn governed_intent_survives_source_failure_repair_and_native_proof() {
    let fixture = Fixture::new();
    fs::write(fixture.options.root.join("payload.txt"), "broken\n").unwrap();
    // Use Decapod's real intent custody, not a synthetic receipt or an agent's
    // completion claim. The source adapter below executes the actual checks.
    let intent = "Repair the consumer source while preserving both required checks";
    decapod::core::trajectory::init_trajectory(
        &fixture.options.root,
        decapod::core::trajectory::TrajectoryInit {
            run_id: "consumer_repair".into(),
            task_id: Some("consumer_source_repair".into()),
            intent_id: None,
            original_intent: intent.into(),
            derived_intent: "Restore payload.txt and collect fresh source-bound proof".into(),
            active_boundaries: vec!["payload.txt".into()],
            repo_scope: vec!["payload.txt".into()],
            destination: Some("validated source repair with an inspectable receipt".into()),
            current_phase: Some("execution".into()),
            next_transitions: vec!["validation".into()],
            blockers: vec![],
        },
    )
    .unwrap();
    fixture.commit("introduce source defect");
    let failed_head = git(&fixture.options.root, &["rev-parse", "HEAD"]);
    let mut host = fixture.host();
    let failed = fixture.validate(&mut host);
    assert!(!failed.ready, "{failed:?}");
    assert_eq!(
        host.execution_attempts, 1,
        "failure must come from real execution"
    );
    let failed_path = failed.evidence_path.as_ref().unwrap();
    assert!(
        !failed_path
            .join("completion/native-readiness-receipt.json")
            .exists()
    );
    assert!(!failed_path.join("completion/verdict.json").exists());
    assert!(!failed_path.join("historical-proof.json").exists());
    let evidence: SuiteEvidence =
        serde_json::from_slice(&fs::read(failed_path.join("observations.json")).unwrap()).unwrap();
    assert_eq!(
        evidence.results.len(),
        2,
        "every reviewed command must execute"
    );
    assert!(
        evidence
            .results
            .iter()
            .all(|result| matches!(result.observation, Observation::Executed { .. }))
    );
    let executed: Vec<_> = evidence
        .results
        .iter()
        .filter(|result| matches!(result.observation, Observation::Executed { .. }))
        .collect();
    for (index, result) in executed.iter().enumerate() {
        let diagnostic: serde_json::Value = serde_json::from_slice(
            &fs::read(failed_path.join(format!("native-{index}.diagnostic.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(
            diagnostic["check"],
            serde_json::to_value(&result.key).unwrap()
        );
        assert_eq!(diagnostic["termination"], "exited");
    }
    let stderr = fs::read_to_string(failed_path.join("native-1.stderr")).unwrap_or_default()
        + &fs::read_to_string(failed_path.join("native-0.stderr")).unwrap_or_default();
    assert!(
        stderr.contains("consumer source is invalid"),
        "{failed:?}: {stderr}"
    );
    fs::write(fixture.options.root.join("payload.txt"), "ready\n").unwrap();
    decapod::core::trajectory::record_trajectory(
        &fixture.options.root,
        "consumer_repair",
        decapod::core::trajectory::TrajectoryUpdate {
            modified_files: vec!["payload.txt".into()],
            current_phase: Some("validation".into()),
            tool_calls: vec!["repair payload.txt after the required source check failed".into()],
            ..Default::default()
        },
    )
    .unwrap();
    fixture.commit("repair actual source defect");
    assert_ne!(
        git(&fixture.options.root, &["rev-parse", "HEAD"]),
        failed_head
    );
    let repaired = fixture.validate(&mut fixture.host());
    assert!(repaired.ready, "{repaired:?}");
    assert_ne!(failed.evidence_path, repaired.evidence_path);
    let directory = repaired.evidence_path.unwrap();
    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(directory.join("completion/native-readiness-receipt.json")).unwrap(),
    )
    .unwrap();
    let governance = fs::read(fixture.options.root.join(".decapod/governance.json")).unwrap();
    let document: serde_json::Value = serde_json::from_slice(&governance).unwrap();
    let trajectory =
        decapod::core::trajectory::load_trajectory(&fixture.options.root, "consumer_repair")
            .unwrap();
    assert_eq!(trajectory.original_intent, intent);
    assert_eq!(
        trajectory.task_id.as_deref(),
        Some("consumer_source_repair")
    );
    assert_eq!(trajectory.modified_files, ["payload.txt"]);
    assert_eq!(receipt["status"], "ok");
    assert_eq!(receipt["scope"], "repository_local_readiness");
    assert_eq!(
        receipt["material"]["head_revision"],
        git(&fixture.options.root, &["rev-parse", "HEAD"])
    );
    use sha2::{Digest, Sha256};
    assert_eq!(
        receipt["governance_digest"],
        format!("sha256:{:x}", Sha256::digest(&governance))
    );
    assert_eq!(
        receipt["trajectory_digest"],
        decapod::core::readiness_evidence::digest(&document["sections"]["trajectory"]).unwrap()
    );
    assert!(!directory.starts_with(&fixture.options.root));
    for artifact in [
        "completion/native-readiness-receipt.json",
        "completion/verdict.json",
        "historical-proof.json",
    ] {
        assert!(directory.join(artifact).is_file(), "{artifact}");
    }
}

#[test]
fn unknown_profile_is_rejected_before_adapter_selection() {
    let mut fixture = Fixture::new();
    fixture.proof.required_profile.as_mut().unwrap().id = "repo.pr-readiness.v999".into();
    fixture.save_config();
    fixture.commit("unsupported profile");
    let mut host = fixture.host();
    blocked(&fixture.validate(&mut host), "UNKNOWN_PROOF_PROFILE");
    assert_eq!(host.selections, 0);
}

#[test]
fn changed_workflow_and_uppercase_justfile_require_source_review() {
    for path in [WORKFLOW, "Justfile"] {
        let fixture = Fixture::new();
        let sources =
            runtime::discover_source_inventory(&fixture.options, &Cancellation::default()).unwrap();
        assert!(
            sources.files.contains_key(path),
            "recipe omitted from discovery: {path}"
        );
        let mut bytes = fs::read(fixture.options.root.join(path)).unwrap();
        bytes.extend_from_slice(b"\n# newly required source check\n");
        fs::write(fixture.options.root.join(path), bytes).unwrap();
        fixture.commit("change reviewed build definition");
        let mut host = fixture.host();
        blocked(
            &fixture.validate(&mut host),
            "PROOF_PROFILE_RECONCILIATION_REQUIRED",
        );
        assert_eq!(host.selections, 0);
    }
}

#[test]
fn changed_command_and_execution_fields_require_explicit_review() {
    for field in [
        "args",
        "program",
        "environment",
        "toolchain",
        "directory",
        "timeout",
        "required",
    ] {
        let mut fixture = Fixture::new();
        let execution = fixture
            .proof
            .repository_contract
            .as_mut()
            .unwrap()
            .executions
            .get_mut("inspect-source")
            .unwrap();
        match field {
            "args" => fixture.proof.commands[0].args.push("unreviewed".into()),
            "program" => execution.program = "/unreviewed/python3".into(),
            "environment" => {
                execution
                    .environment
                    .insert("NEW_INPUT".into(), "changed".into());
            }
            "toolchain" => {
                execution
                    .toolchain_files
                    .insert("new-runtime".into(), "/unreviewed/runtime".into());
            }
            "directory" => execution.directory = "scripts".into(),
            "timeout" => execution.timeout_seconds += 1,
            "required" => fixture.proof.commands[0].required = false,
            _ => unreachable!(),
        }
        fixture.save_config();
        fixture.commit("change proof contract without review");
        let mut host = fixture.host();
        blocked(&fixture.validate(&mut host), "STALE_COMMAND_CONTRACT");
        assert_eq!(host.selections, 0, "{field} escaped review binding");
    }
}

#[test]
fn reviewed_but_missing_duplicate_or_unknown_coverage_fails_closed() {
    for defect in [
        "missing-workflow",
        "duplicate-workflow",
        "duplicate-command",
        "unknown-command",
        "unmapped-command",
        "unknown-source",
    ] {
        let mut fixture = Fixture::new();
        let coverage = &mut fixture
            .proof
            .repository_contract
            .as_mut()
            .unwrap()
            .source_coverage;
        match defect {
            "missing-workflow" => coverage.clear(),
            "duplicate-workflow" => coverage.push(coverage[0].clone()),
            "duplicate-command" => coverage[0].commands.push("inspect-source".into()),
            "unknown-command" => coverage[0].commands.push("not-a-proof".into()),
            "unmapped-command" => {
                coverage[0].commands.pop();
            }
            "unknown-source" => coverage[0].source = ".github/workflows/absent.yml".into(),
            _ => unreachable!(),
        }
        fixture.review_commands();
        fixture.save_config();
        fixture.commit("review structurally incomplete coverage");
        let mut host = fixture.host();
        blocked(&fixture.validate(&mut host), "REPOSITORY_CONTRACT_INVALID");
        assert_eq!(host.selections, 0, "{defect} admitted");
    }
}

#[test]
fn reviewed_empty_optional_and_duplicate_required_commands_are_not_admitted() {
    for defect in ["empty", "optional", "duplicate", "missing-execution"] {
        let mut fixture = Fixture::new();
        match defect {
            "empty" => fixture.proof.commands.clear(),
            "optional" => fixture.proof.commands[0].required = false,
            "duplicate" => fixture
                .proof
                .commands
                .push(fixture.proof.commands[0].clone()),
            "missing-execution" => {
                fixture
                    .proof
                    .repository_contract
                    .as_mut()
                    .unwrap()
                    .executions
                    .remove("inspect-source");
            }
            _ => unreachable!(),
        }
        fixture.review_commands();
        fixture.save_config();
        fixture.commit("review invalid required proof set");
        let mut host = fixture.host();
        blocked(&fixture.validate(&mut host), "REPOSITORY_CONTRACT_INVALID");
        assert_eq!(host.selections, 0, "{defect} admitted");
    }
}

#[test]
fn accepted_strict_profile_cannot_be_downgraded_by_candidate_local_contract() {
    let fixture = Fixture::with_accepted_profile(readiness_profile::REQUIRED_PROFILE);
    let mut host = fixture.host();
    blocked(&fixture.validate(&mut host), "REQUIRED_PROFILE_DOWNGRADE");
    assert_eq!(host.selections, 0);
}

#[test]
fn candidate_repinning_cannot_replace_an_accepted_required_local_check() {
    let mut fixture = Fixture::new();
    // Establish the reviewed local contract on the independently observed base.
    git(&fixture.options.root, &["push", "origin", "HEAD:main"]);
    git(&fixture.options.root, &["fetch", "origin", "main"]);
    fs::write(fixture.options.root.join("second.txt"), "invalid source\n").unwrap();
    let command = fixture
        .proof
        .commands
        .iter_mut()
        .find(|command| command.name == "inspect-second")
        .unwrap();
    command.args = [
        "-I",
        "-S",
        "-B",
        "-c",
        "print('claimed success without inspection')",
    ]
    .map(str::to_owned)
    .into();
    fixture.review_commands();
    fixture.save_config();
    fixture.commit("replace accepted check and repin its claimed review");
    let mut host = fixture.host();
    blocked(
        &fixture.validate(&mut host),
        "ACCEPTED_REPOSITORY_CONTRACT_CHANGED",
    );
    assert_eq!(host.selections, 0);
}

#[test]
fn repinned_local_policy_removal_execution_and_coverage_changes_fail_before_selection() {
    for change in [
        "remove",
        "environment",
        "toolchain",
        "timeout",
        "coverage",
        "ci-only",
    ] {
        let mut fixture = Fixture::new();
        git(&fixture.options.root, &["push", "origin", "HEAD:main"]);
        git(&fixture.options.root, &["fetch", "origin", "main"]);
        let contract = fixture.proof.repository_contract.as_mut().unwrap();
        match change {
            "remove" => {
                fixture
                    .proof
                    .commands
                    .retain(|command| command.name != "inspect-second");
                contract.executions.remove("inspect-second");
                contract.source_coverage[0]
                    .commands
                    .retain(|name| name != "inspect-second");
            }
            "environment" => {
                contract
                    .executions
                    .get_mut("inspect-second")
                    .unwrap()
                    .environment
                    .insert("UNREVIEWED_INPUT".into(), "replacement".into());
            }
            "toolchain" => {
                contract
                    .executions
                    .get_mut("inspect-second")
                    .unwrap()
                    .toolchain_files
                    .insert("replacement".into(), "/unreviewed/runtime".into());
            }
            "timeout" => {
                contract
                    .executions
                    .get_mut("inspect-second")
                    .unwrap()
                    .timeout_seconds += 1
            }
            "coverage" => contract.source_coverage[0]
                .commands
                .retain(|name| name != "inspect-second"),
            "ci-only" => {
                contract.source_coverage[0].commands.clear();
                contract.source_coverage[0].ci_only = true;
                contract.source_coverage[0].reason =
                    Some("candidate claims external coverage".into());
            }
            _ => unreachable!(),
        }
        fixture.review_commands();
        fixture.save_config();
        fixture.commit("repin replacement local verification policy");
        let mut host = fixture.host();
        blocked(
            &fixture.validate(&mut host),
            "ACCEPTED_REPOSITORY_CONTRACT_CHANGED",
        );
        assert_eq!(host.selections, 0, "{change}");
        assert_eq!(host.execution_attempts, 0, "{change}");
    }
}

#[test]
fn accepted_local_contract_executes_current_checks_and_never_reuses_an_old_source_pass() {
    let fixture = Fixture::new();
    git(&fixture.options.root, &["push", "origin", "HEAD:main"]);
    git(&fixture.options.root, &["fetch", "origin", "main"]);
    let mut host = fixture.host();
    let historical = fixture.validate(&mut host);
    assert!(historical.ready, "{historical:?}");
    assert_eq!(historical.scope, repository_profile::READINESS_SCOPE);
    assert_eq!(host.execution_attempts, 1);
    fs::write(
        fixture.options.root.join("second.txt"),
        "changed after successful proof\n",
    )
    .unwrap();
    fixture.commit("invalidate source evidence");
    let mut host = fixture.host();
    let current = fixture.validate(&mut host);
    assert!(!current.ready, "{current:?}");
    assert!(!current.proof_ready);
    assert_eq!(host.execution_attempts, 1);
    assert!(
        historical.ready,
        "historical observations must remain intact"
    );
}

#[test]
fn candidate_can_add_required_local_checks_without_replacing_accepted_ones() {
    let mut fixture = Fixture::new();
    git(&fixture.options.root, &["push", "origin", "HEAD:main"]);
    git(&fixture.options.root, &["fetch", "origin", "main"]);
    let mut additional = fixture.proof.commands[0].clone();
    let contract = fixture.proof.repository_contract.as_mut().unwrap();
    let execution = contract.executions[&additional.name].clone();
    additional.name = "inspect-third".into();
    contract
        .executions
        .insert(additional.name.clone(), execution);
    contract.source_coverage[0]
        .commands
        .push(additional.name.clone());
    fixture.proof.commands.push(additional);
    fixture.review_commands();
    fixture.save_config();
    fixture.commit("add an extra required local check");
    let mut host = fixture.host();
    let report = fixture.validate(&mut host);
    assert!(report.ready, "{report:?}");
    assert_eq!(report.scope, repository_profile::READINESS_SCOPE);
    assert_eq!(host.execution_attempts, 1);
}

#[test]
fn source_change_during_selection_prevents_any_execution() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    host.change_source_after_selection = true;
    let report = fixture.validate(&mut host);
    assert!(!report.ready, "{report:?}");
    assert_eq!(host.execution_attempts, 0);
}

#[test]
fn accepted_target_moving_after_successful_checks_prevents_readiness() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    host.move_target_after_execution = true;
    let report = fixture.validate(&mut host);
    assert!(!report.ready, "{report:?}");
    assert_eq!(host.execution_attempts, 1);
    assert!(
        report
            .evidence_path
            .unwrap()
            .join("observations.json")
            .is_file()
    );
}

#[test]
fn proof_authorization_denial_stops_before_native_execution() {
    let fixture = Fixture::new();
    let mut host = LocalHost::new(&fixture, true);
    let report = fixture.validate(&mut host);
    blocked(&report, "fixture denied proof execution");
    let directory = report.evidence_path.unwrap();
    assert!(!directory.join("observations.json").exists());
    assert!(!directory.join("native-0.stdout").exists());
}

#[test]
fn python_permission_does_not_relax_the_existing_strict_native_capability() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    host.force_strict_capability = true;
    blocked(&fixture.validate(&mut host), "interpreter");
}

#[test]
fn repository_local_profile_cannot_claim_buildkite_provider_execution() {
    let mut fixture = Fixture::new();
    fixture.proof.validation_buildkite_preflights = true;
    fixture.save_config();
    fixture.commit("request unsupported provider");
    let mut host = fixture.host();
    blocked(
        &fixture.validate(&mut host),
        "REPOSITORY_PROFILE_PROVIDER_UNSUPPORTED",
    );
    assert_eq!(host.selections, 0);
}

#[test]
fn public_dispatch_selects_repository_host_before_reading_a_cargo_manifest() {
    // This guards the public CLI/RPC routing seam separately from the real
    // runtime execution above; it is not represented as a CLI end-to-end run.
    let source = include_str!("../src/decapod/lib.rs");
    let body = source
        .split_once("fn run_current_workbranch_readiness_with(")
        .unwrap()
        .1
        .split_once("\nfn ")
        .unwrap()
        .0;
    let repository_host = body
        .find("runtime::repository_host::RepositoryHost::new(")
        .unwrap();
    let cargo_manifest = body.find("workspace_root.join(\"Cargo.toml\")").unwrap();
    assert!(repository_host < cargo_manifest);
    assert!(body[repository_host..cargo_manifest].contains("return Ok(report)"));
}

#[test]
fn malformed_accepted_configuration_is_not_treated_as_first_adoption() {
    let fixture = Fixture::with_accepted_config(Some("[proof.required_profile\nid = 'broken'\n"));
    let mut host = fixture.host();
    let report = fixture.validate(&mut host);
    assert!(!report.ready, "{report:?}");
    assert!(!report.blockers.is_empty());
    assert_eq!(host.selections, 0);
    assert_eq!(host.execution_attempts, 0);
}

#[test]
fn configuration_change_after_successful_execution_invalidates_readiness() {
    let fixture = Fixture::new();
    let mut host = fixture.host();
    host.change_config_after_execution = true;
    let report = fixture.validate(&mut host);
    assert!(!report.ready, "{report:?}");
    assert_eq!(host.execution_attempts, 1);
    let directory = report.evidence_path.unwrap();
    assert!(
        !directory
            .join("completion/native-readiness-receipt.json")
            .exists()
    );
    let evidence: SuiteEvidence =
        serde_json::from_slice(&fs::read(directory.join("observations.json")).unwrap()).unwrap();
    assert_eq!(evidence.results.len(), 2);
    assert!(evidence.results.iter().all(|result| matches!(
        result.observation,
        Observation::Executed {
            conclusion: Conclusion::Success,
            exit_code: Some(0),
            ..
        },
    )));
}
