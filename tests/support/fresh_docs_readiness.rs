//! Genuine CLI admission followed by complete sealed runtime proof.
use super::consumer_readiness_fixture::{
    CONFIG, Fixture, GovernedAdmission, git, governed_command,
};
use decapod::core::bounded_process::{BoundedCommand, Cancellation};
use decapod::core::readiness_evidence::{Observation, SuiteEvidence};
use decapod::core::validation_runtime as runtime;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(unix)]
fn governed_failure(diagnostic: String) -> ! {
    // Preserve the actual error if the aggregate target expires before
    // libtest prints its captured failure summary; keep diagnostics bounded.
    let bytes = diagnostic.as_bytes();
    let _ = io::Write::write_all(&mut io::stderr(), &bytes[..bytes.len().min(16_384)]);
    panic!("{diagnostic}");
}

#[cfg(unix)]
fn governed_ok(root: &Path, storage: &Path, args: &[&str]) -> serde_json::Value {
    let output = governed_command(root, storage, args)
        .bounded_output(Duration::from_secs(30))
        .unwrap();
    if !output.status.success() {
        let diagnostic = format!(
            "{args:?}: stdout={} stderr={}\n",
            String::from_utf8_lossy(&output.stdout[..output.stdout.len().min(8192)]),
            String::from_utf8_lossy(&output.stderr[..output.stderr.len().min(8192)])
        );
        governed_failure(diagnostic);
    }
    serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null)
}

// The existing required entrypoint target owns the single #[test] caller.
// Keep shared support free of test cases so API regressions execute once.
#[cfg(unix)]
pub fn fresh_governed_docs_cli_admission_then_runtime_executes_first_proof() {
    governed_docs_proof(false, false);
}

#[cfg(unix)]
pub fn canonical_local_cli_proves_and_rejects_stale_authority() {
    governed_docs_proof(true, false);
}

#[cfg(unix)]
pub fn canonical_local_workunit_rejects_superseded_proof_and_recovers() {
    governed_docs_proof(true, true);
}

