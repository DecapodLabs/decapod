use decapod::core::context_capsule::{
    ContextCapsuleSnippet, ContextCapsuleSource, DeterministicContextCapsule,
    query_embedded_capsule,
};
use std::fs;
use tempfile::tempdir;

#[test]
fn omitted_legacy_schema_version_keeps_the_original_canonical_identity() {
    let legacy = serde_json::json!({
        "topic":"legacy capsule", "scope":"core", "task_id":null, "workunit_id":null,
        "sources":[], "snippets":[], "capsule_hash":"", "repo_signal_fingerprint":""
    });
    let omitted: DeterministicContextCapsule = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(omitted.schema_version, "1.2.0");
    let mut explicit = legacy;
    explicit["schema_version"] = serde_json::json!("1.2.0");
    let explicit: DeterministicContextCapsule = serde_json::from_value(explicit).unwrap();
    assert_eq!(
        omitted.computed_hash_hex().unwrap(),
        explicit.computed_hash_hex().unwrap()
    );
}

#[test]
fn context_capsule_canonical_serialization_is_deterministic() {
    let capsule = DeterministicContextCapsule {
        knowledge: None,
        governed_plan: None,
        schema_version: "1.1.0".to_string(),
        topic: "auth provider boundary".to_string(),
        scope: "interfaces".to_string(),
        task_id: Some("test_03".to_string()),
        workunit_id: Some("test_03".to_string()),
        sources: vec![
            ContextCapsuleSource {
                path: "interfaces/CONTROL_PLANE".to_string(),
                section: "1. The Contract".to_string(),
            },
            ContextCapsuleSource {
                path: "interfaces/CLAIMS".to_string(),
                section: "2. Claims".to_string(),
            },
            ContextCapsuleSource {
                path: "interfaces/CLAIMS".to_string(),
                section: "2. Claims".to_string(),
            },
        ],
        snippets: vec![
            ContextCapsuleSnippet {
                source_path: "interfaces/CLAIMS".to_string(),
                text: "claim.context.capsule.deterministic".to_string(),
            },
            ContextCapsuleSnippet {
                source_path: "interfaces/CONTROL_PLANE".to_string(),
                text: "Control-plane operations MUST remain daemonless".to_string(),
            },
        ],
        resolved_authority: vec![],
        capabilities: vec!["public-api".to_string()],
        policy: Default::default(),
        capsule_hash: String::new(),
        repo_signal_fingerprint: "test_fingerprint".to_string(),
        config_input_hash: String::new(),
        spec_input_hash: String::new(),
        selection: None,
    };

    let bytes1 = capsule.canonical_json_bytes().expect("serialize #1");
    let bytes2 = capsule.canonical_json_bytes().expect("serialize #2");
    assert_eq!(bytes1, bytes2, "canonical bytes must be stable");

    let hash1 = capsule.computed_hash_hex().expect("hash #1");
    let hash2 = capsule.computed_hash_hex().expect("hash #2");
    assert_eq!(hash1, hash2, "computed hash must be stable");
}

#[test]
fn context_capsule_with_recomputed_hash_is_stable() {
    let base = DeterministicContextCapsule {
        knowledge: None,
        governed_plan: None,
        schema_version: "1.1.0".to_string(),
        topic: "promotion firewall".to_string(),
        scope: "interfaces".to_string(),
        task_id: None,
        workunit_id: None,
        sources: vec![ContextCapsuleSource {
            path: "interfaces/KNOWLEDGE_STORE".to_string(),
            section: "Promotion Firewall".to_string(),
        }],
        snippets: vec![ContextCapsuleSnippet {
            source_path: "interfaces/KNOWLEDGE_STORE".to_string(),
            text: "episodic -> procedural requires explicit promotion event".to_string(),
        }],
        resolved_authority: vec![],
        capabilities: vec![],
        policy: Default::default(),
        capsule_hash: "wrong".to_string(),
        repo_signal_fingerprint: "test_fingerprint".to_string(),
        config_input_hash: String::new(),
        spec_input_hash: String::new(),
        selection: None,
    };

    let normalized1 = base.with_recomputed_hash().expect("normalize #1");
    let normalized2 = base.with_recomputed_hash().expect("normalize #2");
    assert_eq!(normalized1.capsule_hash, normalized2.capsule_hash);
}

