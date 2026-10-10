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
    assert!(body.contains("Refresh does not rewrite authored intent or architecture"));
}
