// Moved from src/decapod/core/validate.rs
use super::{
    SurfaceKind, VALIDATION_RECEIPT_PATH, ValidationContext, advisory, fail,
    is_allowed_non_code_path, is_decapod_isolated_worktree, is_non_code_path, note, pass,
    predict_ci_outcome, skip, strip_git_quotes, validate_control_plane_contract,
    validate_git_workspace_context, validate_publication_bundle_currency,
    validate_root_dockerfile_seed_detection, validate_spec_drift, validate_watcher_audit, warn,
};
use super::{is_protected_git_branch, parse_ahead_behind_counts};
use crate::core::events;
use crate::core::store::{Store, StoreKind};
use crate::core::{research_claims, trajectory};
use crate::plan_governance::{self, GovernedPlan, PlanState, ScopeConstraints};
use std::fs;
use std::path::Path;
use std::sync::atomic::Ordering;
use tempfile::tempdir;

fn git_init(dir: &std::path::Path) {
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(dir)
        .output()
        .expect("git init");
    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir)
        .output()
        .expect("git config email");
    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir)
        .output()
        .expect("git config name");
}

fn git_commit(dir: &std::path::Path, msg: &str) {
    fs::write(dir.join(".gitkeep"), "").expect("write gitkeep");
    std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(dir)
        .output()
        .expect("git add");
    std::process::Command::new("git")
        .args(["commit", "-m", msg])
        .current_dir(dir)
        .output()
        .expect("git commit");
}

#[test]
fn watcher_audit_gate_observes_canonical_sqlite_event() {
    let dir = tempdir().expect("tempdir");
    let store = Store {
        kind: StoreKind::Repo,
        root: dir.path().to_path_buf(),
    };
    events::append(
        &store.root,
        events::WATCHER,
        &serde_json::json!({
            "event_id": "watcher-validation",
            "ts": "1785620000Z",
            "event_type": "watcher.run",
            "actor": "watcher"
        }),
    )
    .expect("append canonical watcher event");
    let ctx = ValidationContext::new();
    validate_watcher_audit(&store, &ctx).expect("watcher audit gate");
    assert_eq!(ctx.warn_count.load(Ordering::Relaxed), 0);
    assert_eq!(ctx.pass_count.load(Ordering::Relaxed), 1);
    assert!(!dir.path().join("watcher.events.jsonl").exists());
}

#[test]
fn healthy_spec_drift_gate_emits_no_warning() {
    let dir = tempdir().expect("tempdir");
    let specs = dir.path().join(".decapod/managed/specs");
    fs::create_dir_all(&specs).expect("create specs");
    for (name, body) in [
        (
            "INTERFACES.md",
            "# Interfaces\n## Inbound Contracts\n## Data Ownership\n",
        ),
        (
            "SEMANTICS.md",
            "# Semantics\n## State Machines\n## Invariants\n",
        ),
        (
            "OPERATIONS.md",
            "# Operations\n## Service Level Objectives\n## Monitoring\n## Incident Response\n",
        ),
        (
            "SECURITY.md",
            "# Security\n## Threat Model\n## Authentication\n## Authorization\n## Data Classification\n",
        ),
    ] {
        fs::write(specs.join(name), body).expect("write healthy spec");
    }

    let ctx = ValidationContext::new();
    validate_spec_drift(&ctx, dir.path()).expect("spec drift gate");
    assert_eq!(ctx.warn_count.load(Ordering::Relaxed), 0);
    assert_eq!(ctx.fail_count.load(Ordering::Relaxed), 0);
}

