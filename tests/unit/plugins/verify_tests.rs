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
        first["readiness"]["laya_readiness"]["request_id"],
        "sha256:laya-request-first"
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
        "/readiness/laya_readiness/binding/material/accepted_base_revision",
        "/readiness/laya_readiness/binding/material/toolchain_digest",
        "/readiness/laya_readiness/binding/material/tree_digest",
        "/readiness/laya_readiness/binding/governance_digest",
        "/readiness/laya_readiness/binding/proof_plan_digest",
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

#[test]
fn scoped_validation_json_diagnostics_unwrap_nested_blockers_and_stages() {
    let remote_readiness = serde_json::json!({
        "status": "fail",
        "scope": "publication_readiness",
        "publication_ready": false,
        "report": null,
        "readiness": {
            "scope": "source_pr_readiness",
            "ready": false,
            "proof_ready": false,
            "blockers": [{
                "code": "READINESS_BLOCKED",
                "detail": "PROOF_PROFILE_RECONCILIATION_REQUIRED: source inventory differs; path=/private/repo token=secret"
            }]
        }
    });
    let diag = super::extract_safe_validation_diagnostic(
        serde_json::to_string(&remote_readiness).unwrap().as_bytes(),
        b"",
    );
    assert_eq!(diag.stage, Some("readiness"));
    assert_eq!(diag.scope, Some("source_pr_readiness"));
    assert_eq!(
        diag.labels,
        vec!["PROOF_PROFILE_RECONCILIATION_REQUIRED", "READINESS_BLOCKED"]
    );
    assert!(diag.message.contains("stage=readiness"));
    assert!(diag.message.contains("scope=source_pr_readiness"));
    assert!(
        diag.message
            .contains("PROOF_PROFILE_RECONCILIATION_REQUIRED")
    );
    assert!(diag.message.contains("PROOF_HOOK_FAILED"));
    assert!(!diag.message.contains("/private/repo"));
    assert!(!diag.message.contains("secret"));
    assert!(diag.message.len() < 1024);

    let post_readiness = serde_json::json!({
        "status": "fail",
        "scope": "publication_readiness",
        "publication_ready": false,
        "report": { "status": "pass", "fail_count": 0, "failures": [] },
        "readiness": {
            "scope": "repository_local_readiness",
            "ready": false,
            "proof_ready": true,
            "blockers": [{
                "code": "READINESS_BLOCKED",
                "detail": "publication_readiness: Cannot publish: FINGERPRINT_ONLY_SPECS \u{2014} living specs unchanged vs master"
            }]
        }
    });
    let post_diag = super::extract_safe_validation_diagnostic(
        serde_json::to_string(&post_readiness).unwrap().as_bytes(),
        b"",
    );
    assert_eq!(post_diag.stage, Some("post_validation_readiness"));
    assert_eq!(post_diag.scope, Some("repository_local_readiness"));
    assert_eq!(
        post_diag.labels,
        vec!["FINGERPRINT_ONLY_SPECS", "READINESS_BLOCKED"]
    );

    let untrusted_trajectory = serde_json::json!({
        "status": "fail",
        "scope": "publication_readiness",
        "readiness": {
            "scope": "repository_local_readiness",
            "ready": false,
            "proof_ready": true,
            "blockers": [{
                "code": "READINESS_BLOCKED",
                "detail": "publication_readiness: Cannot publish: trajectory 'evil-PROOF_HOOK_FAILED-id' has blockers: https://secret.invalid/leaked"
            }]
        }
    });
    let traj_diag = super::extract_safe_validation_diagnostic(
        serde_json::to_string(&untrusted_trajectory)
            .unwrap()
            .as_bytes(),
        b"",
    );
    assert_eq!(
        traj_diag.labels,
        vec!["TRAJECTORY_BLOCKERS_PRESENT", "READINESS_BLOCKED"]
    );
    assert!(!traj_diag.labels.contains(&"PROOF_HOOK_FAILED"));
    assert!(!traj_diag.message.contains("evil"));
    assert!(!traj_diag.message.contains("secret.invalid"));

    let gate_failure = serde_json::json!({
        "status": "fail",
        "scope": "publication_readiness",
        "report": {
            "status": "fail",
            "fail_count": 4,
            "failures": [
                "AUTOREMEDIABLE_VALIDATION_ERROR code=PR_MERGE_CONFLICT severity=blocking\nMerge conflict preflight failed for secret-branch",
                "Rule custom.secret_rule: Rule violated: path /Users/private/secret.txt",
                "AUTOREMEDIABLE_VALIDATION_ERROR code=cargo_fmt_failed severity=transient\nfmt failed in /private/src/secret.rs",
                "Intent spec still has placeholder product outcome; capture explicit intent before implementation."
            ]
        },
        "readiness": null
    });
    let gate_diag = super::extract_safe_validation_diagnostic(
        serde_json::to_string(&gate_failure).unwrap().as_bytes(),
        b"",
    );
    assert_eq!(gate_diag.stage, Some("project_gates"));
    assert_eq!(gate_diag.scope, Some("publication_readiness"));
    assert_eq!(
        gate_diag.labels,
        vec![
            "PR_MERGE_CONFLICT",
            "VALIDATION_RULE_VIOLATED",
            "CARGO_FMT_FAILED",
            "LIVING_SPECS_INCOMPLETE"
        ]
    );
    assert!(!gate_diag.message.contains("secret"));
    assert!(!gate_diag.message.contains("custom.secret_rule"));

    let wp_failure = serde_json::json!({
        "success": false,
        "gate": "workspace_protection",
        "error": "Currently on protected branch 'secret-main'.",
        "is_protected": true
    });
    let wp_diag = super::extract_safe_validation_diagnostic(
        serde_json::to_string(&wp_failure).unwrap().as_bytes(),
        b"",
    );
    assert_eq!(wp_diag.stage, Some("workspace_protection"));
    assert_eq!(
        wp_diag.labels,
        vec!["WORKSPACE_PROTECTION", "GOVERNANCE_PROTECTED_BRANCH"]
    );
    assert!(!wp_diag.message.contains("secret-main"));
}

