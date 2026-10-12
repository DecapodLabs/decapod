//! The refresh instructions must stay executable and survive docs regeneration.
use decapod::core::{assets, docs_cli};
use std::process::Command;

#[test]
fn refresh_contract_is_embedded_and_matches_cli_surfaces() {
    let doc = assets::get_embedded_doc("docs/agent/command-contracts.md").unwrap();
    let embedded: serde_json::Value = serde_json::from_str(&doc).unwrap();
    let body = embedded["sections"]["concepts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(body.contains(docs_cli::specs_refresh_contract()));
    assert!(body.contains(docs_cli::readiness_contract()));
    let temp = tempfile::tempdir().unwrap();
    for args in [
        vec!["rpc", "--op", "specs.refresh", "--help"],
        vec!["validate", "--refresh-specs", "--help"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
            .args(&args)
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let invalid = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .arg("specs.refresh")
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("unrecognized subcommand"));
    for name in ["AGENTS.md", "CLAUDE.md", "CODEX.md", "GEMINI.md"] {
        let template = assets::get_template(name).unwrap();
        assert!(
            template.contains("decapod rpc --op specs.refresh"),
            "{name}"
        );
        assert!(
            template.contains("decapod validate --refresh-specs"),
            "{name}"
        );
        assert!(
            template.contains("no top-level `decapod specs.refresh`"),
            "{name}"
        );
    }
}

#[test]
fn docs_ingest_presents_both_refresh_surfaces_and_semantic_boundary() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join(".decapod")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["docs", "ingest"])
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body = String::from_utf8(output.stdout).unwrap();
    assert!(body.contains(docs_cli::specs_refresh_contract()));
    assert!(body.contains(docs_cli::readiness_contract()));
    assert!(body.contains(docs_cli::readiness_config_contract()));
    assert!(body.contains("Refresh does not rewrite authored intent or architecture"));
}

#[test]
fn agent_template_requires_report_driven_exact_candidate_repair() {
    let template = include_str!("../src/decapod/core/assets.rs");
    for required in [
        "follow the actionable instructions and recovery steps in its validation report",
        "repair the reported root causes on the same work branch",
        "Never bypass a reported gate, fabricate evidence, weaken required checks",
        "Work branch pushes and iteration are allowed before preflight",
        "**MUST NOT** create a PR (including a draft) until intent is converged",
        "the branch is mergeable into the accepted target",
        "succeeds against the exact current candidate and accepted target",
        "actual Buildkite execution evidence is required",
        "GitHub Actions results neither satisfy nor block that profile",
        "without submitting or waiting for another preflight",
        "scope=ci_local",
        "publication_ready=false",
    ] {
        assert!(
            template.contains(required),
            "missing report-driven publication contract: {required}"
        );
    }
}

#[test]
fn readiness_generator_matches_checked_in_agent_docs() {
    let command_docs = include_str!("../docs/agent/command-contracts.md");
    let config_docs = include_str!("../docs/agent/config-schema.md");
    assert!(command_docs.contains(docs_cli::readiness_contract()));
    assert!(config_docs.contains(docs_cli::readiness_config_contract()));
}

#[test]
fn readiness_guidance_keeps_branch_push_separate_from_pr() {
    let contract = docs_cli::readiness_contract();
    for required in [
        "Work branch pushes and iteration are allowed before preflight",
        "Hold PR creation (including a draft) until intent is converged",
        "the branch is mergeable into the accepted target",
        "exact current candidate and accepted target",
        "never bypass a reported gate or fabricate evidence",
        "`workspace publish` combines branch push and PR creation",
        "scope=ci_local",
        "publication_ready=false",
        "never writes a full-readiness receipt or waits for its own suite",
        "GitHub workflow results and artifacts never satisfy or block this Buildkite validation profile",
    ] {
        assert!(
            contract.contains(required),
            "missing readiness contract: {required}"
        );
    }
    for source in [
        include_str!("../src/decapod/core/assets.rs"),
        include_str!("../src/decapod/core/docs_cli.rs"),
        include_str!("../src/decapod/cli.rs"),
        include_str!("../src/decapod/lib.rs"),
        include_str!("../src/decapod/core/validate.rs"),
    ] {
        for stale in [
            "before push or PR",
            "before push or pull request",
            "cannot authorize push or PR",
            "**MUST NOT** push the work branch or create a PR",
            "only then push the work branch and create its PR",
            "must not publish the original work branch before success",
            "readiness before pushing the work branch or creating a PR",
        ] {
            assert!(!source.contains(stale), "stale guidance: {stale}");
        }
    }
}
