use decapod::core::store::{Store, StoreKind};
use decapod::plugins::decide::{
    complete_session, decision_trees, get_decision, get_session, initialize_decide_db,
    list_decisions, list_sessions, next_question, record_decision, start_session, suggest_trees,
};
use decapod::plugins::federation::initialize_federation_db;
use tempfile::tempdir;

fn test_store() -> (tempfile::TempDir, Store) {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_federation_db(&root).unwrap();
    initialize_decide_db(&root).unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root,
    };
    (tmp, store)
}

#[test]
fn test_decision_trees_available() {
    let trees = decision_trees();
    assert_eq!(trees.len(), 4);

    let ids: Vec<&str> = trees.iter().map(|t| t.id).collect();
    assert!(ids.contains(&"web-app"));
    assert!(ids.contains(&"microservice"));
    assert!(ids.contains(&"cli-tool"));
    assert!(ids.contains(&"library"));
}

#[test]
fn test_suggest_trees_web_app() {
    let suggestions = suggest_trees("build a web application");
    assert!(!suggestions.is_empty());
    assert_eq!(suggestions[0].tree_id, "web-app");
    assert!(suggestions[0].score > 0.0);
}

#[test]
fn test_suggest_trees_microservice() {
    let suggestions = suggest_trees("create a REST API microservice");
    assert!(!suggestions.is_empty());
    assert_eq!(suggestions[0].tree_id, "microservice");
}

#[test]
fn test_suggest_trees_cli() {
    let suggestions = suggest_trees("build a CLI tool for terminal");
    assert!(!suggestions.is_empty());
    assert_eq!(suggestions[0].tree_id, "cli-tool");
}

#[test]
fn test_suggest_trees_no_match() {
    let suggestions = suggest_trees("xyz qqq zzz");
    assert!(suggestions.is_empty());
}

#[test]
fn test_session_lifecycle() {
    let (_tmp, store) = test_store();

    // Start session
    let session = start_session(&store, "web-app", "Test App", "Testing", "test-agent").unwrap();
    assert!(session.id.starts_with("DS_"));
    assert_eq!(session.tree_id, "web-app");
    assert_eq!(session.status, "active");
    assert!(session.federation_node_id.is_some());

    // List sessions
    let sessions = list_sessions(&store, None).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, session.id);

    // List active sessions
    let active = list_sessions(&store, Some("active")).unwrap();
    assert_eq!(active.len(), 1);

    // Complete session
    complete_session(&store, &session.id).unwrap();

    // Verify completed
    let completed_session = get_session(&store, &session.id).unwrap();
    assert_eq!(completed_session.status, "completed");
    assert!(completed_session.completed_at.is_some());

    // List completed sessions
    let completed = list_sessions(&store, Some("completed")).unwrap();
    assert_eq!(completed.len(), 1);
}

#[test]
fn test_record_decision() {
    let (_tmp, store) = test_store();

    let session = start_session(&store, "cli-tool", "Test CLI", "", "test-agent").unwrap();

    let decision = record_decision(
        &store,
        &session.id,
        "language",
        "rust",
        "Love Rust",
        "test-agent",
    )
    .unwrap();

    assert!(decision.id.starts_with("DD_"));
    assert_eq!(decision.session_id, session.id);
    assert_eq!(decision.question_id, "language");
    assert_eq!(decision.chosen_value, "rust");
    assert_eq!(decision.chosen_label, "Rust");
    assert_eq!(decision.rationale, "Love Rust");
    assert!(decision.federation_node_id.is_some());
}

#[test]
fn test_duplicate_answer_rejected() {
    let (_tmp, store) = test_store();

    let session = start_session(&store, "cli-tool", "Test CLI", "", "test-agent").unwrap();

    record_decision(&store, &session.id, "language", "rust", "", "test-agent").unwrap();

    // Duplicate should fail
    let result = record_decision(&store, &session.id, "language", "go", "", "test-agent");
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("already answered"));
}

#[test]
fn test_invalid_tree_rejected() {
    let (_tmp, store) = test_store();

    let result = start_session(&store, "nonexistent-tree", "Bad", "", "test-agent");
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("Unknown tree"));
}

#[test]
fn test_invalid_option_rejected() {
    let (_tmp, store) = test_store();

    let session = start_session(&store, "cli-tool", "Test CLI", "", "test-agent").unwrap();

    let result = record_decision(&store, &session.id, "language", "cobol", "", "test-agent");
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("Invalid value"));
}