#[test]
fn scoped_validation_diagnostics_respect_token_boundaries_large_json_and_untrusted_notes() {
    let subtoken = super::extract_safe_validation_diagnostic(
        b"LAYA_READINESS_BLOCKED: private repository detail",
        b"",
    );
    assert_eq!(subtoken.labels, vec!["LAYA_READINESS_BLOCKED"]);
    assert!(!subtoken.labels.contains(&"READINESS_BLOCKED"));
    assert!(!subtoken.message.contains("private"));

    let big_notes = "note mentioning PROOF_HOOK_FAILED ".repeat(40_000);
    assert!(big_notes.len() > 1024 * 1024);
    let large_json = serde_json::json!({
        "status": "fail",
        "scope": "publication_readiness",
        "report": {
            "status": "pass",
            "fail_count": 0,
            "notes": [big_notes],
            "failures": []
        },
        "readiness": {
            "scope": "repository_local_readiness",
            "ready": false,
            "proof_ready": true,
            "blockers": [{
                "code": "READINESS_BLOCKED",
                "detail": "publication_readiness: ACCEPTED_REPOSITORY_CONTRACT_CHANGED: local checks changed"
            }]
        }
    });
    let large_bytes = serde_json::to_vec(&large_json).unwrap();
    assert!(large_bytes.len() > 1024 * 1024);
    let large_diag = super::extract_safe_validation_diagnostic(&large_bytes, b"");
    assert_eq!(large_diag.stage, Some("post_validation_readiness"));
    assert_eq!(
        large_diag.labels,
        vec!["ACCEPTED_REPOSITORY_CONTRACT_CHANGED", "READINESS_BLOCKED"]
    );
    assert!(!large_diag.labels.contains(&"PROOF_HOOK_FAILED"));

    let many_blockers = serde_json::json!({
        "status": "fail",
        "scope": "publication_readiness",
        "readiness": {
            "scope": "source_pr_readiness",
            "ready": false,
            "proof_ready": false,
            "blockers": [
                {"code": "READINESS_BLOCKED", "detail": "FINGERPRINT_ONLY_SPECS: x"},
                {"code": "STALE_SPEC_REVIEW", "detail": "x"},
                {"code": "GOVERNANCE_CHECKPOINT_MISSING", "detail": "x"},
                {"code": "PROOF_PROFILE_RECONCILIATION_REQUIRED", "detail": "x"},
                {"code": "ACCEPTED_REPOSITORY_CONTRACT_CHANGED", "detail": "x"},
                {"code": "CONTROLLER_CONTRACT_UNSUPPORTED", "detail": "x"},
                {"code": "SNAPSHOT_NOT_READY", "detail": "x"},
                {"code": "REQUIRED_PROOF_NOT_AUTHENTICATED", "detail": "x"},
                {"code": "SNAPSHOT_UPLOAD_PENDING", "detail": "x"}
            ]
        }
    });
    let capped = super::extract_safe_validation_diagnostic(
        serde_json::to_string(&many_blockers).unwrap().as_bytes(),
        b"",
    );
    assert!(capped.labels.len() <= 8);
    assert!(!capped.labels.contains(&"READINESS_BLOCKED"));
    assert!(capped.message.len() < 1024);
}

