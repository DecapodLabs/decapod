use super::*;
use tempfile::TempDir;

fn git(repo: &Path, args: &[&str]) -> String {
    text(run(repo, "git", args, "test git").unwrap())
        .unwrap()
        .trim()
        .to_string()
}
fn write(repo: &Path, path: &str, body: &str) {
    let path = repo.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}
fn repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "master"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    git(
        dir.path(),
        &["config", "user.email", "test@example.invalid"],
    );
    write(dir.path(), "README.md", "base\n");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    git(dir.path(), &["checkout", "-qb", "feature"]);
    dir
}
fn bundle(repo: &Path) {
    for path in REQUIRED_PR_GOVERNANCE_ARTIFACTS {
        write(repo, path, "{\"test\":true}\n");
    }
    write(
        repo,
        ".decapod/managed/specs/INTERFACES.md",
        "# Current interfaces\nPublication reads remote proof.\n",
    );
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "bundle"]);
}
fn proof() -> RemoteProof {
    RemoteProof {
        head: "a".repeat(40),
        base: "b".repeat(40),
        paths: REQUIRED_PR_GOVERNANCE_ARTIFACTS
            .iter()
            .map(|p| p.to_string())
            .chain([".decapod/managed/specs/INTERFACES.md".to_string()])
            .collect(),
    }
}
fn pr(proof: &RemoteProof) -> serde_json::Value {
    serde_json::json!({"number":7,"html_url":"https://github.com/owner/repo/pull/7","state":"open",
        "head":{"ref":"feature","sha":proof.head,"repo":{"full_name":"owner/repo"}},
        "base":{"ref":"master","sha":proof.base,"repo":{"full_name":"owner/repo"}},
        "changed_files":proof.paths.len()})
}
fn bytes(value: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&value).unwrap()
}
fn files(proof: &RemoteProof) -> serde_json::Value {
    serde_json::json!([proof
        .paths
        .iter()
        .map(|p| serde_json::json!({"filename":p,"status":"modified"}))
        .collect::<Vec<_>>()])
}

#[test]
fn artifact_inventory_missing_and_out_of_diff_matrix() {
    for missing in REQUIRED_PR_GOVERNANCE_ARTIFACTS {
        let mut p = proof();
        p.paths.remove(*missing);
        let error = require_bundle_paths(&p.paths).unwrap_err().to_string();
        assert!(error.contains(missing));
        assert!(error.contains("REMOTE_GOVERNANCE_DIFF_MISSING"));
    }
}

#[test]
fn tracked_but_unstaged_artifact_bytes_do_not_pass() {
    let dir = repo();
    bundle(dir.path());
    ensure_validation_artifacts_staged(dir.path()).unwrap();
    for path in REQUIRED_PR_GOVERNANCE_ARTIFACTS {
        let old = std::fs::read(dir.path().join(path)).unwrap();
        write(dir.path(), path, "changed");
        assert!(
            ensure_validation_artifacts_staged(dir.path())
                .unwrap_err()
                .to_string()
                .contains("UNSTAGED_GOVERNANCE_ARTIFACT")
        );
        std::fs::write(dir.path().join(path), old).unwrap();
    }
}

#[test]
fn committed_bundle_requires_exact_bytes_and_clean_index() {
    let dir = repo();
    bundle(dir.path());
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    verify_committed_bundle(dir.path(), &head).unwrap();
    write(
        dir.path(),
        REQUIRED_PR_GOVERNANCE_ARTIFACTS[0],
        "different bytes",
    );
    git(dir.path(), &["add", "."]);
    assert!(
        verify_committed_bundle(dir.path(), &head)
            .unwrap_err()
            .to_string()
            .contains("UNCOMMITTED_GOVERNANCE_ARTIFACT")
    );
}

