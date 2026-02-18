use decapod::core::todo;
use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn run_decapod(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("failed to execute decapod");
    assert!(
        output.status.success(),
        "decapod {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn todo_help_schema_and_docs_stay_in_sync() {
    let expected = [
        "add",
        "list",
        "get",
        "done",
        "archive",
        "comment",
        "edit",
        "claim",
        "release",
        "rebuild",
        "categories",
        "register-agent",
        "ownerships",
        "heartbeat",
        "presence",
        "worker-run",
        "handoff",
        "add-owner",
        "remove-owner",
        "list-owners",
        "register-expertise",
        "expertise",
    ];

    let help = run_decapod(&["todo", "--help"]);
    for command in &expected {
        let re = Regex::new(&format!(r"(?m)^\s+{}\s+", regex::escape(command)))
            .expect("valid help regex");
        assert!(
            re.is_match(&help),
            "todo --help missing command: {}",
            command
        );
    }

    let schema = todo::schema();
    let schema_cmds: HashSet<String> = schema["commands"]
        .as_array()
        .expect("commands array")
        .iter()
        .filter_map(|item| {
            item.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    for command in &expected {
        assert!(
            schema_cmds.contains(*command),
            "todo schema missing command: {}",
            command
        );
    }

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let docs = fs::read_to_string(repo_root.join("constitution/plugins/TODO.md"))
        .expect("read TODO plugin docs");

    for command in &expected {
        assert!(
            docs.contains(&format!("decapod todo {}", command)),
            "plugins/TODO.md missing CLI surface entry for command: {}",
            command
        );
    }

    assert!(
        docs.contains("decapod data schema --subsystem todo"),
        "plugins/TODO.md missing JSON schema retrieval command"
    );
    assert!(
        !docs.contains("decapod todo schema"),
        "plugins/TODO.md references removed command: decapod todo schema"
    );
}

#[test]
fn container_help_schema_and_docs_stay_in_sync() {
    let help = run_decapod(&["auto", "container", "run", "--help"]);
    for flag in [
        "--agent",
        "--cmd",
        "--task-id",
        "--push",
        "--pr",
        "--pr-base",
        "--pr-title",
        "--pr-body",
        "--keep-worktree",
        "--inherit-env",
        "--local-only",
    ] {
        assert!(
            help.contains(flag),
            "container run --help missing flag: {}",
            flag
        );
    }

    let schema_out = run_decapod(&[
        "data",
        "schema",
        "--subsystem",
        "container",
        "--deterministic",
    ]);
    for field in [
        "\"task_id\"",
        "\"pr\"",
        "\"pr_base\"",
        "\"pr_title\"",
        "\"pr_body\"",
        "\"keep_worktree\"",
        "\"inherit_env\"",
        "\"local_only\"",
    ] {
        assert!(
            schema_out.contains(field),
            "container schema missing field: {}",
            field
        );
    }

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let docs = fs::read_to_string(repo_root.join("constitution/plugins/CONTAINER.md"))
        .expect("read CONTAINER plugin docs");
    for snippet in [
        "decapod auto container run",
        "--task-id",
        "--pr",
        "--inherit-env",
        "--local-only",
        "DECAPOD_CLAIM_AUTORUN",
    ] {
        assert!(
            docs.contains(snippet),
            "plugins/CONTAINER.md missing content: {}",
            snippet
        );
    }
}
