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
            None,
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
            observed_path: crate::core::trajectory::path::Judgment::unavailable(
                false,
                "fixture has no path witness",
            ),
            ready: true,
            proof_ready: true,
            laya_readiness: Some(completed.laya.clone()),
            scope: validation_runtime::READINESS_SCOPE,
            target: None,
            resolved_targets: vec![],
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

#[test]
fn capture_task_binding_with_budget_can_extend_a_lease_without_rotating_custody() {
    let _agent_guard = crate::core::todo::test_current_agent_id_guard("archive-boundary-agent");
    let (_temp, repo, store, _base) = fixture();
    let now_secs = crate::core::time::now_epoch_z()
        .trim_end_matches('Z')
        .parse::<u64>()
        .unwrap();
    let agent_id = crate::current_agent_id();
    let shortened = todo::renew_claim_lease(&store.root, TASK, &agent_id, Some(60)).unwrap();
    assert_eq!(shortened.get("status").and_then(|v| v.as_str()), Some("ok"));
    let shortened = todo::capture_validation_binding(&store.root, TASK).unwrap();
    let custody_before =
        todo::capture_execution_custody_binding(&store.root, TASK, &agent_id).unwrap();

    // Mode parsing: unknown/absent values never mutate custody.
    assert_eq!(
        todo::ValidationLeaseTtlMode::from_env_value(None),
        todo::ValidationLeaseTtlMode::Warn
    );
    assert_eq!(
        todo::ValidationLeaseTtlMode::from_env_value(Some("bogus")),
        todo::ValidationLeaseTtlMode::Warn
    );
    assert_eq!(
        todo::ValidationLeaseTtlMode::from_env_value(Some(" STRICT ")),
        todo::ValidationLeaseTtlMode::Strict
    );
    assert_eq!(
        todo::ValidationLeaseTtlMode::from_env_value(Some("renew")),
        todo::ValidationLeaseTtlMode::Renew
    );

    // Default (warn): a 60s lease against a 1800s budget yields a read-only
    // advisory and leaves revision and lease_generation untouched, so workunit
    // execution checkpoints stay valid (no EXECUTION_STATE_CHANGED).
    match todo::ensure_validation_lease_ttl(
        &store.root,
        TASK,
        1800,
        todo::ValidationLeaseTtlMode::Warn,
    )
    .unwrap()
    {
        todo::ValidationLeaseTtl::Low {
            remaining_secs,
            required_secs,
            advisory,
        } => {
            assert!(remaining_secs <= 60);
            assert_eq!(required_secs, 1800);
            assert!(advisory.contains("VALIDATION_LEASE_TTL_LOW"), "{advisory}");
            assert!(advisory.contains("decapod todo renew"), "{advisory}");
        }
        other => panic!("expected low-lease advisory, got {other:?}"),
    }
    assert_eq!(
        todo::capture_validation_binding(&store.root, TASK).unwrap(),
        shortened
    );
    let binding = capture_task_binding_with_budget(&store, &repo, Duration::from_secs(1800))
        .unwrap()
        .unwrap();
    assert_eq!(binding.task, shortened);

    // Strict: fail fast without mutating custody.
    let err = todo::ensure_validation_lease_ttl(
        &store.root,
        TASK,
        1800,
        todo::ValidationLeaseTtlMode::Strict,
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("VALIDATION_LEASE_TTL_INSUFFICIENT")
    );
    assert_eq!(
        todo::capture_validation_binding(&store.root, TASK).unwrap(),
        shortened
    );

    // Sufficient budget is a no-op in every mode.
    assert!(matches!(
        todo::ensure_validation_lease_ttl(
            &store.root,
            TASK,
            1,
            todo::ValidationLeaseTtlMode::Renew
        )
        .unwrap(),
        todo::ValidationLeaseTtl::Sufficient { .. }
    ));
    assert_eq!(
        todo::capture_validation_binding(&store.root, TASK).unwrap(),
        shortened
    );

    // Explicit renew extends the deadline of the current generation.
    assert!(matches!(
        todo::ensure_validation_lease_ttl(
            &store.root,
            TASK,
            1800,
            todo::ValidationLeaseTtlMode::Renew
        )
        .unwrap(),
        todo::ValidationLeaseTtl::Renewed { lease_secs: 1800 }
    ));
    let renewed = todo::capture_validation_binding(&store.root, TASK).unwrap();
    assert_eq!(renewed.lease_generation, shortened.lease_generation);
    assert_eq!(renewed.revision, shortened.revision);
    assert_ne!(renewed.checkpoint_digest, shortened.checkpoint_digest);
    assert_eq!(
        todo::capture_execution_custody_binding(&store.root, TASK, &agent_id).unwrap(),
        custody_before
    );
    let refreshed_exp_raw: Option<String> = DbBroker::new(&store.root)
        .with_conn(
            &todo::todo_db_path(&store.root),
            "test",
            None,
            "test.validation_history.read_lease",
            |conn| {
                todo::check_validation_binding_on_conn(conn, &shortened, false)?;
                conn.query_row(
                    "SELECT lease_expires_at FROM tasks WHERE id = ?1",
                    db::params![TASK],
                    |row| row.get(0),
                )
                .map_err(crate::core::error::DecapodError::from)
            },
        )
        .unwrap();
    let refreshed_exp = refreshed_exp_raw
        .as_deref()
        .unwrap()
        .trim_end_matches('Z')
        .parse::<u64>()
        .unwrap();
    assert!(refreshed_exp >= now_secs + 1800);
}