#[test]
fn test_record_on_completed_session_fails() {
    let (_tmp, store) = test_store();

    let session = start_session(&store, "cli-tool", "Test CLI", "", "test-agent").unwrap();
    complete_session(&store, &session.id).unwrap();

    let result = record_decision(&store, &session.id, "language", "rust", "", "test-agent");
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("not 'active'"));
}

#[test]
fn test_next_question_conditional() {
    let (_tmp, store) = test_store();

    let session = start_session(&store, "web-app", "Test Web", "", "test-agent").unwrap();

    // First question should be "runtime"
    let next = next_question(&store, &session.id).unwrap();
    assert!(!next.complete);
    let q = next.question.unwrap();
    assert_eq!(q["id"], "runtime");

    // Answer runtime = typescript
    record_decision(
        &store,
        &session.id,
        "runtime",
        "typescript",
        "",
        "test-agent",
    )
    .unwrap();

    // Next should be "framework" (typescript conditional), not "framework_wasm"
    let next = next_question(&store, &session.id).unwrap();
    assert!(!next.complete);
    let q = next.question.unwrap();
    assert_eq!(q["id"], "framework");
}

#[test]
fn test_next_question_wasm_conditional() {
    let (_tmp, store) = test_store();

    let session = start_session(&store, "web-app", "Test WASM", "", "test-agent").unwrap();

    // Answer runtime = wasm
    record_decision(&store, &session.id, "runtime", "wasm", "", "test-agent").unwrap();

    // Next should be "framework_wasm", not "framework"
    let next = next_question(&store, &session.id).unwrap();
    assert!(!next.complete);
    let q = next.question.unwrap();
    assert_eq!(q["id"], "framework_wasm");
}

#[test]
fn test_next_question_complete() {
    let (_tmp, store) = test_store();

    let session = start_session(&store, "cli-tool", "Test CLI", "", "test-agent").unwrap();

    // Answer all 4 questions
    record_decision(&store, &session.id, "language", "rust", "", "test-agent").unwrap();
    record_decision(
        &store,
        &session.id,
        "distribution",
        "binary",
        "",
        "test-agent",
    )
    .unwrap();
    record_decision(
        &store,
        &session.id,
        "config_format",
        "toml",
        "",
        "test-agent",
    )
    .unwrap();
    record_decision(
        &store,
        &session.id,
        "output_format",
        "text_json",
        "",
        "test-agent",
    )
    .unwrap();

    // Should be complete
    let next = next_question(&store, &session.id).unwrap();
    assert!(next.complete);
    assert!(next.question.is_none());
}

#[test]
fn test_get_session_with_decisions() {
    let (_tmp, store) = test_store();

    let session = start_session(&store, "cli-tool", "Test CLI", "", "test-agent").unwrap();
    record_decision(&store, &session.id, "language", "rust", "", "test-agent").unwrap();
    record_decision(
        &store,
        &session.id,
        "distribution",
        "binary",
        "",
        "test-agent",
    )
    .unwrap();

    let full_session = get_session(&store, &session.id).unwrap();
    let decisions = full_session.decisions.unwrap();
    assert_eq!(decisions.len(), 2);
    assert_eq!(decisions[0].question_id, "language");
    assert_eq!(decisions[1].question_id, "distribution");
}

#[test]
fn test_list_and_get_decisions() {
    let (_tmp, store) = test_store();

    let session = start_session(&store, "cli-tool", "Test CLI", "", "test-agent").unwrap();
    let d1 = record_decision(&store, &session.id, "language", "rust", "", "test-agent").unwrap();

    // List by session
    let decisions = list_decisions(&store, Some(&session.id), None).unwrap();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].id, d1.id);

    // List by tree
    let decisions = list_decisions(&store, None, Some("cli-tool")).unwrap();
    assert_eq!(decisions.len(), 1);

    // Get by ID
    let decision = get_decision(&store, &d1.id).unwrap();
    assert_eq!(decision.chosen_value, "rust");
}

#[test]
fn test_complete_nonexistent_session_fails() {
    let (_tmp, store) = test_store();

    let result = complete_session(&store, "DS_NONEXISTENT");
    assert!(result.is_err());
}

#[test]
fn test_trees_have_questions_and_options() {
    for tree in decision_trees() {
        assert!(
            !tree.questions.is_empty(),
            "Tree '{}' has no questions",
            tree.id
        );
        for q in tree.questions {
            assert!(
                !q.options.is_empty(),
                "Question '{}' in tree '{}' has no options",
                q.id,
                tree.id
            );
            for opt in q.options {
                assert!(!opt.value.is_empty());
                assert!(!opt.label.is_empty());
                assert!(!opt.rationale.is_empty());
            }
        }
    }
}
