//! Project meaning comes from repository evidence, not the latest task or
//! generated capability pack. These fixtures deliberately are not Decapod.
use decapod::core::project_specs::{
    LOCAL_PROJECT_SPECS, LOCAL_PROJECT_SPECS_ARCHITECTURE, LOCAL_PROJECT_SPECS_INTENT,
    first_markdown_content_line, local_project_specs_context, material_spec_body,
    mermaid_syntax_errors, refresh_specs_from_codebase,
};
use decapod::core::scaffold::{
    DiagramStyle, SpecsSeed, refresh_project_specs, refresh_project_specs_from_config,
};
use std::fs;
use std::path::Path;

const PURPOSE: &str =
    "Recipe Kit helps home cooks scale ingredient quantities for any serving count.";
const ARCHITECTURE: &str =
    "A Python library with a pure ingredient-scaling core and a text import boundary.";

fn fixture(root: &Path, style: &str) {
    fs::create_dir_all(root.join(".decapod")).unwrap();
    fs::write(root.join(".decapod/config.toml"), format!(
        "schema_version = \"1.0.0\"\n[init]\nspecs = true\ndiagram_style = \"{style}\"\nentrypoints = []\n[repo]\nproduct_name = \"Recipe Kit\"\nproduct_summary = \"{PURPOSE}\"\narchitecture_direction = \"{ARCHITECTURE}\"\nproduct_type = \"library\"\nprimary_languages = [\"Python\"]\ndetected_surfaces = [\"python\"]\ndeclared_capabilities = []\n"
    )).unwrap();
    fs::write(
        root.join("README.md"),
        format!("# Recipe Kit\n\n{PURPOSE}\n"),
    )
    .unwrap();
    fs::write(
        root.join("recipe.py"),
        "def scale(amount, ratio):\n    return amount * ratio\n",
    )
    .unwrap();
}

#[test]
fn non_decapod_project_scaffolds_purpose_before_implementation_for_every_spec() {
    for style in ["ascii", "mermaid"] {
        let temp = tempfile::tempdir().unwrap();
        fixture(temp.path(), style);
        let manifest = refresh_project_specs_from_config(temp.path()).unwrap();
        assert_eq!(manifest.template_version, "scaffold-v5");
        assert_eq!(manifest.files.len(), LOCAL_PROJECT_SPECS.len());
        for spec in LOCAL_PROJECT_SPECS {
            let body = fs::read_to_string(temp.path().join(spec.path)).unwrap();
            assert_eq!(
                first_markdown_content_line(&body).as_deref(),
                Some(PURPOSE),
                "{}",
                spec.path
            );
            assert!(body.contains("purpose to architecture to implementation depth"));
            assert!(body.contains("source paths and symbols"));
            assert!(body.contains("must not replace project-wide identity"));
            assert!(body.contains("not an implementation attestation"));
            assert!(!body.contains("Buildkite"), "{}", spec.path);
            assert!(!body.contains("Decapod is the"), "{}", spec.path);
            assert!(mermaid_syntax_errors(&body).is_empty(), "{}", spec.path);
        }
        let architecture =
            fs::read_to_string(temp.path().join(LOCAL_PROJECT_SPECS_ARCHITECTURE)).unwrap();
        assert!(architecture.find(PURPOSE).unwrap() < architecture.find("## Direction").unwrap());
        assert!(architecture.contains(ARCHITECTURE));
        assert!(!architecture.contains("Storage adapter reads or writes"));
        assert!(!architecture.contains("Invalid input is rejected before side effects"));
        let semantics =
            fs::read_to_string(temp.path().join(".decapod/managed/specs/SEMANTICS.md")).unwrap();
        assert!(!semantics.contains("Mutation events are replayable"));
        assert!(!semantics.contains("Draft --> InProgress"));
        assert!(semantics.contains("do not substitute\nthe agent's task lifecycle"));
    }
}

fn material_words(body: &str) -> String {
    material_spec_body(body)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn narrow_task_and_repeated_refresh_preserve_whole_project_specs() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fixture(root, "mermaid");
    refresh_project_specs_from_config(root).unwrap();
    fs::write(root.join(LOCAL_PROJECT_SPECS_INTENT), format!(
        "# Intent\n\n## Current Task\nFix fractional servings in text imports.\n\n## Product Outcome\n{PURPOSE}\n\n## Scope\nScaling and import workflows remain the project scope.\n"
    )).unwrap();
    fs::write(root.join(LOCAL_PROJECT_SPECS_ARCHITECTURE), format!(
        "# Architecture\n\n## Recent Change\nAdjust one rounding branch in the import parser.\n\n## Architecture Overview\n{ARCHITECTURE}\n\n## Implementation Depth\nThe scale function in recipe.py multiplies quantities by the serving ratio.\n"
    )).unwrap();
    let before = LOCAL_PROJECT_SPECS
        .iter()
        .map(|spec| (spec.path, fs::read_to_string(root.join(spec.path)).unwrap()))
        .collect::<Vec<_>>();
    let task_seed = SpecsSeed {
        product_name: Some("Fractional import fix".to_string()),
        product_summary: Some("Fix one rounding branch".to_string()),
        architecture_direction: Some("Change the import parser".to_string()),
        product_type: Some("library".to_string()),
        primary_languages: vec!["Python".to_string()],
        detected_surfaces: vec!["python".to_string()],
        done_criteria: None,
        capabilities: vec![],
    };
    assert!(refresh_project_specs(root, DiagramStyle::Mermaid, Some(&task_seed)).is_err());
    for (path, body) in &before {
        assert_eq!(*body, fs::read_to_string(root.join(path)).unwrap());
    }
    fs::write(
        root.join("recipe.py"),
        "def scale(amount, ratio):\n    return float(amount) * ratio\n",
    )
    .unwrap();
    let capabilities = vec![
        "persistent-state".to_string(),
        "background-processing".to_string(),
    ];
    refresh_specs_from_codebase(root, &capabilities).unwrap();
    let first = LOCAL_PROJECT_SPECS
        .iter()
        .map(|spec| (spec.path, fs::read_to_string(root.join(spec.path)).unwrap()))
        .collect::<Vec<_>>();
    for ((path, before), (_, after)) in before.iter().zip(&first) {
        assert_eq!(material_words(before), material_words(after), "{path}");
    }
    refresh_specs_from_codebase(root, &capabilities).unwrap();
    for (path, body) in first {
        assert_eq!(body, fs::read_to_string(root.join(path)).unwrap(), "{path}");
    }
    let context = local_project_specs_context(root);
    assert_eq!(context.intent.as_deref(), Some(PURPOSE));
    assert_eq!(context.architecture.as_deref(), Some(ARCHITECTURE));
    assert!(
        context
            .update_guidance
            .contains("must not replace project-wide identity")
    );
    assert!(
        context
            .update_guidance
            .contains("assumptions, and unresolved gaps")
    );
}
