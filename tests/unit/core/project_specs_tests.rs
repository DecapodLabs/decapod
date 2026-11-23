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
fn config_hash_input_normalizes_line_endings() {
    let unix = "[repo]\nname = \"decapod\"\n";
    let windows = "[repo]\r\nname = \"decapod\"\r\n";

    assert_eq!(
        normalize_text_for_hash(unix),
        normalize_text_for_hash(windows)
    );
}

#[test]
fn config_hash_tracks_both_project_authority_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let decapod = dir.path().join(".decapod");
    fs::create_dir_all(&decapod).unwrap();
    fs::write(decapod.join("config.toml"), "[repo]\nname = \"decapod\"\n").unwrap();
    fs::write(decapod.join("OVERRIDE.md"), "# override\n").unwrap();

    let before = config_input_hash(dir.path()).unwrap();
    fs::write(decapod.join("OVERRIDE.md"), "# changed override\n").unwrap();
    let after_override = config_input_hash(dir.path()).unwrap();
    assert_ne!(before, after_override);

    fs::write(
        decapod.join("config.toml"),
        "[repo]\nname = \"different\"\n",
    )
    .unwrap();
    let after_config = config_input_hash(dir.path()).unwrap();
    assert_ne!(after_override, after_config);
}

#[test]
fn codebase_attestation_preserves_authored_spec_content() {
    let body = "# Intent\n\nAuthored product contract.\n";
    let updated = update_codebase_attestation(body, "abc123", "`src/` (2 files)");
    assert!(updated.contains("Authored product contract."));
    assert!(updated.contains("Repository signal fingerprint: `abc123`"));
}

#[test]
fn refresh_preserves_proof_surfaces_and_other_authored_sections_twice() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(LOCAL_PROJECT_SPECS_VALIDATION);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let authored = "# Validation\n\n## Proof Surfaces\n\n- Verify the project-specific doc contract.\n\n## Verification Method\n\nKeep dated evidence and exact commands.\n\n## Participant Promotion Proof\n\nEvidence from the project owner.\n";
    fs::write(&path, authored).unwrap();
    let capabilities = vec!["persistent-state".to_string(), "public-api".to_string()];
    refresh_specs_from_codebase(dir.path(), &capabilities).unwrap();
    let first = fs::read_to_string(&path).unwrap();
    assert!(first.contains(authored.trim_end()));
    refresh_specs_from_codebase(dir.path(), &capabilities).unwrap();
    assert_eq!(first, fs::read_to_string(&path).unwrap());
}

#[test]
fn refresh_refuses_generated_ranges_that_swallow_proof_surfaces_before_any_write() {
    let dir = tempfile::tempdir().unwrap();
    let intent_path = dir.path().join(LOCAL_PROJECT_SPECS_INTENT);
    let validation_path = dir.path().join(LOCAL_PROJECT_SPECS_VALIDATION);
    fs::create_dir_all(intent_path.parent().unwrap()).unwrap();
    fs::write(&intent_path, "# Intent\n\nPreserve this too.\n").unwrap();
    let malformed = "# Validation\n\n<!-- decapod:capability-overlay:persistent-state:start -->\n## Proof Surfaces\n\n- Project-specific authored obligation.\n<!-- decapod:capability-overlay:persistent-state:end -->\n\n## Promotion Gates\n\nKeep these gates.\n";
    fs::write(&validation_path, malformed).unwrap();
    let error = refresh_specs_from_codebase(dir.path(), &[])
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("SPEC_REFRESH_AUTHORED_CONTENT_LOSS"),
        "{error}"
    );
    assert!(error.contains("VALIDATION.md"), "{error}");
    assert_eq!(fs::read_to_string(&validation_path).unwrap(), malformed);
    assert_eq!(
        fs::read_to_string(&intent_path).unwrap(),
        "# Intent\n\nPreserve this too.\n"
    );
    assert!(!dir.path().join(LOCAL_PROJECT_SPECS_MANIFEST).exists());
}