#[test]
fn diagnose_task_includes_todo_scoped_tmp_cleanup_when_ready_to_publish() {
    use crate::core::{todo, workspace};

    let task = todo::Task {
        id: "bugs_01m7r4tqkjg4a8nh".to_string(),
        hash: "01m7r4".to_string(),
        title: "Test #1798".to_string(),
        description: String::new(),
        tags: String::new(),
        owner: String::new(),
        due: None,
        r#ref: String::new(),
        status: "open".to_string(),
        created_at: "1795032963Z".to_string(),
        updated_at: "1795032963Z".to_string(),
        completed_at: None,
        closed_at: None,
        dir_path: ".".to_string(),
        scope: "root".to_string(),
        parent_task_id: None,
        priority: "medium".to_string(),
        depends_on: String::new(),
        blocks: String::new(),
        category: "bugs".to_string(),
        component: String::new(),
        assigned_to: "swarm-tmpclean-1798".to_string(),
        assigned_at: Some("1795032963Z".to_string()),
        owners: Vec::new(),
        comments: Vec::new(),
        one_shot: 0,
        revision: 3,
    };
    let ws = workspace::WorkspaceStatus {
        can_work: true,
        git: workspace::GitStatus {
            current_branch: "agent/swarm-tmpclean-1798/bugs_01m7r4tqkjg4a8nh-tmp-cleanup"
                .to_string(),
            is_protected: false,
            in_worktree: true,
            worktree_path: None,
            is_main_repo: false,
            has_local_mods: false,
        },
        container: workspace::ContainerStatus {
            in_container: false,
            container_id: None,
            image: None,
            docker_available: false,
        },
        blockers: Vec::new(),
        required_actions: Vec::new(),
        root_isolation: None,
        credentials: None,
    };

    // Before verification baseline exists, cleanup_guidance is null.
    let unverified = super::diagnose_task(&task, None, &ws, false, "master");
    assert!(unverified["cleanup_guidance"].is_null());
    assert_ne!(unverified["next_command"], "decapod workspace publish");

    // Once verification passes, next_command is `decapod workspace publish` and cleanup_guidance is populated.
    let artifacts = serde_json::json!({
        "completed_at": "1795033000Z",
        "task_id": "bugs_01m7r4tqkjg4a8nh",
        "task_revision": 3,
        "proof_plan_results": [
            {
                "proof_gate": "validate_passes",
                "command": "decapod validate",
                "status": "pass",
                "output_hash": "a".repeat(64)
            }
        ]
    });
    let target = super::VerifyTarget {
        todo_id: "bugs_01m7r4tqkjg4a8nh".to_string(),
        revision: 3,
        status: "open".to_string(),
        proof_plan: None,
        artifacts: Some(artifacts.to_string()),
        last_verified_at: Some("1795033000Z".to_string()),
        last_verified_status: Some("pass".to_string()),
        verification_policy_days: 90,
    };
    let ready = super::diagnose_task(&task, Some(&target), &ws, false, "master");
    assert_eq!(ready["next_command"], "decapod workspace publish");
    let guidance = ready["cleanup_guidance"]
        .as_str()
        .expect("cleanup_guidance must be present when ready to publish");
    assert!(guidance.contains("bugs_01m7r4tqkjg4a8nh"));
    assert!(guidance.contains("/tmp/decapod_validate/bugs_01m7r4tqkjg4a8nh"));
    assert!(guidance.contains("wildcard or root-level deletion of /tmp"));
    assert!(guidance.contains("/tmp/decapod"));
}