#[test]
fn actual_bare_remote_proves_artifact_bytes_and_detects_divergence() {
    let dir = repo();
    bundle(dir.path());
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "--bare", "-q"]);
    let target = remote.path().to_str().unwrap();
    git(dir.path(), &["push", target, "master"]);
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    git(
        dir.path(),
        &[
            "push",
            "-u",
            "--",
            target,
            &format!("{head}:refs/heads/feature"),
        ],
    );
    // A fetch origin unrelated to the selected push endpoint is not evidence.
    git(
        dir.path(),
        &["remote", "add", "origin", "/nonexistent/wrong-fetch-target"],
    );
    let evidence = verify_remote(dir.path(), target, "feature", "master", &head).unwrap();
    assert_eq!(evidence.head, head);
    for path in REQUIRED_PR_GOVERNANCE_ARTIFACTS {
        assert_eq!(
            run(
                remote.path(),
                "git",
                &["show", &format!("{head}:{path}")],
                "remote bytes"
            )
            .unwrap(),
            std::fs::read(dir.path().join(path)).unwrap()
        );
    }
    assert!(
        verify_remote(dir.path(), target, "feature", "master", &"c".repeat(40))
            .unwrap_err()
            .to_string()
            .contains("REMOTE_HEAD_MISMATCH")
    );
    write(dir.path(), "other.txt", "remote moves");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "move"]);
    git(dir.path(), &["push", target, "feature"]);
    assert!(
        verify_remote_unchanged(dir.path(), target, "feature", "master", &evidence)
            .unwrap_err()
            .to_string()
            .contains("REMOTE_PUBLICATION_RACE")
    );
}

#[test]
fn existing_pr_is_reused_and_full_remote_diff_checked() {
    let p = proof();
    let mut calls = Vec::new();
    let mut replies = vec![
        bytes(serde_json::json!([[{"number":7}]])),
        bytes(pr(&p)),
        bytes(files(&p)),
        bytes(pr(&p)),
    ]
    .into_iter();
    let result = ensure_and_verify_pr_with(
        &mut |args| {
            calls.push(args.join(" "));
            Ok(replies.next().unwrap())
        },
        "owner/repo",
        "feature",
        "master",
        &p,
        "title",
        "body",
    )
    .unwrap();
    assert_eq!(result, "https://github.com/owner/repo/pull/7");
    assert!(!calls.iter().any(|call| call.contains("pr create")));
    assert!(calls.iter().any(|call| call.contains("--paginate --slurp")));
    assert!(!calls.iter().any(|call| call.contains("-C ")));
}

#[test]
fn new_pr_created_as_draft_and_read_back_after_create() {
    let p = proof();
    let mut calls = Vec::new();
    let mut replies = vec![
        bytes(serde_json::json!([[]])),
        b"https://github.com/owner/repo/pull/7".to_vec(),
        bytes(serde_json::json!([[{"number":7}]])),
        bytes(pr(&p)),
        bytes(files(&p)),
        bytes(pr(&p)),
    ]
    .into_iter();
    ensure_and_verify_pr_with(
        &mut |args| {
            calls.push(args.join(" "));
            Ok(replies.next().unwrap())
        },
        "owner/repo",
        "feature",
        "master",
        &p,
        "title",
        "body",
    )
    .unwrap();
    assert!(
        calls
            .iter()
            .any(|call| call.starts_with("pr create --draft --repo owner/repo"))
    );
}

#[test]
fn pr_create_failure_is_actionable_partial_publication() {
    let p = proof();
    let mut index = 0;
    let error = ensure_and_verify_pr_with(
        &mut |_| {
            index += 1;
            if index == 2 {
                Err(failure("GitHub PR creation failed"))
            } else {
                Ok(bytes(serde_json::json!([[]])))
            }
        },
        "owner/repo",
        "feature",
        "master",
        &p,
        "title",
        "body",
    )
    .unwrap_err();
    let error = after_push(error).to_string();
    assert!(error.contains("branch was pushed"));
    assert!(error.contains("not verified"));
    assert!(error.contains("retry `decapod workspace publish`"));
}

#[test]
fn lost_create_response_is_recovered_by_exact_pr_readback() {
    let p = proof();
    let mut replies = vec![
        Ok(bytes(serde_json::json!([[]]))),
        Err(failure("connection lost")),
        Ok(bytes(serde_json::json!([[{"number":7}]]))),
        Ok(bytes(pr(&p))),
        Ok(bytes(files(&p))),
        Ok(bytes(pr(&p))),
    ]
    .into_iter();
    ensure_and_verify_pr_with(
        &mut |_| replies.next().unwrap(),
        "owner/repo",
        "feature",
        "master",
        &p,
        "title",
        "body",
    )
    .unwrap();
}