#[test]
fn refresh_preserves_bullets_adjacent_to_inline_and_consecutive_generated_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(LOCAL_PROJECT_SPECS_VALIDATION);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let body = "# Validation\n\n## Proof Surfaces\n- Authored before.<!-- decapod:capability-overlay:public-api:start -->\nGenerated API obligations.\n<!-- decapod:capability-overlay:public-api:end --> Authored after.\n<!-- decapod:capability-overlay:persistent-state:start -->\nGenerated storage obligations.\n<!-- decapod:capability-overlay:persistent-state:end -->\n\n## Promotion Gates\nKeep the gate.\n";
    fs::write(&path, body).unwrap();
    for _ in 0..2 {
        refresh_specs_from_codebase(dir.path(), &[]).unwrap();
        let updated = fs::read_to_string(&path).unwrap();
        assert!(updated.contains("- Authored before."));
        assert!(updated.contains("Authored after."));
        assert!(updated.contains("Keep the gate."));
    }
}

#[test]
fn codebase_attestation_refresh_collapses_legacy_duplicates() {
    let body = "# Validation\n\nAuthored guidance.\n\n\
## Codebase Attestation\n\n\
- Repository signal fingerprint: `old-one`\n\
- Significant implementation surfaces: `src/` (1 files)\n\
- Refreshed from the current codebase by `decapod specs.refresh`\n\
<!-- decapod:codebase-attestation:end -->\n\n\
<!-- decapod:codebase-attestation:start -->\n\
## Codebase Attestation\n\n\
- Repository signal fingerprint: `old-two`\n\
- Significant implementation surfaces: `src/` (1 files)\n\
- Refreshed from the current codebase by `decapod specs.refresh`\n\
<!-- decapod:codebase-attestation:end -->\n\n\
## Bounded Execution\n\nKeep this authored section.\n";

    let updated = update_codebase_attestation(body, "new-fingerprint", "`src/` (2 files)");
    assert_eq!(
        updated.matches(CODEBASE_ATTESTATION_START).count(),
        1,
        "refresh must leave one attestation start marker"
    );
    assert_eq!(
        updated.matches(CODEBASE_ATTESTATION_END).count(),
        1,
        "refresh must leave one attestation end marker"
    );
    assert_eq!(updated.matches("## Codebase Attestation").count(), 1);
    assert!(updated.contains("Authored guidance."));
    assert!(updated.contains("## Bounded Execution"));
    assert!(updated.contains("new-fingerprint"));
    assert!(!updated.contains("old-one"));
    assert!(!updated.contains("old-two"));
    assert_eq!(
        updated,
        update_codebase_attestation(&updated, "new-fingerprint", "`src/` (2 files)"),
        "repeated refresh must be byte-stable"
    );
}

#[test]
fn normalize_markdown_heading_boundaries_repairs_compacted_specs() {
    let compacted = "# Validation## Validation Philosophy\ntext.## Validation Harness\n## Idempotency Contracts| Operation | Key ||---|---|| create | id |\n## Checklist- [ ] verify\n### Nested Heading\n```text\n## keep inline\n```## Invariants\n";
    let normalized = normalize_markdown_heading_boundaries(compacted);

    assert_eq!(
        normalized,
        "# Validation\n\n## Validation Philosophy\ntext.\n\n## Validation Harness\n\n## Idempotency Contracts\n\n| Operation | Key |\n|---|---|\n| create | id |\n\n## Checklist\n\n- [ ] verify\n### Nested Heading\n```text\n## keep inline\n```\n\n## Invariants\n"
    );
}

#[test]
fn mermaid_syntax_errors_guard_generated_diagrams() {
    let valid = "```mermaid\nflowchart LR\n  A[Intent] --> B[Proof]\n```\n";
    assert!(mermaid_syntax_errors(valid).is_empty());

    let invalid = "```mermaid\nflowchart LR\n  A[Intent --> B[Proof]\n";
    let errors = mermaid_syntax_errors(invalid);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("unclosed Mermaid fence"))
    );
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

#[test]
fn codebase_attestation_names_the_executable_refresh_surface() {
    let updated = update_codebase_attestation("# Intent\nAuthored purpose.\n", "proof", "src/");
    assert!(updated.contains("by `decapod rpc --op specs.refresh`"));
    assert!(!updated.contains("by `decapod specs.refresh`"));
    assert!(updated.contains("Authored purpose."));
}

