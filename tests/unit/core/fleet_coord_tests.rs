// Moved from src/decapod/core/fleet_coord.rs
use super::*;

fn claim(
    task_id: &str,
    agent: &str,
    scope: &str,
    category: &str,
    dir: &str,
    lease: Option<&str>,
) -> ActiveClaimView {
    ActiveClaimView {
        task_id: task_id.to_string(),
        agent_id: agent.to_string(),
        scope: scope.to_string(),
        category: category.to_string(),
        dir_path: dir.to_string(),
        lease_expires_at: lease.map(str::to_string),
        lease_state: LeaseState::Unspecified,
        assigned_at: Some("100Z".to_string()),
        lease_generation: 1,
        lease_lifecycle: LeaseLifecycle::Claimed,
        intent_anchor: format!("intent:todo:{task_id}"),
        capacity_units: 1,
        dependency_readiness: DependencyReadiness::ready(),
    }
}

#[test]
fn lease_expiry_and_reclaim_rules() {
    assert_eq!(lease_state(None, "200Z"), LeaseState::Unspecified);
    assert_eq!(lease_state(Some("150Z"), "200Z"), LeaseState::Expired);
    assert_eq!(lease_state(Some("250Z"), "200Z"), LeaseState::Active);
    assert!(lease_allows_reclaim(Some("150Z"), "200Z"));
    assert!(!lease_allows_reclaim(Some("250Z"), "200Z"));
    assert!(!lease_allows_reclaim(None, "200Z"));
    assert_eq!(lease_expires_at(100, DEFAULT_CLAIM_LEASE_SECS), "1900Z");
    assert_eq!(
        lease_expires_at(100, MAX_CLAIM_LEASE_SECS + 100),
        format_epoch_z(100 + MAX_CLAIM_LEASE_SECS)
    );
}

#[test]
fn lease_generation_and_lifecycle_helpers() {
    assert_eq!(next_lease_generation(0, false), 1);
    assert_eq!(next_lease_generation(3, false), 3);
    assert_eq!(next_lease_generation(3, true), 4);
    assert_eq!(extended_generation(1), 2);
    assert_eq!(handoff_lease_generation(0), 1);
    assert_eq!(handoff_lease_generation(3), 4);
    assert_eq!(
        default_intent_anchor("feat_01abc"),
        "intent:todo:feat_01abc"
    );
    assert!(LeaseLifecycle::Claimed.holds_exclusive_custody());
    assert!(LeaseLifecycle::Extended.holds_exclusive_custody());
    assert!(!LeaseLifecycle::Yielded.holds_exclusive_custody());
    assert!(exclusive_lease_requires_proof(
        LeaseLifecycle::Claimed,
        true
    ));
    assert!(!exclusive_lease_requires_proof(
        LeaseLifecycle::Yielded,
        true
    ));
}

#[test]
fn handoff_lease_transfer_advances_generation_and_reissues_lease() {
    let transfer = plan_handoff_lease_transfer(
        "agent-a",
        "agent-b",
        2,
        LeaseLifecycle::Extended,
        "intent:houseboat:wave4",
        1_000,
        DEFAULT_CLAIM_LEASE_SECS,
    );
    assert_eq!(transfer.previous_agent, "agent-a");
    assert_eq!(transfer.next_agent, "agent-b");
    assert_eq!(transfer.prior_generation, 2);
    assert_eq!(transfer.next_generation, 3);
    assert_eq!(transfer.prior_lifecycle, LeaseLifecycle::Extended);
    assert_eq!(transfer.next_lifecycle, LeaseLifecycle::Claimed);
    assert_eq!(transfer.intent_anchor, "intent:houseboat:wave4");
    assert_eq!(
        transfer.lease_expires_at,
        lease_expires_at(1_000, DEFAULT_CLAIM_LEASE_SECS)
    );
    assert_eq!(transfer.lease_seconds, DEFAULT_CLAIM_LEASE_SECS);
}

#[test]
fn path_overlap_is_boundary_aware() {
    assert!(paths_overlap(
        "src/decapod/core",
        "src/decapod/core/todo.rs"
    ));
    assert!(paths_overlap("/repo/src/foo/", "/repo/src/foo/bar"));
    assert!(!paths_overlap("src/foo", "src/foobar"));
    assert!(!paths_overlap("", "src/foo"));
    assert!(!paths_overlap("src/a", "src/b"));
}

#[test]
fn scope_and_category_overlap_ignore_broad_roots() {
    assert!(scopes_overlap("architecture", "architecture"));
    assert!(!scopes_overlap("root", "architecture"));
    assert!(!scopes_overlap("workspace", "documentation"));
    assert!(categories_overlap("feature", "feature"));
    assert!(!categories_overlap("", "feature"));
}