#[test]
fn empty_task_projection_does_not_claim_missing_canonical_evidence() {
    let dir = tempdir().expect("tempdir");
    let store = Store {
        kind: StoreKind::Repo,
        root: dir.path().to_path_buf(),
    };
    events::append(
        &store.root,
        events::BROKER,
        &serde_json::json!({
            "event_id": "broker-validation",
            "ts": "1785620000Z",
            "event_type": "broker.request",
            "actor": "test"
        }),
    )
    .expect("append canonical broker event");
    let db_path = store.root.join(crate::core::schemas::LOCAL_DB_NAME);
    let conn = crate::core::db::db_connect(&db_path.to_string_lossy()).expect("open store");
    conn.execute_batch("CREATE TABLE IF NOT EXISTS tasks (id TEXT PRIMARY KEY)")
        .expect("create empty task projection");

    let ctx = ValidationContext::new();
    validate_control_plane_contract(&store, &ctx).expect("control plane gate");
    assert_eq!(ctx.fail_count.load(Ordering::Relaxed), 0);
}

#[test]
fn test_validate_root_dockerfile_seed_detection() {
    let tmp = tempdir().expect("temporary repository");
    fs::write(
            tmp.path().join("Dockerfile"),
            "ARG DECAPOD_IMAGE=ghcr.io/decapodlabs/decapod:v0.72.13\nFROM $DECAPOD_IMAGE\nLABEL org.decapod.managed=\"workspace\"\n",
        )
        .expect("write root Dockerfile");

    let ctx = ValidationContext::new();
    validate_root_dockerfile_seed_detection(&ctx, tmp.path())
        .expect("root Dockerfile validation should complete");

    assert_eq!(ctx.fail_count.load(Ordering::Relaxed), 1);
    assert!(ctx.fails.lock().unwrap().iter().any(|failure| {
        failure.contains("Root Dockerfile contains Decapod workspace image markers")
    }));
}

fn decapod_worktree_fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = tempdir().expect("tempdir");
    let main_root = tmp.path().join("main");
    fs::create_dir_all(&main_root).expect("create main");
    git_init(&main_root);
    git_commit(&main_root, "init");

    let wt_path = main_root
        .join(".decapod")
        .join("workspaces")
        .join("agent-test-task");
    fs::create_dir_all(wt_path.parent().unwrap()).expect("create workspaces dir");
    let output = std::process::Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            "agent/test/task",
            wt_path.to_str().unwrap(),
        ])
        .current_dir(&main_root)
        .output()
        .expect("git worktree add");
    assert!(
        output.status.success(),
        "git worktree add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    (tmp, main_root, wt_path)
}

#[test]
fn protected_branch_matching_is_limited_to_protected_refs() {
    assert!(is_protected_git_branch("master"));
    assert!(is_protected_git_branch("main"));
    assert!(is_protected_git_branch("release/2026.05"));
    assert!(!is_protected_git_branch("agent/codex/fix"));
    assert!(!is_protected_git_branch("feature/main-cleanup"));
}

#[test]
fn parses_git_ahead_behind_counts() {
    assert_eq!(parse_ahead_behind_counts("3\t1\n"), Some((3, 1)));
    assert_eq!(parse_ahead_behind_counts("0 12"), Some((0, 12)));
    assert_eq!(parse_ahead_behind_counts("bad 12"), None);
}

#[test]
fn ci_prediction_preserves_actionable_validation_signals() {
    let failures = vec!["gate failed".to_string()];
    let warnings = vec!["review this warning".to_string()];

    let failed = predict_ci_outcome(1, 1, &failures, &warnings);
    assert_eq!(failed.result, "fail");
    assert_eq!(failed.confidence, "high");
    assert_eq!(failed.reasons, failures);
    assert!(!failed.recommendations.is_empty());

    let review = predict_ci_outcome(0, 1, &[], &warnings);
    assert_eq!(review.result, "review");
    assert_eq!(review.confidence, "medium");
    assert_eq!(review.reasons, warnings);

    let passing = predict_ci_outcome(0, 0, &[], &[]);
    assert_eq!(passing.result, "pass");
    assert_eq!(passing.confidence, "high");
}

