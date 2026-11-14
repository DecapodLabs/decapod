//! Canonical archival and consumption of genuinely executed repository proof.
//! The existing Host reads a real bare Git accepted target; it executes both
//! required Python checks through the production sealed repository adapter.
//! CLI admission and independent remote authentication are separate assurances.
use super::*;
use crate::core::{context_capsule, trajectory, workunit};
use crate::plan_governance::{self, InitPlanInput, PlanPatch, PlanState, ScopeConstraints};
use std::fs;

#[allow(dead_code)]
#[path = "../../common/consumer_readiness_fixture.rs"]
mod consumer;

const TASK: &str = "code_observed_workunit";
const INTENT: &str = "intent:observed-workunit";

fn prepared() -> (consumer::Fixture, Store) {
    let mut fixture = consumer::Fixture::new();
    let root = &fixture.options.root;
    // Unlike the adapter-only fixture, canonical admission reads the complete
    // project schema and approval policy, not just its proof subsection.
    let config = crate::cli::DecapodProjectConfig {
        proof: fixture.proof.clone(),
        ..Default::default()
    };
    fs::write(
        root.join(consumer::CONFIG),
        toml::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
    fixture.options.git = crate::readiness_installed_tool(root, "git").unwrap();
    // Declare the narrow kernel-generated output before source capture. The
    // production verifier must still reject undeclared untracked inputs.
    let ignore = root.join(".gitignore");
    let mut ignored = fs::read_to_string(&ignore).unwrap();
    ignored.push_str(".decapod/managed/artifacts/provenance/completion_evidence/\n");
    fs::write(ignore, ignored).unwrap();

    // Preserve real local target transport while giving the repository resolver
    // its canonical forge identity. No network or external proof is simulated.
    consumer::git(
        &fixture.upstream,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/consumer/project.git",
        ],
    );
    let store = Store {
        kind: super::super::store::StoreKind::Repo,
        root: root.join(".decapod/data"),
    };
    todo::initialize_todo_db(&store.root).unwrap();
    DbBroker::new(&store.root).with_conn(&todo::todo_db_path(&store.root), "fixture", None, "test.seed", |conn| {
        conn.execute("INSERT INTO tasks(id,hash,title,status,created_at,updated_at,dir_path,scope,owner,assigned_to,assigned_at,lease_expires_at,lease_generation,lease_lifecycle,intent_anchor,one_shot,revision) VALUES(?1,'observed-fixture','Observe local proof','in_progress','1Z','1Z',?2,'repo',?3,?3,'1Z','9999999999Z',1,'claimed',?4,1,1)", db::params![TASK,root.to_string_lossy().as_ref(),crate::current_agent_id(),INTENT])?;
        Ok(())
    }).unwrap();
    todo::task_journal_checkpoint(&store.root, None).unwrap();
    plan_governance::init_plan(
        root,
        InitPlanInput {
            title: "Observe local proof".into(),
            intent: "Inspect both exact source inputs".into(),
            todo_ids: vec![TASK.into()],
            proof_hooks: vec!["validate_passes".into()],
            unknowns: vec![],
            human_questions: vec![],
            stop_conditions: vec![],
            unresolved_contradictions: vec![],
            deferred_questions: vec![],
            constraints: ScopeConstraints::default(),
            phases: vec![],
        },
    )
    .unwrap();
    plan_governance::patch_plan(
        root,
        PlanPatch {
            state: Some(PlanState::Approved),
            ..Default::default()
        },
    )
    .unwrap();
    trajectory::init_trajectory(
        root,
        trajectory::TrajectoryInit {
            run_id: "observed-workunit".into(),
            task_id: Some(TASK.into()),
            intent_id: Some(INTENT.into()),
            original_intent: "Inspect both exact source inputs".into(),
            derived_intent: "Inspect both exact source inputs".into(),
            active_boundaries: vec!["local proof only".into()],
            repo_scope: vec!["payload.txt".into(), "second.txt".into()],
            destination: None,
            current_phase: None,
            next_transitions: vec![],
            blockers: vec![],
        },
    )
    .unwrap();
    let capsule = context_capsule::DeterministicContextCapsule {
        schema_version: "1.1.0".into(),
        topic: "Observe local proof".into(),
        scope: "interfaces".into(),
        task_id: Some(TASK.into()),
        workunit_id: None,
        sources: vec![],
        snippets: vec![],
        resolved_authority: vec![],
        capabilities: vec![],
        policy: crate::core::capsule_policy::CapsulePolicyBinding {
            risk_tier: "low".into(),
            policy_hash: "sha256:fixture-policy".into(),
            policy_version: "1".into(),
            policy_path: "fixture-policy".into(),
            repo_revision: consumer::git(root, &["rev-parse", "HEAD"]),
        },
        capsule_hash: String::new(),
        repo_signal_fingerprint: "fixture".into(),
        config_input_hash: String::new(),
        spec_input_hash: String::new(),
    };
    context_capsule::write_context_capsule(root, &capsule).unwrap();
    workunit::init_workunit(root, TASK, INTENT).unwrap();
    workunit::add_state_ref(root, TASK, &format!(".decapod/managed/context/{TASK}.json")).unwrap();
    workunit::set_proof_plan(root, TASK, &["validate_passes".into()]).unwrap();
    workunit::transition_status(root, TASK, workunit::WorkUnitStatus::Executing).unwrap();
    workunit::transition_status(root, TASK, workunit::WorkUnitStatus::Claimed).unwrap();
    fixture.commit("bind accepted intent, scope and local workunit requirements");
    workunit::checkpoint_workunit(root, TASK, "before-verification", None).unwrap();
    (fixture, store)
}