#[test]
fn project_summaries_skip_task_preambles_generated_blocks_and_fenced_headings() {
    let intent = "# Intent\n\n<!-- decapod:declared-capabilities:start -->\n## Declared Capability Surfaces\n- background-processing\n<!-- decapod:declared-capabilities:end -->\n\n## Current Task\nFix fractional servings.\n\n```markdown\n## Product Outcome\nThis is only an example.\n```\n\n## Product Outcome\nRecipe Kit helps home cooks scale ingredient quantities.\n";
    assert_eq!(
        project_purpose_summary(intent).as_deref(),
        Some("Recipe Kit helps home cooks scale ingredient quantities.")
    );
    let architecture = "# Architecture\n\n## Current Task\nFix fractional servings.\n\n## Direction\nA pure Python scaling core exposed as a library.\n";
    assert_eq!(
        project_architecture_summary(architecture).as_deref(),
        Some("A pure Python scaling core exposed as a library.")
    );
    assert!(project_purpose_summary("# Intent\n\nUnscoped legacy prose.\n").is_none());
    assert!(project_purpose_summary("<!-- decapod:declared-capabilities:start -->\n- background-processing\n<!-- decapod:declared-capabilities:end -->").is_none());
}

#[test]
fn project_context_prefers_configured_identity_and_leaves_legacy_scope_unknown() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(LOCAL_PROJECT_SPECS_DIR)).unwrap();
    fs::write(
        dir.path().join(LOCAL_PROJECT_SPECS_INTENT),
        "# Intent\n\nFix one rounding branch.\n",
    )
    .unwrap();
    fs::write(
        dir.path().join(LOCAL_PROJECT_SPECS_ARCHITECTURE),
        "# Architecture\n\nAdjust the import parser.\n",
    )
    .unwrap();
    let unknown = local_project_specs_context(dir.path());
    assert!(unknown.intent.is_none());
    assert!(unknown.architecture.is_none());
    assert!(
        unknown
            .update_guidance
            .contains("leave project identity unknown")
    );
    let mut config = crate::cli::DecapodProjectConfig::default();
    config.repo.product_summary = Some("Recipe Kit scales ingredient quantities.".to_string());
    config.repo.architecture_direction = Some("A pure Python library.".to_string());
    fs::write(
        dir.path().join(".decapod/config.toml"),
        toml::to_string(&config).expect("serialize complete project config fixture"),
    )
    .unwrap();
    crate::cli::DecapodProjectConfig::load(dir.path())
        .expect("configured identity fixture must satisfy the project config schema");
    let configured = local_project_specs_context(dir.path());
    assert_eq!(
        configured.intent.as_deref(),
        Some("Recipe Kit scales ingredient quantities.")
    );
    assert_eq!(
        configured.architecture.as_deref(),
        Some("A pure Python library.")
    );
}

#[test]
fn nested_task_headings_cannot_establish_project_identity() {
    let body = "# Intent\n\n## Current Task\n### Overview\nFix parser rounding.\n### Direction\nEdit the parser.\n";
    assert!(project_purpose_summary(body).is_none());
    assert!(project_architecture_summary(body).is_none());
}

#[test]
fn purpose_first_specs_keep_distinct_domain_context() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(LOCAL_PROJECT_SPECS_DIR)).unwrap();
    for (path, section, summary) in [
        (
            LOCAL_PROJECT_SPECS_INTERFACES,
            "Contract Overview",
            "The scaling API accepts quantities and a serving ratio.",
        ),
        (
            LOCAL_PROJECT_SPECS_VALIDATION,
            "Validation Strategy",
            "Golden recipes and fractional-serving tests exercise scaling.",
        ),
        (
            LOCAL_PROJECT_SPECS_SEMANTICS,
            "Semantics Overview",
            "Scaling preserves ingredient order and unit labels.",
        ),
        (
            LOCAL_PROJECT_SPECS_OPERATIONS,
            "Deployment Model",
            "The library runs inside its caller's Python process.",
        ),
        (
            LOCAL_PROJECT_SPECS_SECURITY,
            "Threat Model",
            "Untrusted text enters through the recipe import boundary.",
        ),
    ] {
        fs::write(dir.path().join(path), format!("# Spec\n\n## Project Purpose\nRecipe Kit scales ingredients.\n\n## {section}\n{summary}\n")).unwrap();
    }
    let ctx = local_project_specs_context(dir.path());
    assert_eq!(
        ctx.interfaces.as_deref(),
        Some("The scaling API accepts quantities and a serving ratio.")
    );
    assert_eq!(
        ctx.validation.as_deref(),
        Some("Golden recipes and fractional-serving tests exercise scaling.")
    );
    assert_eq!(
        ctx.semantics.as_deref(),
        Some("Scaling preserves ingredient order and unit labels.")
    );
    assert_eq!(
        ctx.operations.as_deref(),
        Some("The library runs inside its caller's Python process.")
    );
    assert_eq!(
        ctx.security.as_deref(),
        Some("Untrusted text enters through the recipe import boundary.")
    );
}