#[test]
fn wrong_target_head_base_state_or_repository_never_passes() {
    let p = proof();
    for (pointer, value) in [
        ("/head/sha", "c".repeat(40)),
        ("/base/sha", "c".repeat(40)),
        ("/head/ref", "wrong".into()),
        ("/base/ref", "wrong".into()),
        ("/head/repo/full_name", "attacker/repo".into()),
        ("/base/repo/full_name", "wrong/repo".into()),
        ("/state", "closed".into()),
    ] {
        let mut value_pr = pr(&p);
        *value_pr.pointer_mut(pointer).unwrap() = value.into();
        let parsed: PullRequest = serde_json::from_value(value_pr).unwrap();
        assert!(
            check_pr(&parsed, "owner/repo", "feature", "master", &p)
                .unwrap_err()
                .to_string()
                .contains("PR_TARGET_MISMATCH")
        );
    }
}

#[test]
fn missing_remote_artifact_and_truncated_pr_diff_fail() {
    let p = proof();
    let mut f = files(&p);
    f[0].as_array_mut().unwrap().pop();
    let mut replies = vec![
        bytes(serde_json::json!([[{"number":7}]])),
        bytes(pr(&p)),
        bytes(f),
    ]
    .into_iter();
    let error = ensure_and_verify_pr_with(
        &mut |_| Ok(replies.next().unwrap()),
        "owner/repo",
        "feature",
        "master",
        &p,
        "title",
        "body",
    )
    .unwrap_err();
    assert!(error.to_string().contains("PR_DIFF_MISMATCH"));
}

#[test]
fn pr_race_after_diff_readback_fails() {
    let p = proof();
    let mut changed = pr(&p);
    changed["head"]["sha"] = "c".repeat(40).into();
    let mut replies = vec![
        bytes(serde_json::json!([[{"number":7}]])),
        bytes(pr(&p)),
        bytes(files(&p)),
        bytes(changed),
    ]
    .into_iter();
    assert!(
        ensure_and_verify_pr_with(
            &mut |_| Ok(replies.next().unwrap()),
            "owner/repo",
            "feature",
            "master",
            &p,
            "title",
            "body"
        )
        .is_err()
    );
}

#[test]
fn diagnostics_and_success_remote_never_echo_credentials() {
    let secret = "top-secret-token";
    assert!(
        !redact_remote(&format!(
            "https://user:{secret}@github.com/owner/repo?token={secret}"
        ))
        .contains(secret)
    );
    assert!(
        !publish_push_failure(
            &format!("fatal https://user:{secret}@github.com/owner/repo"),
            "feature",
            "origin"
        )
        .contains(secret)
    );
}

fn plan_fixture(repo: &Path) {
    plan_governance::init_plan(
        repo,
        plan_governance::InitPlanInput {
            title: "Publication".into(),
            intent: "Verify current contracts".into(),
            todo_ids: vec!["bugs_current".into()],
            proof_hooks: vec![],
            unknowns: vec![],
            human_questions: vec![],
            stop_conditions: vec![],
            unresolved_contradictions: vec![],
            deferred_questions: vec![],
            constraints: Default::default(),
            phases: vec![],
        },
    )
    .unwrap();
    for path in plan_governance::PUBLICATION_REVIEW_SPECS {
        write(repo, path, "# Current contract\nStill accurate.\n");
    }
}
fn review_all(repo: &Path) {
    for path in plan_governance::PUBLICATION_REVIEW_SPECS {
        plan_governance::review_spec(
            repo,
            path,
            plan_governance::SpecReviewDisposition::UnchangedWithReason,
            "Inspected current implementation; existing contract still describes behavior",
        )
        .unwrap();
    }
}

#[test]
fn code_change_requires_current_review_not_unrelated_spec_churn() {
    let dir = repo();
    plan_fixture(dir.path());
    write(dir.path(), "api/onboarding.go", "new contract");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "api change"]);
    assert!(
        verify_spec_reviews(dir.path(), "master")
            .unwrap_err()
            .to_string()
            .contains("SPEC_REVIEW_REQUIRED")
    );
    review_all(dir.path());
    verify_spec_reviews(dir.path(), "master").unwrap();
    write(dir.path(), "api/onboarding.go", "changed replay semantics");
    assert!(
        verify_spec_reviews(dir.path(), "master")
            .unwrap_err()
            .to_string()
            .contains("STALE_SPEC_REVIEW")
    );
    review_all(dir.path());
    verify_spec_reviews(dir.path(), "master").unwrap();
    write(
        dir.path(),
        plan_governance::PUBLICATION_REVIEW_SPECS[0],
        "different authored contract",
    );
    assert!(
        verify_spec_reviews(dir.path(), "master")
            .unwrap_err()
            .to_string()
            .contains("STALE_SPEC_REVIEW")
    );
}