#[test]
fn detect_overlaps_skips_expired_yielded_and_same_agent() {
    let mut active = vec![
        claim(
            "t1",
            "agent-a",
            "architecture",
            "feature",
            "src/core",
            Some("500Z"),
        ),
        claim(
            "t2",
            "agent-b",
            "architecture",
            "feature",
            "src/core/todo.rs",
            Some("50Z"),
        ),
        claim(
            "t3",
            "agent-c",
            "documentation",
            "docs",
            "docs/PLAYBOOK",
            Some("500Z"),
        ),
    ];
    active[1].lease_lifecycle = LeaseLifecycle::Claimed;
    let mut yielded = claim(
        "t4",
        "agent-d",
        "architecture",
        "feature",
        "src/core/workspace.rs",
        Some("500Z"),
    );
    yielded.lease_lifecycle = LeaseLifecycle::Yielded;
    active.push(yielded);

    let overlaps = detect_overlaps(
        "t-new",
        "agent-a",
        "architecture",
        "feature",
        "src/core/workspace.rs",
        &active,
        "100Z",
    );
    assert!(overlaps.is_empty());

    let overlaps = detect_overlaps(
        "t-new",
        "agent-e",
        "architecture",
        "feature",
        "src/core/workspace.rs",
        &active,
        "100Z",
    );
    assert!(
        overlaps
            .iter()
            .any(|o| o.task_id == "t1" && o.reason == "path_overlap")
    );
    assert!(!overlaps.iter().any(|o| o.task_id == "t2"));
    assert!(!overlaps.iter().any(|o| o.task_id == "t4"));
}

#[test]
fn fleet_health_projection_counts_capacity_and_risk() {
    let mut claims = vec![
        claim(
            "t1",
            "agent-a",
            "architecture",
            "feature",
            "src/a",
            Some("50Z"),
        ),
        claim(
            "t2",
            "agent-b",
            "architecture",
            "bug",
            "src/b",
            Some("500Z"),
        ),
        claim(
            "t3",
            "agent-c",
            "architecture",
            "docs",
            "docs",
            Some("120Z"),
        ),
    ];
    claims[2].lease_lifecycle = LeaseLifecycle::Claimed;
    // now=100, t3 expires at 120 -> within 5min risk window
    let health = project_fleet_health_with_capacity(claims, "100Z", 4);
    assert_eq!(health.claim_count, 3);
    assert_eq!(health.expired_count, 1);
    assert_eq!(health.agent_count, 3);
    assert!(health.overlap_count >= 1);
    assert!(health.expiry_risk_count >= 1);
    assert!(health.capacity.reserved_units >= 1);
    assert!(!health.intent_anchors.is_empty());
    assert_eq!(health.expired_leases[0].task_id, "t1");
}

#[test]
fn dependency_readiness_requires_completion_and_passing_proof() {
    let readiness = evaluate_dependency_readiness(
        "task-main",
        vec![
            DependencyNodeView {
                task_id: "dep-b".to_string(),
                status: "done".to_string(),
                validation_status: Some("passed".to_string()),
                proof_refs: vec!["proof:dep-b:validate:sha256:b".to_string()],
            },
            DependencyNodeView {
                task_id: "dep-a".to_string(),
                status: "open".to_string(),
                validation_status: None,
                proof_refs: Vec::new(),
            },
        ],
        &[("task-main".to_string(), "dep-a".to_string())],
    );
    assert_eq!(readiness.state, DependencyReadinessState::Waiting);
    assert_eq!(readiness.dependencies[0].task_id, "dep-a");
    assert_eq!(readiness.blockers[0].reason, "dependency_not_complete");
    assert_eq!(readiness.proof_refs, vec!["proof:dep-b:validate:sha256:b"]);

    let proof_blocked = evaluate_dependency_readiness(
        "task-main",
        vec![DependencyNodeView {
            task_id: "dep-a".to_string(),
            status: "done".to_string(),
            validation_status: Some("claimed".to_string()),
            proof_refs: Vec::new(),
        }],
        &[],
    );
    assert_eq!(proof_blocked.state, DependencyReadinessState::ProofBlocked);
    assert_eq!(
        proof_blocked.blockers[0].reason,
        "dependency_proof_not_passed"
    );
}

#[test]
fn dependency_readiness_detects_missing_nodes_and_reachable_cycles() {
    let missing = evaluate_dependency_readiness(
        "task-main",
        vec![DependencyNodeView {
            task_id: "dep-missing".to_string(),
            status: "missing".to_string(),
            validation_status: None,
            proof_refs: Vec::new(),
        }],
        &[("task-main".to_string(), "dep-missing".to_string())],
    );
    assert_eq!(missing.state, DependencyReadinessState::Missing);

    let cycle = evaluate_dependency_readiness(
        "task-main",
        Vec::new(),
        &[
            ("task-main".to_string(), "dep-a".to_string()),
            ("dep-a".to_string(), "dep-b".to_string()),
            ("dep-b".to_string(), "dep-a".to_string()),
        ],
    );
    assert_eq!(cycle.state, DependencyReadinessState::Cycle);
    assert_eq!(cycle.cycle, vec!["dep-a", "dep-b", "dep-a"]);
}

#[test]
fn fleet_projection_surfaces_dependency_and_proof_blockers() {
    let mut blocked = claim(
        "task-blocked",
        "agent-a",
        "architecture",
        "feature",
        "src/core",
        Some("500Z"),
    );
    blocked.dependency_readiness = DependencyReadiness {
        state: DependencyReadinessState::ProofBlocked,
        blockers: vec![DependencyBlocker {
            task_id: "dep-a".to_string(),
            reason: "dependency_proof_not_passed".to_string(),
            status: "done".to_string(),
            validation_status: Some("claimed".to_string()),
        }],
        ..DependencyReadiness::ready()
    };
    let health = project_fleet_health(vec![blocked], "100Z");
    assert_eq!(health.dependency_blocked_count, 1);
    assert_eq!(health.proof_blocked_count, 1);
    assert_eq!(health.dependency_blocked_claims[0].task_id, "task-blocked");
}
