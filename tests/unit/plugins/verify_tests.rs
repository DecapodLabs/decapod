// Moved from src/decapod/plugins/verify.rs
use super::{
    resolve_artifact_path_for_todo, validation_epoch_changed_reason, validation_proof_reason,
};
use std::fs;
use tempfile::tempdir;

fn readiness_document(run: &str) -> serde_json::Value {
    serde_json::json!({
        "status": "ok", "scope": "full", "publication_ready": true,
        "report": {
            "failures": [], "elapsed_ms": 15,
            "validation_epoch": {"epoch_id": "epoch-current"},
            "gate_timings": [{"name": "policy", "elapsed_ms": 3}, {"name": "specs", "elapsed_ms": 8}]
        },
        "readiness": {
            "evidence_path": format!("/private/tmp/decapod-readiness-{run}"),
            "ready": true, "proof_ready": true, "scope": "repository_local_readiness",
            "blockers": [], "stage_dispositions": [{"stage": "rust-tests", "disposition": "real"}],
            "laya_readiness": {
                "schema": "decapod.laya-readiness.v1", "mode": "disabled", "decision": "disabled",
                "request_id": format!("sha256:laya-request-{run}"),
                "binding": {
                    "evidence_digest": format!("sha256:laya-evidence-{run}"),
                    "material": {"head_revision": "head", "accepted_base_revision": "base"},
                    "configuration_digest": "laya-config", "governance_digest": "governance",
                    "proof_plan_digest": "plan"
                }
            },
            "vega_readiness": {
                "schema": "decapod.vega-readiness.v1", "mode": "disabled", "decision": "disabled",
                "request_id": format!("sha256:request-{run}"),
                "binding": {
                    "evidence_digest": format!("sha256:evidence-{run}"),
                    "material": {"head_revision": "head", "accepted_base_revision": "base",
                        "toolchain_digest": "tools", "tree_digest": "tree"},
                    "configuration_digest": "config", "governance_digest": "governance",
                    "proof_plan_digest": "plan"
                }
            }
        }
    })
}

