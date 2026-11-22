use super::*;
use crate::core::broker::DbBroker;
use crate::core::db_maintenance::CloudRepositoryAccessStatus;
use crate::core::store::{Store, StoreKind};
use crate::core::todo::{self, ClaimMode, TodoCommand};
use crate::plan_governance::{GovernedPlan, ScopeConstraints, save_plan};
use crate::plugins::policy::{PolicyCli, PolicyCommand, initialize_policy_db, run_policy_cli_as};
use std::process::Command;
use tempfile::TempDir;

fn init_git_repo_with_config(protected_paths: &[&str]) -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    assert!(
        Command::new("git")
            .args(["init", "-b", "master"])
            .current_dir(root)
            .output()
            .expect("git init")
            .status
            .success()
    );
    assert!(
        Command::new("git")
            .args(["config", "user.email", "test@decapod.local"])
            .current_dir(root)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        Command::new("git")
            .args(["config", "user.name", "Decapod Test"])
            .current_dir(root)
            .output()
            .unwrap()
            .status
            .success()
    );

    let decapod_dir = root.join(".decapod");
    let data_dir = decapod_dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(root.join("src/api")).unwrap();
    std::fs::write(root.join("src/api/routes.rs"), "pub fn ok() {}\n").unwrap();
    std::fs::create_dir_all(root.join(".github/workflows")).unwrap();
    std::fs::write(root.join(".github/workflows/ci.yml"), "name: ci\n").unwrap();

    let protected_toml = protected_paths
        .iter()
        .map(|p| format!("\"{p}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let cfg = format!(
        r#"schema_version = "1.0.0"

[init]
specs = true
ci = false
diagram_style = "ascii"
entrypoints = ["AGENTS.md"]

[repo]
name = "grant-test"
summary = "Effective grant test repo"
base_branch = "master"

[governance]
protected_paths = [{protected_toml}]
approval_categories = ["destructive_operations"]
"#
    );
    std::fs::write(decapod_dir.join("config.toml"), cfg).unwrap();

    assert!(
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(root)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        Command::new("git")
            .args(["commit", "-m", "init master"])
            .current_dir(root)
            .output()
            .unwrap()
            .status
            .success()
    );

    todo::initialize_todo_db(&data_dir).unwrap();
    initialize_policy_db(&data_dir).unwrap();
    dir
}

fn add_task_for_test(
    store_root: &Path,
    repo_root: &Path,
    title: &str,
    scope: &str,
    parent: Option<&str>,
) -> String {
    let out = todo::add_task_local(
        store_root,
        &TodoCommand::Add {
            title: title.to_string(),
            description: String::new(),
            priority: "medium".to_string(),
            tags: String::new(),
            owner: String::new(),
            due: None,
            r#ref: String::new(),
            scope: scope.to_string(),
            dir: Some(repo_root.to_string_lossy().to_string()),
            depends_on: String::new(),
            blocks: String::new(),
            parent: parent.map(str::to_string),
            one_shot: 0,
        },
    )
    .expect("add task");
    out["id"].as_str().expect("task id").to_string()
}