fn observe(fixture: &consumer::Fixture, store: &Store) -> HistoricalValidationProof {
    let before = capture_task_binding(store, &fixture.options.root)
        .unwrap()
        .unwrap();
    let report = fixture.validate(&mut fixture.host());
    assert!(
        report.ready && report.proof_ready,
        "actual local suite: {report:?}"
    );
    finish_archive(store, &fixture.options.root, &before, &report).unwrap();
    lookup(&fixture.options.root, store, &before.candidate_sha).unwrap()
}

#[test]
fn actual_local_execution_establishes_current_workunit_proof_and_recovers() {
    let (fixture, store) = prepared();
    let root = &fixture.options.root;
    // Fabricated output bytes and a caller pass cannot establish the observation.
    fs::write(
        root.join(".decapod/data/fabricated.txt"),
        "all checks passed",
    )
    .unwrap();
    workunit::record_proof_result(
        root,
        TASK,
        "validate_passes",
        "pass",
        Some(".decapod/data/fabricated.txt".into()),
    )
    .unwrap();
    assert!(workunit::transition_status(root, TASK, workunit::WorkUnitStatus::Verified).is_err());
    assert_eq!(
        workunit::load_workunit(root, TASK).unwrap().status,
        workunit::WorkUnitStatus::Claimed
    );
    let started = std::time::Instant::now();
    let proof = observe(&fixture, &store);
    let execution_elapsed = started.elapsed();
    let mut without_witness = proof.record.clone();
    without_witness.proof.current_inputs = None;
    without_witness.workunit_subject = None;
    let record_bytes = serde_json::to_vec(&proof.record).unwrap().len();
    let witness_growth = record_bytes - serde_json::to_vec(&without_witness).unwrap().len();
    let verified =
        workunit::transition_status(root, TASK, workunit::WorkUnitStatus::Verified).unwrap();
    assert_eq!(
        verified.validation_record.as_ref().unwrap().record_id,
        proof.record_id()
    );
    assert_eq!(verified.proof_results.len(), 1);
    assert!(verified.proof_results[0].artifact_ref.is_none());
    let reconstruction = std::time::Instant::now();
    workunit::validate_verified_manifest(root, &verified).unwrap();
    let reconstruction_elapsed = reconstruction.elapsed();
    // No new canonical record or workunit event is created by read-only reuse.
    let before = workunit::workunit_journal_status(root, TASK).unwrap();
    workunit::validate_verified_manifest(root, &verified).unwrap();
    assert_eq!(
        before,
        workunit::workunit_journal_status(root, TASK).unwrap()
    );
    fs::remove_file(workunit::workunit_path(root, TASK).unwrap()).unwrap();
    let recovered = workunit::recover_workunit(root, TASK).unwrap();
    assert!(recovered.resumable);
    workunit::resume_workunit(root, TASK, "after-interruption", recovered.generation).unwrap();
    workunit::validate_verified_manifest(root, &workunit::load_workunit(root, TASK).unwrap())
        .unwrap();
    // Bypass successful libtest capture so hosted logs retain actual costs.
    use std::io::Write;
    let _ = std::io::stderr().write_all(format!("observed-workunit: record_bytes={record_bytes}, witness_growth_bytes={witness_growth}, execution={execution_elapsed:?}, reconstruction={reconstruction_elapsed:?}\n").as_bytes());
}

