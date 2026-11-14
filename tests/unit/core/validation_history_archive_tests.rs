//! Exercise the real capture/append boundary with native and provider suites.
//! The successful observations are fixtures, not claims of external execution;
//! their complete-suite shape passes the production verifier before archival.
use super::*;
use crate::core::{governance_document, store::StoreKind, trajectory};
use std::fs;

const TASK: &str = "code_validation_archive_boundary";

fn git(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new(crate::readiness_installed_tool(root, "git").unwrap())
        .current_dir(root)
        .env_clear()
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Validation archive test")
        .env("GIT_AUTHOR_EMAIL", "archive-test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Validation archive test")
        .env("GIT_COMMITTER_EMAIL", "archive-test@example.invalid")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn fixture() -> (tempfile::TempDir, PathBuf, Store, String) {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir_all(repo.join(".decapod")).unwrap();
    fs::write(
        repo.join(".decapod/config.toml"),
        toml::to_string(&crate::cli::DecapodProjectConfig::default()).unwrap(),
    )
    .unwrap();
    fs::write(repo.join(".gitignore"), "/.decapod/data/\n").unwrap();
    fs::write(repo.join("tracked.txt"), "accepted baseline\n").unwrap();
    git(&repo, &["init", "--quiet", "--initial-branch=master"]);
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/example/archive.git",
        ],
    );
    git(
        &repo,
        &["add", ".gitignore", "tracked.txt", ".decapod/config.toml"],
    );
    git(&repo, &["commit", "--quiet", "-m", "accepted baseline"]);
    let accepted_base = git(&repo, &["rev-parse", "HEAD"]);
    git(
        &repo,
        &["switch", "--quiet", "-c", "agent/archive-boundary"],
    );

    let store = Store {
        kind: StoreKind::Repo,
        root: repo.join(".decapod/data"),
    };
    todo::initialize_todo_db(&store.root).unwrap();
    DbBroker::new(&store.root)
        .with_conn(
            &todo::todo_db_path(&store.root),
            "test",
            None,
            "test.validation_history.seed",
            |conn| {
                conn.execute(
                    "INSERT INTO tasks(id,hash,title,status,created_at,updated_at,dir_path,scope,
                        owner,assigned_to,assigned_at,lease_expires_at,lease_generation,
                        lease_lifecycle,intent_anchor,one_shot,revision)
                     VALUES(?1,'archive-fixture','Original archive task','in_progress','1Z','1Z',
                        '.','',?2,?2,'1Z','9999999999Z',2,'claimed','intent:archive-boundary',1,7)",
                    db::params![TASK, crate::current_agent_id()],
                )?;
                Ok(())
            },
        )
        .unwrap();
    todo::task_journal_checkpoint(&store.root, None).unwrap();
    let trajectory = trajectory::fresh_trajectory(
        &repo,
        trajectory::TrajectoryInit {
            run_id: "archive-boundary".into(),
            task_id: Some(TASK.into()),
            intent_id: Some("intent:archive-boundary".into()),
            original_intent: "Retain complete validation for the original task".into(),
            derived_intent: "Retain complete validation for the original task".into(),
            active_boundaries: vec!["tracked.txt".into()],
            repo_scope: vec!["tracked.txt".into()],
            destination: None,
            current_phase: None,
            next_transitions: vec![],
            blockers: vec![],
        },
    )
    .unwrap();
    // Use the real compact governance writer. Runtime binds the stored section,
    // not its expanded projection returned by read_section.
    governance_document::write_section(
        &repo,
        "trajectory",
        &serde_json::to_value(&trajectory).unwrap(),
    )
    .unwrap();
    fs::write(repo.join("tracked.txt"), "candidate with validated work\n").unwrap();
    git(&repo, &["add", "tracked.txt", ".decapod/governance.json"]);
    git(&repo, &["commit", "--quiet", "-m", "candidate checkpoint"]);
    (temp, repo, store, accepted_base)
}