#[test]
fn validation_replay_ignores_only_run_evidence_identity() {
    let first = readiness_document("first");
    let mut second = readiness_document("second");
    second["report"]["elapsed_ms"] = 900.into();
    second["report"]["gate_timings"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert_ne!(first, second);
    assert_eq!(
        super::normalize_validation_document(&first),
        super::normalize_validation_document(&second)
    );
    // Normalization operates on a copy; the complete execution receipt is kept.
    assert_eq!(
        first["readiness"]["vega_readiness"]["request_id"],
        "sha256:request-first"
    );
}

#[test]
fn validation_replay_retains_material_policy_and_failure_changes() {
    let baseline = readiness_document("first");
    let expected = super::normalize_validation_document(&baseline);
    for pointer in [
        "/status",
        "/scope",
        "/publication_ready",
        "/report/validation_epoch/epoch_id",
        "/readiness/ready",
        "/readiness/proof_ready",
        "/readiness/scope",
        "/readiness/laya_readiness/mode",
        "/readiness/laya_readiness/decision",
        "/readiness/laya_readiness/binding/material/head_revision",
        "/readiness/laya_readiness/binding/configuration_digest",
        "/readiness/vega_readiness/mode",
        "/readiness/vega_readiness/decision",
        "/readiness/vega_readiness/binding/material/head_revision",
        "/readiness/vega_readiness/binding/material/accepted_base_revision",
        "/readiness/vega_readiness/binding/material/toolchain_digest",
        "/readiness/vega_readiness/binding/material/tree_digest",
        "/readiness/vega_readiness/binding/configuration_digest",
        "/readiness/vega_readiness/binding/governance_digest",
        "/readiness/vega_readiness/binding/proof_plan_digest",
        "/readiness/stage_dispositions/0/disposition",
    ] {
        let mut changed = baseline.clone();
        *changed.pointer_mut(pointer).unwrap() = "changed".into();
        assert_ne!(
            expected,
            super::normalize_validation_document(&changed),
            "{pointer}"
        );
    }
    for pointer in ["/report/failures", "/readiness/blockers"] {
        let mut changed = baseline.clone();
        changed
            .pointer_mut(pointer)
            .unwrap()
            .as_array_mut()
            .unwrap()
            .push("failed".into());
        assert_ne!(
            expected,
            super::normalize_validation_document(&changed),
            "{pointer}"
        );
    }
    let mut unknown = baseline.clone();
    unknown["evidence_digest"] = "new-semantic-field".into();
    assert_ne!(expected, super::normalize_validation_document(&unknown));
    let mut malformed = baseline.clone();
    malformed["readiness"]["evidence_path"] = serde_json::Value::Null;
    assert_ne!(expected, super::normalize_validation_document(&malformed));
}

#[test]
fn future_named_readiness_receipt_masks_only_execution_fields() {
    let mut first = readiness_document("first");
    let mut second = readiness_document("second");
    for (document, run) in [(&mut first, "first"), (&mut second, "second")] {
        document["readiness"]["next_readiness"] = serde_json::json!({
            "request_id": format!("request-{run}"),
            "binding": {
                "evidence_digest": format!("evidence-{run}"),
                "governance_digest": "stable-policy"
            },
            "decision": "ready"
        });
    }
    let expected = super::normalize_validation_document(&first);
    assert_eq!(expected, super::normalize_validation_document(&second));
    second["readiness"]["next_readiness"]["decision"] = "blocked".into();
    assert_ne!(expected, super::normalize_validation_document(&second));
}

#[test]
fn stable_failed_validation_remains_blocked_with_explicit_reason() {
    assert_eq!(
        validation_proof_reason(false, true, "bugs_01test"),
        "decapod validate did not pass; output hash is unchanged from the baseline, so verification remains blocked. Agent: run `decapod validate --format json` to inspect the current blockers, fix the reported validation gate, then run `decapod qa verify todo bugs_01test`"
    );
}

#[test]
fn failed_validation_with_changed_output_reports_validation_failure() {
    assert_eq!(
        validation_proof_reason(false, false, "bugs_01test"),
        "decapod validate did not pass. Agent: run `decapod validate --format json` to inspect the current blockers, fix the reported validation gate, then run `decapod qa verify todo bugs_01test`"
    );
}

#[test]
fn passing_validation_with_changed_output_reports_hash_drift() {
    assert_eq!(
        validation_proof_reason(true, false, "bugs_01test"),
        "validate output hash changed. Next: review the drift, then recapture with `decapod qa verify regen bugs_01test` if the change is intentional"
    );
}

#[test]
fn stale_epoch_reports_supported_aggregate_deadlock_recovery() {
    let reason = validation_epoch_changed_reason("epoch-old", "epoch-new", "bugs_01test");

    assert!(reason.contains("decapod qa verify recover"));
    assert!(reason.contains("--artifact bugs_01test=<current-file>"));
    assert!(reason.contains("without reopening"));
    assert!(reason.contains("actual passing validation"));
    assert!(!reason.contains("qa verify prune"));
    assert!(!reason.contains("todo claim"));
    assert!(!reason.contains("todo done"));
}

#[test]
fn artifact_path_resolves_inside_task_workspace_when_missing_from_parent() {
    let tmp = tempdir().expect("tmpdir");
    let host = tmp.path();
    let workspace = host
        .join(".decapod")
        .join("workspaces")
        .join("agent-unknown-bugs-01kzartxxx");
    fs::create_dir_all(workspace.join("tests")).expect("ws tests");
    fs::write(workspace.join("tests/storage_fixtures.rs"), "fn t() {}\n").expect("fixture");

    let resolved =
        resolve_artifact_path_for_todo(host, Some("bugs_01kzartxxx"), "tests/storage_fixtures.rs");
    assert_eq!(
        resolved,
        workspace.join("tests/storage_fixtures.rs"),
        "parent checkout must find the workspace-only artifact"
    );
}

#[test]
fn failed_child_diagnostics_are_bounded_fixed_labels_not_raw_output() {
    let diagnostic = super::safe_validation_failure_diagnostic(
        b"NEEDS_PLAN_APPROVAL: private task text secret=do-not-forward",
        b"READINESS_BLOCKED: https://secret.invalid/token?credential=private",
    );
    assert!(diagnostic.contains("NEEDS_PLAN_APPROVAL"));
    assert!(diagnostic.contains("READINESS_BLOCKED"));
    assert!(diagnostic.contains("decapod validate --format json"));
    assert!(!diagnostic.contains("private"));
    assert!(!diagnostic.contains("credential"));
    assert!(!diagnostic.contains("secret.invalid"));
    assert!(diagnostic.len() < 1024);

    let unknown = super::safe_validation_failure_diagnostic(b"unknown secret", b"raw error");
    assert!(unknown.contains("no recognized safe blocker label"));
    assert!(!unknown.contains("unknown secret"));
    let mut oversized = vec![b'x'; 1024 * 1024];
    oversized.extend_from_slice(b"NEEDS_PLAN_APPROVAL");
    assert!(
        !super::safe_validation_failure_diagnostic(&oversized, b"").contains("NEEDS_PLAN_APPROVAL")
    );
}
