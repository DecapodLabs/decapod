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
    config["proof"] = toml::Value::try_from(&fixture.proof).unwrap();
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
    git(
        &root,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/consumer/project.git",
        ],
    );
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
    for execution in contract.executions.values_mut() {
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
    let _ = io::Write::write_all(
        &mut io::stderr(),
        format!(
            "Fresh docs CLI admission and complete runtime: {:?}\n",
            started.elapsed()
        )
        .as_bytes(),
    );
}