#[test]
fn unchanged_review_requires_reason_and_decision_cannot_self_approve() {
    let dir = repo();
    plan_fixture(dir.path());
    let path = plan_governance::PUBLICATION_REVIEW_SPECS[0];
    assert!(
        plan_governance::review_spec(
            dir.path(),
            path,
            plan_governance::SpecReviewDisposition::UnchangedWithReason,
            " "
        )
        .is_err()
    );
    plan_governance::review_spec(
        dir.path(),
        path,
        plan_governance::SpecReviewDisposition::RequiresDecision,
        "Human must choose replay contract",
    )
    .unwrap();
    assert!(
        plan_governance::review_spec(
            dir.path(),
            path,
            plan_governance::SpecReviewDisposition::UnchangedWithReason,
            "Agent says approved"
        )
        .unwrap_err()
        .to_string()
        .contains("SPEC_REVIEW_DECISION_REQUIRED")
    );
    assert!(
        plan_governance::review_spec(
            dir.path(),
            "../outside",
            plan_governance::SpecReviewDisposition::Updated,
            "no"
        )
        .is_err()
    );
}

#[test]
fn legacy_plan_loads_without_reviews() {
    let dir = repo();
    plan_fixture(dir.path());
    let plan = plan_governance::load_plan(dir.path()).unwrap().unwrap();
    let mut value = serde_json::to_value(plan).unwrap();
    value.as_object_mut().unwrap().remove("spec_reviews");
    value["schema_version"] = "1.0.0".into();
    write(dir.path(), plan_governance::PLAN_PATH, &value.to_string());
    assert!(
        plan_governance::load_plan(dir.path())
            .unwrap()
            .unwrap()
            .spec_reviews
            .is_empty()
    );
}

#[test]
fn epoch_excludes_receipt_but_binds_plan_claims_and_code_content() {
    use crate::core::validation_epoch::active_validation_epoch;
    let dir = repo();
    plan_fixture(dir.path());
    write(dir.path(), "api/contract.go", "contract v1");
    let first = active_validation_epoch(dir.path()).unwrap();
    write(
        dir.path(),
        crate::core::validate::VALIDATION_RECEIPT_PATH,
        "receipt written after validation",
    );
    assert_eq!(first, active_validation_epoch(dir.path()).unwrap());
    write(dir.path(), "api/contract.go", "contract v2");
    assert_ne!(first, active_validation_epoch(dir.path()).unwrap());
    write(dir.path(), "api/contract.go", "contract v1");
    assert_eq!(first, active_validation_epoch(dir.path()).unwrap());
    review_all(dir.path());
    assert_ne!(first, active_validation_epoch(dir.path()).unwrap());
    let reviewed = active_validation_epoch(dir.path()).unwrap();
    write(dir.path(), research_claims::CLAIMS_PATH, "changed claims");
    assert_ne!(reviewed, active_validation_epoch(dir.path()).unwrap());
}

#[test]
fn explicit_human_resolution_preserves_provenance_and_unchanged_bytes() {
    let dir = repo();
    plan_fixture(dir.path());
    let path = plan_governance::PUBLICATION_REVIEW_SPECS[0];
    plan_governance::review_spec(
        dir.path(),
        path,
        plan_governance::SpecReviewDisposition::RequiresDecision,
        "Choose replay policy",
    )
    .unwrap();
    let before = std::fs::read(dir.path().join(path)).unwrap();
    assert!(plan_governance::resolve_spec_review(dir.path(), path, "", "accepted").is_err());
    let plan = plan_governance::resolve_spec_review(
        dir.path(),
        path,
        "review-thread:human-answer-17",
        "Human accepted documented one-time replay behavior",
    )
    .unwrap();
    assert_eq!(before, std::fs::read(dir.path().join(path)).unwrap());
    assert_eq!(
        plan.spec_reviews[0].decision_ref.as_deref(),
        Some("review-thread:human-answer-17")
    );
    assert!(plan.spec_reviews[0].reason.contains("Choose replay policy"));
    assert_eq!(
        plan.spec_reviews[0].disposition,
        plan_governance::SpecReviewDisposition::UnchangedWithReason
    );
}