#[test]
fn red_green_1_path_narrowing_allows_child_subset_and_denies_excluded_workflows() {
    let fixture = init_git_repo_with_config(&[]);
    let root = fixture.path();
    let store_root = root.join(".decapod/data");

    let parent_id = add_task_for_test(
        &store_root,
        root,
        "Parent root scope",
        "allow:[src/**];deny:[.github/workflows/**]",
        None,
    );
    todo::claim_task_with_lease_local(
        &store_root,
        &parent_id,
        "agent-parent",
        ClaimMode::Exclusive,
        Some(1800),
        None,
    )
    .unwrap();

    let narrowed_child = add_task_for_test(
        &store_root,
        root,
        "Narrow API child",
        "allow:[src/api/**]",
        Some(&parent_id),
    );
    todo::claim_task_with_lease_local(
        &store_root,
        &narrowed_child,
        "agent-child",
        ClaimMode::Exclusive,
        Some(1800),
        None,
    )
    .unwrap();

    // Narrowed child targeting src/api/routes.rs -> Allowed.
    let mut ok_req = GrantEvaluationRequest::for_operation("workspace.publish", "agent-child");
    ok_req.task_id = Some(narrowed_child.clone());
    ok_req.target_path = Some("src/api/routes.rs".into());
    ok_req.now_epoch_s = Some(1_700_000_000);
    let ok_grant = evaluate_effective_grant(root, &store_root, &ok_req);
    assert_eq!(ok_grant.decision, GrantDecision::Allowed, "{ok_grant:?}");
    assert_eq!(ok_grant.effective_allowances, vec!["src/api/**"]);
    assert_eq!(ok_grant.effective_denials, vec![".github/workflows/**"]);

    // Narrowed child cannot touch src/lib.rs outside src/api/**.
    let mut sibling_src_req =
        GrantEvaluationRequest::for_operation("workspace.publish", "agent-child");
    sibling_src_req.task_id = Some(narrowed_child.clone());
    sibling_src_req.target_path = Some("src/lib.rs".into());
    sibling_src_req.now_epoch_s = Some(1_700_000_000);
    let sibling_grant = evaluate_effective_grant(root, &store_root, &sibling_src_req);
    assert_eq!(sibling_grant.decision, GrantDecision::Denied);
    assert!(
        sibling_grant
            .reasons
            .iter()
            .any(|r| matches!(r, GrantRejectionReason::PathNotAllowed { .. }))
    );

    // Child requesting .github/workflows/** -> cannot widen past src/** and workflows stay denied.
    let widening_child = add_task_for_test(
        &store_root,
        root,
        "Widening attempt child",
        "allow:[.github/workflows/**]",
        Some(&parent_id),
    );
    todo::claim_task_with_lease_local(
        &store_root,
        &widening_child,
        "agent-rogue",
        ClaimMode::Exclusive,
        Some(1800),
        None,
    )
    .unwrap();

    let mut bad_req = GrantEvaluationRequest::for_operation("workspace.publish", "agent-rogue");
    bad_req.task_id = Some(widening_child);
    bad_req.target_path = Some(".github/workflows/ci.yml".into());
    bad_req.now_epoch_s = Some(1_700_000_000);
    let bad_grant = evaluate_effective_grant(root, &store_root, &bad_req);
    assert_eq!(bad_grant.decision, GrantDecision::Denied);
    assert!(bad_grant.effective_allowances.is_empty());
    assert!(bad_grant.reasons.iter().any(|r| matches!(
        r,
        GrantRejectionReason::PathExplicitlyDenied { .. }
            | GrantRejectionReason::PathNotAllowed { .. }
    )));
}