#[test]
fn observed_success_cannot_authorize_changed_source_target_policy_or_candidate() {
    let (fixture, store) = prepared();
    let root = &fixture.options.root;
    let proof = observe(&fixture, &store);
    let verified =
        workunit::transition_status(root, TASK, workunit::WorkUnitStatus::Verified).unwrap();
    let historical_digest = proof.record_digest().to_owned();
    for path in ["payload.txt", ".decapod/config.toml"] {
        let original = fs::read(root.join(path)).unwrap();
        fs::write(
            root.join(path),
            [original.as_slice(), b"\nchanged\n"].concat(),
        )
        .unwrap();
        assert!(
            workunit::validate_verified_manifest(root, &verified).is_err(),
            "accepted altered {path}"
        );
        fs::write(root.join(path), original).unwrap();
        workunit::validate_verified_manifest(root, &verified).unwrap();
    }
    // Relevant untracked input, source mode and branch identity are material.
    fs::write(root.join("new-source.txt"), "unverified source").unwrap();
    assert!(workunit::validate_verified_manifest(root, &verified).is_err());
    fs::remove_file(root.join("new-source.txt")).unwrap();
    consumer::git(root, &["checkout", "-b", "other-candidate"]);
    assert!(workunit::validate_verified_manifest(root, &verified).is_err());
    consumer::git(root, &["checkout", "consumer-work"]);
    consumer::git(
        root,
        &[
            "update-ref",
            "refs/remotes/origin/main",
            proof.candidate_sha(),
        ],
    );
    assert!(workunit::validate_verified_manifest(root, &verified).is_err());
    consumer::git(
        root,
        &[
            "update-ref",
            "refs/remotes/origin/main",
            &proof.record.proof.material.accepted_base_revision,
        ],
    );
    workunit::validate_verified_manifest(root, &verified).unwrap();
    assert_eq!(
        lookup(root, &store, proof.candidate_sha())
            .unwrap()
            .record_digest(),
        historical_digest
    );
    let mut substituted = verified.clone();
    substituted
        .validation_record
        .as_mut()
        .unwrap()
        .candidate_sha = "b".repeat(40);
    assert!(workunit::validate_verified_manifest(root, &substituted).is_err());
    assert!(
        workunit::write_workunit_checked(
            root,
            &substituted,
            "forge-reference",
            before_generation(root)
        )
        .is_err()
    );
}

fn before_generation(root: &Path) -> u64 {
    workunit::workunit_journal_status(root, TASK)
        .unwrap()
        .generation
        .unwrap()
}

#[test]
fn changed_workunit_during_actual_execution_cannot_publish_a_canonical_success() {
    let (fixture, store) = prepared();
    let root = &fixture.options.root;
    let before = capture_task_binding(&store, root).unwrap().unwrap();
    let report = fixture.validate(&mut fixture.host());
    assert!(report.ready, "{report:?}");
    workunit::add_state_ref(root, TASK, "state:new-obligation").unwrap();
    let error = finish_archive(&store, root, &before, &report).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("workunit changed during full validation"),
        "{error}"
    );
    assert!(lookup(root, &store, &before.candidate_sha).is_err());
    assert_eq!(
        workunit::load_workunit(root, TASK).unwrap().status,
        workunit::WorkUnitStatus::Claimed
    );
}