#[test]
fn isolated_worktree_detection_uses_owning_main_repo() {
    let (_tmp, main_root, wt_path) = decapod_worktree_fixture();

    assert!(is_decapod_isolated_worktree(&main_root, &wt_path));
    assert!(
        is_decapod_isolated_worktree(&wt_path, &wt_path),
        "validation can pass the worktree as main_root; detection must still resolve the owner repo"
    );
}

#[test]
fn issue_757_workspace_status_and_validate_agree_on_decapod_worktree() {
    let (_tmp, _main_root, wt_path) = decapod_worktree_fixture();
    let ctx = ValidationContext::new();

    let status = crate::workspace::get_workspace_status(&wt_path)
        .expect("workspace status should resolve the isolated worktree");
    assert!(status.git.in_worktree);
    assert!(!status.git.is_main_repo);
    assert!(status.can_work);

    validate_git_workspace_context(&ctx, &wt_path, &wt_path).expect("workspace context validation");

    assert_eq!(
        ctx.fail_count.load(Ordering::Relaxed),
        0,
        "unexpected validation failures: {:?}",
        ctx.fails.lock().unwrap()
    );
}

#[test]
fn external_worktree_reports_context_mismatch_instead_of_generic_failure() {
    let (_tmp, main_root, _decapod_path) = decapod_worktree_fixture();
    let external_path = main_root
        .parent()
        .expect("fixture parent")
        .join("external-worktree");
    let output = std::process::Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            "external/agent",
            external_path.to_str().unwrap(),
        ])
        .current_dir(&main_root)
        .output()
        .expect("external worktree should start");
    assert!(output.status.success());

    let ctx = ValidationContext::new();
    validate_git_workspace_context(&ctx, &main_root, &external_path)
        .expect("workspace context validation");
    let failures = ctx.fails.lock().unwrap();
    assert_eq!(ctx.fail_count.load(Ordering::Relaxed), 1);
    assert!(failures.iter().any(|failure| {
        failure.contains("workspace_context_mismatch") && failure.contains("not Decapod-owned")
    }));
}

// --- is_allowed_non_code_path ---

#[test]
fn non_code_allows_docs_directory() {
    assert!(is_allowed_non_code_path("docs/agent/api-index.md"));
    assert!(is_allowed_non_code_path("docs/sub/deep/file.txt"));
    assert!(!is_allowed_non_code_path("src/main.rs"));
}

#[test]
fn non_code_allows_markdown_anywhere() {
    // .md extension is always allowed regardless of directory
    assert!(is_allowed_non_code_path("README.md"));
    assert!(is_allowed_non_code_path("docs/guide.md"));
    assert!(is_allowed_non_code_path("sub/dir/notes.md"));
    assert!(is_allowed_non_code_path("src/lib.md"));
    assert!(!is_allowed_non_code_path("src/lib.rs"));
}

#[test]
fn non_code_allows_decapod_config_and_override() {
    assert!(is_allowed_non_code_path(".decapod/config.toml"));
    assert!(is_allowed_non_code_path(".decapod/OVERRIDE.md"));
    assert!(!is_allowed_non_code_path(".decapod/other.toml"));
    assert!(!is_allowed_non_code_path(".decapod/data/store.jsonl"));
}

#[test]
fn non_code_allows_generated_specs_and_artifacts() {
    assert!(is_allowed_non_code_path(".decapod/managed/specs/INTENT.md"));
    assert!(is_allowed_non_code_path(
        ".decapod/managed/artifacts/plan.json"
    ));
    assert!(is_allowed_non_code_path(
        ".decapod/managed/specs/deep/nested.md"
    ));
    // .md files always pass via the ends_with(".md") rule, so this is allowed too
    assert!(is_allowed_non_code_path(".decapod/managed/other.md"));
    // A non-.md file in generated/ but outside specs/ or artifacts/ is rejected
    assert!(!is_allowed_non_code_path(".decapod/managed/other.json"));
}

#[test]
fn non_code_allows_contracts() {
    assert!(is_allowed_non_code_path(".decapod/contracts/interface.md"));
    assert!(!is_allowed_non_code_path(".decapod/contracts")); // no trailing slash/file
}