#[test]
fn red_green_2_rejects_copied_parent_grant_forged_approval_plan_approved_and_candidate_widening() {
    let fixture = init_git_repo_with_config(&[".github/workflows/**"]);
    let root = fixture.path();
    let store_root = root.join(".decapod/data");

    // Checkout a non-protected branch to test candidate policy widening rejection.
    assert!(
        Command::new("git")
            .args(["checkout", "-b", "agent/alice/candidate"])
            .current_dir(root)
            .output()
            .unwrap()
            .status
            .success()
    );

    let parent_id = add_task_for_test(
        &store_root,
        root,
        "Parent task",
        "allow:[src/**];budget:5",
        None,
    );
    todo::claim_task_with_lease_local(
        &store_root,
        &parent_id,
        "agent-parent",
        ClaimMode::Exclusive,
        Some(1800),
        None,
    )
    .unwrap();

    let child_id = add_task_for_test(
        &store_root,
        root,
        "Child task",
        "allow:[src/api/**]",
        Some(&parent_id),
    );
    todo::claim_task_with_lease_local(
        &store_root,
        &child_id,
        "agent-child",
        ClaimMode::Exclusive,
        Some(1800),
        None,
    )
    .unwrap();

    // 1) Copied parent grant rejected.
    let mut copied_req = GrantEvaluationRequest::for_operation("workspace.publish", "agent-child");
    copied_req.task_id = Some(child_id.clone());
    copied_req.target_path = Some("src/api/routes.rs".into());
    copied_req.presented_parent_grant_task_id = Some(parent_id.clone());
    let copied_grant = evaluate_effective_grant(root, &store_root, &copied_req);
    assert_eq!(copied_grant.decision, GrantDecision::Denied);
    assert!(
        copied_grant
            .reasons
            .iter()
            .any(|r| matches!(r, GrantRejectionReason::CopiedParentGrantRejected { .. }))
    );

    // 2) Forged approval token rejected.
    let mut forged_req = GrantEvaluationRequest::for_operation("todo.archive", "agent-child");
    forged_req.task_id = Some(child_id.clone());
    forged_req.presented_approval_token = Some("forged_human_token".into());
    let forged_grant = evaluate_effective_grant(root, &store_root, &forged_req);
    assert_eq!(forged_grant.decision, GrantDecision::Denied);
    assert!(
        forged_grant
            .reasons
            .iter()
            .any(|r| matches!(r, GrantRejectionReason::ForgedOrUnknownApproval { .. }))
    );

    // 3) PlanState::Approved cannot substitute for operational approval.
    let plan = GovernedPlan {
        schema_version: "1.0.0".into(),
        title: "Test plan".into(),
        intent: "Ship API change".into(),
        state: PlanState::Approved,
        accepted_selected_work_revision: None,
        todo_ids: vec![child_id.clone()],
        proof_hooks: vec![],
        unknowns: vec![],
        human_questions: vec![],
        stop_conditions: vec![],
        unresolved_contradictions: vec![],
        deferred_questions: vec![],
        constraints: ScopeConstraints::default(),
        phases: vec![],
        updated_at: "1700000000Z".into(),
        spec_reviews: vec![],
        reflection: None,
    };
    save_plan(root, &plan).unwrap();

    let mut plan_req = GrantEvaluationRequest::for_operation("todo.archive", "agent-child");
    plan_req.task_id = Some(child_id.clone());
    plan_req.treat_plan_approval_as_permission = true;
    let plan_grant = evaluate_effective_grant(root, &store_root, &plan_req);
    assert_eq!(plan_grant.decision, GrantDecision::Denied);
    assert!(
        plan_grant
            .reasons
            .iter()
            .any(|r| matches!(r, GrantRejectionReason::PlanApprovalNotAuthorization { .. }))
    );

    // 4) Candidate branch removing protected_paths from .decapod/config.toml rejected.
    let original_cfg = std::fs::read_to_string(root.join(".decapod/config.toml")).unwrap();
    let weakened_cfg = original_cfg.replace("[\".github/workflows/**\"]", "[]");
    std::fs::write(root.join(".decapod/config.toml"), weakened_cfg).unwrap();

    let mut cand_req = GrantEvaluationRequest::for_operation("workspace.publish", "agent-child");
    cand_req.task_id = Some(child_id.clone());
    cand_req.target_path = Some("src/api/routes.rs".into());
    let cand_grant = evaluate_effective_grant(root, &store_root, &cand_req);
    assert_eq!(cand_grant.decision, GrantDecision::Denied);
    assert!(cand_grant.reasons.iter().any(|r| matches!(
        r,
        GrantRejectionReason::CandidatePolicyWideningRejected { .. }
    )));
    std::fs::write(root.join(".decapod/config.toml"), original_cfg).unwrap();

    // 5) Delegation budget widening beyond parent cap rejected atomically.
    let broker = DbBroker::new(&store_root);
    let err = broker
        .with_transaction(
            &todo_db_path(&store_root),
            "agent-parent",
            None,
            "test.delegate.widen",
            |conn| {
                record_delegation_policy_on_conn(
                    conn,
                    "1700000000Z",
                    "agent-parent",
                    &DelegationPolicyRecord {
                        parent_task_id: parent_id.clone(),
                        child_task_id: Some(child_id.clone()),
                        grantor_actor: "agent-parent".into(),
                        recipient_actor: None,
                        allowed_paths: vec!["src/api/**".into()],
                        denied_paths: vec![],
                        allowed_operations: None,
                        max_total_budget_units: Some(10),
                        max_child_budget_units: Some(10),
                        max_delegation_depth: None,
                        resource_scope: None,
                        policy_revision: "rev1".into(),
                        expires_at_epoch_s: None,
                    },
                )
            },
        )
        .unwrap_err();
    assert!(
        err.to_string().contains("GRANT_DELEGATION_WIDENING"),
        "{err}"
    );
}