fn current_receipt_fixture(repo: &Path) -> crate::core::validate::ValidationReceipt {
    use crate::core::validate::ValidationReceipt;
    plan_fixture(repo);
    research_claims::ensure_template(repo, false).unwrap();
    let run = trajectory::init_trajectory(
        repo,
        trajectory::TrajectoryInit {
            run_id: "publication-proof".into(),
            task_id: Some("bugs_current".into()),
            intent_id: None,
            original_intent: "test publication".into(),
            derived_intent: "test current proof".into(),
            active_boundaries: vec![],
            repo_scope: vec![".".into()],
            destination: None,
            current_phase: None,
            next_transitions: vec![],
            blockers: vec![],
        },
    )
    .unwrap();
    ValidationReceipt {
        schema_version: "1.0.0".into(),
        kind: "validation_receipt".into(),
        decapod_release: entrypoint_integrity::RELEASE_VERSION.into(),
        git_revision: git(repo, &["rev-parse", "HEAD"]),
        repo_signal_fingerprint: project_specs::repo_signal_fingerprint(repo).unwrap(),
        trajectory_run_id: Some(run.run_id),
        trajectory_artifact_hash: Some(run.artifact_hash),
        validation_epoch: crate::core::validation_epoch::active_validation_epoch(repo).unwrap(),
        status: "ok".into(),
        pass_count: 1,
        fail_count: 0,
        warn_count: 0,
        elapsed_ms: 1,
        drift_findings: vec![],
        temporary_artifacts_cleaned: 0,
        failures: None,
        warnings: None,
        gate_timings: None,
        parallelism: None,
        ci_prediction: None,
        receipt_hash: String::new(),
    }
    .with_recomputed_hash()
    .unwrap()
}
fn save_receipt(repo: &Path, receipt: &crate::core::validate::ValidationReceipt) {
    write(
        repo,
        crate::core::validate::VALIDATION_RECEIPT_PATH,
        &serde_json::to_string_pretty(receipt).unwrap(),
    );
}

#[test]
fn current_receipt_accepts_proof_commit_but_rejects_stale_code_epoch_task_and_revision() {
    let dir = repo();
    write(dir.path(), "api/contract.go", "current API");
    let receipt = current_receipt_fixture(dir.path());
    save_receipt(dir.path(), &receipt);
    verify_validation_artifacts_for_publish(dir.path()).unwrap();
    git(dir.path(), &["add", "."]);
    git(
        dir.path(),
        &["commit", "-qm", "validated content and proof"],
    );
    verify_validation_artifacts_for_publish(dir.path()).unwrap();
    write(dir.path(), "api/contract.go", "stale API");
    assert!(
        verify_validation_artifacts_for_publish(dir.path())
            .unwrap_err()
            .to_string()
            .contains("STALE_PUBLICATION_VALIDATION")
    );
    write(dir.path(), "api/contract.go", "current API");
    let mut invalid = receipt.clone();
    invalid.git_revision = "c".repeat(40);
    save_receipt(dir.path(), &invalid.with_recomputed_hash().unwrap());
    assert!(
        verify_validation_artifacts_for_publish(dir.path())
            .unwrap_err()
            .to_string()
            .contains("STALE_PUBLICATION_REVISION")
    );
    save_receipt(dir.path(), &receipt);
    let mut plan = plan_governance::load_plan(dir.path()).unwrap().unwrap();
    plan.todo_ids = vec!["wrong_task".into()];
    plan_governance::save_plan(dir.path(), &plan).unwrap();
    // Re-hashing an epoch cannot hide a mismatched plan/trajectory subject.
    let mut wrong_task = receipt.clone();
    wrong_task.validation_epoch =
        crate::core::validation_epoch::active_validation_epoch(dir.path()).unwrap();
    save_receipt(dir.path(), &wrong_task.with_recomputed_hash().unwrap());
    assert!(
        verify_validation_artifacts_for_publish(dir.path())
            .unwrap_err()
            .to_string()
            .contains("STALE_PUBLICATION_TASK")
    );
}