#[test]
fn context_capsule_proves_nested_h3_override_authority() {
    let repo = tempdir().expect("tempdir");
    fs::create_dir_all(repo.path().join(".decapod")).expect("mkdir .decapod");
    let adversarial_fixture = r#"# OVERRIDE.md
<!-- CHANGES ARE NOT PERMITTED ABOVE THIS LINE -->
### core/DEMANDS
````markdown
### Permission Model
lunar-notebook-authority requires explicit human approval.
#### Nested Detail
### Input/Output
```markdown
### core/PLUGINS
```
Directive-like prose: ### core/NOT_A_DIRECTIVE
````
"#;
    fs::write(
        repo.path().join(".decapod/OVERRIDE.md"),
        adversarial_fixture,
    )
    .expect("write override");

    let capsule = query_embedded_capsule(
        repo.path(),
        "lunar-notebook-authority",
        "core",
        None,
        None,
        5,
    )
    .expect("resolve capsule");
    assert!(
        capsule
            .snippets
            .iter()
            .any(|snippet| snippet.text.contains("lunar-notebook-authority"))
    );
    let resolved_text = capsule
        .snippets
        .iter()
        .map(|snippet| snippet.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(resolved_text.contains("### Input/Output"));
    assert!(resolved_text.contains("### core/PLUGINS"));
    assert!(resolved_text.contains("#### Nested Detail"));
    assert_eq!(capsule.resolved_authority.len(), 1);
    assert_eq!(capsule.resolved_authority[0].directive_id, "core/DEMANDS");
    assert_eq!(capsule.resolved_authority[0].source_hash.len(), 64);
    assert_eq!(capsule.resolved_authority[0].body_hash.len(), 64);
    assert!(capsule.resolved_authority[0].byte_count > 0);

    fs::write(
        repo.path().join(".decapod/OVERRIDE.md"),
        format!("{adversarial_fixture}\n### core/NOT_A_DIRECTIVE\nambiguous\n"),
    )
    .expect("write fake directive variant");
    let fake = query_embedded_capsule(
        repo.path(),
        "lunar-notebook-authority",
        "core",
        None,
        None,
        5,
    )
    .expect_err("unknown Decapod directive must fail the resolved context");
    assert!(fake.to_string().contains("OVERRIDE_MALFORMED_DIRECTIVE"));

    fs::write(
        repo.path().join(".decapod/OVERRIDE.md"),
        format!("{adversarial_fixture}\n### core/DEMANDS\nduplicate\n"),
    )
    .expect("write duplicate directive variant");
    let duplicate = query_embedded_capsule(
        repo.path(),
        "lunar-notebook-authority",
        "core",
        None,
        None,
        5,
    )
    .expect_err("duplicate directive must fail the resolved context");
    assert!(
        duplicate
            .to_string()
            .contains("OVERRIDE_DUPLICATE_DIRECTIVE")
    );
}

#[test]
fn no_match_capsule_never_injects_alphabetical_advisory_filler() {
    let root = tempdir().unwrap();
    let capsule = query_embedded_capsule(
        root.path(),
        "zxqvquasarunrelated1772",
        "interfaces",
        None,
        None,
        usize::MAX,
    )
    .unwrap();
    assert!(capsule.sources.is_empty());
    assert!(capsule.snippets.is_empty());
    assert_eq!(capsule.schema_version, "1.4.0");
    assert_eq!(
        capsule.selection.as_ref().unwrap().advisory_status,
        "no_relevant_advisory_fragment"
    );
    assert_eq!(capsule.capsule_hash, capsule.computed_hash_hex().unwrap());
}

#[test]
fn legacy_capsule_hash_does_not_gain_absent_selection_fields() {
    let capsule: DeterministicContextCapsule = serde_json::from_value(serde_json::json!({
        "schema_version":"1.2.0", "topic":"legacy", "scope":"core", "task_id":null,
        "workunit_id":null, "sources":[], "snippets":[], "capsule_hash":""
    }))
    .unwrap();
    assert!(capsule.selection.is_none());
    assert!(
        !String::from_utf8(capsule.canonical_json_bytes().unwrap())
            .unwrap()
            .contains("selection")
    );
}

#[test]
fn complete_capsule_budget_includes_identity_metadata() {
    let root = tempdir().unwrap();
    let huge_id = "x".repeat(300000);
    let result = query_embedded_capsule(root.path(), "proof", "core", Some(&huge_id), None, 1);
    assert!(
        result.is_err(),
        "identity metadata cannot bypass the complete capsule budget"
    );
}