#[cfg(unix)]
fn governed_docs_proof(local_target: bool, workunit_currentness: bool) {
    use decapod::core::governance_document;
    let started = std::time::Instant::now();
    let mut fixture = Fixture::new();
    let storage = fixture._storage.path().to_path_buf();
    let root = fixture.options.root.clone();
    fs::create_dir(storage.join("home")).unwrap();
    git(&root, &["checkout", "-B", "main"]);
    fs::write(root.join("README.md"), "# Governed consumer\n\nReviewed source commands inspect the committed documentation payload.\n").unwrap();
    // The adapter-only fixture deliberately stores a proof subsection.
    // Public CLI init must start without that partial config so it can
    // generate the complete schema before the real contract is reviewed.
    fs::remove_file(root.join(CONFIG)).unwrap();
    governed_ok(
        &root,
        &storage,
        &[
            "init",
            "--force",
            "--backend",
            "local",
            "--no-container-workspaces",
        ],
    );
    let failure_flag = storage.join("required-check-failure");
    if workunit_currentness {
        // Explicitly reviewed external input, outside the captured source/tool
        // bytes. This creates a real later required-check failure, without
        // pretending to establish complete external-input observability.
        fixture
            .proof
            .commands
            .iter_mut()
            .find(|command| command.name == "inspect-source")
            .unwrap()
            .args = vec![
            "-I".into(),
            "-S".into(),
            "-B".into(),
            "-c".into(),
            format!(
                "import os; assert not os.path.exists({:?}), 'observed required failure'; exec(compile(open('scripts/check_source.py').read(), 'scripts/check_source.py', 'exec'))",
                failure_flag.to_str().unwrap()
            ),
        ];
        fixture.review_commands();
    }
    // Init preserves product context; the fixture explicitly re-reviews its
    // real repository command/source contract after generated surfaces exist.
    let config = fs::read_to_string(root.join(CONFIG)).unwrap();
    let mut config: toml::Value = toml::from_str(&config).unwrap();
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
    // This scenario proves first adoption through the genuine CLI. Its
    // workspace-specific execution environment is authored only after the
    // workspace exists; do not pretend that later policy was accepted on main.
    // Separate consumer tests preserve and execute an already accepted contract.
    config["proof"] = if local_target {
        fixture
            .proof
            .repository_contract
            .as_mut()
            .unwrap()
            .canonical_local_target = true;
        fixture.review_commands();
        toml::Value::try_from(&fixture.proof).unwrap()
    } else {
        toml::Value::try_from(decapod::core::proof::ProjectProofConfig::default()).unwrap()
    };
    config["repo"]["base_branch"] = "main".into();
    fs::write(root.join(CONFIG), toml::to_string_pretty(&config).unwrap()).unwrap();
    // Init seeds the accepted baseline. Authoring refresh belongs in the
    // managed task worktree below, after its real claim and orientation.
    git(&root, &["add", "-A"]);
    git(
        &root,
        &[
            "commit",
            "-m",
            "docs: accept governed consumer configuration",
        ],
    );
    git(&root, &["push", "origin", "main"]);
    if !local_target {
        git(
            &root,
            &[
                "remote",
                "set-url",
                "origin",
                "https://github.com/consumer/project.git",
            ],
        );
    }

    let task = governed_ok(
        &root,
        &storage,
        &[
            "todo",
            "add",
            "docs: document reviewed source proof",
            "--format",
            "json",
        ],
    );
    let id = task["id"].as_str().unwrap();
    assert!(
        id.starts_with("docs_"),
        "fixture must preserve docs applicability: {task}"
    );
    governed_ok(&root, &storage, &["todo", "claim", "--id", id]);
    let branch = format!("agent/docs-bootstrap/{id}-first-proof");
    let workspace = governed_ok(
        &root,
        &storage,
        &["workspace", "ensure", "--branch", &branch],
    );
    let workspace = PathBuf::from(workspace["worktree_path"].as_str().unwrap());
    governed_ok(
        &workspace,
        &storage,
        &["infer", "orientation", "--task-id", id],
    );
    governed_ok(&workspace, &storage, &["govern", "plan", "approve"]);
    let intent = workspace.join(".decapod/managed/specs/INTENT.md");
    let prose = fs::read_to_string(&intent).unwrap();
    fs::write(intent, format!("{prose}\n## Reviewed documentation proof\n\nThe docs task validates both committed payload files through the required real Python commands. A failed assertion must prevent readiness and receipt issuance.\n")).unwrap();
    fixture.options.root = workspace.clone();
    let contract = fixture.proof.repository_contract.as_mut().unwrap();
    for execution in contract.executions.values_mut().filter(|_| !local_target) {
        execution.environment.insert(
            "FIXTURE_WORKTREE".into(),
            workspace.to_string_lossy().into_owned(),
        );
    }
    fixture.review_commands();
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
    let mut config: toml::Value =
        toml::from_str(&fs::read_to_string(workspace.join(CONFIG)).unwrap()).unwrap();
    config["proof"] = toml::Value::try_from(&fixture.proof).unwrap();
    fs::write(
        workspace.join(CONFIG),
        toml::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
    fs::write(workspace.join("payload.txt"), "ready\n").unwrap();
    governed_ok(&workspace, &storage, &["rpc", "--op", "specs.refresh"]);
    for name in ["ARCHITECTURE", "INTERFACES", "SECURITY"] {
        governed_ok(
            &workspace,
            &storage,
            &[
                "govern",
                "plan",
                "review-spec",
                "--path",
                &format!(".decapod/managed/specs/{name}.md"),
                "--disposition",
                "unchanged-with-reason",
                "--reason",
                "Documentation fixture retains product architecture, interfaces and security; reviewed command environment binds the actual managed workspace",
            ],
        );
    }
    git(&workspace, &["add", "-A"]);
    governed_ok(
        &workspace,
        &storage,
        &[
            "govern",
            "artifacts",
            "checkpoint",
            "--id",
            "docs-first-proof",
            "--summary",
            "Fresh documentation source and real reviewed command contract",
        ],
    );
    git(&workspace, &["add", governance_document::GOVERNANCE_PATH]);
    git(
        &workspace,
        &["commit", "-m", "docs: describe first governed proof"],
    );
    governed_ok(
        &workspace,
        &storage,
        &["workspace", "ensure", "--branch", &branch],
    );
    assert!(
        governance_document::read_section(&workspace, "validation")
            .unwrap()
            .is_none()
    );
    assert!(
        !workspace
            .join(decapod::core::capsule_policy::GENERATED_POLICY_REL_PATH)
            .exists(),
        "fresh worktrees must not need an ignored policy cache to admit proof"
    );
    let prepared_status = git(&workspace, &["status", "--porcelain"]);
    assert!(
        prepared_status.is_empty(),
        "prepared candidate must be clean: {prepared_status}"
    );
    let head = git(&workspace, &["rev-parse", "HEAD"]);
    if workunit_currentness {
        prove_current_workunit(&workspace, &storage, id, &head, &failure_flag);
        return;
    }
    // These are genuine admitted CLI invocations with provider flags removed by
    // governed_command. A diagnostic skip cannot authorize any full boundary.
    for (args, name, value) in [
        (
            vec!["validate", "--format", "json"],
            "DECAPOD_VALIDATE_SKIP_GIT_GATES",
            "0",
        ),
        (
            vec!["rpc", "--op", "validate.run", "--params", "{}"],
            "DECAPOD_VALIDATE_SKIP_TOOLING_GATES",
            "1",
        ),
        (
            vec!["workspace", "publish"],
            "DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES",
            "true",
        ),
        (
            vec!["todo", "done", "--id", id, "--validated", "--commit", &head],
            "DECAPOD_VALIDATE_SKIP_GIT_GATES",
            "1",
        ),
    ] {
        let output = governed_command(&workspace, &storage, &args)
            .env(name, value)
            .bounded_output(Duration::from_secs(30))
            .unwrap();
        let diagnostic = format!(
            "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            diagnostic.contains("READINESS_GATE_OVERRIDE_UNSUPPORTED"),
            "{args:?} with {name}: {diagnostic}"
        );
        let response = serde_json::from_slice::<serde_json::Value>(&output.stdout).ok();
        if let Some(response) = &response {
            assert_ne!(response["success"], true);
            assert_ne!(response["publication_ready"], true);
            assert_ne!(response["result"]["publication_ready"], true);
        }
        assert!(
            !output.status.success()
                || response.is_some_and(|response| response["success"] == false),
            "an exit-zero transport must still report a failed judgment: {diagnostic}"
        );
        assert!(
            governance_document::read_section(&workspace, "validation")
                .unwrap()
                .is_none()
        );
        assert_eq!(git(&workspace, &["rev-parse", "HEAD"]), head);
        assert!(git(&workspace, &["status", "--porcelain"]).is_empty());
    }
    let premature_done = governed_command(
        &workspace,
        &storage,
        &["todo", "done", "--id", id, "--validated", "--commit", &head],
    )
    .bounded_output(Duration::from_secs(30))
    .unwrap();
    assert!(
        !premature_done.status.success(),
        "proof-backed todo completion must fail before first validation proof executes: stdout={} stderr={}",
        String::from_utf8_lossy(&premature_done.stdout),
        String::from_utf8_lossy(&premature_done.stderr)
    );
    let pre_proof_task = governed_ok(
        &workspace,
        &storage,
        &["todo", "get", "--id", id, "--format", "json"],
    );
    assert_ne!(pre_proof_task["item"]["status"], "done");
    assert!(pre_proof_task["item"]["completed_at"].is_null());
    assert_eq!(git(&workspace, &["rev-parse", "HEAD"]), head);
    assert!(git(&workspace, &["status", "--porcelain"]).is_empty());
    if local_target {
        prove_canonical_local_cli(&workspace, &root, &storage, id, &head);
        return;
    }
    let mut host = fixture.host();
    let admission_observation = std::rc::Rc::new(std::cell::RefCell::new(None));
    host.governed_admission = Some(GovernedAdmission {
        storage: storage.clone(),
        observation: admission_observation.clone(),
    });
    let readiness = fixture.validate(&mut host);
    let observed_admission = admission_observation.borrow();
    let admission = observed_admission.as_ref().unwrap_or_else(|| {
        governed_failure(format!(
            "Actual CLI admission must pass before source proof: {readiness:?}"
        ))
    });
    assert_eq!(admission["scope"], "ci_local");
    assert_eq!(admission["publication_ready"], false);
    assert_eq!(admission["report"]["fail_count"], 0);
    if !readiness.ready {
        governed_failure(format!(
            "Expected complete runtime readiness: {readiness:?}"
        ));
    }
    assert_eq!(readiness.exit_code(), 0);
    assert_eq!(host.execution_attempts, 1);
    assert_eq!(git(&workspace, &["rev-parse", "HEAD"]), head);
    let final_status = git(&workspace, &["status", "--porcelain"]);
    assert!(
        final_status.is_empty(),
        "CLI admission and complete runtime must preserve the candidate: {final_status}"
    );
    let evidence = readiness
        .evidence_path
        .as_ref()
        .expect("real proof must have executed, not failed at prerequisites");
    let observations: SuiteEvidence =
        serde_json::from_slice(&fs::read(evidence.join("observations.json")).unwrap()).unwrap();
    assert_eq!(
        observations.results.len(),
        2,
        "every reviewed command must execute"
    );
    assert!(
        observations
            .results
            .iter()
            .all(|result| matches!(result.observation, Observation::Executed { .. }))
    );
    for artifact in [
        "completion/native-readiness-receipt.json",
        "completion/verdict.json",
        "historical-proof.json",
    ] {
        assert!(evidence.join(artifact).is_file(), "{artifact}");
    }
    // A prerequisite receipt must never appear as a substitute for full proof.
    assert!(
        governance_document::read_section(&workspace, "validation")
            .unwrap()
            .is_none()
    );
    assert!(readiness.proof_ready);
    let rules: serde_json::Value =
        serde_json::from_slice(&fs::read(evidence.join("terminal-rules.json")).unwrap()).unwrap();
    assert_eq!(rules["overall_verdict"], "pass");
    assert!(
        rules["results"].as_array().unwrap().iter().any(|rule| {
            rule["rule_id"] == "decapod.evidence.required_and_fresh" && rule["verdict"] == "pass"
        }),
        "the docs evidence rule must consume the actual complete proof"
    );
    assert!(
        !rules["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|rule| rule["rule_id"] == "decapod.evidence.proof_backed_completion")
    );
    let intent_id = governance_document::read_section(&workspace, "trajectory")
        .unwrap()
        .and_then(|value| {
            value
                .get("intent_id")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        })
        .expect("governed orientation must populate trajectory intent_id");
    // Workunit journals are workspace-local. Initialize their runtime through
    // the public CLI before introducing the negative completion declaration;
    // the root repository's initialized datastore does not initialize this one.
    governed_ok(&workspace, &storage, &["init", "--refresh", "--proof"]);
    governed_ok(
        &workspace,
        &storage,
        &[
            "govern",
            "workunit",
            "init",
            "--task-id",
            id,
            "--intent-ref",
            &intent_id,
        ],
    );
    governed_ok(
        &workspace,
        &storage,
        &[
            "govern",
            "workunit",
            "set-proof-plan",
            "--task-id",
            id,
            "--gate",
            "docs-proof",
        ],
    );
    let premature_verified = governed_command(
        &workspace,
        &storage,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            id,
            "--to",
            "verified",
        ],
    )
    .bounded_output(Duration::from_secs(30))
    .unwrap();
    assert!(
        !premature_verified.status.success(),
        "workunit transition to verified must fail without passing proof-plan gates"
    );
    governed_ok(
        &workspace,
        &storage,
        &[
            "govern",
            "workunit",
            "checkpoint",
            "--task-id",
            id,
            "--operation-id",
            "docs-unverified-completion",
        ],
    );
    assert_eq!(git(&workspace, &["rev-parse", "HEAD"]), head);
    assert!(
        git(&workspace, &["status", "--porcelain"]).is_empty(),
        "runtime workunit state under .decapod/data must keep candidate clean"
    );
    let mut unverified_host = fixture.host();
    let unverified = fixture.validate(&mut unverified_host);
    assert!(
        !unverified.ready,
        "declared unverified workunit must prevent readiness: {unverified:?}"
    );
    assert_ne!(unverified.exit_code(), 0);
    assert!(
        unverified.blockers.iter().any(|blocker| {
            blocker.detail.contains("TERMINAL_WORKUNIT_UNPROVEN")
                || blocker.detail.contains("TERMINAL_VALIDATION_RULES_FAILED")
        }),
        "terminal workunit check must reject unproven/unverified declaration: {unverified:?}"
    );
    if let Some(unverified_evidence) = &unverified.evidence_path {
        assert!(
            !unverified_evidence
                .join("completion/native-readiness-receipt.json")
                .exists()
        );
        assert!(!unverified_evidence.join("historical-proof.json").exists());
    }
    let _ = io::Write::write_all(
        &mut io::stderr(),
        format!(
            "Fresh docs CLI admission and complete runtime: {:?}\n",
            started.elapsed()
        )
        .as_bytes(),
    );
}

#[cfg(unix)]
fn prove_current_workunit(
    workspace: &Path,
    storage: &Path,
    task: &str,
    head: &str,
    failure_flag: &Path,
) {
    let wu = |args: &[&str]| {
        let mut command = vec!["govern", "workunit"];
        command.extend_from_slice(args);
        command.extend(["--task-id", task]);
        governed_ok(workspace, storage, &command)
    };
    let validate = |success: bool| {
        let output = governed_command(workspace, storage, &["validate", "--format", "json"])
            .bounded_output(Duration::from_secs(300))
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            output.status.success(),
            success,
            "{value} {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(value["readiness"]["ready"], success, "{value}");
        assert_eq!(
            value["readiness"]["scope"], "repository_local_git_readiness",
            "{value}"
        );
        assert_eq!(value["publication_ready"], false);
        value
    };
    let governance = governed_ok(workspace, storage, &["govern", "artifacts", "status"]);
    let intent = governance["sections"]["trajectory"]["intent_id"]
        .as_str()
        .unwrap();
    wu(&["init", "--intent-ref", intent]);
    wu(&["attach-spec", "--ref", ".decapod/managed/specs/INTENT.md"]);
    governed_ok(
        workspace,
        storage,
        &[
            "govern",
            "capsule",
            "query",
            "--topic",
            "canonical current workunit proof",
            "--scope",
            "interfaces",
            "--task-id",
            task,
            "--workunit-id",
            task,
            "--write",
        ],
    );
    wu(&[
        "attach-state",
        "--ref",
        &format!(".decapod/managed/context/{task}.json"),
    ]);
    wu(&["set-proof-plan", "--gate", "validate_passes"]);
    wu(&["checkpoint", "--operation-id", "before-first-full-proof"]);
    wu(&["transition", "--to", "executing"]);
    wu(&["transition", "--to", "claimed"]);
    validate(true);
    let verified = wu(&["transition", "--to", "verified"]);
    assert_eq!(verified["status"], "VERIFIED");
    let original_reference = verified["validation_record"].clone();
    assert!(original_reference.is_object());
    let generation = wu(&["journal-status"])["generation"].as_u64().unwrap();

    fs::write(failure_flag, "operator-owned external failure control\n").unwrap();
    let failed = validate(false);
    let evidence = PathBuf::from(failed["readiness"]["evidence_path"].as_str().unwrap());
    // Captures follow canonical check order, not declaration insertion order.
    // Bind the failure assertion to its actual check instead of accepting an
    // unrelated check's result or assuming that inspect-source is native-0.
    let captures: Vec<_> = (0..2)
        .filter(|index| {
            let path = evidence.join(format!("native-{index}.diagnostic.json"));
            if !path.exists() {
                return false;
            }
            let diagnostic: serde_json::Value =
                serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
            diagnostic["schema_version"] == "1.0.0"
                && diagnostic["check"]["job"] == "inspect-source"
                && diagnostic["check"]["provider"] == "native"
                && diagnostic["check"]["workflow"] == ".decapod/config.toml"
                && diagnostic["check"]["profile"] == "repo.pr-readiness.v1"
                && diagnostic["check"]["matrix"] == serde_json::json!({})
        })
        .collect();
    assert_eq!(
        captures.len(),
        1,
        "failure control must have one bound capture"
    );
    let capture = captures[0];
    let steps: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence.join(format!("native-{capture}.steps.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(steps[0]["exit_code"], 1, "{steps}");
    assert_eq!(steps[0]["termination"], "Exited", "{steps}");
    assert!(steps[0]["error"].is_null(), "{steps}");
    assert!(
        fs::read_to_string(evidence.join(format!("native-{capture}.stderr")))
            .unwrap()
            .contains("observed required failure")
    );
    let withdrawn = wu(&["get"]);
    assert_eq!(withdrawn["status"], "CLAIMED");
    assert!(withdrawn["validation_record"].is_null());
    assert!(withdrawn["validation_epoch"].is_null());
    assert_eq!(withdrawn["proof_results"], serde_json::json!([]));
    assert_eq!(withdrawn["proof_plan"], verified["proof_plan"]);
    assert!(wu(&["journal-status"])["generation"].as_u64().unwrap() > generation);
    let old_promotion = rejected_local_claim(
        workspace,
        storage,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            task,
            "--to",
            "verified",
        ],
    );
    assert!(
        old_promotion.contains("WORKUNIT_PROOF_STALE"),
        "{old_promotion}"
    );
    // Failure narrows current authority without deleting the real prior success.
    let historical = governed_ok(workspace, storage, &["qa", "verify", "commit", head]);
    assert_eq!(historical["status"], "passed");
    assert_eq!(historical["publication_ready"], false);
    fs::remove_file(failure_flag).unwrap();
    wu(&["checkpoint", "--operation-id", "after-observed-failure"]);
    validate(true);
    let repaired = wu(&["transition", "--to", "verified"]);
    assert_eq!(repaired["status"], "VERIFIED");
    assert_ne!(repaired["validation_record"], original_reference);

    governed_ok(
        workspace,
        storage,
        &[
            "todo",
            "renew",
            "--id",
            task,
            "--lease-seconds",
            "86400",
            "--format",
            "json",
        ],
    );
    assert_eq!(wu(&["journal-status"])["resumable"], false);
    rejected_local_claim(
        workspace,
        storage,
        &[
            "govern",
            "workunit",
            "resume",
            "--task-id",
            task,
            "--operation-id",
            "old-custody-refused",
        ],
    );
    wu(&["transition", "--to", "claimed"]);
    wu(&["checkpoint", "--operation-id", "renewed-custody"]);
    validate(true);
    let renewed = wu(&["transition", "--to", "verified"]);
    assert_eq!(renewed["status"], "VERIFIED");
    assert_ne!(renewed["validation_record"], repaired["validation_record"]);
    assert_eq!(wu(&["journal-status"])["resumable"], true);
    wu(&["resume", "--operation-id", "current-custody-resume"]);
    // General historical inspection still refuses ambiguous custody.
    let ambiguous = rejected_local_claim(workspace, storage, &["qa", "verify", "commit", head]);
    assert!(
        ambiguous.contains("different task contracts or custody"),
        "{ambiguous}"
    );
    assert_eq!(git(workspace, &["rev-parse", "HEAD"]), head);
    assert!(git(workspace, &["status", "--porcelain"]).is_empty());
}

/// Uses the actual unmodified CLI boundary, with provider environment removed by
/// governed_command. No Host substitution, fake Git result or success receipt.
#[cfg(unix)]
fn prove_canonical_local_cli(
    workspace: &Path,
    root: &Path,
    storage: &Path,
    task: &str,
    head: &str,
) {
    let start = std::time::Instant::now();
    let store_path = root.join(".decapod/data");
    let store_before = retained_bytes(&store_path);
    let output = governed_command(workspace, storage, &["validate", "--format", "json"])
        .bounded_output(Duration::from_secs(300))
        .unwrap();
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "actual local validate: {} {diagnostic}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert!(output.status.success(), "{result} {diagnostic}");
    // Preserve the established native validator status; readiness is a separate judgment.
    assert_eq!(result["status"], "ok", "{result}");
    assert_eq!(
        result["publication_ready"], false,
        "forge target is not proven"
    );
    assert_eq!(result["readiness"]["ready"], true, "{result}");
    assert_eq!(result["readiness"]["proof_ready"], true);
    assert_eq!(
        result["readiness"]["scope"],
        "repository_local_git_readiness"
    );
    let evidence = PathBuf::from(result["readiness"]["evidence_path"].as_str().unwrap());
    let observations: SuiteEvidence =
        serde_json::from_slice(&fs::read(evidence.join("observations.json")).unwrap()).unwrap();
    assert_eq!(observations.results.len(), 2);
    assert!(
        observations
            .results
            .iter()
            .all(|row| matches!(row.observation, Observation::Executed { .. }))
    );
    for path in [
        "completion/native-readiness-receipt.json",
        "completion/verdict.json",
        "historical-proof.json",
    ] {
        assert!(evidence.join(path).is_file(), "{path}");
    }
    assert_eq!(git(workspace, &["rev-parse", "HEAD"]), head);
    assert!(git(workspace, &["status", "--porcelain"]).is_empty());
    let validation_ms = start.elapsed().as_millis();
    let inspection = std::time::Instant::now();
    let historical = governed_ok(workspace, storage, &["qa", "verify", "commit", head]);
    assert_eq!(historical["status"], "passed");
    assert_eq!(historical["publication_ready"], false);
    let inspection_ms = inspection.elapsed().as_millis();
    let bytes = retained_bytes(&evidence);
    let repeat = std::time::Instant::now();
    let again = governed_command(workspace, storage, &["validate", "--format", "json"])
        .bounded_output(Duration::from_secs(300))
        .unwrap();
    let again_json: serde_json::Value = serde_json::from_slice(&again.stdout).unwrap();
    assert!(
        again.status.success(),
        "unchanged validation: {again_json} {}",
        String::from_utf8_lossy(&again.stderr)
    );
    assert_eq!(again_json["readiness"]["ready"], true);
    let _ = io::Write::write_all(&mut io::stderr(), format!(
        "canonical-local metrics: validation_ms={} inspection_ms={} unchanged_validation_ms={} retained_evidence_bytes={} canonical_store_growth_bytes={}\n",
        validation_ms, inspection_ms, repeat.elapsed().as_millis(), bytes, retained_bytes(&store_path).saturating_sub(store_before),
    ).as_bytes());
    // Material mutation cannot reuse the original successful observation.
    fs::write(workspace.join("payload.txt"), "changed after validation\n").unwrap();
    rejected_local_claim(
        workspace,
        storage,
        &[
            "todo",
            "done",
            "--id",
            task,
            "--validated",
            "--commit",
            head,
        ],
    );
    rejected_local_claim(workspace, storage, &["validate", "--format", "json"]);
    fs::write(workspace.join("payload.txt"), "ready\n").unwrap();
    // Local proof cannot be spent at the supported forge publication boundary,
    // even if an unrestricted shell could independently configure/push a remote.
    rejected_local_claim(workspace, storage, &["workspace", "publish"]);
    // A manually edited success artifact cannot establish proof for altered
    // source. Authentic canonical history remains a historical observation.
    fs::write(workspace.join("payload.txt"), "forged candidate input\n").unwrap();
    let observations_path = evidence.join("observations.json");
    fs::write(&observations_path, br#"{"passed":true,"ready":true}"#).unwrap();
    rejected_local_claim(
        workspace,
        storage,
        &[
            "todo",
            "done",
            "--id",
            task,
            "--validated",
            "--commit",
            head,
        ],
    );
    fs::write(workspace.join("payload.txt"), "ready\n").unwrap();
    // The accepted branch advances independently while the worker retains its
    // old origin/main. Current validation must observe the canonical branch.
    fs::write(root.join("second.txt"), "accepted target moved\n").unwrap();
    git(root, &["add", "second.txt"]);
    git(
        root,
        &["commit", "-m", "advance independent accepted local target"],
    );
    let stale = rejected_local_claim(workspace, storage, &["validate", "--format", "json"]);
    assert!(stale.contains("STALE_ACCEPTED_BASE"), "{stale}");
    // Revoking local authority on the canonical target cannot be repaired by a
    // candidate still declaring that authority or by a provider fallback.
    let config_path = root.join(CONFIG);
    let mut config: toml::Value =
        toml::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["proof"]["repository_contract"]["canonical_local_target"] = false.into();
    fs::write(config_path, toml::to_string_pretty(&config).unwrap()).unwrap();
    git(root, &["add", CONFIG]);
    git(
        root,
        &["commit", "-m", "revoke canonical local target authority"],
    );
    let revoked = rejected_local_claim(workspace, storage, &["validate", "--format", "json"]);
    assert!(
        revoked.contains("LOCAL_TARGET_POLICY_UNACCEPTED"),
        "{revoked}"
    );
    eprintln!(
        "Actual offline CLI local proof and stale/forged/revoked authority cases: {:?}",
        start.elapsed()
    );
}

#[cfg(unix)]
fn rejected_local_claim(workspace: &Path, storage: &Path, args: &[&str]) -> String {
    let output = governed_command(workspace, storage, args)
        .bounded_output(Duration::from_secs(300))
        .unwrap();
    let response = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .unwrap_or(serde_json::Value::Null);
    assert!(
        !output.status.success() || response["success"] == false || response["status"] == "fail",
        "unsupported claim succeeded: {args:?}: {response} {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_ne!(response["publication_ready"], true);
    assert_ne!(response["readiness"]["ready"], true);
    format!("{response} {}", String::from_utf8_lossy(&output.stderr))
}

#[cfg(unix)]
fn retained_bytes(path: &Path) -> u64 {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let metadata = fs::symlink_metadata(entry.path()).unwrap();
            if metadata.is_file() {
                metadata.len()
            } else if metadata.is_dir() {
                retained_bytes(&entry.path())
            } else {
                0
            }
        })
        .sum()
}