#[test]
fn readme_identity_uses_product_section_or_intro_never_task_section_fallback() {
    let scoped = "# Recipe Kit\n\n## Current Task\nFix parser rounding.\n\n## Introduction\nRecipe Kit scales ingredients.\n";
    assert_eq!(
        readme_project_summary(scoped).as_deref(),
        Some("Recipe Kit scales ingredients.")
    );
    assert!(
        readme_project_summary("# Recipe Kit\n\n## Current Task\nFix parser rounding.\n").is_none()
    );
    assert_eq!(readme_project_summary("# Recipe Kit\n\nRecipe Kit scales ingredients.\n\n## Current Task\nFix parser rounding.\n").as_deref(), Some("Recipe Kit scales ingredients."));
}

#[test]
fn managed_specs_and_generated_spec_context_exclude_maintainer_entrypoint_drift_setting() {
    const FORBIDDEN_KEY: &str = "_allow_entrypoint_version_drift";

    // Check fresh scaffolded living specs and generated context
    let dir = tempfile::tempdir().unwrap();
    let specs_dir = dir.path().join(".decapod/managed/specs");
    fs::create_dir_all(&specs_dir).unwrap();
    for spec in LOCAL_PROJECT_SPECS {
        if let Some(rendered) = crate::core::scaffold::render_project_spec_content(
            spec.path,
            crate::core::scaffold::DiagramStyle::Mermaid,
            None,
        ) {
            assert!(
                !rendered.contains(FORBIDDEN_KEY),
                "spec template {} must not expose {FORBIDDEN_KEY}",
                spec.path
            );
            fs::write(dir.path().join(spec.path), rendered).unwrap();
        }
    }
    let ctx = local_project_specs_context(dir.path());
    let serialized = format!("{ctx:?}");
    assert!(
        !serialized.contains(FORBIDDEN_KEY),
        "generated local_project_specs_context must not expose {FORBIDDEN_KEY}"
    );

    // Check repository-managed specs (including release-alignment guidance in OPERATIONS.md)
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_specs = repo_root.join(".decapod/managed/specs");
    if repo_specs.is_dir() {
        for spec in LOCAL_PROJECT_SPECS {
            let path = repo_root.join(spec.path);
            if path.is_file() {
                let text = fs::read_to_string(&path).unwrap();
                assert!(
                    !text.contains(FORBIDDEN_KEY),
                    "agent-facing managed spec {} must not expose maintainer-only {FORBIDDEN_KEY}",
                    spec.path
                );
            }
        }
        let ops_path = repo_root.join(LOCAL_PROJECT_SPECS_OPERATIONS);
        if ops_path.is_file() {
            let ops = fs::read_to_string(&ops_path).unwrap();
            assert!(
                ops.contains("### Current Release Alignment"),
                "OPERATIONS.md must preserve release-alignment guidance boundary"
            );
            assert!(
                !ops.contains(FORBIDDEN_KEY),
                "OPERATIONS.md release-alignment guidance must not expose {FORBIDDEN_KEY}"
            );
        }
        let repo_ctx = local_project_specs_context(repo_root);
        let repo_ctx_json = format!("{repo_ctx:?}");
        assert!(
            !repo_ctx_json.contains(FORBIDDEN_KEY),
            "repo local_project_specs_context must not expose {FORBIDDEN_KEY}"
        );
    }
}

