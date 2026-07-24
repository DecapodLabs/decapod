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
