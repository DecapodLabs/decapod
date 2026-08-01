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
fn detect_overlaps_skips_expired_and_same_agent() {
    let active = vec![
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
    let overlaps = detect_overlaps(
        "t-new",
        "agent-a",
        "architecture",
        "feature",
        "src/core/workspace.rs",
        &active,
        "100Z",
    );
    // same agent t1 skipped; expired t2 skipped; t3 different domain → no overlaps
    assert!(overlaps.is_empty());

    let overlaps = detect_overlaps(
        "t-new",
        "agent-d",
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
    assert!(
        overlaps
            .iter()
            .any(|o| o.task_id == "t1" && o.reason == "scope_overlap")
    );
    assert!(
        overlaps
            .iter()
            .any(|o| o.task_id == "t1" && o.reason == "category_overlap")
    );
    assert!(!overlaps.iter().any(|o| o.task_id == "t2"));
}

#[test]
fn fleet_health_projection_counts_expired_and_overlaps() {
    let claims = vec![
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
            Some("500Z"),
        ),
    ];
    let health = project_fleet_health(claims, "100Z");
    assert_eq!(health.claim_count, 3);
    assert_eq!(health.expired_count, 1);
    assert_eq!(health.agent_count, 3);
    assert!(health.overlap_count >= 1);
    assert!(
        health
            .overlaps
            .iter()
            .any(|o| o.reason == "scope_overlap" && o.surface == "scope")
    );
    assert_eq!(health.expired_leases[0].task_id, "t1");
}