#[test]
fn non_code_rejects_source_files() {
    assert!(!is_allowed_non_code_path("src/lib.rs"));
    assert!(!is_allowed_non_code_path("lib/python/main.py"));
    assert!(!is_allowed_non_code_path("package.json"));
    assert!(!is_allowed_non_code_path("Cargo.toml"));
}

// --- is_non_code_path (porcelain line parsing) ---

#[test]
fn non_code_path_ordinary_modified() {
    assert!(is_non_code_path(" M docs/guide.md"));
    assert!(is_non_code_path("M  .decapod/config.toml"));
    assert!(!is_non_code_path(" M src/main.rs"));
}

#[test]
fn non_code_path_short_line_rejected() {
    assert!(!is_non_code_path(" M ")); // too short
    assert!(!is_non_code_path(""));
}

#[test]
fn non_code_path_rename_both_sides_must_qualify() {
    assert!(is_non_code_path("R  docs/old.md -> docs/new.md"));
    assert!(!is_non_code_path("R  docs/old.md -> src/new.rs"));
    assert!(!is_non_code_path("R  src/old.rs -> docs/new.md"));
}

#[test]
fn non_code_path_copy_both_sides_must_qualify() {
    assert!(is_non_code_path("C  docs/template.md -> docs/copy.md"));
    assert!(!is_non_code_path("C  docs/template.md -> src/copy.rs"));
}

// --- strip_git_quotes ---

#[test]
fn strip_quotes_removes_surrounding_double_quotes() {
    assert_eq!(strip_git_quotes("\"weird file.md\""), "weird file.md");
    assert_eq!(strip_git_quotes("normal.md"), "normal.md");
    assert_eq!(strip_git_quotes("\"\""), "");
}

#[test]
fn non_code_path_quoted_filename() {
    assert!(is_non_code_path(" M \"weird name.md\""));
    assert!(!is_non_code_path(" M \"weird name.rs\""));
}

#[test]
fn surface_kind_variants_are_correct() {
    assert_eq!(SurfaceKind::Authority, SurfaceKind::Authority);
    assert_eq!(SurfaceKind::Evidence, SurfaceKind::Evidence);
    assert_eq!(SurfaceKind::Projection, SurfaceKind::Projection);
    assert_ne!(SurfaceKind::Authority, SurfaceKind::Projection);
    assert_ne!(SurfaceKind::Evidence, SurfaceKind::Projection);
}

#[test]
fn surface_kind_serialization() {
    use serde_json::json;
    let authority = json!(SurfaceKind::Authority);
    let evidence = json!(SurfaceKind::Evidence);
    let projection = json!(SurfaceKind::Projection);
    assert_eq!(authority, "Authority");
    assert_eq!(evidence, "Evidence");
    assert_eq!(projection, "Projection");
}

#[test]
fn typed_result_vocabulary_clean_run_has_zero_warn() {
    let ctx = ValidationContext::new();
    pass("clean pass", &ctx);
    note("methodology note only", &ctx);
    advisory("non-blocking advisory", &ctx);
    skip("optional surface absent", &ctx);
    assert_eq!(ctx.pass_count.load(Ordering::Relaxed), 1);
    assert_eq!(ctx.skip_count.load(Ordering::Relaxed), 1);
    assert_eq!(ctx.note_count.load(Ordering::Relaxed), 1);
    assert_eq!(ctx.advisory_count.load(Ordering::Relaxed), 1);
    assert_eq!(ctx.warn_count.load(Ordering::Relaxed), 0);
    assert_eq!(ctx.fail_count.load(Ordering::Relaxed), 0);
}