#[test]
fn scoped_validation_diagnostics_ignore_ttl_low_advisory_and_expose_execution_state_change() {
    let detail = "Validation error: EXECUTION_STATE_CHANGED: EXECUTION_POLICY_CHANGED: source, plan, intent, boundaries, policy or required approval changed; review the change and create a fresh checkpoint (`decapod govern workunit checkpoint --task-id code_01m7yeeq4kzbm5q8 --operation-id <id>` or `decapod govern trajectory checkpoint --run-id run_01 --checkpoint-id <id>`), private_path=/Users/secret/repo";
    let child_json = serde_json::json!({
        "status": "fail",
        "scope": "publication_readiness",
        "publication_ready": false,
        "report": {
            "status": "fail",
            "fail_count": 1,
            "failures": [
                format!("publication_readiness: READINESS_BLOCKED: {detail}")
            ]
        },
        "readiness": {
            "scope": "repository_local_readiness",
            "ready": false,
            "proof_ready": false,
            "blockers": [{
                "code": "READINESS_BLOCKED",
                "detail": detail
            }]
        }
    });
    let stderr_warning = b"VALIDATION_LEASE_TTL_LOW: remaining task claim lease (3590s) for 'code_01m7yeeq4kzbm5q8' is shorter than the validation budget (7500s); validation continues without mutating custody, but a lease that expires mid-run fails with VALIDATION_HISTORY_TASK_STALE; run `decapod todo renew --id code_01m7yeeq4kzbm5q8 --lease-seconds 7500` and refresh any workunit/governance checkpoint before running validation\n";

    let diag = super::extract_safe_validation_diagnostic(
        serde_json::to_string(&child_json).unwrap().as_bytes(),
        stderr_warning,
    );
    assert_eq!(diag.stage, Some("readiness"));
    assert_eq!(diag.scope, Some("repository_local_readiness"));
    assert_eq!(
        diag.labels,
        vec![
            "EXECUTION_POLICY_CHANGED",
            "EXECUTION_STATE_CHANGED",
            "READINESS_BLOCKED"
        ]
    );
    assert!(
        !diag.labels.contains(&"VALIDATION_HISTORY_TASK_STALE"),
        "TTL advisory prose on stderr must never inject VALIDATION_HISTORY_TASK_STALE: {diag:?}"
    );
    assert!(!diag.message.contains("/Users/secret/repo"));
    assert!(diag.message.len() < 1024);

    // Non-JSON fallback must also filter VALIDATION_LEASE_TTL_LOW advisory lines.
    let non_json_diag = super::extract_safe_validation_diagnostic(
        b"I/O error: Validation error: EXECUTION_STATE_CHANGED: EXECUTION_TASK_CUSTODY_CHANGED: lease rotated secret=hidden",
        stderr_warning,
    );
    assert!(
        !non_json_diag
            .labels
            .contains(&"VALIDATION_HISTORY_TASK_STALE"),
        "non-JSON fallback must ignore VALIDATION_LEASE_TTL_LOW prose: {non_json_diag:?}"
    );
    assert!(
        non_json_diag
            .labels
            .contains(&"EXECUTION_TASK_CUSTODY_CHANGED"),
        "{non_json_diag:?}"
    );
    assert!(
        !non_json_diag.message.contains("hidden"),
        "{non_json_diag:?}"
    );
}

