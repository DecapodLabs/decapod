// Moved from src/decapod/plugins/selective_test.rs
use super::*;

#[test]
fn parses_comma_and_space_separated_paths() {
    assert_eq!(
        get_changed_files_from_arg("src/main.rs, tests/cli.rs Cargo.toml"),
        vec![
            "src/main.rs".to_string(),
            "tests/cli.rs".to_string(),
            "Cargo.toml".to_string()
        ]
    );
}

#[test]
fn maps_current_source_layout_to_existing_test_targets() {
    let mut tests_to_run = HashMap::new();
    add_tests_for_file("src/decapod/core/todo.rs", &mut tests_to_run);
    add_tests_for_file("src/decapod/plugins/selective_test.rs", &mut tests_to_run);

    assert!(tests_to_run.contains_key("todo_enforcement"));
    assert!(tests_to_run.contains_key("todo_rebuild_compat"));
    assert!(tests_to_run.contains_key("cli_contract_enforcement"));
    assert!(!tests_to_run.contains_key("plugins_selective_test_tests"));
}

#[test]
fn ignores_generated_and_fixture_paths() {
    assert!(is_ignored_path(".decapod/generated/specs/README.md"));
    assert!(is_ignored_path("tests/fixtures/repo/file.txt"));
    assert!(!is_ignored_path("src/decapod/lib.rs"));
}