#[test]
fn deterministic_active_lease_renewal_preserves_evidence_while_expired_and_reclaimed_fail() {
    let _agent_guard = crate::core::todo::test_current_agent_id_guard("archive-boundary-agent");
    let (_temp, _repo, store, _accepted_base) = fixture();
    let agent_id = crate::current_agent_id();

    // Seed a deterministic clock/lease-generation/revision fixture:
    // t=1000Z claim, expires=2800Z, lease_generation=1, revision=1.
    DbBroker::new(&store.root)
        .with_conn(
            &todo::todo_db_path(&store.root),
            "test",
            None,
            "test.1892.seed",
            |conn| {
                conn.execute(
                    "UPDATE tasks
                     SET lease_expires_at = '2800Z',
                         lease_generation = 1,
                         lease_lifecycle = 'claimed',
                         revision = 1,
                         updated_at = '1000Z',
                         assigned_at = '1000Z'
                     WHERE id = ?1",
                    db::params![TASK],
                )?;
                conn.execute("DELETE FROM events WHERE stream = 'todo'", [])?;
                Ok(())
            },
        )
        .unwrap();
    todo::task_journal_checkpoint(&store.root, None).unwrap();

    let binding_at_1000 = todo::capture_validation_binding_at(&store.root, TASK, "1000Z").unwrap();
    assert_eq!(binding_at_1000.lease_generation, 1);
    assert_eq!(binding_at_1000.revision, 1);

    // Positive fresh renewal at t=2500Z (before 2800Z expiry) on the same generation/evidence.
    let heartbeat =
        todo::heartbeat_claim_lease_at(&store.root, TASK, &agent_id, Some(1800), Some(1), "2500Z")
            .unwrap();
    assert_eq!(heartbeat["status"], "ok");
    assert_eq!(heartbeat["result"]["extended"], true);
    assert_eq!(heartbeat["result"]["lease_expires_at"], "4300Z");
    assert_eq!(heartbeat["result"]["lease_generation"], 1);

    // Independently read persisted lease and journal after renewal.
    DbBroker::new(&store.root)
        .with_conn(
            &todo::todo_db_path(&store.root),
            "test",
            None,
            "test.1892.verify_persisted",
            |conn| {
                let (exp, lease_gen, rev, assignee): (String, i64, i64, String) = conn.query_row(
                    "SELECT lease_expires_at, lease_generation, revision, assigned_to FROM tasks WHERE id = ?1",
                    db::params![TASK],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )?;
                assert_eq!(exp, "4300Z");
                assert_eq!(lease_gen, 1);
                assert_eq!(rev, 1);
                assert_eq!(assignee, agent_id);
                // Positive control: binding captured before heartbeat remains valid at t=2500Z.
                todo::check_validation_binding_at_on_conn(conn, &binding_at_1000, false, "2500Z")?;
                // Negative control 1: once clock reaches t=4300Z (expired), binding MUST fail.
                let expired_err = todo::check_validation_binding_at_on_conn(
                    conn,
                    &binding_at_1000,
                    false,
                    "4300Z",
                )
                .unwrap_err()
                .to_string();
                assert!(
                    expired_err.contains("VALIDATION_HISTORY_TASK_STALE")
                        && expired_err.contains("TASK_LEASE_EXPIRED_OR_RELEASED"),
                    "expected expired custody failure, got: {expired_err}"
                );
                Ok(())
            },
        )
        .unwrap();

    // Negative control 2: heartbeat at t=4300Z (expired) must refuse.
    let expired_hb_err =
        todo::heartbeat_claim_lease_at(&store.root, TASK, &agent_id, Some(1800), Some(1), "4300Z")
            .unwrap_err()
            .to_string();
    assert!(
        expired_hb_err.contains("LEASE_HEARTBEAT_STALE"),
        "expected LEASE_HEARTBEAT_STALE on expired lease, got: {expired_hb_err}"
    );

    // Negative control 3: reclaimed lease (generation bumped to 2, new owner) invalidates gen-1 binding.
    todo::with_task_journal(
        &DbBroker::new(&store.root),
        &todo::todo_db_path(&store.root),
        "competing-agent",
        None,
        "todo.claim",
        todo::TaskJournalSelection::Ids(vec![TASK.into()]),
        |conn| {
            conn.execute(
                "UPDATE tasks SET assigned_to='competing-agent', lease_expires_at='9999999999Z',
                 lease_generation=2, lease_lifecycle='reclaimed', revision=revision+1 WHERE id=?1",
                db::params![TASK],
            )?;
            Ok(())
        },
    )
    .unwrap();
    DbBroker::new(&store.root)
        .with_conn(
            &todo::todo_db_path(&store.root),
            "test",
            None,
            "test.1892.reclaimed_check",
            |conn| {
                let err = todo::check_validation_binding_at_on_conn(
                    conn,
                    &binding_at_1000,
                    false,
                    "2600Z",
                )
                .unwrap_err()
                .to_string();
                assert!(
                    err.contains("VALIDATION_HISTORY_TASK_STALE"),
                    "reclaimed lease must invalidate gen-1 binding: {err}"
                );
                Ok(())
            },
        )
        .unwrap();
}

