use decapod::core::context_capsule::{
    ContextCapsuleSnippet, ContextCapsuleSource, DeterministicContextCapsule,
    query_embedded_capsule,
};
use std::fs;
use tempfile::tempdir;

#[test]
fn context_capsule_canonical_serialization_is_deterministic() {
    let capsule = DeterministicContextCapsule {
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