#[test]
fn typed_result_vocabulary_advisory_and_note_do_not_mutate_warnings() {
    let ctx = ValidationContext::new();
    note("informational methodology", &ctx);
    advisory("review recommended", &ctx);
    note("another note", &ctx);
    assert_eq!(ctx.warn_count.load(Ordering::Relaxed), 0);
    assert!(ctx.warns.lock().unwrap().is_empty());
    assert_eq!(ctx.note_count.load(Ordering::Relaxed), 2);
    assert_eq!(ctx.advisory_count.load(Ordering::Relaxed), 1);
    assert_eq!(ctx.notes.lock().unwrap().len(), 2);
    assert_eq!(ctx.advisories.lock().unwrap().len(), 1);
}

#[test]
fn typed_result_vocabulary_condition_warning_and_blocking_failure() {
    let ctx = ValidationContext::new();
    warn("condition-specific warning", &ctx);
    fail("blocking finding", &ctx);
    assert_eq!(ctx.warn_count.load(Ordering::Relaxed), 1);
    assert_eq!(ctx.fail_count.load(Ordering::Relaxed), 1);
    assert_eq!(
        ctx.warns.lock().unwrap().clone(),
        vec!["condition-specific warning".to_string()]
    );
    assert_eq!(
        ctx.fails.lock().unwrap().clone(),
        vec!["blocking finding".to_string()]
    );
    // notes/advisories stay orthogonal
    assert_eq!(ctx.note_count.load(Ordering::Relaxed), 0);
    assert_eq!(ctx.advisory_count.load(Ordering::Relaxed), 0);
}

#[test]
fn typed_result_vocabulary_skip_is_not_pass() {
    let ctx = ValidationContext::new();
    skip("skipped gate", &ctx);
    assert_eq!(ctx.skip_count.load(Ordering::Relaxed), 1);
    assert_eq!(ctx.pass_count.load(Ordering::Relaxed), 0);
    assert_eq!(ctx.warn_count.load(Ordering::Relaxed), 0);
}

#[test]
fn skip_fingerprint_gates_requires_truthy_value() {
    // Empty string is what GitHub Actions emits for `false && '1' || ''`.
    // Presence alone must not bypass fingerprint enforcement on PRs.
    unsafe {
        std::env::remove_var("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES");
    }
    assert!(!super::skip_fingerprint_gates());

    unsafe {
        std::env::set_var("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES", "");
    }
    assert!(!super::skip_fingerprint_gates());

    unsafe {
        std::env::set_var("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES", "0");
    }
    assert!(!super::skip_fingerprint_gates());

    unsafe {
        std::env::set_var("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES", "1");
    }
    assert!(super::skip_fingerprint_gates());

    unsafe {
        std::env::remove_var("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES");
    }
}

