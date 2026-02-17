use decapod::core::assurance::{AssuranceEngine, AssuranceEvaluateInput, AssurancePhase};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn init_repo(root: &Path, branch: &str) {
    Command::new("git")
        .args(["init", "-b", branch])
        .current_dir(root)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(root)
        .output()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(root)
        .output()
        .expect("git config name");
}

fn seed_docs(root: &Path) {
    fs::create_dir_all(root.join("docs/decisions")).expect("docs dir");
    fs::create_dir_all(root.join(".decapod/generated")).expect("generated dir");
    fs::write(
        root.join("docs/spec.md"),
        "## Auth\nUse provider abstraction.\n## Verify\nRun validate.\n",
    )
    .expect("spec");
    fs::write(
        root.join("docs/architecture.md"),
        "## Services\nService boundaries are explicit.\n",
    )
    .expect("architecture");
    fs::write(
        root.join("docs/security.md"),
        "## Secrets\nNever hardcode.\n",
    )
    .expect("security");
    fs::write(root.join("docs/ops.md"), "## Runbook\nDocument runbook.\n").expect("ops");
}

#[test]
fn assurance_reconciliations_are_deterministic_for_same_input() {
    let tmp = tempdir().expect("temp");
    init_repo(tmp.path(), "feature/x");
    seed_docs(tmp.path());
    fs::write(
        tmp.path().join("docs/decisions/ADR-010-auth.md"),
        "# ADR Auth\nmust use auth_provider\n",
    )
    .expect("adr");

    let engine = AssuranceEngine::new(tmp.path());
    let input = AssuranceEvaluateInput {
        op: "build".to_string(),
        params: serde_json::json!({"auth_provider":"oauth"}),
        touched_paths: vec!["src/auth/mod.rs".to_string()],
        diff_summary: Some("auth updates".to_string()),
        session_id: Some("s1".to_string()),
        phase: Some(AssurancePhase::Build),
        time_budget_s: Some(60),
    };

    let a = engine.evaluate(&input).expect("first eval");
    let b = engine.evaluate(&input).expect("second eval");
    assert_eq!(
        a.advisory
            .reconciliations
            .must
            .iter()
            .map(|p| &p.r#ref)
            .collect::<Vec<_>>(),
        b.advisory
            .reconciliations
            .must
            .iter()
            .map(|p| &p.r#ref)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        a.advisory
            .reconciliations
            .recommended
            .iter()
            .map(|p| &p.r#ref)
            .collect::<Vec<_>>(),
        b.advisory
            .reconciliations
            .recommended
            .iter()
            .map(|p| &p.r#ref)
            .collect::<Vec<_>>()
    );
}

#[test]
fn assurance_reconciliation_caps_never_exceed_five() {
    let tmp = tempdir().expect("temp");
    init_repo(tmp.path(), "feature/x");
    seed_docs(tmp.path());
    for i in 0..20 {
        fs::write(
            tmp.path().join(format!("docs/decisions/ADR-{i:03}.md")),
            format!("# ADR {i}\nservice_{i}\n"),
        )
        .expect("adr");
    }

    let engine = AssuranceEngine::new(tmp.path());
    let out = engine
        .evaluate(&AssuranceEvaluateInput {
            op: "build".to_string(),
            params: serde_json::json!({}),
            touched_paths: vec!["src/service.rs".to_string()],
            diff_summary: None,
            session_id: None,
            phase: Some(AssurancePhase::Build),
            time_budget_s: None,
        })
        .expect("eval");
    assert!(out.advisory.reconciliations.must.len() <= 5);
    assert!(out.advisory.reconciliations.recommended.len() <= 5);
}

#[test]
fn contradictions_trigger_decision_interlock_and_adr_reconciliation() {
    let tmp = tempdir().expect("temp");
    init_repo(tmp.path(), "feature/x");
    seed_docs(tmp.path());
    fs::write(
        tmp.path().join("docs/decisions/ADR-020-auth.md"),
        "# ADR Auth\nmust use src/auth\n",
    )
    .expect("adr");

    let engine = AssuranceEngine::new(tmp.path());
    let out = engine
        .evaluate(&AssuranceEvaluateInput {
            op: "modify-auth".to_string(),
            params: serde_json::json!({}),
            touched_paths: vec!["src/auth/provider.rs".to_string()],
            diff_summary: None,
            session_id: None,
            phase: Some(AssurancePhase::Plan),
            time_budget_s: None,
        })
        .expect("eval");

    assert_eq!(
        out.interlock.as_ref().map(|i| i.code.as_str()),
        Some("decision_required")
    );
    assert!(
        out.advisory
            .reconciliations
            .must
            .iter()
            .any(|p| p.kind == "adr")
    );
}

#[test]
fn completion_phase_requires_verification_proofs() {
    let tmp = tempdir().expect("temp");
    init_repo(tmp.path(), "feature/x");
    seed_docs(tmp.path());
    let engine = AssuranceEngine::new(tmp.path());
    let out = engine
        .evaluate(&AssuranceEvaluateInput {
            op: "complete".to_string(),
            params: serde_json::json!({}),
            touched_paths: vec!["src/lib.rs".to_string()],
            diff_summary: None,
            session_id: None,
            phase: Some(AssurancePhase::Complete),
            time_budget_s: None,
        })
        .expect("eval");
    assert_eq!(
        out.interlock.as_ref().map(|i| i.code.as_str()),
        Some("verification_required")
    );
}

#[test]
fn protected_branch_or_workspace_state_triggers_workspace_interlock() {
    let tmp = tempdir().expect("temp");
    init_repo(tmp.path(), "master");
    seed_docs(tmp.path());
    let engine = AssuranceEngine::new(tmp.path());
    let out = engine
        .evaluate(&AssuranceEvaluateInput {
            op: "build".to_string(),
            params: serde_json::json!({"auth_provider":"oauth"}),
            touched_paths: vec!["src/lib.rs".to_string()],
            diff_summary: None,
            session_id: None,
            phase: Some(AssurancePhase::Build),
            time_budget_s: None,
        })
        .expect("eval");
    assert_eq!(
        out.interlock.as_ref().map(|i| i.code.as_str()),
        Some("workspace_required")
    );
}