fn provider_results(proof: &mut CompletedReadinessEvidence, source_tree: &str) {
    use evidence::*;
    // Same reviewed provider-attestation shape exercised by the core evidence
    // tests: exact source/transport/base, substantive steps and actual artifact
    // measurements are retained instead of invented process exit statuses.
    proof.expected_plan.buildkite_enabled = true;
    for (index, (check, result)) in proof
        .expected_plan
        .checks
        .iter_mut()
        .zip(&mut proof.suite.results)
        .enumerate()
    {
        check.executor = Provider::GithubActions;
        let contract = GithubExecutionContract {
            schema_version: GITHUB_EXECUTION_SCHEMA_VERSION.into(),
            repository: proof.material.repository.clone(),
            workflow: check.key.workflow.clone(),
            job: check.key.job.clone(),
            provider_job_name: format!("{} (reviewed executor)", check.key.job),
            matrix: check.key.matrix.clone(),
            profile: check.key.profile.clone(),
            source_definition_digest: check.definition_digest.clone(),
            source_head: proof.material.head_revision.clone(),
            accepted_base_revision: proof.material.accepted_base_revision.clone(),
            transport_revision: "c".repeat(40),
            source_tree: source_tree.into(),
            required_steps: vec![GithubRequiredStep {
                name: "Run complete substantive check".into(),
                definition_digest: evidence::digest(&"reviewed source command").unwrap(),
                failure_mode: GithubFailureMode::RequiredTerminalSuccess,
            }],
        };
        check.invocation_digest = contract.digest().unwrap();
        result.executor = Provider::GithubActions;
        result.invocation_digest = check.invocation_digest.clone();
        result.observation = Observation::GithubActions {
            attestation: Box::new(GithubExecutionAttestation {
                observed_run_head: contract.transport_revision.clone(),
                observed_job_name: contract.provider_job_name.clone(),
                steps: contract
                    .required_steps
                    .iter()
                    .map(|step| GithubStepObservation {
                        name: step.name.clone(),
                        conclusion: Conclusion::Success,
                    })
                    .collect(),
                contract,
                run_id: 42,
                run_attempt: 2,
                job_id: 100 + index as u64,
                run_conclusion: Conclusion::Success,
                job_conclusion: Conclusion::Success,
                response_digest: evidence::digest(&format!("authenticated response {index}"))
                    .unwrap(),
                log_digest: evidence::digest(&format!("exact job log {index}")).unwrap(),
                artifact_digest: (check.kind == CheckKind::CandidateBuild)
                    .then(|| evidence::digest(&"actual candidate artifact bytes").unwrap()),
            }),
        };
    }
}