#[test]
fn scoped_validation_diagnostics_distinguish_all_task_history_and_unbound_preconditions() {
    let stale_cases = [
        (
            "TASK_PROOF_PLAN_MALFORMED",
            "malformed declared task proof plan",
        ),
        ("TASK_PROOF_PLAN_EMPTY", "empty declared task proof plan"),
        (
            "TASK_HISTORY_CHECKPOINT_MISSING",
            "missing canonical task history",
        ),
        (
            "TASK_LIVE_JOURNAL_DRIFT",
            "live task differs from its journal",
        ),
        (
            "TASK_CHANGED_DURING_VALIDATION",
            "task changed during validation",
        ),
        (
            "TASK_LEASE_EXPIRED_OR_RELEASED",
            "live validation custody expired or was released before archival",
        ),
        (
            "TASK_ADVANCED_DURING_VALIDATION",
            "task advanced during validation",
        ),
        (
            "TASK_CAPTURED_IDENTITY_MISMATCH",
            "captured task identity differs",
        ),
        (
            "TASK_CAPTURED_CHECKPOINT_ABSENT",
            "captured checkpoint is absent",
        ),
        (
            "TASK_CHRONOLOGY_RESTORED_OR_GAPPED",
            "restored or incomplete chronology requires new proof",
        ),
        (
            "TASK_LIFECYCLE_CUSTODY_CHANGED",
            "task intent, dependency, custody or chronology changed",
        ),
        ("TASK_CUSTODY_REACQUIRED", "released custody was reacquired"),
        (
            "TASK_ARCHIVED_REQUIRES_HISTORY",
            "archived tasks require their existing completion history",
        ),
    ];

    for (subcode, detail) in stale_cases {
        let blocker_detail = format!(
            "Validation error: VALIDATION_HISTORY_TASK_STALE: {subcode}: {detail}; secret=/private/db"
        );
        let doc = serde_json::json!({
            "status": "fail",
            "scope": "publication_readiness",
            "report": {
                "status": "fail",
                "fail_count": 1,
                "failures": [format!("publication_readiness: READINESS_BLOCKED: {blocker_detail}")]
            },
            "readiness": {
                "scope": "repository_local_readiness",
                "ready": false,
                "proof_ready": false,
                "blockers": [{
                    "code": "READINESS_BLOCKED",
                    "detail": blocker_detail
                }]
            }
        });
        let diag = super::extract_safe_validation_diagnostic(
            serde_json::to_string(&doc).unwrap().as_bytes(),
            b"",
        );
        assert_eq!(diag.stage, Some("readiness"), "subcode={subcode}");
        assert_eq!(
            diag.scope,
            Some("repository_local_readiness"),
            "subcode={subcode}"
        );
        assert_eq!(
            diag.labels,
            vec![
                subcode,
                "VALIDATION_HISTORY_TASK_STALE",
                "READINESS_BLOCKED"
            ],
            "subcode={subcode}"
        );
        assert!(!diag.message.contains("/private/db"));
        assert!(diag.message.len() < 1024);
    }

    let unbound_cases = [
        ("TASK_MISSING", "missing task"),
        (
            "TASK_HISTORY_CHECKPOINT_MISSING",
            "no complete canonical task checkpoint",
        ),
        (
            "TASK_LIVE_JOURNAL_DRIFT",
            "live task differs from its journal",
        ),
        (
            "TASK_EXCLUSIVE_CUSTODY_REQUIRED",
            "validation requires current exclusive task custody",
        ),
    ];
    for (subcode, detail) in unbound_cases {
        let blocker_detail = format!(
            "I/O error: Validation error: VALIDATION_HISTORY_TASK_UNBOUND: {subcode}: {detail}; path=/tmp/secret"
        );
        let doc = serde_json::json!({
            "status": "fail",
            "scope": "publication_readiness",
            "readiness": {
                "scope": "source_pr_readiness",
                "ready": false,
                "proof_ready": false,
                "blockers": [{
                    "code": "READINESS_BLOCKED",
                    "detail": blocker_detail
                }]
            }
        });
        let diag = super::extract_safe_validation_diagnostic(
            serde_json::to_string(&doc).unwrap().as_bytes(),
            b"",
        );
        assert_eq!(diag.stage, Some("readiness"), "subcode={subcode}");
        assert_eq!(diag.scope, Some("source_pr_readiness"), "subcode={subcode}");
        assert_eq!(
            diag.labels,
            vec![
                subcode,
                "VALIDATION_HISTORY_TASK_UNBOUND",
                "READINESS_BLOCKED"
            ],
            "subcode={subcode}"
        );
        assert!(!diag.message.contains("/tmp/secret"));
    }

    let incomplete_journal = serde_json::json!({
        "status": "fail",
        "scope": "publication_readiness",
        "readiness": {
            "scope": "repository_local_readiness",
            "ready": false,
            "proof_ready": false,
            "blockers": [{
                "code": "READINESS_BLOCKED",
                "detail": "Validation error: TASK_JOURNAL_INCOMPLETE_HISTORY: uncheckpointed domain-event suffix for code_secret"
            }]
        }
    });
    let diag = super::extract_safe_validation_diagnostic(
        serde_json::to_string(&incomplete_journal)
            .unwrap()
            .as_bytes(),
        b"",
    );
    assert_eq!(
        diag.labels,
        vec!["TASK_JOURNAL_INCOMPLETE_HISTORY", "READINESS_BLOCKED"]
    );
    assert!(!diag.message.contains("code_secret"));
}

