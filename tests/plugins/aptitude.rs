use decapod::core::store::Store;
use decapod::core::store::StoreKind;
use decapod::plugins::aptitude::{
    PreferenceInput, add_preference, aptitude_db_path, delete_preference,
    generate_contextual_reminders, get_preference, get_preferences_by_category,
    get_prompts_for_context, initialize_aptitude_db, list_preferences, match_patterns,
    record_observation,
};
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_preference_lifecycle() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_aptitude_db(&root).unwrap();

    let store = Store {
        kind: StoreKind::Repo,
        root: root.clone(),
    };

    // 1. Add preference
    let input = PreferenceInput {
        category: "git".to_string(),
        key: "ssh_key".to_string(),
        value: "ed25519".to_string(),
        context: Some("for GitHub".to_string()),
        source: "user_request".to_string(),
        confidence: Some(100),
    };
    let id = add_preference(&store, input).unwrap();
    assert!(!id.is_empty());

    // 2. Get preference
    let pref = get_preference(&store, "git", "ssh_key")
        .unwrap()
        .expect("Preference not found");
    assert_eq!(pref.category, "git");
    assert_eq!(pref.key, "ssh_key");
    assert_eq!(pref.value, "ed25519");
    assert_eq!(pref.access_count, 1);

    // 3. List preferences
    let prefs = list_preferences(&store, Some("git")).unwrap();
    assert_eq!(prefs.len(), 1);

    // 4. List by category
    let grouped = get_preferences_by_category(&store).unwrap();
    assert!(grouped.contains_key("git"));

    // 5. Delete preference
    let deleted = delete_preference(&store, "git", "ssh_key").unwrap();
    assert!(deleted);

    let missing = get_preference(&store, "git", "ssh_key").unwrap();
    assert!(missing.is_none());
}

#[test]
fn test_preference_update_on_conflict() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_aptitude_db(&root).unwrap();

    let store = Store {
        kind: StoreKind::Repo,
        root: root.clone(),
    };

    // Add initial preference
    let input = PreferenceInput {
        category: "style".to_string(),
        key: "indent".to_string(),
        value: "2".to_string(),
        context: None,
        source: "user_request".to_string(),
        confidence: Some(80),
    };
    add_preference(&store, input).unwrap();

    // Update with same category/key
    let input2 = PreferenceInput {
        category: "style".to_string(),
        key: "indent".to_string(),
        value: "4".to_string(),
        context: Some("updated".to_string()),
        source: "user_request".to_string(),
        confidence: Some(100),
    };
    add_preference(&store, input2).unwrap();

    // Should have only one preference
    let prefs = list_preferences(&store, None).unwrap();
    assert_eq!(prefs.len(), 1);
    assert_eq!(prefs[0].value, "4");
    assert_eq!(prefs[0].confidence, 100);
}

#[test]
fn test_pattern_matching() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_aptitude_db(&root).unwrap();

    let store = Store {
        kind: StoreKind::Repo,
        root: root.clone(),
    };

    // Test against default patterns
    let matches = match_patterns(&store, "I always use 4 spaces for indentation").unwrap();
    assert!(!matches.is_empty());

    let matches = match_patterns(&store, "prefer to use conventional commits").unwrap();
    assert!(!matches.is_empty());

    let matches = match_patterns(&store, "use ssh key ed25519").unwrap();
    assert!(!matches.is_empty());
}

#[test]
fn test_observation_recording() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_aptitude_db(&root).unwrap();

    let store = Store {
        kind: StoreKind::Repo,
        root: root.clone(),
    };

    // Record observation
    let id = record_observation(&store, "I always prefer tabs over spaces", Some("style")).unwrap();
    assert!(!id.is_empty());
}

#[test]
fn test_agent_prompts() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_aptitude_db(&root).unwrap();

    let store = Store {
        kind: StoreKind::Repo,
        root: root.clone(),
    };

    // Get prompts for context
    let prompts = get_prompts_for_context(&store, "git_operations", None).unwrap();
    assert!(!prompts.is_empty());
    assert_eq!(prompts[0].context, "git_operations");
}

#[test]
fn test_contextual_reminders() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    initialize_aptitude_db(&root).unwrap();

    let store = Store {
        kind: StoreKind::Repo,
        root: root.clone(),
    };

    // Add a preference
    let input = PreferenceInput {
        category: "git".to_string(),
        key: "branch_pattern".to_string(),
        value: "feature/".to_string(),
        context: None,
        source: "user_request".to_string(),
        confidence: Some(100),
    };
    add_preference(&store, input).unwrap();

    // Get reminders
    let reminders = generate_contextual_reminders(&store, "git").unwrap();
    assert!(!reminders.is_empty());
}

#[test]
fn test_db_path() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let db_path = aptitude_db_path(&root);
    assert!(db_path.to_string_lossy().ends_with("memory.db"));
}

#[test]
fn test_cli_has_no_skill_subcommand() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path();
    let init = Command::new("git")
        .current_dir(dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let decapod_init = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(dir)
        .args(["init", "--force"])
        .output()
        .expect("decapod init");
    assert!(
        decapod_init.status.success(),
        "decapod init failed: {}\n{}",
        String::from_utf8_lossy(&decapod_init.stdout),
        String::from_utf8_lossy(&decapod_init.stderr)
    );

    let help = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(dir)
        .args(["data", "aptitude", "--help"])
        .output()
        .expect("aptitude help");
    assert!(help.status.success(), "aptitude help failed");
    let help_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&help.stdout),
        String::from_utf8_lossy(&help.stderr)
    );
    assert!(
        !help_text.contains("skill"),
        "aptitude help should not expose skill commands:\n{help_text}"
    );

    let removed = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(dir)
        .args(["data", "aptitude", "skill"])
        .output()
        .expect("removed subcommand");
    assert!(
        !removed.status.success(),
        "removed skill subcommand should be rejected"
    );
    let removed_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&removed.stdout),
        String::from_utf8_lossy(&removed.stderr)
    );
    assert!(
        removed_text.contains("unrecognized subcommand"),
        "removed skill subcommand should fail as an unknown command:\n{removed_text}"
    );
}