#[test]
fn native_and_provider_capture_archive_and_lookup_survive_evidence_directory_deletion() {
    for executor in [
        evidence::Provider::Native,
        evidence::Provider::GithubActions,
    ] {
        let (_temp, repo, store, accepted_base) = fixture();
        let head = git(&repo, &["rev-parse", "HEAD"]);
        let source_tree = git(&repo, &["rev-parse", "HEAD^{tree}"]);
        let source_status = git(&repo, &["status", "--porcelain=v1"]);
        let governance_bytes = fs::read(repo.join(GOVERNANCE)).unwrap();
        let raw: Value = serde_json::from_slice(&governance_bytes).unwrap();
        let raw_trajectory_digest = evidence::digest(&raw["sections"]["trajectory"]).unwrap();
        let expanded = governance_document::read_section(&repo, "trajectory")
            .unwrap()
            .unwrap();
        assert_ne!(
            raw_trajectory_digest,
            evidence::digest(&expanded).unwrap(),
            "fixture must exercise compact governance versus expanded trajectory"
        );
        let binding = capture_task_binding(&store, &repo).unwrap().unwrap();
        assert_eq!(binding.task.task_id, TASK);
        assert_eq!(binding.candidate_sha, head);
        assert_eq!(
            binding.governance_digest,
            format!("sha256:{:x}", Sha256::digest(&governance_bytes))
        );
        assert_eq!(binding.trajectory_digest, raw_trajectory_digest);

        let mut completed = test_fixture::record().proof;
        completed.invocation_id = crate::core::ulid::new_ulid();
        completed.material.repository = "example/archive".into();
        completed.material.head_revision = head.clone();
        completed.material.accepted_base_revision = accepted_base;
        completed.material.work_branch = git(&repo, &["branch", "--show-current"]);
        completed.governance_digest = format!("sha256:{:x}", Sha256::digest(&governance_bytes));
        completed.trajectory_digest = raw_trajectory_digest;
        completed.suite.before = completed.material.clone();
        completed.suite.after = completed.material.clone();
        for result in &mut completed.suite.results {
            result.identity = completed.material.clone();
        }
        if executor == evidence::Provider::GithubActions {
            provider_results(&mut completed, &source_tree);
        }
        completed.suite.expected_plan_digest = completed.expected_plan.digest().unwrap();
        completed.laya.binding.material = completed.material.clone();
        completed.laya.binding.governance_digest = completed.governance_digest.clone();
        completed.laya.binding.proof_plan_digest = completed.expected_plan.digest().unwrap();
        completed.laya.binding.evidence_digest = evidence::digest(&completed.suite).unwrap();
        // The live disabled-Laya path intentionally leaves this empty, while
        // governance independently binds the original nonempty trajectory ID.
        completed.laya.binding.trajectory_run_id.clear();
        completed.laya.request_id = evidence::digest(&completed.laya.binding).unwrap();
        let completed = CompletedReadinessEvidence::from_runtime(
            &completed.invocation_id,
            &completed.scope,
            &completed.material,
            &completed.governance_digest,
            &completed.trajectory_digest,
            &completed.expected_plan,
            &completed.suite,
            &completed.laya,
            None,
        )
        .unwrap();
        let temporary_evidence = tempfile::tempdir().unwrap();
        let removed_path = temporary_evidence.path().to_path_buf();
        fs::write(
            removed_path.join(PROOF_FILE),
            serde_json::to_vec_pretty(&completed).unwrap(),
        )
        .unwrap();
        let report = ValidationReport {
            ready: true,
            proof_ready: true,
            laya_readiness: Some(completed.laya.clone()),
            scope: validation_runtime::READINESS_SCOPE,
            agent_guidance: validation_runtime::ValidationGuidance::for_outcome(true),
            stage_dispositions: vec![],
            blockers: vec![],
            evidence_path: Some(removed_path.clone()),
            completed_evidence: Some(completed.clone()),
        };

        // The production append uses the canonical broker transaction and the
        // exact pre-validation task binding. No fixture event writer is used.
        finish_archive(&store, &repo, &binding, &report).unwrap();
        temporary_evidence.close().unwrap();
        assert!(!removed_path.exists());
        let durable = lookup(&repo, &store, &head).unwrap();
        assert_eq!(durable.record.result, "success");
        assert_eq!(durable.task_binding(), &binding.task);
        assert_eq!(durable.validation_epoch(), &binding.epoch);
        assert_eq!(durable.record.proof.expected_plan, completed.expected_plan);
        assert_eq!(durable.record.proof.suite, completed.suite);
        assert_eq!(durable.record.proof.material, completed.material);
        assert_eq!(durable.record_id(), completed.invocation_id);
        let conn =
            db::db_connect_for_validate(&events::canonical_db_path(&store.root).to_string_lossy())
                .unwrap();
        durable.recheck_on_conn(&conn).unwrap();
        assert_eq!(
            events::query_subject_on_conn(
                &conn,
                events::VERIFICATION,
                "commit",
                &head,
                MAX_RECORDS
            )
            .unwrap()
            .len(),
            1
        );
        assert_eq!(git(&repo, &["rev-parse", "HEAD"]), head);
        assert_eq!(git(&repo, &["status", "--porcelain=v1"]), source_status);
        assert_eq!(fs::read(repo.join(GOVERNANCE)).unwrap(), governance_bytes);
        assert_eq!(
            todo::capture_validation_binding(&store.root, TASK).unwrap(),
            binding.task
        );
    }
}