fn git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn seed_publication_bundle(dir: &Path) {
    for surface in ["AGENTS.md", "CLAUDE.md", "CODEX.md", "GEMINI.md"] {
        fs::write(
            dir.join(surface),
            format!(
                "<!-- decapod-release: {} -->\n<!-- decapod-fingerprint: deadbeef -->\n# {surface}\n",
                env!("CARGO_PKG_VERSION")
            ),
        )
        .expect("entrypoint");
    }
    let docker = dir.join(".decapod/managed/Dockerfile.decapod");
    fs::create_dir_all(docker.parent().unwrap()).expect("managed");
    fs::write(
        &docker,
        format!(
            "# Generated by decapod container profile\nARG DECAPOD_VERSION={}\n",
            env!("CARGO_PKG_VERSION")
        ),
    )
    .expect("dockerfile");
    let specs = dir.join(".decapod/managed/specs");
    fs::create_dir_all(&specs).expect("specs");
    fs::write(specs.join("INTENT.md"), "# Intent\n\nBase.\n").expect("intent");
    fs::write(specs.join(".manifest.json"), "{}\n").expect("manifest");

    research_claims::ensure_template(dir, false).expect("claims template");
    let plan = GovernedPlan {
        schema_version: "1.0.0".to_string(),
        title: "fixture".to_string(),
        intent: "fixture intent".to_string(),
        state: PlanState::Approved,
        todo_ids: vec!["test_01fixture".to_string()],
        proof_hooks: vec![],
        unknowns: vec![],
        human_questions: vec![],
        stop_conditions: vec![],
        unresolved_contradictions: vec![],
        deferred_questions: vec![],
        constraints: ScopeConstraints::default(),
        phases: vec![],
        updated_at: "1780000000Z".to_string(),
    };
    plan_governance::save_plan(dir, &plan).expect("plan");
    trajectory::init_trajectory(
        dir,
        trajectory::TrajectoryInit {
            run_id: "run_fixture".to_string(),
            task_id: Some("test_01fixture".to_string()),
            intent_id: None,
            original_intent: "original".to_string(),
            derived_intent: "derived".to_string(),
            active_boundaries: vec!["b".to_string()],
            repo_scope: vec![".".to_string()],
            destination: None,
            current_phase: None,
            next_transitions: vec![],
            blockers: vec![],
        },
    )
    .expect("trajectory");
    // Parse-only receipt (currency gate does not require integrity/HEAD bind).
    fs::write(
        dir.join(VALIDATION_RECEIPT_PATH),
        r#"{
  "schema_version": "1.0.0",
  "kind": "validation_receipt",
  "decapod_release": "0.0.0",
  "git_revision": "deadbeef",
  "repo_signal_fingerprint": "0",
  "trajectory_run_id": null,
  "trajectory_artifact_hash": null,
  "validation_epoch": {
    "schema_version": "1.0.0",
    "epoch_id": "ve_test",
    "evaluator_identity": "test",
    "evaluator_set_hash": "sha256:00",
    "constitution_version": "test",
    "constitution_hash": "sha256:00",
    "validation_profile": "test",
    "validation_profile_hash": "sha256:00",
    "proof_rubric": "test",
    "proof_rubric_hash": "sha256:00",
    "generated_specs_manifest_hash": "sha256:00",
    "generated_specs_fingerprint": "0",
    "material_hashes": {}
  },
  "status": "ok",
  "pass_count": 0,
  "fail_count": 0,
  "warn_count": 0,
  "elapsed_ms": 0,
  "drift_findings": [],
  "temporary_artifacts_cleaned": 0,
  "receipt_hash": "sha256:00"
}
"#,
    )
    .expect("validation receipt");
}

/// A/B/G: inherited bundle at HEAD with only app commits must not fail this gate
/// for missing release-bound paths, and must not demand per-commit participation.
#[test]
fn publication_bundle_currency_accepts_inherited_bundle_without_diff_churn() {
    unsafe {
        std::env::remove_var("DECAPOD_VALIDATE_SKIP_GIT_GATES");
        std::env::remove_var("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES");
    }
    let tmp = tempdir().expect("tmpdir");
    let dir = tmp.path();
    git(dir, &["init", "-b", "master"]);
    git(dir, &["config", "user.email", "t@t.com"]);
    git(dir, &["config", "user.name", "t"]);
    seed_publication_bundle(dir);
    git(dir, &["add", "."]);
    git(dir, &["commit", "-m", "base with full bundle"]);

    git(dir, &["checkout", "-b", "agent/test/currency"]);
    fs::write(dir.join("app.txt"), "feature\n").expect("app");
    fs::write(
        dir.join(".decapod/managed/specs/INTENT.md"),
        "# Intent\n\nBase.\n\n## Material rewrite for #1232\n",
    )
    .expect("material intent");
    // Do not touch entrypoints, Dockerfile, governance, or manifest.
    git(dir, &["add", "app.txt", ".decapod/managed/specs/INTENT.md"]);
    git(dir, &["commit", "-m", "app + material intent only"]);
    fs::write(dir.join("app2.txt"), "more\n").expect("app2");
    git(dir, &["add", "app2.txt"]);
    git(dir, &["commit", "-m", "incremental app commit"]);

    // Prove intermediate commits did not re-touch the release-bound bundle.
    let log = std::process::Command::new("git")
        .args(["log", "master..HEAD", "--name-only", "--pretty=format:"])
        .current_dir(dir)
        .output()
        .expect("git log");
    let names = String::from_utf8_lossy(&log.stdout);
    for forbidden in [
        "AGENTS.md",
        "CLAUDE.md",
        "CODEX.md",
        "GEMINI.md",
        ".decapod/managed/Dockerfile.decapod",
        ".decapod/governance/claims.json",
        ".decapod/governance/plan.json",
    ] {
        assert!(
            !names.lines().any(|l| l.trim() == forbidden),
            "feature commits must not need {forbidden} churn; saw:\n{names}"
        );
    }

    let ctx = ValidationContext::new();
    validate_publication_bundle_currency(&ctx, dir).expect("gate runs");
    assert_eq!(
        ctx.fail_count.load(Ordering::Relaxed),
        0,
        "inherited loadable bundle at HEAD must pass currency gate without per-commit participation"
    );
    assert!(ctx.pass_count.load(Ordering::Relaxed) >= 1);
}