#[test]
fn red_green_3_prestate_drift_lease_expiry_harness_and_policy_revision_invalidate_and_re_form() {
    let fixture = init_git_repo_with_config(&[]);
    let root = fixture.path();
    let store_root = root.join(".decapod/data");

    let task_id = add_task_for_test(&store_root, root, "Bound task", "allow:[src/**]", None);
    todo::claim_task_with_lease_local(
        &store_root,
        &task_id,
        "agent-a",
        ClaimMode::Exclusive,
        Some(100),
        None,
    )
    .unwrap();

    let mut base_req = GrantEvaluationRequest::for_operation("workspace.publish", "agent-a");
    base_req.task_id = Some(task_id.clone());
    base_req.target_path = Some("src/api/routes.rs".into());
    let g0 = evaluate_effective_grant(root, &store_root, &base_req);
    assert_eq!(g0.decision, GrantDecision::Allowed);
    let exp_secs = parse_epoch_z(g0.prestate.lease_expires_at.as_deref().unwrap()).unwrap();

    // 1) Prestate drift S0 -> S1 (new commit advances HEAD).
    std::fs::write(root.join("src/api/routes.rs"), "pub fn v2() {}\n").unwrap();
    assert!(
        Command::new("git")
            .args(["add", "src/api/routes.rs"])
            .current_dir(root)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        Command::new("git")
            .args(["commit", "-m", "advance HEAD to S1"])
            .current_dir(root)
            .output()
            .unwrap()
            .status
            .success()
    );

    let mut stale_prestate_req = base_req.clone();
    stale_prestate_req.expected_prestate_digest = Some(g0.prestate.prestate_digest.clone());
    let s1 = evaluate_effective_grant(root, &store_root, &stale_prestate_req);
    assert_eq!(s1.decision, GrantDecision::Stale);
    assert!(
        s1.reasons
            .iter()
            .any(|r| matches!(r, GrantRejectionReason::PrestateDrift { .. }))
    );

    // 2) Lease expiration -> Stale(LeaseExpired).
    let mut expired_req = base_req.clone();
    expired_req.now_epoch_s = Some(exp_secs + 1);
    let expired_grant = evaluate_effective_grant(root, &store_root, &expired_req);
    assert_eq!(expired_grant.decision, GrantDecision::Stale);
    assert!(
        expired_grant
            .reasons
            .iter()
            .any(|r| matches!(r, GrantRejectionReason::LeaseExpired { .. }))
    );

    // 3) Harness change -> Stale(HarnessOrSessionMismatch).
    let mut harness_req = base_req.clone();
    harness_req.expected_harness_id = Some("harness-other".into());
    let harness_grant = evaluate_effective_grant(root, &store_root, &harness_req);
    assert_eq!(harness_grant.decision, GrantDecision::Stale);
    assert!(
        harness_grant
            .reasons
            .iter()
            .any(|r| matches!(r, GrantRejectionReason::HarnessOrSessionMismatch { .. }))
    );

    // 4) Policy revision change -> Stale(PolicyRevisionMismatch).
    let mut rev_req = base_req.clone();
    rev_req.expected_policy_revision = Some("000000:stale".into());
    let rev_grant = evaluate_effective_grant(root, &store_root, &rev_req);
    assert_eq!(rev_grant.decision, GrantDecision::Stale);
    assert!(
        rev_grant
            .reasons
            .iter()
            .any(|r| matches!(r, GrantRejectionReason::PolicyRevisionMismatch { .. }))
    );

    // 5) Lawful renewal & re-formation against fresh prestate -> Allowed.
    todo::renew_claim_lease(&store_root, &task_id, "agent-a", Some(1800)).unwrap();
    let fresh = evaluate_effective_grant(root, &store_root, &base_req);
    assert_eq!(fresh.decision, GrantDecision::Allowed);
    assert_ne!(fresh.prestate.prestate_digest, g0.prestate.prestate_digest);
}

