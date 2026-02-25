use decapod::core::context_capsule::{
    ContextCapsuleSnippet, ContextCapsuleSource, DeterministicContextCapsule,
};

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
                path: "interfaces/CONTROL_PLANE.md".to_string(),
                section: "1. The Contract".to_string(),
            },
            ContextCapsuleSource {
                path: "interfaces/CLAIMS.md".to_string(),
                section: "2. Claims".to_string(),
            },
            ContextCapsuleSource {
                path: "interfaces/CLAIMS.md".to_string(),
                section: "2. Claims".to_string(),
            },
        ],
        snippets: vec![
            ContextCapsuleSnippet {
                source_path: "interfaces/CLAIMS.md".to_string(),
                text: "claim.context.capsule.deterministic".to_string(),
            },
            ContextCapsuleSnippet {
                source_path: "interfaces/CONTROL_PLANE.md".to_string(),
                text: "Control-plane operations MUST remain daemonless".to_string(),
            },
        ],
        policy: Default::default(),
        capsule_hash: String::new(),
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
            path: "interfaces/KNOWLEDGE_STORE.md".to_string(),
            section: "Promotion Firewall".to_string(),
        }],
        snippets: vec![ContextCapsuleSnippet {
            source_path: "interfaces/KNOWLEDGE_STORE.md".to_string(),
            text: "episodic -> procedural requires explicit promotion event".to_string(),
        }],
        policy: Default::default(),
        capsule_hash: "wrong".to_string(),
    };

    let normalized1 = base.with_recomputed_hash().expect("normalize #1");
    let normalized2 = base.with_recomputed_hash().expect("normalize #2");
    assert_eq!(normalized1.capsule_hash, normalized2.capsule_hash);
}
