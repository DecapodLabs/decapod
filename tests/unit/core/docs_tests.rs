// Moved from src/decapod/core/docs.rs
use super::*;

#[test]
fn selection_metadata_remains_bounded_for_a_corpus_sized_query() {
    let root = tempfile::tempdir().unwrap();
    let query = assets::list_docs()
        .into_iter()
        .filter_map(|path| assets::get_doc(&path))
        .collect::<Vec<_>>()
        .join(" ");
    let first = resolve_scoped_context(
        root.path(),
        Some(&query),
        Some("workspace.ensure"),
        &[],
        &[],
        usize::MAX,
        None,
    );
    let second = resolve_scoped_context(
        root.path(),
        Some(&query),
        Some("workspace.ensure"),
        &[],
        &[],
        usize::MAX,
        None,
    );
    let bytes = serde_json::to_vec(&first.selection).unwrap();
    assert!(
        bytes.len() < 48 * 1024,
        "metadata budget exceeded: {}",
        bytes.len()
    );
    assert_eq!(bytes, serde_json::to_vec(&second.selection).unwrap());
    assert!(first.selection.context_bytes <= MAX_CONTEXT_BYTES);
    assert!(
        first.selection.selected[0]
            .reasons
            .iter()
            .any(|reason| reason == "operation:workspace.ensure")
    );
}

#[test]
fn truncate_chars_respects_char_boundaries() {
    let input = "alpha — beta";
    assert_eq!(truncate_chars(input, 7), "alpha —...");
}

#[test]
fn repeated_terms_do_not_increase_section_score() {
    let once = "# A current\nUse cache for this fixture.\n# Z reference\ncache";
    let repeated = format!(
        "# A current\nUse cache for this fixture.\n# Z reference\n{}",
        "cache ".repeat(20)
    );
    let terms = vec!["cache".into()];
    assert_eq!(
        get_best_fragment_for_terms("research/fixture", once, &terms)
            .unwrap()
            .title,
        get_best_fragment_for_terms("research/fixture", &repeated, &terms)
            .unwrap()
            .title
    );
}

#[test]
fn no_match_is_empty_and_operation_binding_survives_weak_overlap() {
    let root = tempfile::tempdir().unwrap();
    let no_match = resolve_scoped_context(
        root.path(),
        Some("zxqvquasarunrelated1772"),
        None,
        &[],
        &[],
        2,
        None,
    );
    assert!(no_match.fragments.is_empty());
    assert_eq!(
        no_match.selection.advisory_status,
        "no_relevant_advisory_fragment"
    );
    let bound = resolve_scoped_context(
        root.path(),
        Some("zxqvquasarunrelated1772"),
        Some("workspace.ensure"),
        &[],
        &[],
        1,
        None,
    );
    assert_eq!(bound.fragments.len(), 1);
    assert!(
        bound.selection.selected[0]
            .reasons
            .contains(&"operation:workspace.ensure".into())
    );
    assert!(
        bound.fragments[0]
            .excerpt
            .contains("Fetch the protected base")
    );
}

#[test]
fn keyword_stuffing_cannot_displace_explicit_operation() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".decapod")).unwrap();
    std::fs::write(root.path().join(".decapod/OVERRIDE.md"), format!(
        "<!-- CHANGES ARE NOT PERMITTED ABOVE THIS LINE -->\n### research/ADAM_2014\n````markdown\n{}\n````\n", "keywordstuffingfixture ".repeat(20))).unwrap();
    let context = resolve_scoped_context(
        root.path(),
        Some("keywordstuffingfixture"),
        Some("workspace.ensure"),
        &[],
        &[],
        1,
        None,
    );
    assert_eq!(context.fragments.len(), 1);
    assert_eq!(
        context.selection.selected[0].node_id,
        "docs/agent/contribution-conventions.md"
    );
    assert!(
        !context.fragments[0]
            .excerpt
            .contains("keywordstuffingfixture")
    );
    assert_eq!(
        context.selection.excluded["research/ADAM_2014"],
        "context_budget"
    );
}

#[test]
fn advisory_research_remains_queryable_with_source_identity_and_bounded_text() {
    let root = tempfile::tempdir().unwrap();
    let query = || {
        resolve_scoped_context(
            root.path(),
            Some("adam optimizer"),
            None,
            &[],
            &[],
            usize::MAX,
            Some("research"),
        )
    };
    let first = query();
    assert!(
        first
            .selection
            .selected
            .iter()
            .any(|s| s.node_id == "research/ADAM_2014")
    );
    assert!(
        first
            .selection
            .selected
            .iter()
            .all(|s| s.binding == "advisory"
                && s.authority == "doctrine"
                && s.source_hash.len() == 64)
    );
    assert!(first.fragments.len() <= MAX_CONTEXT_FRAGMENTS);
    assert!(first.selection.context_bytes <= MAX_CONTEXT_BYTES);
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&query()).unwrap()
    );
}

#[test]
fn overrides_replace_active_projection_and_invalidate_source_identity() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".decapod")).unwrap();
    let path = root.path().join(".decapod/OVERRIDE.md");
    let write = |body| {
        std::fs::write(&path, format!("<!-- CHANGES ARE NOT PERMITTED ABOVE THIS LINE -->\n### core/DEMANDS\n````markdown\n{body}\n````\n")).unwrap()
    };
    let query = || {
        resolve_scoped_context(
            root.path(),
            Some("uniquedecisionfixture"),
            None,
            &[],
            &[],
            1,
            None,
        )
    };
    write("uniquedecisionfixture: current A");
    let a = query();
    assert_eq!(a.fragments.len(), 1);
    assert!(a.fragments[0].excerpt.contains("current A"));
    assert!(!a.fragments[0].excerpt.contains("## decide"));
    write("uniquedecisionfixture: revised C");
    let c = query();
    assert_ne!(
        a.selection.selected[0].source_hash,
        c.selection.selected[0].source_hash
    );
    assert_eq!(
        c.selection.selected[0].source_layer,
        "repository_project_override"
    );
}

#[test]
fn mandates_have_existing_source_sections_and_real_gate_identifiers() {
    let root = tempfile::tempdir().unwrap();
    for (op, expected) in [("agent.init", 4), ("workspace.ensure", 3), ("validate", 2)] {
        let mandates = resolve_mandates(root.path(), op);
        assert_eq!(mandates.len(), expected, "missing mandate source for {op}");
        assert!(
            mandates
                .iter()
                .all(|m| !m.fragment.excerpt.is_empty() && m.check_tag.starts_with("gate."))
        );
    }
}