#[test]
fn red_green_4_property_order_independence_monotonicity_replay_and_strict_parser() {
    // 1) All 24 permutations of 4 layers must produce identical composed scope & digest.
    let l1 = DelegationLayer {
        allowed_operations: Some(BTreeSet::from([
            "workspace.publish".into(),
            "todo.done".into(),
        ])),
        max_budget_units: Some(8),
        allowances: ScopeAllowanceSet::from_patterns([PathPattern::parse("src/**").unwrap()]),
        ..DelegationLayer::new("l1")
    };
    let l2 = DelegationLayer {
        allowances: ScopeAllowanceSet::from_patterns([PathPattern::parse("src/api/**").unwrap()]),
        denials: BTreeSet::from([PathPattern::parse(".github/workflows/**").unwrap()]),
        max_budget_units: Some(3),
        ..DelegationLayer::new("l2")
    };
    let l3 = DelegationLayer {
        denials: BTreeSet::from([PathPattern::parse("src/api/secret.rs").unwrap()]),
        allowed_operations: Some(BTreeSet::from(["workspace.publish".into()])),
        max_budget_units: Some(5),
        ..DelegationLayer::new("l3")
    };
    let l4 = DelegationLayer {
        allowed_resources: Some(BTreeSet::from(["repo:main".into()])),
        expires_at_epoch_s: Some(1_800_000_000),
        ..DelegationLayer::new("l4")
    };

    let base_composed =
        compose_layers_order_independent(&[l1.clone(), l2.clone(), l3.clone(), l4.clone()]);
    assert_eq!(
        base_composed.allowances.canonical_strings(),
        vec!["src/api/**"]
    );
    assert_eq!(base_composed.max_budget_units, Some(3));
    assert_eq!(
        base_composed
            .denials
            .iter()
            .map(PathPattern::canonical_string)
            .collect::<Vec<_>>(),
        vec![".github/workflows/**", "src/api/secret.rs"]
    );

    let items = [l1, l2, l3, l4];
    for a in 0..4 {
        for b in 0..4 {
            if b == a {
                continue;
            }
            for c in 0..4 {
                if c == a || c == b {
                    continue;
                }
                let d = 6 - a - b - c;
                let perm = [
                    items[a].clone(),
                    items[b].clone(),
                    items[c].clone(),
                    items[d].clone(),
                ];
                let composed = compose_layers_order_independent(&perm);
                assert_eq!(
                    composed, base_composed,
                    "permutation ({a},{b},{c},{d}) diverged"
                );
            }
        }
    }

    // 2) Replay determinism on full evaluate_effective_grant.
    let fixture = init_git_repo_with_config(&[]);
    let root = fixture.path();
    let store_root = root.join(".decapod/data");
    let task_id = add_task_for_test(&store_root, root, "Replay task", "allow:[src/**]", None);
    todo::claim_task_with_lease_local(
        &store_root,
        &task_id,
        "agent-replay",
        ClaimMode::Exclusive,
        Some(1800),
        None,
    )
    .unwrap();
    let mut req = GrantEvaluationRequest::for_operation("workspace.publish", "agent-replay");
    req.task_id = Some(task_id);
    req.target_path = Some("src/api/routes.rs".into());
    req.now_epoch_s = Some(1_700_000_000);
    let r1 = evaluate_effective_grant(root, &store_root, &req);
    let r2 = evaluate_effective_grant(root, &store_root, &req);
    assert_eq!(r1, r2);
    assert_eq!(r1.grant_digest, r2.grant_digest);

    // 3) Strict parser ambiguity & invalid pattern rejection.
    for bad in [
        "paths:[src/**",
        "paths:[a,,b]",
        "unknown:foo",
        "src/**/routes.rs",
        "src/*.rs",
        "../outside",
        "/etc/passwd",
        "bad\\slash",
        "deny:[]",
        "budget:not_a_number",
        "paths:src/**;paths:docs/**",
    ] {
        assert!(
            parse_scope_spec(bad).is_err(),
            "ambiguous/invalid scope '{bad}' must be rejected"
        );
    }

    // 4) Missing mandatory fields & unknown actor rejection.
    let empty_op = GrantEvaluationRequest::for_operation("", "agent-replay");
    let g_empty_op = evaluate_effective_grant(root, &store_root, &empty_op);
    assert_eq!(g_empty_op.decision, GrantDecision::Denied);
    assert!(
        g_empty_op
            .reasons
            .iter()
            .any(|r| matches!(r, GrantRejectionReason::MissingMandatoryField { .. }))
    );

    let unknown_actor = GrantEvaluationRequest::for_operation("workspace.publish", "unknown");
    let g_unknown = evaluate_effective_grant(root, &store_root, &unknown_actor);
    assert_eq!(g_unknown.decision, GrantDecision::Denied);
    assert!(
        g_unknown
            .reasons
            .iter()
            .any(|r| matches!(r, GrantRejectionReason::ActorSpoofingRejected { .. }))
    );
}

