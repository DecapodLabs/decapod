// Moved from src/decapod/core/todo.rs
#[test]
fn claim_container_error_summary_hides_preflight_dump() {
    let summary = super::summarize_claim_container_error(
        "Validation error: AUTOREMEDIABLE_VALIDATION_ERROR code=container_runtime_preflight_failed\nstderr:\nvery long host-specific output",
    );

    assert_eq!(
        summary,
        "Container runtime preflight failed. Check Docker/Podman availability and permissions."
    );
    assert!(!summary.contains("AUTOREMEDIABLE"));
    assert!(!summary.contains("stderr"));
}
