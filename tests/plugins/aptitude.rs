use decapod::core::db::Connection;
use decapod::core::store::Store;
use decapod::core::store::StoreKind;
use decapod::plugins::aptitude::{
    PreferenceInput, add_preference, aptitude_db_path, delete_preference,
    generate_contextual_reminders, get_preference, get_preferences_by_category,
    get_prompts_for_context, initialize_aptitude_db, list_preferences, match_patterns,
    record_observation,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

fn resolve_decapod_bin() -> PathBuf {
    let cargo_bin = env!("CARGO_BIN_EXE_decapod");
    if let Ok(path) = Path::new(cargo_bin).canonicalize() {
        return path;
    }
    if let Ok(runfiles_dir) = std::env::var("RUNFILES_DIR") {
        let path = Path::new(&runfiles_dir).join("_main").join("decapod");
        if path.exists() {
            return path;
        }
    }
    if let Some(parent) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|path| path.to_path_buf()))
    {
        let path = parent.join("decapod");
        if path.exists() {
            return path;
        }
    }
    PathBuf::from(cargo_bin)
}

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

fn prompt_rows(conn: &Connection) -> Vec<serde_json::Value> {
    let mut stmt = conn
        .prepare(
            "SELECT id, context, prompt_text, priority, active, usage_count,
                    last_shown_at, created_at, updated_at
             FROM agent_prompts ORDER BY id",
        )
        .unwrap();
    let mut rows = stmt.query([]).unwrap();
    let mut result = Vec::new();
    while let Some(row) = rows.next().unwrap() {
        result.push(serde_json::to_value(row.parameters()).unwrap());
    }
    result
}

#[test]
fn repeated_initialization_preserves_prompt_rows_and_disabled_defaults() {
    let tmp = tempdir().unwrap();
    initialize_aptitude_db(tmp.path()).unwrap();
    let conn = Connection::open(aptitude_db_path(tmp.path())).unwrap();
    let default_id: String = conn
        .query_row(
            "SELECT id FROM agent_prompts ORDER BY context, prompt_text LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(
        conn.execute(
            "UPDATE agent_prompts
             SET active=0, usage_count=17, priority=3,
                 last_shown_at='historical', updated_at='retained'
             WHERE id=?1",
            decapod::params![&default_id],
        )
        .unwrap(),
        1
    );
    assert_eq!(
        conn.execute(
            "INSERT INTO agent_prompts(
                 id, context, prompt_text, priority, active, usage_count, created_at)
             SELECT 'custom-prompt', context, 'Retain custom configuration', 7, 0, 9, 'original'
             FROM agent_prompts WHERE id=?1",
            decapod::params![&default_id],
        )
        .unwrap(),
        1
    );
    assert_eq!(
        conn.execute(
            "INSERT INTO agent_prompts(
                 id, context, prompt_text, priority, active, usage_count, created_at)
             SELECT 'historical-duplicate', context, prompt_text, 4, 0, 22, 'original'
             FROM agent_prompts WHERE id=?1",
            decapod::params![&default_id],
        )
        .unwrap(),
        1
    );

    let before = prompt_rows(&conn);
    for _ in 0..8 {
        initialize_aptitude_db(tmp.path()).unwrap();
        assert_eq!(prompt_rows(&conn), before);
    }
}

#[test]
fn initialization_restores_only_the_missing_default_beside_a_custom_prompt() {
    let tmp = tempdir().unwrap();
    initialize_aptitude_db(tmp.path()).unwrap();
    let conn = Connection::open(aptitude_db_path(tmp.path())).unwrap();
    let (removed_id, context, prompt_text, priority): (String, String, String, i32) = conn
        .query_row(
            "SELECT id, context, prompt_text, priority
             FROM agent_prompts ORDER BY context, prompt_text LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        conn.execute(
            "DELETE FROM agent_prompts WHERE id=?1",
            decapod::params![&removed_id],
        )
        .unwrap(),
        1
    );
    conn.execute(
        "INSERT INTO agent_prompts(
             id, context, prompt_text, priority, active, usage_count,
             last_shown_at, created_at, updated_at)
         VALUES('same-context-custom', ?1, ?2, 7, 0, 9, 'historical', 'original', 'retained')",
        decapod::params![&context, format!("Custom alternative to: {prompt_text}")],
    )
    .unwrap();
    let retained = prompt_rows(&conn);

    initialize_aptitude_db(tmp.path()).unwrap();
    let current = prompt_rows(&conn);
    assert_eq!(current.len(), retained.len() + 1);
    for row in &retained {
        assert!(
            current.contains(row),
            "initializer changed a retained prompt"
        );
    }
    let restored_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM agent_prompts WHERE context=?1 AND prompt_text=?2",
            decapod::params![&context, &prompt_text],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(restored_count, 1);
    let (restored_id, restored_priority, active, usage_count): (String, i32, i32, i64) = conn
        .query_row(
            "SELECT id, priority, active, usage_count
             FROM agent_prompts WHERE context=?1 AND prompt_text=?2",
            decapod::params![&context, &prompt_text],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert!(!restored_id.is_empty());
    assert_ne!(restored_id, removed_id);
    assert_eq!(restored_priority, priority);
    assert_eq!(active, 1);
    assert_eq!(usage_count, 0);

    initialize_aptitude_db(tmp.path()).unwrap();
    assert_eq!(prompt_rows(&conn), current);
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
    assert!(db_path.to_string_lossy().ends_with("decapod.db"));
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

    let decapod_init = Command::new(resolve_decapod_bin())
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

    let help = Command::new(resolve_decapod_bin())
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

    let removed = Command::new(resolve_decapod_bin())
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