#[test]
fn adversarial_cumulative_shared_parent_budget_and_depth_limit() {
    let fixture = init_git_repo_with_config(&[]);
    let root = fixture.path();
    let store_root = root.join(".decapod/data");

    let parent_id = add_task_for_test(
        &store_root,
        root,
        "Parent budget cap 3",
        "allow:[src/**]",
        None,
    );
    todo::claim_task_with_lease_local(
        &store_root,
        &parent_id,
        "coordinator",
        ClaimMode::Exclusive,
        Some(1800),
        None,
    )
    .unwrap();

    let c1 = add_task_for_test(
        &store_root,
        root,
        "Child 1 (2 units)",
        "allow:[src/api/**]",
        Some(&parent_id),
    );
    let c2 = add_task_for_test(
        &store_root,
        root,
        "Child 2 (2 units)",
        "allow:[src/api/**]",
        Some(&parent_id),
    );
    let c3 = add_task_for_test(
        &store_root,
        root,
        "Child 3 (1 unit)",
        "allow:[src/api/**]",
        Some(&parent_id),
    );

    let broker = DbBroker::new(&store_root);
    broker
        .with_transaction(
            &todo_db_path(&store_root),
            "coordinator",
            None,
            "test.delegate.pair",
            |conn| {
                for (cid, cap) in [(&c1, 2u64), (&c2, 2u64), (&c3, 1u64)] {
                    record_delegation_policy_on_conn(
                        conn,
                        "1700000000Z",
                        "coordinator",
                        &DelegationPolicyRecord {
                            parent_task_id: parent_id.clone(),
                            child_task_id: Some(cid.clone()),
                            grantor_actor: "coordinator".into(),
                            recipient_actor: None,
                            allowed_paths: vec!["src/api/**".into()],
                            denied_paths: vec![],
                            allowed_operations: None,
                            max_total_budget_units: Some(3),
                            max_child_budget_units: Some(cap),
                            max_delegation_depth: Some(1),
                            resource_scope: None,
                            policy_revision: "rev1".into(),
                            expires_at_epoch_s: None,
                        },
                    )?;
                }
                Ok(())
            },
        )
        .unwrap();

    // Child 1 reserves 2/3 -> succeeds.
    todo::claim_task_with_lease_local(
        &store_root,
        &c1,
        "worker-1",
        ClaimMode::Exclusive,
        Some(1800),
        None,
    )
    .unwrap();

    // Child 2 reserves 2/3 -> 2 + 2 = 4 > 3 -> REFUSED atomically, claim must roll back!
    let c2_err = todo::claim_task_with_lease_local(
        &store_root,
        &c2,
        "worker-2",
        ClaimMode::Exclusive,
        Some(1800),
        None,
    )
    .unwrap_err();
    assert!(
        c2_err
            .to_string()
            .contains("CUMULATIVE_DELEGATION_BUDGET_EXCEEDED"),
        "{c2_err}"
    );
    let c2_task = todo::get_task_local(&store_root, &c2).unwrap().unwrap();
    assert!(
        c2_task.assigned_to.is_empty(),
        "rolled back claim must leave c2 unclaimed"
    );

    // Child 3 reserves 1/3 -> 2 + 1 = 3 <= 3 -> SUCCEEDS!
    todo::claim_task_with_lease_local(
        &store_root,
        &c3,
        "worker-3",
        ClaimMode::Exclusive,
        Some(1800),
        None,
    )
    .unwrap();

    // Child 1 has remaining delegation depth 0 (parent gave depth=1) -> delegating to grandchild fails.
    let gc_err = broker
        .with_transaction(
            &todo_db_path(&store_root),
            "worker-1",
            None,
            "test.delegate.depth",
            |conn| {
                record_delegation_policy_on_conn(
                    conn,
                    "1700000000Z",
                    "worker-1",
                    &DelegationPolicyRecord {
                        parent_task_id: c1.clone(),
                        child_task_id: None,
                        grantor_actor: "worker-1".into(),
                        recipient_actor: None,
                        allowed_paths: vec!["src/api/**".into()],
                        denied_paths: vec![],
                        allowed_operations: None,
                        max_total_budget_units: Some(1),
                        max_child_budget_units: Some(1),
                        max_delegation_depth: Some(1),
                        resource_scope: None,
                        policy_revision: "rev1".into(),
                        expires_at_epoch_s: None,
                    },
                )
            },
        )
        .unwrap_err();
    assert!(
        gc_err
            .to_string()
            .contains("GRANT_DELEGATION_DEPTH_EXCEEDED"),
        "{gc_err}"
    );
}