/// C: base release pin older than running binary and no release-bound refresh
/// on the branch ⇒ this gate fails (sibling integrity would also fail).
#[test]
fn publication_bundle_currency_fails_when_release_advanced_without_refresh() {
    unsafe {
        std::env::remove_var("DECAPOD_VALIDATE_SKIP_GIT_GATES");
        std::env::remove_var("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES");
    }
    let tmp = tempdir().expect("tmpdir");
    let dir = tmp.path();
    git(dir, &["init", "-b", "master"]);
    git(dir, &["config", "user.email", "t@t.com"]);
    git(dir, &["config", "user.name", "t"]);
    seed_publication_bundle(dir);
    // Force base pin to a non-running release while leaving content otherwise intact.
    fs::write(
        dir.join("AGENTS.md"),
        "<!-- decapod-release: 0.0.0-stale -->\n<!-- decapod-fingerprint: deadbeef -->\n# AGENTS.md\n",
    )
    .expect("stale agents");
    git(dir, &["add", "."]);
    git(dir, &["commit", "-m", "base with stale release pin"]);

    git(dir, &["checkout", "-b", "agent/test/release-advance"]);
    fs::write(dir.join("only-app.txt"), "x\n").expect("app");
    git(dir, &["add", "only-app.txt"]);
    git(dir, &["commit", "-m", "app without release-bound refresh"]);

    let ctx = ValidationContext::new();
    validate_publication_bundle_currency(&ctx, dir).expect("gate runs");
    assert!(
        ctx.fail_count.load(Ordering::Relaxed) >= 1,
        "release advance without refreshing release-bound paths must fail"
    );
}

/// Missing loadable governance at HEAD fails the gate even if files are
/// "inherited" from nowhere — presence/load is required.
#[test]
fn publication_bundle_currency_fails_when_plan_missing() {
    unsafe {
        std::env::remove_var("DECAPOD_VALIDATE_SKIP_GIT_GATES");
    }
    let tmp = tempdir().expect("tmpdir");
    let dir = tmp.path();
    git(dir, &["init", "-b", "master"]);
    git(dir, &["config", "user.email", "t@t.com"]);
    git(dir, &["config", "user.name", "t"]);
    seed_publication_bundle(dir);
    git(dir, &["add", "."]);
    git(dir, &["commit", "-m", "base"]);
    git(dir, &["checkout", "-b", "agent/test/missing-plan"]);
    fs::write(dir.join("app.txt"), "x\n").expect("app");
    git(dir, &["add", "app.txt"]);
    git(dir, &["commit", "-m", "app"]);
    fs::remove_file(dir.join(plan_governance::PLAN_PATH)).expect("remove plan");

    let ctx = ValidationContext::new();
    validate_publication_bundle_currency(&ctx, dir).expect("gate runs");
    assert!(
        ctx.fail_count.load(Ordering::Relaxed) >= 1,
        "missing plan at HEAD must fail currency gate"
    );
}