/// Reuse genuinely accepted proof for portable-evidence tests. The extra file
/// is an explicitly declared optional artifact; it cannot satisfy the required
/// full-validation gate and is independently hashed before export.
pub(crate) fn verified_fixture() -> (tempfile::TempDir, PathBuf, &'static str) {
    let (fixture, store) = prepared();
    let root = fixture.options.root.clone();
    fs::write(
        root.join(".decapod/data/portable-proof.txt"),
        "optional diagnostic artifact\n",
    )
    .unwrap();
    workunit::record_proof_result(
        &root,
        TASK,
        "diagnostic-artifact",
        "pass",
        Some(".decapod/data/portable-proof.txt".into()),
    )
    .unwrap();
    observe(&fixture, &store);
    workunit::transition_status(&root, TASK, workunit::WorkUnitStatus::Verified).unwrap();
    (fixture._storage, root, TASK)
}

#[test]
fn new_declaration_downgrades_actual_verified_proof_and_preserves_history() {
    let (_temp, root, task) = verified_fixture();
    let before = workunit::load_workunit(&root, task).unwrap();
    let reference = before.validation_record.clone().unwrap();
    let store = Store {
        kind: super::super::store::StoreKind::Repo,
        root: root.join(".decapod/data"),
    };
    for outcome in ["fail", "pass"] {
        let claimed = workunit::record_proof_result(
            &root,
            task,
            "validate_passes",
            outcome,
            Some(".decapod/data/portable-proof.txt".into()),
        )
        .unwrap();
        assert_eq!(claimed.status, workunit::WorkUnitStatus::Claimed);
        assert!(claimed.validation_record.is_none());
        assert!(claimed.validation_epoch.is_none());
        assert!(workunit::validate_verified_manifest(&root, &claimed).is_err());
    }
    assert_eq!(
        lookup_record(
            &root,
            &store,
            &reference.candidate_sha,
            &reference.record_id
        )
        .unwrap()
        .record_digest(),
        reference.record_digest
    );
}

#[test]
fn weaker_workunit_cannot_discharge_additional_canonical_task_obligations() {
    let (fixture, store) = prepared();
    let root = &fixture.options.root;
    todo::with_task_journal(&DbBroker::new(&store.root), &todo::todo_db_path(&store.root),
        &crate::current_agent_id(), Some(TASK), "test.additional_proof_obligation",
        todo::TaskJournalSelection::Ids(vec![TASK.into()]), |conn| {
            conn.execute("INSERT INTO task_verification(todo_id, proof_plan, verification_policy_days, updated_at) VALUES (?1,?2,0,'1Z')",
                db::params![TASK, r#"["validate_passes","independent_security_review"]"#])?;
            Ok(())
        }).unwrap();
    workunit::checkpoint_workunit(root, TASK, "additional-task-obligation", None).unwrap();
    let mut host = fixture.host();
    let report = fixture.validate(&mut host);
    assert_eq!(
        host.execution_attempts, 1,
        "real source checks must have executed"
    );
    assert!(
        !report.ready,
        "extra task obligations must block readiness: {report:?}"
    );
    assert!(
        report.proof_ready,
        "the executed source suite independently passed: {report:?}"
    );
    assert!(
        report
            .blockers
            .iter()
            .any(|b| b.detail.contains("TERMINAL_VALIDATION_RULES_FAILED")),
        "{report:?}"
    );
    let head = consumer::git(root, &["rev-parse", "HEAD"]);
    assert!(lookup(root, &store, &head).is_err());
    assert_eq!(
        workunit::load_workunit(root, TASK).unwrap().status,
        workunit::WorkUnitStatus::Claimed
    );
}