#[test]
fn cloud_verified_server_scope_and_witness_distinction() {
    let fixture = init_git_repo_with_config(&[]);
    let root = fixture.path();
    let store_root = root.join(".decapod/data");

    let denied_status = CloudRepositoryAccessStatus {
        repository: "repo-1".into(),
        effective_tier: RepoAccessTier::Read,
        seat_authorized: false,
        policy_bound: true,
        policy_revision: Some(1),
        policy_hash: None,
        accepted_commit: None,
        max_delegated_tier: RepoAccessTier::Read,
        require_owner_for_repair: false,
        authority_generation: None,
        repair_eligible: false,
    };
    let mut req = GrantEvaluationRequest::for_operation("workspace.publish", "agent-cloud");
    req.verified_cloud_scope = Some(VerifiedCloudScope::from_access_status(&denied_status));
    let grant = evaluate_effective_grant(root, &store_root, &req);
    assert_eq!(grant.decision, GrantDecision::Denied);
    assert!(
        grant
            .reasons
            .iter()
            .any(|r| matches!(r, GrantRejectionReason::CloudScopeDenied { .. }))
    );

    let ro_status = CloudRepositoryAccessStatus {
        repository: "repo-1".into(),
        effective_tier: RepoAccessTier::Write,
        seat_authorized: true,
        policy_bound: true,
        policy_revision: Some(1),
        policy_hash: None,
        accepted_commit: None,
        max_delegated_tier: RepoAccessTier::Read,
        require_owner_for_repair: false,
        authority_generation: None,
        repair_eligible: false,
    };
    let mut ro_req = GrantEvaluationRequest::for_operation("workspace.publish", "agent-cloud");
    ro_req.verified_cloud_scope = Some(VerifiedCloudScope::from_access_status(&ro_status));
    let ro_grant = evaluate_effective_grant(root, &store_root, &ro_req);
    assert_eq!(ro_grant.decision, GrantDecision::Denied);

    // Witnessed external consequence remains strictly distinct from Decapod-mediated execution.
    let consequence = OperationConsequenceRecord {
        decision: GrantDecision::Allowed,
        grant_digest: "digest123".into(),
        operation: "github.pr.merge".into(),
        outcome: ExecutionOutcomeKind::WitnessedExternal {
            witness_executor: "github-api".into(),
            observed: true,
            verification_gap: Some("remote merge state attested externally".into()),
        },
        note: "admission != remote execution".into(),
    };
    assert!(consequence.decision.is_allowed());
    assert!(matches!(
        consequence.outcome,
        ExecutionOutcomeKind::WitnessedExternal { .. }
    ));

    // Ensure policy.db explicit live approval admits destructive ops cleanly without self-approval.
    let store = Store {
        kind: StoreKind::Repo,
        root: store_root.clone(),
    };
    let fp = derive_fingerprint("todo.archive", None, "global");
    run_policy_cli_as(
        &store,
        PolicyCli {
            command: PolicyCommand::Approve {
                id: fp,
                actor: "operator".into(),
                scope: "global".into(),
                expires_in_seconds: 900,
            },
        },
        "operator",
    )
    .unwrap();
    // Basic actor still denied by trust floor; operator (core trust) with live approval succeeds.
    let basic_req = GrantEvaluationRequest::for_operation("todo.archive", "agent-1");
    let basic_grant = evaluate_effective_grant(root, &store_root, &basic_req);
    assert_eq!(basic_grant.decision, GrantDecision::Denied);
    assert!(
        basic_grant
            .reasons
            .iter()
            .any(|r| matches!(r, GrantRejectionReason::TrustLevelInsufficient { .. }))
    );

    let approved_req = GrantEvaluationRequest::for_operation("todo.archive", "operator");
    let approved_grant = evaluate_effective_grant(root, &store_root, &approved_req);
    assert_eq!(approved_grant.decision, GrantDecision::Allowed);
    assert!(approved_grant.approval_required);
}
