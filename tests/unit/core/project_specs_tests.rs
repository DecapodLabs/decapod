// Moved from src/decapod/core/project_specs.rs
use super::*;

#[test]
fn first_markdown_content_line_uses_bullet_text_before_code_fence() {
    let markdown = r#"# Intent

## Product Outcome
- Decapod governs agent work.

## Product View
```mermaid
flowchart LR
```
"#;

    assert_eq!(
        first_markdown_content_line(markdown).as_deref(),
        Some("Decapod governs agent work.")
    );
}

#[test]
fn first_markdown_content_line_ignores_html_and_fenced_blocks() {
    let markdown = r#"<p align="center">ignored</p>

```bash
cargo install decapod
```

Real product summary.
"#;

    assert_eq!(
        first_markdown_content_line(markdown).as_deref(),
        Some("Real product summary.")
    );
}

#[test]
fn repo_signal_fingerprint_changes_when_source_changes() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("main.rs"), "fn main() {}\n").unwrap();
    let before = repo_signal_fingerprint(dir.path()).unwrap();
    fs::write(
        src.join("main.rs"),
        "fn main() { println!(\"changed\"); }\n",
    )
    .unwrap();
    let after = repo_signal_fingerprint(dir.path()).unwrap();
    assert_ne!(before, after);
}

#[test]
fn codebase_attestation_preserves_authored_spec_content() {
    let body = "# Intent\n\nAuthored product contract.\n";
    let updated = update_codebase_attestation(body, "abc123", "`src/` (2 files)");
    assert!(updated.contains("Authored product contract."));
    assert!(updated.contains("Repository signal fingerprint: `abc123`"));
}

#[test]
fn material_spec_body_ignores_fingerprint_and_capability_blocks() {
    let authored = "# Intent\n\n## Product Outcome\n- Ship governed agent workflows.\n";
    let with_generated = format!(
        "{authored}\n\
         <!-- decapod:declared-capabilities:start -->\n\
         ## Declared Capability Surfaces\n\
         - `control-plane`\n\
         <!-- decapod:declared-capabilities:end -->\n\
         <!-- decapod:capability-overlay:public-api:start -->\n\
         ## Public API Overlay\n\
         - versioned contracts\n\
         <!-- decapod:capability-overlay:public-api:end -->\n\
         <!-- decapod:codebase-attestation:start -->\n\
         ## Codebase Attestation\n\
         - Repository signal fingerprint: `oldfp`\n\
         <!-- decapod:codebase-attestation:end -->\n"
    );
    let fingerprint_only = with_generated.replace("`oldfp`", "`newfp`");

    assert_eq!(
        material_spec_body(&with_generated).trim(),
        authored.trim(),
        "generated blocks must not contribute material prose"
    );
    assert!(
        !material_spec_bodies_differ(&with_generated, &fingerprint_only),
        "fingerprint-only attestation refresh is not a material rewrite"
    );

    let rewritten = format!(
        "{}\n## Change Log\n- Require material living-spec rewrites beyond fingerprints (#1183).\n{}",
        authored,
        "<!-- decapod:codebase-attestation:start -->\n## Codebase Attestation\n- Repository signal fingerprint: `newfp`\n<!-- decapod:codebase-attestation:end -->\n"
    );
    assert!(
        material_spec_bodies_differ(&with_generated, &rewritten),
        "authored prose changes must count as material"
    );
}

#[test]
fn material_specs_change_vs_base_detects_fingerprint_only_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(root)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(root)
        .status()
        .unwrap();

    let specs = root.join(".decapod/managed/specs");
    fs::create_dir_all(&specs).unwrap();
    let intent = specs.join("INTENT.md");
    let base_body = "# Intent\n\nAuthored contract.\n\n<!-- decapod:codebase-attestation:start -->\n## Codebase Attestation\n- Repository signal fingerprint: `aaa`\n<!-- decapod:codebase-attestation:end -->\n";
    fs::write(&intent, base_body).unwrap();
    // Seed remaining living specs so the report covers the full set.
    for name in [
        "README.md",
        "ARCHITECTURE.md",
        "INTERFACES.md",
        "VALIDATION.md",
        "SEMANTICS.md",
        "OPERATIONS.md",
        "SECURITY.md",
    ] {
        fs::write(specs.join(name), format!("# {name}\n\nBaseline.\n")).unwrap();
    }
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(root)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "base"])
        .current_dir(root)
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["checkout", "-q", "-b", "feature"])
        .current_dir(root)
        .status()
        .unwrap();

    // Default branch after git init may be master or main depending on config.
    let base = if std::process::Command::new("git")
        .args(["rev-parse", "--verify", "master"])
        .current_dir(root)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        "master"
    } else {
        "main"
    };

    let fingerprint_only = base_body.replace("`aaa`", "`bbb`");
    fs::write(&intent, fingerprint_only).unwrap();
    let report = material_specs_change_vs_base(root, base).expect("report");
    assert!(
        !report.has_material_change,
        "fingerprint-only refresh must not pass material gate: {report:?}"
    );
    assert!(
        report
            .fingerprint_only_changed_paths
            .iter()
            .any(|p| p.ends_with("INTENT.md")),
        "{report:?}"
    );

    fs::write(
        &intent,
        "# Intent\n\nAuthored contract plus material rewrite for #1183.\n\n<!-- decapod:codebase-attestation:start -->\n## Codebase Attestation\n- Repository signal fingerprint: `bbb`\n<!-- decapod:codebase-attestation:end -->\n",
    )
    .unwrap();
    let report = material_specs_change_vs_base(root, base).expect("report after rewrite");
    assert!(report.has_material_change, "{report:?}");
    assert!(
        report
            .material_changed_paths
            .iter()
            .any(|p| p.ends_with("INTENT.md")),
        "{report:?}"
    );
}

#[test]
fn legacy_manifest_without_capability_provenance_is_readable() {
    let legacy = r#"{
          "schema_version": "1.0.0",
          "template_version": "scaffold-v3",
          "generated_at": "1Z",
          "repo_signal_fingerprint": "abc",
          "files": []
        }"#;
    let parsed: ProjectSpecsManifest = serde_json::from_str(legacy).unwrap();
    assert!(parsed.declared_capabilities.is_empty());
    assert!(parsed.capability_definition_version.is_empty());
}