#[test]
fn manifest_reads_legacy_fallback_and_refresh_migrates_to_managed_root() {
    let dir = tempfile::tempdir().unwrap();
    let legacy_path = dir.path().join(LEGACY_PROJECT_SPECS_MANIFEST);
    let canonical_path = dir.path().join(LOCAL_PROJECT_SPECS_MANIFEST);
    fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
    fs::write(
        dir.path().join(LOCAL_PROJECT_SPECS_INTENT),
        "# Intent\n\nAuthored purpose.\n",
    )
    .unwrap();
    fs::write(
        &legacy_path,
        serde_json::json!({
            "schema_version": LOCAL_PROJECT_SPECS_MANIFEST_SCHEMA,
            "template_version": "legacy-v1",
            "generated_at": "1700000000Z",
            "repo_signal_fingerprint": "a".repeat(64),
            "files": []
        })
        .to_string(),
    )
    .unwrap();

    // Backward-compatible read falls back to legacy `.decapod/managed/specs/.manifest.json`.
    let loaded = read_specs_manifest(dir.path())
        .unwrap()
        .expect("legacy manifest");
    assert_eq!(loaded.template_version, "legacy-v1");
    assert_eq!(
        resolve_specs_manifest_path(dir.path()).as_deref(),
        Some(legacy_path.as_path())
    );

    // Refresh writes canonical `.decapod/managed/.manifest.json` and removes legacy file (#1079).
    let refreshed = refresh_specs_manifest(dir.path(), &[]).unwrap();
    assert_eq!(refreshed.template_version, "legacy-v1");
    assert!(canonical_path.is_file());
    assert!(!legacy_path.exists());
    assert_eq!(
        resolve_specs_manifest_path(dir.path()).as_deref(),
        Some(canonical_path.as_path())
    );
}

#[test]
fn specs_refresh_bumps_bounded_codebase_contract_content_while_preserving_authored_prose() {
    let dir = tempfile::tempdir().unwrap();
    let arch_path = dir.path().join(LOCAL_PROJECT_SPECS_ARCHITECTURE);
    let iface_path = dir.path().join(LOCAL_PROJECT_SPECS_INTERFACES);
    fs::create_dir_all(arch_path.parent().unwrap()).unwrap();
    fs::create_dir_all(dir.path().join("src/decapod/core")).unwrap();
    fs::write(
        dir.path().join("src/decapod/core/alpha.rs"),
        "pub fn alpha() {}\n",
    )
    .unwrap();
    fs::write(dir.path().join("src/decapod/cli.rs"), "pub struct Cli;\n").unwrap();

    let authored_arch = "# Architecture\n\nAuthored architectural invariants.\n";
    let authored_iface = "# Interfaces\n\nAuthored interface contract.\n";
    fs::write(&arch_path, authored_arch).unwrap();
    fs::write(&iface_path, authored_iface).unwrap();

    refresh_specs_from_codebase(dir.path(), &[]).unwrap();
    let arch_v1 = fs::read_to_string(&arch_path).unwrap();
    let iface_v1 = fs::read_to_string(&iface_path).unwrap();
    assert!(arch_v1.contains("Authored architectural invariants."));
    assert!(arch_v1.contains("<!-- decapod:codebase-contract:start -->"));
    assert!(arch_v1.contains("`alpha`"));
    assert!(iface_v1.contains("`src/decapod/cli.rs`"));
    // Material body still equals authored prose (generated contract block cannot spoof #1183).
    assert!(!material_spec_bodies_differ(authored_arch, &arch_v1));

    // Adding a new core module and RPC surface bumps the spec contents deterministically (#1122).
    fs::write(
        dir.path().join("src/decapod/core/beta.rs"),
        "pub fn beta() {}\n",
    )
    .unwrap();
    fs::write(dir.path().join("src/decapod/rpc.rs"), "pub struct Rpc;\n").unwrap();

    refresh_specs_from_codebase(dir.path(), &[]).unwrap();
    let arch_v2 = fs::read_to_string(&arch_path).unwrap();
    let iface_v2 = fs::read_to_string(&iface_path).unwrap();
    assert_ne!(arch_v1, arch_v2);
    assert_ne!(iface_v1, iface_v2);
    assert!(arch_v2.contains("`beta`"));
    assert!(iface_v2.contains("`src/decapod/rpc.rs`"));
    assert!(arch_v2.contains("Authored architectural invariants."));
    assert!(iface_v2.contains("Authored interface contract."));

    // Idempotent second refresh produces zero diff.
    refresh_specs_from_codebase(dir.path(), &[]).unwrap();
    assert_eq!(arch_v2, fs::read_to_string(&arch_path).unwrap());
    assert_eq!(iface_v2, fs::read_to_string(&iface_path).unwrap());
}