#[test]
fn publication_readiness_survives_active_heartbeat_renewal_and_rejects_custody_rotation() {
    let _agent_guard = crate::core::todo::test_current_agent_id_guard("archive-boundary-agent");
    let (_temp, repo, store, accepted_base) = fixture();
    let agent_id = crate::current_agent_id();
    let head = git(&repo, &["rev-parse", "HEAD"]);
    let now_secs = crate::core::time::now_epoch_z()
        .trim_end_matches('Z')
        .parse::<u64>()
        .unwrap();

    // Start with a short active lease (120s), capture pre-validation binding,
    // then renew via heartbeat (as DECAPOD_VALIDATE_LEASE_TTL=renew does).
    let shortened = todo::renew_claim_lease(&store.root, TASK, &agent_id, Some(120)).unwrap();
    assert_eq!(shortened["status"], "ok");
    let binding_before = capture_task_binding(&store, &repo).unwrap().unwrap();

    let git_program = crate::readiness_installed_tool(&repo, "git").unwrap();
    let input_identity = crate::core::native_proof::IdentityInputs {
        root: repo.canonicalize().unwrap(),
        git_program,
        repository: "example/archive".into(),
        base_branch: "master".into(),
        accepted_base_ref: "refs/heads/master".into(),
        declared_untracked: std::iter::once(
            ".decapod/managed/governance-document.lock".to_string(),
        )
        .collect(),
        configuration: [".decapod/config.toml", ".gitignore"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        workflows: [".gitignore"].into_iter().map(str::to_owned).collect(),
        candidate_inputs: ["tracked.txt"].into_iter().map(str::to_owned).collect(),
    };
    let captured_material = crate::core::native_proof::capture_readonly_identity(
        &input_identity,
        &[],
        crate::core::native_proof::SuiteLimits {
            timeout: std::time::Duration::from_secs(30),
            max_checks: 1024,
            max_hashed_bytes: 2 * 1024 * 1024 * 1024,
            max_output_bytes: 1024 * 1024,
            git_output_limit: 1024 * 1024,
        },
        &crate::core::bounded_process::Cancellation::default(),
    )
    .unwrap();
    assert_eq!(captured_material.head_revision, head);
    assert_eq!(captured_material.accepted_base_revision, accepted_base);
    let current_inputs = CurrentEvidenceInputs::capture(
        &input_identity,
        &validation_runtime::ExecutionSelection {
            policy: crate::core::readiness_profile::ExecutionPolicy {
                buildkite_preflights: false,
                github_actions_preflights: false,
                executors: Default::default(),
            },
            bindings: Default::default(),
            native_invocations: Default::default(),
        },
    )
    .unwrap();
    let laya_cfg = crate::core::laya_readiness::configuration(&repo).unwrap();

    let governance_bytes = fs::read(repo.join(GOVERNANCE)).unwrap();
    let raw: Value = serde_json::from_slice(&governance_bytes).unwrap();
    let raw_trajectory_digest = evidence::digest(&raw["sections"]["trajectory"]).unwrap();
    let mut completed = test_fixture::record().proof;
    completed.invocation_id = crate::core::ulid::new_ulid();
    completed.material = captured_material;
    completed.governance_digest = format!("sha256:{:x}", Sha256::digest(&governance_bytes));
    completed.trajectory_digest = raw_trajectory_digest;
    completed.suite.before = completed.material.clone();
    completed.suite.after = completed.material.clone();
    completed.suite.facts.target_branch = completed.material.base_branch.clone();
    for result in &mut completed.suite.results {
        result.identity = completed.material.clone();
    }
    completed.suite.expected_plan_digest = completed.expected_plan.digest().unwrap();
    completed.laya.mode = laya_cfg.mode;
    completed.laya.binding.configuration_digest = evidence::digest(&laya_cfg).unwrap();
    completed.laya.binding.material = completed.material.clone();
    completed.laya.binding.governance_digest = completed.governance_digest.clone();
    completed.laya.binding.proof_plan_digest = completed.expected_plan.digest().unwrap();
    completed.laya.binding.evidence_digest = evidence::digest(&completed.suite).unwrap();
    completed.laya.binding.trajectory_run_id.clear();
    completed.laya.request_id = evidence::digest(&completed.laya.binding).unwrap();
    let completed = CompletedReadinessEvidence::from_runtime(
        &completed.invocation_id,
        &completed.scope,
        None,
        &completed.material,
        &completed.governance_digest,
        &completed.trajectory_digest,
        &completed.expected_plan,
        &completed.suite,
        &completed.laya,
        Some(current_inputs),
    )
    .unwrap();
    let temp_ev = tempfile::tempdir().unwrap();
    fs::write(
        temp_ev.path().join(PROOF_FILE),
        serde_json::to_vec_pretty(&completed).unwrap(),
    )
    .unwrap();
    let report = ValidationReport {
        observed_path: crate::core::trajectory::path::Judgment::unavailable(
            false,
            "fixture has no path witness",
        ),
        ready: true,
        proof_ready: true,
        laya_readiness: Some(completed.laya.clone()),
        scope: validation_runtime::READINESS_SCOPE,
        target: None,
        resolved_targets: vec![],
        agent_guidance: validation_runtime::ValidationGuidance::for_outcome(true),
        stage_dispositions: vec![],
        blockers: vec![],
        evidence_path: Some(temp_ev.path().to_path_buf()),
        completed_evidence: Some(completed),
    };

    // Active heartbeat renewal changes checkpoint_digest without rotating generation.
    assert!(matches!(
        todo::ensure_validation_lease_ttl(
            &store.root,
            TASK,
            1800,
            todo::ValidationLeaseTtlMode::Renew,
        )
        .unwrap(),
        todo::ValidationLeaseTtl::Renewed { lease_secs: 1800 }
    ));
    let binding_after_hb = capture_task_binding(&store, &repo).unwrap().unwrap();
    assert_ne!(
        binding_after_hb.task.checkpoint_digest,
        binding_before.task.checkpoint_digest
    );
    assert_eq!(
        binding_after_hb.task.lease_generation,
        binding_before.task.lease_generation
    );
    assert_eq!(binding_after_hb.task.revision, binding_before.task.revision);

    // Archive succeeds with pre-renewal binding (heartbeat during validation is valid),
    // and PublicationReadiness::verify_current ALSO succeeds!
    finish_archive(&store, &repo, &binding_before, &report).unwrap();
    let persisted = lookup(&repo, &store, &head).unwrap();
    assert_eq!(persisted.task_binding(), &binding_before.task);
    let readiness = PublicationReadiness::from_report(report, Some(binding_before)).unwrap();
    readiness.verify_current(&repo).unwrap();

    // Independently check persisted expiry >= now_secs + 1800.
    let exp_raw: String = DbBroker::new(&store.root)
        .with_conn(
            &todo::todo_db_path(&store.root),
            "test",
            None,
            "test.read_exp",
            |conn| {
                conn.query_row(
                    "SELECT lease_expires_at FROM tasks WHERE id = ?1",
                    db::params![TASK],
                    |r| r.get(0),
                )
                .map_err(crate::core::error::DecapodError::from)
            },
        )
        .unwrap();
    let exp_secs: u64 = exp_raw.trim_end_matches('Z').parse().unwrap();
    assert!(exp_secs >= now_secs + 1800);

    // Negative control: non-heartbeat todo renew rotates lease_generation and MUST fail verify_current.
    let rotated = todo::renew_claim_lease(&store.root, TASK, &agent_id, Some(3600)).unwrap();
    assert_eq!(rotated["status"], "ok");
    let err = readiness.verify_current(&repo).unwrap_err().to_string();
    assert!(
        err.contains("PUBLICATION_CUSTODY_CHANGED"),
        "generation rotation must fail PublicationReadiness::verify_current: {err}"
    );
}
