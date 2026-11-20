//! Real consumer runtime contracts. Shared support contains no test cases.
#![cfg(any(target_os = "linux", target_os = "macos"))]

use decapod::core::bounded_process::{BoundedCommand, Cancellation};
use decapod::core::readiness_evidence::{Conclusion, Observation, Provider, SuiteEvidence};
use decapod::core::readiness_profile;
use decapod::core::repository_profile;
use decapod::core::validation_runtime::{self as runtime, ValidationReport};
use std::fs;

#[path = "common/consumer_readiness_fixture.rs"]
mod consumer_readiness_fixture;
use consumer_readiness_fixture::{Fixture, LocalHost, WORKFLOW, git};

#[test]
fn admitted_cli_source_file_pass_cannot_mint_verified_workunit() {
    use consumer_readiness_fixture::{CONFIG, governed_command};
    use std::{path::Path, time::Duration};

    let fixture = Fixture::new();
    let storage = fixture._storage.path();
    let root = &fixture.options.root;
    fs::create_dir(storage.join("home")).unwrap();
    git(root, &["checkout", "-B", "main"]);
    // Use the same genuine first-adoption path as fresh_docs_readiness.
    fs::remove_file(root.join(CONFIG)).unwrap();
    let run = |at: &Path, args: &[&str]| {
        governed_command(at, storage, args)
            .bounded_output(Duration::from_secs(30))
            .unwrap()
    };
    let ok = |at: &Path, args: &[&str]| {
        let output = run(at, args);
        assert!(
            output.status.success(),
            "{args:?}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout)
            .unwrap_or(serde_json::Value::Null)
    };
    ok(
        root,
        &[
            "init",
            "--force",
            "--backend",
            "local",
            "--no-container-workspaces",
        ],
    );
    git(root, &["add", "-A"]);
    git(
        root,
        &["commit", "-m", "test: accept isolated workunit fixture"],
    );
    git(root, &["push", "origin", "main"]);
    // Match the existing CLI fixture's supported repository identity. No forge
    // operation or remote execution is performed by this negative control.
    git(
        root,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/consumer/project.git",
        ],
    );
    let task = ok(
        root,
        &[
            "todo",
            "add",
            "test: reject counterfeit workunit proof",
            "--format",
            "json",
        ],
    );
    let id = task["id"].as_str().unwrap();
    ok(root, &["todo", "claim", "--id", id]);
    let branch = format!("agent/docs-bootstrap/{id}-counterfeit");
    let workspace = ok(root, &["workspace", "ensure", "--branch", &branch]);
    let workspace = std::path::PathBuf::from(workspace["worktree_path"].as_str().unwrap());
    ok(&workspace, &["infer", "orientation", "--task-id", id]);
    ok(&workspace, &["govern", "plan", "approve"]);
    let ready = ok(
        &workspace,
        &["govern", "plan", "check-execute", "--todo-id", id],
    );
    assert_eq!(ready["marker"], "EXECUTION_READY");
    let governance = ok(&workspace, &["govern", "artifacts", "status"]);
    let intent = governance["sections"]["trajectory"]["intent_id"]
        .as_str()
        .unwrap();
    ok(
        &workspace,
        &[
            "govern",
            "workunit",
            "init",
            "--task-id",
            id,
            "--intent-ref",
            intent,
        ],
    );
    ok(
        &workspace,
        &[
            "govern",
            "workunit",
            "attach-spec",
            "--task-id",
            id,
            "--ref",
            ".decapod/managed/specs/INTENT.md",
        ],
    );
    ok(
        &workspace,
        &[
            "govern",
            "capsule",
            "query",
            "--topic",
            "canonical observed workunit proof",
            "--scope",
            "interfaces",
            "--task-id",
            id,
            "--workunit-id",
            id,
            "--write",
        ],
    );
    let capsule_ref = format!(".decapod/managed/context/{id}.json");
    ok(
        &workspace,
        &[
            "govern",
            "workunit",
            "attach-state",
            "--task-id",
            id,
            "--ref",
            &capsule_ref,
        ],
    );
    ok(
        &workspace,
        &[
            "govern",
            "workunit",
            "set-proof-plan",
            "--task-id",
            id,
            "--gate",
            "validate_passes",
        ],
    );
    let checkpoint = ok(
        &workspace,
        &[
            "govern",
            "workunit",
            "checkpoint",
            "--task-id",
            id,
            "--operation-id",
            "counterfeit-current",
        ],
    );
    assert_eq!(checkpoint["resumable"], true);
    for status in ["executing", "claimed"] {
        ok(
            &workspace,
            &[
                "govern",
                "workunit",
                "transition",
                "--task-id",
                id,
                "--to",
                status,
            ],
        );
    }
    // A real file passes the artifact-integrity check. It is still only an
    // agent declaration, so test the canonical producer gate beyond that check.
    ok(
        &workspace,
        &[
            "govern",
            "workunit",
            "record-proof",
            "--task-id",
            id,
            "--gate",
            "validate_passes",
            "--status",
            "pass",
            "--artifact",
            ".decapod/managed/specs/INTENT.md",
        ],
    );
    let before = ok(
        &workspace,
        &["govern", "workunit", "journal-status", "--task-id", id],
    );
    let denied = run(
        &workspace,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            id,
            "--to",
            "verified",
        ],
    );
    assert!(!denied.status.success());
    let diagnostic = String::from_utf8_lossy(&denied.stderr);
    assert!(diagnostic.contains("VALIDATION_HISTORY"), "{diagnostic}");
    assert!(
        diagnostic.contains("no canonical full validation proof"),
        "{diagnostic}"
    );
    let manifest = ok(&workspace, &["govern", "workunit", "get", "--task-id", id]);
    assert_eq!(manifest["status"], "CLAIMED");
    assert_eq!(manifest["proof_results"][0]["status"], "pass");
    assert!(manifest.get("validation_record").is_none());
    let after = ok(
        &workspace,
        &["govern", "workunit", "journal-status", "--task-id", id],
    );
    assert_eq!(before["generation"], after["generation"]);
    assert_eq!(after["projection_state"], "current");
}

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
fn diagnostic_overrides_cannot_establish_full_runtime_readiness() {
    const PROBE: &str = "DECAPOD_TEST_READINESS_OVERRIDE_PROBE";
    if let Ok(expected) = std::env::var(PROBE) {
        let fixture = Fixture::new();
        let mut host = fixture.host();
        let report = fixture.validate(&mut host);
        if expected == "allow" {
            assert!(report.ready, "{report:?}");
            assert!(report.proof_ready);
            assert_eq!(host.execution_attempts, 1);
        } else {
            blocked(&report, "READINESS_GATE_OVERRIDE_UNSUPPORTED");
            assert!(!report.proof_ready);
            assert!(report.evidence_path.is_none());
            assert_eq!(host.selections, 0);
            assert_eq!(host.execution_attempts, 0);
        }
        return;
    }
    let prefix = module_path!()
        .split_once("::")
        .map(|(_, path)| format!("{path}::"))
        .unwrap_or_default();
    let case = format!("{prefix}diagnostic_overrides_cannot_establish_full_runtime_readiness");
    for (name, value, expected) in [
        ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "", "deny"),
        ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "0", "deny"),
        ("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "false", "deny"),
        ("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES", "true", "deny"),
        ("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES", "false", "allow"),
    ] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &case, "--nocapture"])
            .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
            .env_remove("DECAPOD_VALIDATE_SKIP_TOOLING_GATES")
            .env_remove("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES")
            .env(PROBE, expected)
            .env(name, value)
            .bounded_output(std::time::Duration::from_secs(120))
            .unwrap();
        assert!(
            output.status.success(),
            "{name}={value:?}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    }
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

/// Registered explicitly by the required CLI target so selection includes it.
pub fn candidate_cannot_repin_a_rewritten_accepted_check_script() {
    let mut fixture = Fixture::new();
    git(&fixture.options.root, &["push", "origin", "HEAD:main"]);
    git(&fixture.options.root, &["fetch", "origin", "main"]);
    fs::write(fixture.options.root.join("payload.txt"), "invalid source\n").unwrap();
    fs::write(
        fixture.options.root.join("scripts/check_source.py"),
        "print('claimed success without inspecting the invalid source')\n",
    )
    .unwrap();
    fixture
        .proof
        .required_profile
        .as_mut()
        .unwrap()
        .reviewed_sources_digest =
        runtime::discover_source_inventory(&fixture.options, &Cancellation::default())
            .unwrap()
            .digest()
            .unwrap();
    fixture.save_config();
    fixture.commit("repin rewritten accepted check with unchanged command");
    let mut host = fixture.host();
    blocked(
        &fixture.validate(&mut host),
        "ACCEPTED_REPOSITORY_CONTRACT_CHANGED",
    );
    assert_eq!(host.selections, 0);
    assert_eq!(host.execution_attempts, 0);
}

#[test]
fn local_source_policy_can_change_after_separate_target_acceptance() {
    let mut fixture = Fixture::new();
    git(&fixture.options.root, &["push", "origin", "HEAD:main"]);
    git(&fixture.options.root, &["fetch", "origin", "main"]);
    let script = fixture.options.root.join("scripts/check_source.py");
    let original = fs::read_to_string(&script).unwrap();
    fs::write(
        script,
        format!("# Reviewed target policy revision\n{original}"),
    )
    .unwrap();
    fixture
        .proof
        .required_profile
        .as_mut()
        .unwrap()
        .reviewed_sources_digest =
        runtime::discover_source_inventory(&fixture.options, &Cancellation::default())
            .unwrap()
            .digest()
            .unwrap();
    fixture.save_config();
    fixture.commit("separately accept the changed source verification policy");
    git(&fixture.options.root, &["push", "origin", "HEAD:main"]);
    git(&fixture.options.root, &["fetch", "origin", "main"]);
    let mut host = fixture.host();
    let report = fixture.validate(&mut host);
    assert!(report.ready, "{report:?}");
    assert_eq!(report.scope, repository_profile::READINESS_SCOPE);
    assert_eq!(host.execution_attempts, 1);
}

#[test]
fn repinned_local_policy_removal_execution_and_coverage_changes_fail_before_selection() {
    for change in [
        "remove",
        "program",
        "directory",
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
            "program" => {
                let shell = fs::canonicalize("/bin/sh").unwrap();
                let shell = shell.to_str().unwrap().to_owned();
                let command = fixture
                    .proof
                    .commands
                    .iter_mut()
                    .find(|command| command.name == "inspect-second")
                    .unwrap();
                command.command = shell.clone();
                command.args = vec![
                    "-c".into(),
                    "printf 'claimed success without inspection'".into(),
                ];
                let execution = contract.executions.get_mut("inspect-second").unwrap();
                execution.program = shell.clone();
                execution.toolchain_files.insert("shell".into(), shell);
            }
            "directory" => {
                fs::create_dir(fixture.options.root.join("nested")).unwrap();
                fs::write(fixture.options.root.join("nested/input.txt"), "input\n").unwrap();
                contract
                    .executions
                    .get_mut("inspect-second")
                    .unwrap()
                    .directory = "nested".into();
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

#[test]
fn two_isolated_worktrees_require_combined_candidate_validation_and_target_freshness() {
    use consumer_readiness_fixture::CONFIG;
    use decapod::core::bounded_process::Cancellation;
    use decapod::core::todo::{self, ClaimMode, TodoCommand};
    use std::path::Path;

    let mut fixture = Fixture::new();
    let main_repo = fixture.options.root.clone();

    // Establish a shared semantic invariant across two regions of `shared_contract.py`:
    // LEFT_DELTA + RIGHT_DELTA <= 1. Each isolated agent edit satisfies the invariant
    // on its own branch, while a clean textual merge violates it until reconciled.
    fs::write(
        main_repo.join("shared_contract.py"),
        "LEFT_DELTA = 0\n\n\nRIGHT_DELTA = 0\n",
    )
    .unwrap();
    fs::write(
        main_repo.join("scripts/check_source.py"),
        "import os\nassert not os.path.exists('Cargo.toml')\nassert open('payload.txt').read() == 'ready\\n', 'consumer source is invalid'\nns = {}\nexec(open('shared_contract.py').read(), ns)\nassert ns['LEFT_DELTA'] + ns['RIGHT_DELTA'] <= 1, 'combined concurrency budget exceeded'\nassert not os.path.realpath(os.getcwd()).startswith(os.path.realpath(os.environ['FIXTURE_WORKTREE'])), 'proof used mutable worktree'\nassert os.environ['DECAPOD_NATIVE_PROOF_ACTIVE'] == '1'\nassert os.path.isdir(os.environ['HOME'])\nprint('consumer-source-inspected')\n",
    )
    .unwrap();

    fs::write(
        main_repo.join(".gitignore"),
        ".decapod/data/\n.decapod/managed/governance-document.lock\n.decapod/workspaces/\n",
    )
    .unwrap();
    let sources =
        runtime::discover_source_inventory(&fixture.options, &Cancellation::default()).unwrap();
    fixture
        .proof
        .required_profile
        .as_mut()
        .unwrap()
        .reviewed_sources_digest = sources.digest().unwrap();
    let root_cfg = toml::Value::try_from(serde_json::json!({
        "schema_version": "1.0.0",
        "init": {"diagram_style": "ascii", "entrypoints": []},
        "repo": {},
        "proof": fixture.proof,
    }))
    .unwrap();
    fs::write(
        main_repo.join(CONFIG),
        toml::to_string_pretty(&root_cfg).unwrap(),
    )
    .unwrap();
    fixture.commit("add shared semantic invariant to consumer contract");
    git(&main_repo, &["checkout", "main"]);
    git(&main_repo, &["merge", "--ff-only", "consumer-work"]);
    git(&main_repo, &["push", "origin", "main"]);
    git(&main_repo, &["checkout", "consumer-work"]);

    // Provision canonical task store and two overlapping tasks in distinct isolated worktrees.
    let store = main_repo.join(".decapod/data");
    fs::create_dir_all(&store).unwrap();
    todo::initialize_todo_db(&store).unwrap();
    let add_scoped = |title: &str| -> String {
        let cmd = TodoCommand::Add {
            title: title.to_string(),
            description: String::new(),
            tags: String::new(),
            owner: String::new(),
            due: None,
            r#ref: String::new(),
            scope: r#"paths:["shared_contract.py"]"#.to_string(),
            dir: Some(main_repo.to_string_lossy().to_string()),
            priority: "medium".to_string(),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        };
        todo::add_task(&store, &cmd).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let task_a = add_scoped("Agent alpha left-delta slice");
    let task_b = add_scoped("Agent beta right-delta slice");
    assert_eq!(
        todo::claim_task(&store, &task_a, "alpha", ClaimMode::Exclusive).unwrap()["status"],
        "ok"
    );
    let beta_claim = todo::claim_task(&store, &task_b, "beta", ClaimMode::Exclusive).unwrap();
    assert_eq!(beta_claim["status"], "ok");
    assert_eq!(
        beta_claim["result"]["integration_readiness"],
        "combined_candidate_reconciliation_required"
    );

    let wt_alpha_raw = main_repo.join(".decapod/workspaces/wt-alpha");
    let wt_beta_raw = main_repo.join(".decapod/workspaces/wt-beta");
    fs::create_dir_all(wt_alpha_raw.parent().unwrap()).unwrap();
    let branch_alpha = format!("agent/alpha/{task_a}-left");
    let branch_beta = format!("agent/beta/{task_b}-right");
    git(
        &main_repo,
        &[
            "worktree",
            "add",
            "-b",
            &branch_alpha,
            wt_alpha_raw.to_str().unwrap(),
            "main",
        ],
    );
    git(
        &main_repo,
        &[
            "worktree",
            "add",
            "-b",
            &branch_beta,
            wt_beta_raw.to_str().unwrap(),
            "main",
        ],
    );
    let wt_alpha = wt_alpha_raw.canonicalize().unwrap();
    let wt_beta = wt_beta_raw.canonicalize().unwrap();
    todo::admit_authoring(&store, &wt_alpha, "alpha").unwrap();
    todo::admit_authoring(&store, &wt_beta, "beta").unwrap();

    // Helper to build an isolated Fixture view targeting a specific worktree checkout
    // without mutating `.decapod/config.toml` across branches.
    let make_wt_options = |wt: &Path, suffix: &str| -> (runtime::Options, Fixture) {
        let mut opts = fixture.options.clone();
        opts.root = wt.to_path_buf();
        opts.evidence_parent = fixture._storage.path().join(format!("evidence-{suffix}"));
        fs::create_dir_all(&opts.evidence_parent).unwrap();
        let sub_fixture = Fixture {
            _storage: tempfile::tempdir().unwrap(),
            upstream: fixture.upstream.clone(),
            options: opts.clone(),
            proof: fixture.proof.clone(),
        };
        (opts, sub_fixture)
    };

    // 1. Both isolated branches independently validate and pass (`ready == true`).
    fs::write(
        wt_alpha.join("shared_contract.py"),
        "LEFT_DELTA = 1\n\n\nRIGHT_DELTA = 0\n",
    )
    .unwrap();
    git(&wt_alpha, &["add", "shared_contract.py"]);
    git(&wt_alpha, &["commit", "-m", "alpha isolated change"]);
    let (_, alpha_fixture) = make_wt_options(&wt_alpha, "alpha-1");
    let mut alpha_host = LocalHost::new(&alpha_fixture, false);
    let alpha_report = alpha_fixture.validate(&mut alpha_host);
    assert!(alpha_report.ready, "{alpha_report:?}");

    fs::write(
        wt_beta.join("shared_contract.py"),
        "LEFT_DELTA = 0\n\n\nRIGHT_DELTA = 1\n",
    )
    .unwrap();
    git(&wt_beta, &["add", "shared_contract.py"]);
    git(&wt_beta, &["commit", "-m", "beta isolated change"]);
    let (_, beta_fixture) = make_wt_options(&wt_beta, "beta-1");
    let mut beta_host = LocalHost::new(&beta_fixture, false);
    let beta_report = beta_fixture.validate(&mut beta_host);
    assert!(beta_report.ready, "{beta_report:?}");

    // 2. Clean textual Git merge into `wt_alpha` breaks the combined semantic invariant:
    //    `runtime::validate` MUST fail (`ready == false`) with no readiness receipt.
    git(&wt_alpha, &["merge", "--no-edit", &branch_beta]);
    let (_, merged_fixture) = make_wt_options(&wt_alpha, "alpha-merged-broken");
    let mut merged_host = LocalHost::new(&merged_fixture, false);
    let merged_report = merged_fixture.validate(&mut merged_host);
    assert!(
        !merged_report.ready,
        "semantically broken combined candidate must not pass readiness: {merged_report:?}"
    );
    let merged_dir = merged_report.evidence_path.unwrap();
    assert!(
        !merged_dir
            .join("completion/native-readiness-receipt.json")
            .exists(),
        "failed combined candidate must not emit a readiness receipt"
    );

    // 3. Reconciling the combined candidate restores readiness (`ready == true`).
    fs::write(
        wt_alpha.join("shared_contract.py"),
        "LEFT_DELTA = 1\n\n\nRIGHT_DELTA = 0\n",
    )
    .unwrap();
    git(&wt_alpha, &["add", "shared_contract.py"]);
    git(
        &wt_alpha,
        &["commit", "-m", "reconcile combined candidate invariant"],
    );
    let (_, reconciled_fixture) = make_wt_options(&wt_alpha, "alpha-reconciled");
    let mut reconciled_host = LocalHost::new(&reconciled_fixture, false);
    let reconciled_report = reconciled_fixture.validate(&mut reconciled_host);
    assert!(reconciled_report.ready, "{reconciled_report:?}");
    assert!(
        reconciled_report
            .evidence_path
            .unwrap()
            .join("completion/native-readiness-receipt.json")
            .is_file()
    );

    // 4. Advancing upstream `refs/heads/main` invalidates prior readiness until re-integrated.
    git(&main_repo, &["checkout", "main"]);
    git(
        &main_repo,
        &[
            "commit",
            "--allow-empty",
            "-m",
            "upstream accepted target advance",
        ],
    );
    git(&main_repo, &["push", "origin", "main"]);

    let (_, stale_target_fixture) = make_wt_options(&wt_alpha, "alpha-stale-target");
    let mut stale_target_host = LocalHost::new(&stale_target_fixture, false);
    let stale_target_report = stale_target_fixture.validate(&mut stale_target_host);
    assert!(
        !stale_target_report.ready,
        "advancing upstream target must invalidate candidate readiness until rebased/merged: {stale_target_report:?}"
    );

    // Fetching and merging the new upstream `origin/main` + revalidating succeeds again.
    git(&wt_alpha, &["fetch", "origin", "main"]);
    git(&wt_alpha, &["merge", "--no-edit", "origin/main"]);
    let (_, refreshed_fixture) = make_wt_options(&wt_alpha, "alpha-refreshed-target");
    let mut refreshed_host = LocalHost::new(&refreshed_fixture, false);
    let refreshed_report = refreshed_fixture.validate(&mut refreshed_host);
    assert!(refreshed_report.ready, "{refreshed_report:?}");
}

fn setup_multi_target_fixture(
    unrelated_missing_executable: bool,
) -> (Fixture, &'static str, &'static str) {
    use decapod::core::native_proof::NativePlatform;
    use decapod::core::proof::ProofDef;
    use decapod::core::repository_profile::{DeploymentTargetContract, RepositoryExecution};
    use std::collections::BTreeMap;

    let mut fixture = Fixture::new();
    let current_platform =
        repository_profile::platform_key(NativePlatform::current().unwrap()).unwrap();
    let other_platform = if current_platform.starts_with("linux") {
        "macos-aarch64"
    } else {
        "linux-x86_64"
    };
    let real_exec = fixture
        .proof
        .repository_contract
        .as_ref()
        .unwrap()
        .executions["inspect-source"]
        .clone();
    let unrelated_exec = if unrelated_missing_executable {
        RepositoryExecution {
            program: "/nonexistent/foreign-host-only-tool".into(),
            toolchain_files: BTreeMap::from([(
                "foreign-runtime".into(),
                "/nonexistent/foreign-runtime".into(),
            )]),
            platforms: BTreeMap::new(),
            environment: BTreeMap::new(),
            directory: ".".into(),
            timeout_seconds: 10,
        }
    } else {
        real_exec.clone()
    };
    let unrelated_cmd = if unrelated_missing_executable {
        "/nonexistent/foreign-host-only-tool".to_string()
    } else {
        real_exec.program.clone()
    };

    fixture.proof.commands = vec![
        ProofDef {
            name: "check-active".into(),
            command: real_exec.program.clone(),
            args: ["-I", "-S", "-B", "-c", "print('active-target-ok')"]
                .map(str::to_owned)
                .into(),
            description: "Active target check".into(),
            required: true,
        },
        ProofDef {
            name: "check-shared".into(),
            command: real_exec.program.clone(),
            args: ["-I", "-S", "-B", "-c", "print('shared-target-ok')"]
                .map(str::to_owned)
                .into(),
            description: "Shared dependency check".into(),
            required: true,
        },
        ProofDef {
            name: "check-unrelated".into(),
            command: unrelated_cmd,
            args: ["-I", "-S", "-B", "-c", "print('unrelated-target-ok')"]
                .map(str::to_owned)
                .into(),
            description: "Unrelated platform target check".into(),
            required: true,
        },
    ];
    let contract = fixture.proof.repository_contract.as_mut().unwrap();
    contract.executions = BTreeMap::from([
        ("check-active".into(), real_exec.clone()),
        ("check-shared".into(), real_exec),
        ("check-unrelated".into(), unrelated_exec),
    ]);
    contract.source_coverage[0].commands = vec![
        "check-active".into(),
        "check-shared".into(),
        "check-unrelated".into(),
    ];
    contract.targets = BTreeMap::from([
        (
            "shared".into(),
            DeploymentTargetContract {
                commands: vec!["check-shared".into()],
                depends_on: vec![],
                shared_inputs: vec!["shared/".into()],
                platforms: vec![],
            },
        ),
        (
            "active-host".into(),
            DeploymentTargetContract {
                commands: vec!["check-active".into()],
                depends_on: vec!["shared".into()],
                shared_inputs: vec!["deploy/active/".into()],
                platforms: vec![current_platform.into()],
            },
        ),
        (
            "unrelated-host".into(),
            DeploymentTargetContract {
                commands: vec!["check-unrelated".into()],
                depends_on: vec!["shared".into()],
                shared_inputs: vec!["deploy/unrelated/".into(), "common/shared.txt".into()],
                platforms: vec![other_platform.into()],
            },
        ),
    ]);
    fixture.review_commands();
    fixture.save_config();
    fixture.commit("configure multi-target deployment contract");
    git(&fixture.options.root, &["push", "origin", "HEAD:main"]);
    git(&fixture.options.root, &["fetch", "origin", "main"]);
    (fixture, current_platform, other_platform)
}

#[test]
fn selected_target_succeeds_without_unrelated_host_executable_while_full_repo_blocks() {
    let (mut fixture, _, _) = setup_multi_target_fixture(true);
    fs::create_dir_all(fixture.options.root.join("deploy/active")).unwrap();
    fs::write(fixture.options.root.join("deploy/active/app.txt"), "v1\n").unwrap();
    fixture.commit("modify active target only");

    // Full-repository validation (target = None) fails because `check-unrelated`'s
    // foreign-host executable is absent on this machine.
    let mut full_host = fixture.host();
    let full_report = fixture.validate(&mut full_host);
    blocked(&full_report, "MISSING_REQUIRED_PROOF_ADAPTER");
    assert_eq!(full_host.execution_attempts, 0);

    // Target-scoped validation for `active-host` succeeds without needing
    // `unrelated-host`'s absent executable, running only `active-host` + `shared`.
    fixture.options.target = Some("active-host".into());
    let mut host = fixture.host();
    let report = fixture.validate(&mut host);
    assert!(report.ready, "{report:?}");
    assert!(report.proof_ready);
    assert!(!report.publication_ready());
    assert_eq!(report.scope, repository_profile::TARGET_READINESS_SCOPE);
    assert_eq!(report.target.as_deref(), Some("active-host"));
    assert_eq!(report.resolved_targets, vec!["active-host", "shared"]);
    assert_eq!(host.execution_attempts, 1);

    let directory = report.evidence_path.unwrap();
    let evidence: SuiteEvidence =
        serde_json::from_slice(&fs::read(directory.join("observations.json")).unwrap()).unwrap();
    let executed_jobs: std::collections::BTreeSet<_> = evidence
        .results
        .iter()
        .map(|r| r.key.job.as_str())
        .collect();
    assert_eq!(
        executed_jobs,
        std::collections::BTreeSet::from(["check-active", "check-shared"])
    );
    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(directory.join("completion/native-readiness-receipt.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["scope"], repository_profile::TARGET_READINESS_SCOPE);
    assert_eq!(receipt["target"], "active-host");
    assert_eq!(
        receipt["resolved_targets"],
        serde_json::json!(["active-host", "shared"])
    );
}

#[test]
fn shared_input_change_expands_affected_target_checks() {
    let (mut fixture, current_platform, _) = setup_multi_target_fixture(false);
    // Allow `unrelated-host` to run on the current platform so we can observe
    // its check executing when its `shared_inputs` path changes.
    fixture
        .proof
        .repository_contract
        .as_mut()
        .unwrap()
        .targets
        .get_mut("unrelated-host")
        .unwrap()
        .platforms = vec![current_platform.into()];
    fixture.review_commands();
    fixture.save_config();
    fixture.commit("enable sibling target on current executor");
    git(&fixture.options.root, &["push", "origin", "HEAD:main"]);
    git(&fixture.options.root, &["fetch", "origin", "main"]);

    fs::create_dir_all(fixture.options.root.join("common")).unwrap();
    fs::write(fixture.options.root.join("common/shared.txt"), "changed\n").unwrap();
    fixture.commit("modify shared input impacting sibling target");

    fixture.options.target = Some("active-host".into());
    let mut host = fixture.host();
    let report = fixture.validate(&mut host);
    assert!(report.ready, "{report:?}");
    assert_eq!(
        report.resolved_targets,
        vec!["active-host", "shared", "unrelated-host"]
    );
    let evidence: SuiteEvidence = serde_json::from_slice(
        &fs::read(report.evidence_path.unwrap().join("observations.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(evidence.results.len(), 3);
}

#[test]
fn unknown_target_cycle_and_incompatible_executor_fail_closed() {
    // 1. Unknown target fails closed before selection/execution.
    let (mut fixture, _, _) = setup_multi_target_fixture(false);
    fixture.options.target = Some("nonexistent-target".into());
    let mut host = fixture.host();
    blocked(&fixture.validate(&mut host), "UNKNOWN_DEPLOYMENT_TARGET");
    assert_eq!(host.selections, 0);
    assert_eq!(host.execution_attempts, 0);

    // 2. Incompatible platform executor for the selected target fails closed.
    fixture.options.target = Some("unrelated-host".into());
    let mut host = fixture.host();
    blocked(&fixture.validate(&mut host), "INCOMPATIBLE_TARGET_EXECUTOR");
    assert_eq!(host.execution_attempts, 0);

    // 3. Shared input expanding into an incompatible target fails closed.
    fs::create_dir_all(fixture.options.root.join("common")).unwrap();
    fs::write(fixture.options.root.join("common/shared.txt"), "changed\n").unwrap();
    fixture.commit("touch shared input owned by incompatible host target");
    fixture.options.target = Some("active-host".into());
    let mut host = fixture.host();
    blocked(&fixture.validate(&mut host), "INCOMPATIBLE_TARGET_EXECUTOR");
    assert_eq!(host.execution_attempts, 0);

    // 4. Even an exact approval cannot make a cyclic dependency executable.
    let (mut cycle_fixture, _, _) = setup_multi_target_fixture(false);
    accept_current_on_main(&cycle_fixture);
    cycle_fixture
        .proof
        .repository_contract
        .as_mut()
        .unwrap()
        .targets
        .get_mut("shared")
        .unwrap()
        .depends_on
        .push("active-host".into());
    cycle_fixture.review_commands();
    save_full_config(&cycle_fixture);
    cycle_fixture.commit("introduce cyclic target dependency");
    let id = transition_id_for_fixture(&cycle_fixture);
    approve_transition_with_caller(&cycle_fixture.options.root, &id, "operator", 900).unwrap();
    cycle_fixture.options.target = Some("active-host".into());
    let mut host = cycle_fixture.host();
    blocked(
        &cycle_fixture.validate(&mut host),
        "DEPLOYMENT_TARGET_CYCLE",
    );
    assert_eq!(host.selections, 0);
    assert_eq!(host.execution_attempts, 0);
}

#[test]
fn target_scoped_receipt_is_rejected_for_full_repo_mismatched_target_and_stale_sha() {
    let (mut fixture, _, _) = setup_multi_target_fixture(true);
    fixture.options.target = Some("active-host".into());
    let head = git(&fixture.options.root, &["rev-parse", "HEAD"]);
    let mut host = fixture.host();
    let report = fixture.validate(&mut host);
    assert!(report.ready, "{report:?}");
    assert!(!report.publication_ready());
    let directory = report.evidence_path.unwrap();
    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(directory.join("completion/native-readiness-receipt.json")).unwrap(),
    )
    .unwrap();

    // Valid for the exact candidate SHA and selected target.
    runtime::verify_native_receipt_scope(&receipt, &head, Some("active-host")).unwrap();

    // Rejected for full-repository PR readiness.
    let err = runtime::verify_native_receipt_scope(&receipt, &head, None).unwrap_err();
    assert!(
        err.to_string().contains("TARGET_RECEIPT_FOR_FULL_REPO"),
        "{err}"
    );

    // Rejected for an unrelated target.
    let err =
        runtime::verify_native_receipt_scope(&receipt, &head, Some("unrelated-host")).unwrap_err();
    assert!(err.to_string().contains("TARGET_RECEIPT_MISMATCH"), "{err}");

    // Rejected for a different candidate SHA.
    let err = runtime::verify_native_receipt_scope(&receipt, &"0".repeat(40), Some("active-host"))
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("NATIVE_RECEIPT_CANDIDATE_MISMATCH"),
        "{err}"
    );
}

#[test]
fn candidate_cannot_weaken_or_remove_accepted_deployment_targets() {
    for defect in [
        "remove-target",
        "remove-command",
        "remove-depends-on",
        "narrow-shared-inputs",
        "change-platforms",
    ] {
        let (mut fixture, current_platform, _) = setup_multi_target_fixture(false);
        let contract = fixture.proof.repository_contract.as_mut().unwrap();
        match defect {
            "remove-target" => {
                contract.targets.remove("unrelated-host");
            }
            "remove-command" => {
                contract
                    .targets
                    .get_mut("active-host")
                    .unwrap()
                    .commands
                    .clear();
                contract
                    .targets
                    .get_mut("active-host")
                    .unwrap()
                    .commands
                    .push("check-shared".into());
            }
            "remove-depends-on" => {
                contract
                    .targets
                    .get_mut("active-host")
                    .unwrap()
                    .depends_on
                    .clear();
            }
            "narrow-shared-inputs" => {
                contract
                    .targets
                    .get_mut("unrelated-host")
                    .unwrap()
                    .shared_inputs
                    .retain(|p| p != "common/shared.txt");
            }
            "change-platforms" => {
                contract
                    .targets
                    .get_mut("unrelated-host")
                    .unwrap()
                    .platforms = vec![current_platform.into()];
            }
            _ => unreachable!(),
        }
        fixture.review_commands();
        fixture.save_config();
        fixture.commit("weaken accepted deployment target contract");
        fixture.options.target = Some("active-host".into());
        let mut host = fixture.host();
        blocked(
            &fixture.validate(&mut host),
            "ACCEPTED_REPOSITORY_CONTRACT_CHANGED",
        );
        assert_eq!(host.selections, 0, "{defect}");
        assert_eq!(host.execution_attempts, 0, "{defect}");
    }
}

fn save_full_config(fixture: &Fixture) {
    let config = toml::Value::try_from(serde_json::json!({
        "schema_version": "1.0.0",
        "init": {"diagram_style": "ascii", "entrypoints": []},
        "repo": {"base_branch": "main"},
        "proof": fixture.proof,
    }))
    .unwrap();
    fs::write(
        fixture
            .options
            .root
            .join(consumer_readiness_fixture::CONFIG),
        toml::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
}

fn accept_current_on_main(fixture: &Fixture) {
    save_full_config(fixture);
    if !git(&fixture.options.root, &["status", "--porcelain"]).is_empty() {
        fixture.commit("record full canonical config for accepted policy");
    }
    let work_branch = git(
        &fixture.options.root,
        &["rev-parse", "--abbrev-ref", "HEAD"],
    );
    git(&fixture.options.root, &["checkout", "main"]);
    git(&fixture.options.root, &["merge", "--ff-only", &work_branch]);
    git(&fixture.options.root, &["push", "origin", "main"]);
    git(&fixture.options.root, &["fetch", "origin", "main"]);
    git(&fixture.options.root, &["checkout", &work_branch]);
}

fn transition_id_for_fixture(fixture: &Fixture) -> String {
    let base_sha = git(
        &fixture.options.root,
        &["rev-parse", "refs/remotes/origin/main"],
    );
    let head_sha = git(&fixture.options.root, &["rev-parse", "HEAD"]);
    let accepted_toml = git(
        &fixture.options.root,
        &["show", "refs/remotes/origin/main:.decapod/config.toml"],
    );
    let accepted = runtime::proof_configuration(accepted_toml.as_bytes()).unwrap();
    let accepted_src = accepted
        .required_profile
        .as_ref()
        .unwrap()
        .reviewed_sources_digest
        .as_str();
    let candidate_src = fixture
        .proof
        .required_profile
        .as_ref()
        .unwrap()
        .reviewed_sources_digest
        .as_str();
    let accepted_cmd = accepted
        .repository_contract
        .as_ref()
        .map(|contract| contract.reviewed_commands_digest.as_str())
        .unwrap_or("");
    let candidate_cmd = fixture
        .proof
        .repository_contract
        .as_ref()
        .unwrap()
        .reviewed_commands_digest
        .as_str();
    repository_profile::repo_policy_transition_id(
        &base_sha,
        &head_sha,
        accepted_src,
        candidate_src,
        accepted_cmd,
        candidate_cmd,
    )
    .unwrap()
}

fn approve_transition_with_caller(
    repo_root: &std::path::Path,
    transition_id: &str,
    caller: &str,
    expires_in_seconds: u64,
) -> Result<String, String> {
    use clap::Parser;
    use decapod::core::store::{Store, StoreKind};
    use decapod::plugins::policy::{PolicyCli, list_approvals, run_policy_cli_as};

    let data_root = repo_root.join(".decapod/data");
    fs::create_dir_all(&data_root).unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: data_root,
    };
    let ttl = expires_in_seconds.to_string();
    run_policy_cli_as(
        &store,
        PolicyCli::try_parse_from([
            "policy",
            "approve",
            "--id",
            transition_id,
            "--scope",
            repository_profile::POLICY_TRANSITION_SCOPE,
            "--expires-in-seconds",
            &ttl,
        ])
        .unwrap(),
        caller,
    )
    .map_err(|e| e.to_string())?;
    let approvals = list_approvals(&store).map_err(|e| e.to_string())?;
    Ok(approvals
        .into_iter()
        .rev()
        .find(|row| {
            row.action_id == transition_id
                && row.scope == repository_profile::POLICY_TRANSITION_SCOPE
        })
        .expect("approved transition row must exist")
        .approval_id)
}

#[test]
fn source_inventory_change_succeeds_after_policy_control_approval_and_full_validation() {
    let mut fixture = Fixture::new();
    accept_current_on_main(&fixture);
    let accepted_src = fixture
        .proof
        .required_profile
        .as_ref()
        .unwrap()
        .reviewed_sources_digest
        .clone();

    let script = fixture.options.root.join("scripts/check_source.py");
    let original = fs::read_to_string(&script).unwrap();
    fs::write(
        script,
        format!("{original}print('upgraded-source-check-executed')\n"),
    )
    .unwrap();
    let new_src = runtime::discover_source_inventory(&fixture.options, &Cancellation::default())
        .unwrap()
        .digest()
        .unwrap();
    fixture
        .proof
        .required_profile
        .as_mut()
        .unwrap()
        .reviewed_sources_digest = new_src.clone();
    save_full_config(&fixture);
    fixture.commit("update inventoried check script and reconcile reviewed_sources_digest");

    let transition_id = transition_id_for_fixture(&fixture);
    let mut host = fixture.host();
    let unapproved = fixture.validate(&mut host);
    blocked(&unapproved, "ACCEPTED_REPOSITORY_CONTRACT_CHANGED");
    let detail = unapproved
        .blockers
        .iter()
        .map(|b| b.detail.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        detail.contains(&format!(
            "decapod govern policy approve --id {transition_id} --scope repo.pr-readiness.v1"
        )),
        "expected exact approval command in blocker: {detail}"
    );
    assert!(detail.contains(&accepted_src), "{detail}");
    assert!(detail.contains(&new_src), "{detail}");
    assert_eq!(host.selections, 0);
    assert_eq!(host.execution_attempts, 0);

    approve_transition_with_caller(&fixture.options.root, &transition_id, "operator", 900).unwrap();

    let mut host = fixture.host();
    let approved = fixture.validate(&mut host);
    assert!(approved.ready, "{approved:?}");
    assert!(approved.proof_ready);
    assert_eq!(approved.scope, repository_profile::READINESS_SCOPE);
    assert_eq!(host.execution_attempts, 1);
    let dir = approved.evidence_path.unwrap();
    assert!(
        dir.join("completion/native-readiness-receipt.json")
            .is_file()
    );
    let stdout = [0, 1]
        .map(|i| fs::read_to_string(dir.join(format!("native-{i}.stdout"))).unwrap())
        .join("\n");
    assert!(stdout.contains("upgraded-source-check-executed"));
}

#[test]
fn command_and_execution_upgrade_succeeds_after_policy_control_approval_and_full_validation() {
    let mut fixture = Fixture::new();
    accept_current_on_main(&fixture);
    let accepted_cmd = fixture
        .proof
        .repository_contract
        .as_ref()
        .unwrap()
        .reviewed_commands_digest
        .clone();

    let command = fixture
        .proof
        .commands
        .iter_mut()
        .find(|c| c.name == "inspect-second")
        .unwrap();
    command.args = [
        "-I",
        "-S",
        "-B",
        "-c",
        "assert open('second.txt').read() == 'also ready\\n'; print('upgraded-tool-execution-ran')",
    ]
    .map(str::to_owned)
    .into();
    fixture
        .proof
        .repository_contract
        .as_mut()
        .unwrap()
        .executions
        .get_mut("inspect-second")
        .unwrap()
        .timeout_seconds = 20;
    fixture.review_commands();
    let new_cmd = fixture
        .proof
        .repository_contract
        .as_ref()
        .unwrap()
        .reviewed_commands_digest
        .clone();
    save_full_config(&fixture);
    fixture.commit("upgrade pinned command args and execution timeout");

    let transition_id = transition_id_for_fixture(&fixture);
    let mut host = fixture.host();
    let unapproved = fixture.validate(&mut host);
    blocked(&unapproved, "ACCEPTED_REPOSITORY_CONTRACT_CHANGED");
    let detail = unapproved
        .blockers
        .iter()
        .map(|b| b.detail.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(detail.contains(&transition_id), "{detail}");
    assert!(detail.contains(&accepted_cmd), "{detail}");
    assert!(detail.contains(&new_cmd), "{detail}");
    assert_eq!(host.selections, 0);

    approve_transition_with_caller(&fixture.options.root, &transition_id, "operator", 900).unwrap();

    let mut host = fixture.host();
    let approved = fixture.validate(&mut host);
    assert!(approved.ready, "{approved:?}");
    assert!(approved.proof_ready);
    assert_eq!(host.execution_attempts, 1);
    let dir = approved.evidence_path.unwrap();
    let stdout = [0, 1]
        .map(|i| fs::read_to_string(dir.join(format!("native-{i}.stdout"))).unwrap())
        .join("\n");
    assert!(stdout.contains("upgraded-tool-execution-ran"));
}

#[test]
fn single_platform_to_platform_map_requires_exact_approval_and_runs_host_checks() {
    let mut fixture = Fixture::new();
    accept_current_on_main(&fixture);
    let platform = convert_fixture_to_per_platform(&mut fixture);
    save_full_config(&fixture);
    fixture.commit("record candidate platform map in canonical config");

    let id = transition_id_for_fixture(&fixture);
    let mut host = fixture.host();
    blocked(
        &fixture.validate(&mut host),
        "ACCEPTED_REPOSITORY_CONTRACT_CHANGED",
    );
    assert_eq!(host.execution_attempts, 0);
    approve_transition_with_caller(&fixture.options.root, &id, "operator", 900).unwrap();

    let report = fixture.validate(&mut host);
    assert!(report.ready && report.proof_ready, "{report:?}");
    assert_eq!(host.execution_attempts, 1);
    assert_eq!(report.scope, repository_profile::READINESS_SCOPE);
    assert!(repository_profile::is_valid_platform_key(&platform));

    fs::write(fixture.options.root.join("extra.txt"), "new head\n").unwrap();
    fixture.commit("advance head after platform transition approval");
    let mut host = fixture.host();
    blocked(
        &fixture.validate(&mut host),
        "ACCEPTED_REPOSITORY_CONTRACT_CHANGED",
    );
    assert_eq!(host.execution_attempts, 0);
}

#[test]
fn target_map_addition_and_update_require_exact_approval_and_preserve_receipt_scope() {
    use decapod::core::native_proof::NativePlatform;
    use decapod::core::repository_profile::DeploymentTargetContract;

    let mut fixture = Fixture::new();
    let platform = repository_profile::platform_key(NativePlatform::current().unwrap()).unwrap();
    let first = fixture.proof.commands[0].name.clone();
    let second = fixture.proof.commands[1].name.clone();
    for commands in [vec![first.clone()], vec![first, second]] {
        accept_current_on_main(&fixture);
        fixture
            .proof
            .repository_contract
            .as_mut()
            .unwrap()
            .targets
            .insert(
                "active-host".into(),
                DeploymentTargetContract {
                    commands: commands.clone(),
                    depends_on: vec![],
                    shared_inputs: vec![],
                    platforms: vec![platform.into()],
                },
            );
        fixture.review_commands();
        save_full_config(&fixture);
        fixture.commit("review deployment target command obligations");
        fixture.options.target = Some("active-host".into());

        let id = transition_id_for_fixture(&fixture);
        let mut host = fixture.host();
        blocked(
            &fixture.validate(&mut host),
            "ACCEPTED_REPOSITORY_CONTRACT_CHANGED",
        );
        assert_eq!(host.execution_attempts, 0);
        approve_transition_with_caller(&fixture.options.root, &id, "operator", 900).unwrap();

        let report = fixture.validate(&mut host);
        assert!(report.ready && report.proof_ready, "{report:?}");
        assert!(!report.publication_ready());
        assert_eq!(report.target.as_deref(), Some("active-host"));
        assert_eq!(report.resolved_targets, ["active-host"]);
        assert_eq!(host.execution_attempts, 1);
        let evidence = report.evidence_path.unwrap();
        let suite: SuiteEvidence =
            serde_json::from_slice(&fs::read(evidence.join("observations.json")).unwrap()).unwrap();
        let executed: std::collections::BTreeSet<_> =
            suite.results.iter().map(|result| &result.key.job).collect();
        assert_eq!(executed, commands.iter().collect());
        let receipt: serde_json::Value = serde_json::from_slice(
            &fs::read(evidence.join("completion/native-readiness-receipt.json")).unwrap(),
        )
        .unwrap();
        let head = git(&fixture.options.root, &["rev-parse", "HEAD"]);
        runtime::verify_native_receipt_scope(&receipt, &head, Some("active-host")).unwrap();
        assert!(
            runtime::verify_native_receipt_scope(&receipt, &head, None)
                .unwrap_err()
                .to_string()
                .contains("TARGET_RECEIPT_FOR_FULL_REPO")
        );
    }
}

#[test]
fn ordinary_agent_without_policy_control_cannot_approve_policy_transition() {
    let mut fixture = Fixture::new();
    accept_current_on_main(&fixture);
    let script = fixture.options.root.join("scripts/check_source.py");
    let original = fs::read_to_string(&script).unwrap();
    fs::write(script, format!("{original}# unauthorized attempt\n")).unwrap();
    fixture
        .proof
        .required_profile
        .as_mut()
        .unwrap()
        .reviewed_sources_digest =
        runtime::discover_source_inventory(&fixture.options, &Cancellation::default())
            .unwrap()
            .digest()
            .unwrap();
    save_full_config(&fixture);
    fixture.commit("candidate policy transition without policy.control");

    let transition_id = transition_id_for_fixture(&fixture);
    for unauthorized in ["ordinary-agent", "unknown"] {
        let err = approve_transition_with_caller(
            &fixture.options.root,
            &transition_id,
            unauthorized,
            900,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("Policy gate denied")
                || err.to_string().contains("requires trust"),
            "unexpected error for {unauthorized}: {err}"
        );
    }

    let mut host = fixture.host();
    blocked(
        &fixture.validate(&mut host),
        "ACCEPTED_REPOSITORY_CONTRACT_CHANGED",
    );
    assert_eq!(host.selections, 0);
}

#[test]
fn stale_policy_transition_approval_fails_closed_when_base_head_or_digest_changes() {
    let mut fixture = Fixture::new();
    accept_current_on_main(&fixture);
    let script = fixture.options.root.join("scripts/check_source.py");
    let original = fs::read_to_string(&script).unwrap();
    fs::write(script, format!("{original}# transition 1\n")).unwrap();
    fixture
        .proof
        .required_profile
        .as_mut()
        .unwrap()
        .reviewed_sources_digest =
        runtime::discover_source_inventory(&fixture.options, &Cancellation::default())
            .unwrap()
            .digest()
            .unwrap();
    save_full_config(&fixture);
    fixture.commit("transition 1");

    let tid1 = transition_id_for_fixture(&fixture);
    approve_transition_with_caller(&fixture.options.root, &tid1, "operator", 900).unwrap();

    // 1. Candidate head changes -> stale approval fails closed.
    fs::write(fixture.options.root.join("note.txt"), "extra commit\n").unwrap();
    fixture.commit("change candidate head SHA");
    let mut host = fixture.host();
    blocked(
        &fixture.validate(&mut host),
        "ACCEPTED_REPOSITORY_CONTRACT_CHANGED",
    );
    assert_eq!(host.selections, 0);

    // Approve for new head, then advance accepted base -> stale approval fails closed.
    let tid2 = transition_id_for_fixture(&fixture);
    approve_transition_with_caller(&fixture.options.root, &tid2, "operator", 900).unwrap();
    let work_branch = git(
        &fixture.options.root,
        &["rev-parse", "--abbrev-ref", "HEAD"],
    );
    git(&fixture.options.root, &["checkout", "main"]);
    git(
        &fixture.options.root,
        &["commit", "--allow-empty", "-m", "advance accepted base"],
    );
    git(&fixture.options.root, &["push", "origin", "main"]);
    git(&fixture.options.root, &["checkout", &work_branch]);
    git(
        &fixture.options.root,
        &["merge", "--no-edit", "origin/main"],
    );
    let mut host = fixture.host();
    blocked(
        &fixture.validate(&mut host),
        "ACCEPTED_REPOSITORY_CONTRACT_CHANGED",
    );
    assert_eq!(host.selections, 0);

    // Approve merged head, then change command digest -> stale approval fails closed.
    let tid3 = transition_id_for_fixture(&fixture);
    approve_transition_with_caller(&fixture.options.root, &tid3, "operator", 900).unwrap();
    fixture
        .proof
        .repository_contract
        .as_mut()
        .unwrap()
        .executions
        .get_mut("inspect-second")
        .unwrap()
        .timeout_seconds += 5;
    fixture.review_commands();
    save_full_config(&fixture);
    fixture.commit("change command digest after approval");
    let mut host = fixture.host();
    blocked(
        &fixture.validate(&mut host),
        "ACCEPTED_REPOSITORY_CONTRACT_CHANGED",
    );
    assert_eq!(host.selections, 0);
}

#[test]
fn revoked_or_expired_policy_transition_approval_fails_closed() {
    use clap::Parser;
    use decapod::core::store::{Store, StoreKind};
    use decapod::plugins::policy::{PolicyCli, policy_db_path, run_policy_cli_as};

    let mut fixture = Fixture::new();
    accept_current_on_main(&fixture);
    let script = fixture.options.root.join("scripts/check_source.py");
    let original = fs::read_to_string(&script).unwrap();
    fs::write(script, format!("{original}# revoke and expiry check\n")).unwrap();
    fixture
        .proof
        .required_profile
        .as_mut()
        .unwrap()
        .reviewed_sources_digest =
        runtime::discover_source_inventory(&fixture.options, &Cancellation::default())
            .unwrap()
            .digest()
            .unwrap();
    save_full_config(&fixture);
    fixture.commit("transition for revoke/expiry test");

    let tid = transition_id_for_fixture(&fixture);
    let approval_id =
        approve_transition_with_caller(&fixture.options.root, &tid, "operator", 900).unwrap();

    let mut host = fixture.host();
    let ok_report = fixture.validate(&mut host);
    assert!(ok_report.ready, "{ok_report:?}");

    // Revoke via `decapod govern policy revoke` -> fails closed.
    let data_root = fixture.options.root.join(".decapod/data");
    let store = Store {
        kind: StoreKind::Repo,
        root: data_root.clone(),
    };
    run_policy_cli_as(
        &store,
        PolicyCli::try_parse_from(["policy", "revoke", "--id", &approval_id]).unwrap(),
        "operator",
    )
    .unwrap();
    let mut host = fixture.host();
    blocked(
        &fixture.validate(&mut host),
        "ACCEPTED_REPOSITORY_CONTRACT_CHANGED",
    );
    assert_eq!(host.selections, 0);

    // Re-approve, then expire the grant -> fails closed.
    let expired_id =
        approve_transition_with_caller(&fixture.options.root, &tid, "operator", 900).unwrap();
    let conn = decapod::core::db::Connection::open(policy_db_path(&data_root)).unwrap();
    conn.execute(
        "UPDATE approvals SET expires_at = '1Z' WHERE approval_id = ?1",
        decapod::params![expired_id],
    )
    .unwrap();
    drop(conn);
    let mut host = fixture.host();
    blocked(
        &fixture.validate(&mut host),
        "ACCEPTED_REPOSITORY_CONTRACT_CHANGED",
    );
    assert_eq!(host.selections, 0);
}

#[test]
fn decapod_strict_profile_downgrade_remains_unconditionally_blocked_even_with_approval() {
    let fixture = Fixture::with_accepted_config(Some(&format!(
        "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"ascii\"\nentrypoints = []\n[repo]\nbase_branch = \"main\"\n[proof.required_profile]\nid = {:?}\nreviewed_sources_digest = {:?}\n",
        readiness_profile::REQUIRED_PROFILE,
        format!("sha256:{}", "0".repeat(64)),
    )));
    save_full_config(&fixture);
    fixture.commit("attempt candidate downgrade from strict decapod profile");
    let tid = transition_id_for_fixture(&fixture);
    approve_transition_with_caller(&fixture.options.root, &tid, "operator", 900).unwrap();
    let mut host = fixture.host();
    blocked(&fixture.validate(&mut host), "REQUIRED_PROFILE_DOWNGRADE");
    assert_eq!(host.selections, 0);
}

fn convert_fixture_to_per_platform(fixture: &mut Fixture) -> String {
    use decapod::core::native_proof::NativePlatform;
    use decapod::core::repository_profile::{CANONICAL_PLATFORM_KEYS, PlatformExecution};
    use std::collections::BTreeMap;
    use std::path::Path;

    let current_key = repository_profile::platform_key(NativePlatform::current().unwrap())
        .unwrap()
        .to_string();
    let sample_exec = fixture
        .proof
        .repository_contract
        .as_ref()
        .unwrap()
        .executions
        .values()
        .next()
        .unwrap()
        .clone();
    let bin_name = Path::new(&sample_exec.program)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    for command in &mut fixture.proof.commands {
        command.command = bin_name.clone();
    }
    let contract = fixture.proof.repository_contract.as_mut().unwrap();
    for execution in contract.executions.values_mut() {
        let host_entry = PlatformExecution {
            program: std::mem::take(&mut execution.program),
            toolchain_files: std::mem::take(&mut execution.toolchain_files),
        };
        let mut platforms = BTreeMap::new();
        for key in CANONICAL_PLATFORM_KEYS {
            if key == current_key {
                platforms.insert(key.to_string(), host_entry.clone());
            } else {
                platforms.insert(
                    key.to_string(),
                    PlatformExecution {
                        program: format!("/nix/store/nonexistent-{key}/bin/{bin_name}"),
                        toolchain_files: BTreeMap::from([(
                            "runtime".into(),
                            format!("/nix/store/nonexistent-{key}/lib/runtime"),
                        )]),
                    },
                );
            }
        }
        execution.platforms = platforms;
    }
    fixture.review_commands();
    fixture.save_config();
    fixture.commit("configure reviewed per-platform executables");
    current_key
}

#[test]
fn per_platform_contract_selects_and_executes_current_host_native_executable() {
    let mut fixture = Fixture::new();
    let current_key = convert_fixture_to_per_platform(&mut fixture);
    // Also attach an owned runtime closure file to the current host platform entry
    // so before/after measurement covers both the native binary and declared closure.
    let owned_closure = fixture._storage.path().join("owned-host-closure.txt");
    fs::write(&owned_closure, b"reviewed host closure\n").unwrap();
    for execution in fixture
        .proof
        .repository_contract
        .as_mut()
        .unwrap()
        .executions
        .values_mut()
    {
        execution
            .platforms
            .get_mut(&current_key)
            .unwrap()
            .toolchain_files
            .insert(
                "owned-host-closure".into(),
                owned_closure
                    .canonicalize()
                    .unwrap()
                    .to_string_lossy()
                    .into(),
            );
    }
    fixture.review_commands();
    fixture.save_config();
    fixture.commit("add owned host closure to per-platform entry");

    let mut host = fixture.host();
    let report = fixture.validate(&mut host);
    assert!(report.ready, "{report:?}");
    assert_eq!(report.exit_code(), 0);
    assert_eq!(report.scope, repository_profile::READINESS_SCOPE);
    assert_eq!(host.execution_attempts, 1);
    let directory = report.evidence_path.unwrap();
    let evidence: SuiteEvidence =
        serde_json::from_slice(&fs::read(directory.join("observations.json")).unwrap()).unwrap();
    assert_eq!(evidence.results.len(), 2);
    assert!(evidence.results.iter().all(|result| matches!(
        result.observation,
        Observation::Executed {
            conclusion: Conclusion::Success,
            exit_code: Some(0),
            ..
        }
    )));
    assert!(
        directory
            .join("completion/native-readiness-receipt.json")
            .is_file()
    );
}

#[test]
fn per_platform_contract_rejects_missing_host_platform_and_incompatible_native_format() {
    // 1. Missing current host platform fails closed before execution.
    let mut missing_fixture = Fixture::new();
    let current_key = convert_fixture_to_per_platform(&mut missing_fixture);
    for execution in missing_fixture
        .proof
        .repository_contract
        .as_mut()
        .unwrap()
        .executions
        .values_mut()
    {
        execution.platforms.remove(&current_key);
    }
    missing_fixture.review_commands();
    missing_fixture.save_config();
    missing_fixture.commit("omit current host platform");
    let mut missing_host = missing_fixture.host();
    let missing_report = missing_fixture.validate(&mut missing_host);
    blocked(&missing_report, "MISSING_PLATFORM_EXECUTION");
    assert_eq!(missing_host.execution_attempts, 0);

    // 2. Host platform pointing to a foreign-architecture binary fails closed at native execution.
    let mut wrong_fmt_fixture = Fixture::new();
    let current_key = convert_fixture_to_per_platform(&mut wrong_fmt_fixture);
    let foreign_dir = wrong_fmt_fixture._storage.path().join("foreign-bin");
    fs::create_dir(&foreign_dir).unwrap();
    let bin_name = wrong_fmt_fixture.proof.commands[0].command.clone();
    let foreign_bin = foreign_dir.join(&bin_name);
    // Minimal 64-byte foreign header: ELF x86_64 on macOS, Mach-O arm64 on Linux.
    let mut header = vec![0u8; 64];
    let (expected_detected, expected_host_phrase) = if cfg!(target_os = "macos") {
        header[0..4].copy_from_slice(b"\x7fELF");
        header[4] = 2; // 64-bit
        header[5] = 1; // little-endian
        header[18..20].copy_from_slice(&0x3Eu16.to_le_bytes()); // x86_64
        ("ELF x86_64", "native macOS validation requires Mach-O")
    } else {
        header[0..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]); // MH_MAGIC_64 little-endian
        header[4..8].copy_from_slice(&0x0100_000Cu32.to_le_bytes()); // arm64
        ("Mach-O arm64", "native Linux validation requires ELF")
    };
    fs::write(&foreign_bin, &header).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&foreign_bin, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let foreign_bin = foreign_bin.canonicalize().unwrap();
    let foreign_path = foreign_bin.to_string_lossy().into_owned();
    for execution in wrong_fmt_fixture
        .proof
        .repository_contract
        .as_mut()
        .unwrap()
        .executions
        .values_mut()
    {
        let entry = execution.platforms.get_mut(&current_key).unwrap();
        entry.program = foreign_path.clone();
        entry.toolchain_files.clear();
        entry
            .toolchain_files
            .insert("tool".into(), foreign_path.clone());
    }
    wrong_fmt_fixture.review_commands();
    wrong_fmt_fixture.save_config();
    wrong_fmt_fixture.commit("point host platform at foreign executable");
    let mut wrong_host = wrong_fmt_fixture.host();
    let wrong_report = wrong_fmt_fixture.validate(&mut wrong_host);
    assert!(!wrong_report.ready, "{wrong_report:?}");
    assert!(!wrong_report.proof_ready);
    assert_eq!(wrong_host.execution_attempts, 1);
    blocked(&wrong_report, &foreign_path);
    blocked(&wrong_report, expected_detected);
    blocked(&wrong_report, expected_host_phrase);
    if let Some(evidence_dir) = wrong_report.evidence_path.as_ref() {
        assert!(
            !evidence_dir
                .join("completion/native-readiness-receipt.json")
                .exists()
        );
    }
}

#[test]
fn per_platform_accepted_contract_rejects_candidate_platform_changes() {
    use decapod::core::native_proof::NativePlatform;

    let current_key = repository_profile::platform_key(NativePlatform::current().unwrap()).unwrap();
    let non_host_key = if current_key == "linux-x86_64" {
        "macos-aarch64"
    } else {
        "linux-x86_64"
    };

    for mutation in [
        "modify-non-host-program",
        "modify-non-host-toolchain",
        "remove-non-host-platform",
        "switch-to-single",
    ] {
        let mut fixture = Fixture::new();
        convert_fixture_to_per_platform(&mut fixture);
        // Accept the multi-platform contract on main before the candidate modifies it.
        git(&fixture.options.root, &["push", "origin", "HEAD:main"]);
        git(&fixture.options.root, &["fetch", "origin", "main"]);

        let execution = fixture
            .proof
            .repository_contract
            .as_mut()
            .unwrap()
            .executions
            .get_mut("inspect-second")
            .unwrap();
        match mutation {
            "modify-non-host-program" => {
                let bin_name = fixture.proof.commands[0].command.clone();
                execution.platforms.get_mut(non_host_key).unwrap().program =
                    format!("/nix/store/evil-{non_host_key}/bin/{bin_name}");
            }
            "modify-non-host-toolchain" => {
                execution
                    .platforms
                    .get_mut(non_host_key)
                    .unwrap()
                    .toolchain_files
                    .insert("injected".into(), "/nix/store/evil/lib/x".into());
            }
            "remove-non-host-platform" => {
                execution.platforms.remove(non_host_key);
            }
            "switch-to-single" => {
                let host_entry = execution.platforms.remove(current_key).unwrap();
                execution.platforms.clear();
                execution.program = host_entry.program;
                execution.toolchain_files = host_entry.toolchain_files;
            }
            _ => unreachable!(),
        }
        fixture.review_commands();
        fixture.save_config();
        fixture.commit("repin candidate change to accepted per-platform contract");
        let mut host = fixture.host();
        blocked(
            &fixture.validate(&mut host),
            "ACCEPTED_REPOSITORY_CONTRACT_CHANGED",
        );
        assert_eq!(host.selections, 0, "{mutation}");
        assert_eq!(host.execution_attempts, 0, "{mutation}");
    }
}
