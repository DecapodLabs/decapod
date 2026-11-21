//! Declarations, executable gates and observations are distinct assurance classes.
//! Never synthesize an "enforced" row to make a missing claim pass.
use decapod::core::{docs, rpc::BlockerKind, workspace};
use std::process::Command;

fn embedded_node(id: &str) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["constitution", "get", id])
        .output()
        .expect("query actual candidate binary");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn claim_doctrine_is_advisory_and_does_not_attest_its_own_execution() {
    let node = embedded_node("interfaces/CLAIMS");
    assert_eq!(node["binding"], "advisory");
    assert!(node.get("enforcement").is_none());
    let concepts = node["sections"]["concepts"].as_array().unwrap();
    assert!(concepts.iter().any(|c| {
        c.as_str()
            .unwrap()
            .contains("Claims are meaningless without executable proof")
    }));
    assert!(
        node["sections"]["standards"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str().unwrap().contains("unverified"))
    );
}

#[test]
fn binding_corpus_discloses_review_and_enforcement_limits_in_actual_binary() {
    for id in ["core/DECAPOD", "core/DEMANDS", "core/SPECS"] {
        let node = embedded_node(id);
        assert_eq!(node["binding"], "binding");
        assert_eq!(node["authority_review"]["control_id"], "core/SPECS");
        assert_eq!(
            node["authority_review"]["required_review"],
            "repository_pr_review"
        );
        assert_eq!(
            node["authority_review"]["enforcement"],
            "not_machine_enforced"
        );
    }
}

#[test]
fn removing_a_real_mandate_or_its_source_cannot_be_hidden_by_synthetic_claims() {
    let root = tempfile::tempdir().unwrap();
    let mandates = docs::resolve_mandates(root.path(), "workspace.ensure");
    // These are actual native registry records with resolved embedded sources,
    // not a test-authored ledger. Runtime observations below are a separate check.
    for (id, gate) in [
        ("no-master", "gate.worktree.no_master"),
        ("validate-before-done", "gate.validation.pass"),
        ("isolated-worktree", "gate.worktree.isolated"),
    ] {
        let mandate = mandates
            .iter()
            .find(|m| m.id == id)
            .expect("native mandate missing");
        assert_eq!(mandate.check_tag, gate);
        assert!(!mandate.fragment.excerpt.is_empty());
    }
    assert_eq!(mandates.len(), 3);
}

#[test]
fn actual_workspace_gate_refuses_root_master_and_unisolated_workbranch() {
    let root = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .current_dir(root.path())
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-b", "master"]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "-c",
        "commit.gpgSign=false",
        "commit",
        "--allow-empty",
        "-m",
        "fixture",
    ]);
    let protected = workspace::get_workspace_status(root.path()).unwrap();
    assert!(!protected.can_work);
    assert!(
        protected
            .blockers
            .iter()
            .any(|b| matches!(b.kind, BlockerKind::ProtectedBranch))
    );
    git(&["checkout", "-b", "agent/fixture"]);
    let unisolated = workspace::get_workspace_status(root.path()).unwrap();
    assert!(!unisolated.can_work);
    assert!(
        unisolated
            .blockers
            .iter()
            .any(|b| matches!(b.kind, BlockerKind::WorkspaceRequired))
    );
}