#[test]
fn scoped_validation_diagnostics_preserve_incompatible_native_executable_and_platform_codes() {
    // 1. Current runtime emits typed INCOMPATIBLE_NATIVE_EXECUTABLE alongside #1544 check/path detail.
    let den_typed = serde_json::json!({
        "status": "fail",
        "scope": "publication_readiness",
        "publication_ready": false,
        "report": {
            "status": "fail",
            "fail_count": 1,
            "failures": [
                "publication_readiness: INCOMPATIBLE_NATIVE_EXECUTABLE: fmt: /nix/store/secret-den-hash-alejandra-4.1.0/bin/alejandra is Mach-O arm64; native Linux validation requires ELF x86_64"
            ]
        },
        "readiness": {
            "scope": "repository_local_readiness",
            "ready": false,
            "proof_ready": false,
            "blockers": [{
                "code": "INCOMPATIBLE_NATIVE_EXECUTABLE",
                "detail": "fmt: /nix/store/secret-den-hash-alejandra-4.1.0/bin/alejandra is Mach-O arm64; native Linux validation requires ELF x86_64"
            }]
        }
    });
    let typed_diag = super::extract_safe_validation_diagnostic(
        serde_json::to_string(&den_typed).unwrap().as_bytes(),
        b"",
    );
    assert_eq!(typed_diag.stage, Some("readiness"));
    assert_eq!(typed_diag.scope, Some("repository_local_readiness"));
    assert_eq!(typed_diag.labels, vec!["INCOMPATIBLE_NATIVE_EXECUTABLE"]);
    assert!(
        typed_diag
            .message
            .contains("INCOMPATIBLE_NATIVE_EXECUTABLE")
    );
    assert!(!typed_diag.message.contains("/nix/store"));
    assert!(!typed_diag.message.contains("alejandra"));
    assert!(typed_diag.message.len() < 1024);

    // 2. Legacy/wrapped READINESS_BLOCKED carrying #1544 executable-format text still surfaces INCOMPATIBLE_NATIVE_EXECUTABLE first.
    let den_wrapped = serde_json::json!({
        "status": "fail",
        "scope": "publication_readiness",
        "publication_ready": false,
        "report": {
            "status": "fail",
            "fail_count": 1,
            "failures": [
                "publication_readiness: READINESS_BLOCKED: shellcheck: /nix/store/secret-den-hash-shellcheck/bin/shellcheck is ELF x86_64 (64-bit); native macOS validation requires Mach-O arm64"
            ]
        },
        "readiness": {
            "scope": "repository_local_readiness",
            "ready": false,
            "proof_ready": false,
            "blockers": [{
                "code": "READINESS_BLOCKED",
                "detail": "shellcheck: /nix/store/secret-den-hash-shellcheck/bin/shellcheck is ELF x86_64 (64-bit); native macOS validation requires Mach-O arm64"
            }]
        }
    });
    let wrapped_diag = super::extract_safe_validation_diagnostic(
        serde_json::to_string(&den_wrapped).unwrap().as_bytes(),
        b"",
    );
    assert_eq!(wrapped_diag.stage, Some("readiness"));
    assert_eq!(wrapped_diag.scope, Some("repository_local_readiness"));
    assert_eq!(
        wrapped_diag.labels,
        vec!["INCOMPATIBLE_NATIVE_EXECUTABLE", "READINESS_BLOCKED"]
    );
    assert!(!wrapped_diag.message.contains("/nix/store"));
    assert!(!wrapped_diag.message.contains("shellcheck"));

    // 3. Per-platform contract selection blockers also survive scoped recovery cleanly.
    let platform_doc = serde_json::json!({
        "status": "fail",
        "scope": "publication_readiness",
        "readiness": {
            "scope": "repository_local_readiness",
            "ready": false,
            "proof_ready": false,
            "blockers": [
                {
                    "code": "REPOSITORY_COMMAND_CONTRACT",
                    "detail": "MISSING_PLATFORM_EXECUTION: no reviewed execution entry for host platform linux-x86_64"
                },
                {
                    "code": "REPOSITORY_COMMAND_CONTRACT",
                    "detail": "INCOMPATIBLE_TARGET_EXECUTOR: deployment target \"macos-only\" requires platform [\"macos-aarch64\"]"
                }
            ]
        }
    });
    let platform_diag = super::extract_safe_validation_diagnostic(
        serde_json::to_string(&platform_doc).unwrap().as_bytes(),
        b"",
    );
    assert_eq!(
        platform_diag.labels,
        vec![
            "MISSING_PLATFORM_EXECUTION",
            "REPOSITORY_COMMAND_CONTRACT",
            "INCOMPATIBLE_TARGET_EXECUTOR"
        ]
    );
    assert!(!platform_diag.message.contains("macos-only"));
}