#[test]
fn invalid_and_missing_artifacts_fail_real_publication_gate() {
    let dir = repo();
    let receipt = current_receipt_fixture(dir.path());
    save_receipt(dir.path(), &receipt);
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "valid bundle"]);
    ensure_required_governance_artifacts_in_pr(dir.path(), "master").unwrap();
    for path in REQUIRED_PR_GOVERNANCE_ARTIFACTS {
        let original = std::fs::read(dir.path().join(path)).unwrap();
        write(dir.path(), path, "invalid JSON");
        assert!(ensure_required_governance_artifacts_in_pr(dir.path(), "master").is_err());
        std::fs::remove_file(dir.path().join(path)).unwrap();
        assert!(ensure_required_governance_artifacts_in_pr(dir.path(), "master").is_err());
        std::fs::write(dir.path().join(path), original).unwrap();
    }
    // Fully valid inherited artifacts still fail if not in the actual PR delta.
    git(dir.path(), &["branch", "current-base"]);
    write(dir.path(), "app.txt", "unrelated");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "no proof changes"]);
    assert!(
        ensure_required_governance_artifacts_in_pr(dir.path(), "current-base")
            .unwrap_err()
            .to_string()
            .contains("not included in the PR diff")
    );
}

#[test]
fn code_fingerprint_covers_root_and_other_implementation_surfaces() {
    for path in [
        "main.go",
        "assets/tools/check.rs",
        "app/route.ts",
        "api/auth.go",
        "backend/auth.py",
        "frontend/app.tsx",
        "web/main.js",
        "services/service.go",
    ] {
        let dir = repo();
        write(dir.path(), path, "before");
        let before = project_specs::repo_signal_fingerprint(dir.path()).unwrap();
        write(dir.path(), path, "after");
        assert_ne!(
            before,
            project_specs::repo_signal_fingerprint(dir.path()).unwrap(),
            "{path}"
        );
        assert!(project_specs::is_implementation_path(path));
    }
}

#[test]
fn spec_review_cli_requires_deliberate_resolution_arguments() {
    use clap::Parser;
    let path = ".decapod/managed/specs/INTERFACES.md";
    assert!(
        crate::cli::Cli::try_parse_from([
            "decapod",
            "govern",
            "plan",
            "review-spec",
            "--path",
            path,
            "--disposition",
            "unchanged-with-reason",
            "--reason",
            "Inspected current API"
        ])
        .is_ok()
    );
    assert!(
        crate::cli::Cli::try_parse_from([
            "decapod",
            "govern",
            "plan",
            "resolve-spec-review",
            "--path",
            path,
            "--reason",
            "approved"
        ])
        .is_err()
    );
    assert!(
        crate::cli::Cli::try_parse_from([
            "decapod",
            "govern",
            "plan",
            "resolve-spec-review",
            "--path",
            path,
            "--decision-ref",
            "human-thread-17",
            "--reason",
            "accepted"
        ])
        .is_ok()
    );
}

#[test]
fn code_changes_cannot_silently_erase_pending_human_decisions() {
    let dir = repo();
    plan_fixture(dir.path());
    let path = plan_governance::PUBLICATION_REVIEW_SPECS[0];
    plan_governance::review_spec(
        dir.path(),
        path,
        plan_governance::SpecReviewDisposition::RequiresDecision,
        "Choose contract",
    )
    .unwrap();
    write(dir.path(), "main.go", "new implementation");
    assert!(
        plan_governance::review_spec(
            dir.path(),
            path,
            plan_governance::SpecReviewDisposition::Updated,
            "Unrelated code changed"
        )
        .is_err()
    );
    assert!(
        plan_governance::resolve_spec_review(
            dir.path(),
            path,
            "old-human-answer",
            "stale approval"
        )
        .is_err()
    );
    plan_governance::review_spec(
        dir.path(),
        path,
        plan_governance::SpecReviewDisposition::RequiresDecision,
        "Human must review current implementation",
    )
    .unwrap();
    plan_governance::resolve_spec_review(
        dir.path(),
        path,
        "current-human-answer",
        "accepted current behavior",
    )
    .unwrap();
}

#[test]
fn unconfirmed_push_never_claims_rejection_or_rollback() {
    for diagnostic in [
        "",
        "connection reset after writing objects",
        "fatal: authentication failed",
        "remote hook declined; failed to push some refs",
    ] {
        let message = publish_push_failure(diagnostic, "feature", "origin");
        assert!(message.contains("PUBLICATION_OUTCOME_UNKNOWN"));
        assert!(message.contains("server may have accepted"));
        assert!(message.contains("git ls-remote"));
        assert!(!message.contains("remote branch has diverged"));
    }
}
