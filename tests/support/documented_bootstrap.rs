//! Execute the generated agent instructions, not a bypassed orientation fixture.
use std::path::Path;
use std::process::{Command, Output};

fn isolated_command(program: &Path, cwd: &Path, home: &Path) -> Command {
    let mut command = Command::new(program);
    command.current_dir(cwd);
    // CI and a developer's active session must not change this fixture's gates,
    // datastore, Git checkout, or identity. In particular, presence of
    // DECAPOD_VALIDATE_SKIP_GIT_GATES would invalidate the negative controls.
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if name.starts_with("DECAPOD_") || name.starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .env_remove("GITHUB_ACTIONS")
        .env_remove("BUILDKITE")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("DECAPOD_AGENT_ID", "bootstrap-test");
    command
}

fn success(output: Output, step: &str) -> Output {
    assert!(
        output.status.success(),
        "{step} failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn generated_agent_sequence_orients_only_after_entering_claimed_workspace() {
    let fixture = super::contract_fixture();
    let root = fixture.path().join("repo");
    let home = fixture.path().join("home");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let binary = super::resolve_decapod_bin();
    let cli = |cwd: &Path, args: &[&str]| {
        isolated_command(&binary, cwd, &home)
            .args(args)
            .output()
            .expect("execute real decapod CLI")
    };
    let git = |cwd: &Path, args: &[&str]| {
        success(
            isolated_command(Path::new("git"), cwd, &home)
                .args(args)
                .output()
                .expect("execute git"),
            &format!("git {args:?}"),
        )
    };

    // Human provisioning is deliberately outside the agent sequence. This is a
    // local-backend Git-worktree test, not Docker or an installer/network test.
    success(
        cli(
            &root,
            &["init", "--no-git", "--no-ci", "--no-container-workspaces"],
        ),
        "human init",
    );
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.name", "Bootstrap Test"]);
    git(&root, &["config", "user.email", "bootstrap@example.invalid"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "Initialize governed fixture"]);
    let initial_head = git(&root, &["rev-parse", "HEAD"]).stdout;

    let instructions = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();
    let initialization = instructions
        .split("## Mandatory Initialization")
        .nth(1)
        .expect("generated initialization section")
        .split("```bash\n")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    // Only the published agent commands are interpreted, without invoking a
    // shell. Package installation/update is a separate human provisioning concern.
    let commands: Vec<_> = initialization
        .lines()
        .filter(|line| line.starts_with("decapod ") || line.starts_with("cd "))
        .flat_map(|line| line.split(" && "))
        .collect();
    assert_eq!(commands.first(), Some(&"decapod docs ingest"));
    assert_eq!(commands.last(), Some(&"decapod rpc --op context.resolve"));
    let mut cwd = root.clone();
    let mut task_id = String::new();
    let mut workspace = None;
    let mut oriented = false;
    let task = "document-bootstrap-order";
    let deny_root_orientation = |task_id: &str| {
        let output = cli(&root, &["infer", "orientation", "--task-id", task_id]);
        assert!(!output.status.success(), "main checkout accepted orientation");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("requires isolated git worktree")
                && stderr.contains("current checkout is not a worktree"),
            "wrong denial: {stderr}"
        );
    };

    for template in commands {
        if template == "cd .decapod/workspaces/<your-worktree>" {
            cwd = workspace.clone().expect("ensure must precede cd");
            assert!(cwd.join(".git").is_file(), "must enter a real Git worktree");
            continue;
        }
        let expanded = template
            .replace("\"<task>\"", task)
            .replace("<task-id>", &task_id)
            .replace("<agent-id>", "bootstrap-test")
            .replace("<topic>", "bootstrap-order");
        assert!(!expanded.contains('<'), "unhandled instruction: {expanded}");
        let args: Vec<_> = expanded.split_whitespace().skip(1).collect();
        if args.starts_with(&["workspace", "ensure"]) {
            assert!(!task_id.is_empty());
            deny_root_orientation(&task_id);
        }
        let output = success(cli(&cwd, &args), &expanded);
        match args.as_slice() {
            ["todo", "add", ..] => {
                let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                task_id = value["id"].as_str().expect("created task id").to_owned();
            }
            ["todo", "claim", ..] => {
                let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(value["id"], task_id);
                assert_eq!(value["result"]["assigned_to"], "bootstrap-test");
                assert_eq!(value["result"]["lease_state"], "active");
            }
            ["workspace", "ensure", ..] => {
                let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(value["can_work"], true);
                assert_eq!(value["blockers"], serde_json::json!([]));
                let path = std::path::PathBuf::from(value["worktree_path"].as_str().unwrap());
                assert!(path.starts_with(root.join(".decapod/workspaces")));
                workspace = Some(path);
                // Creating a workspace alone is insufficient: the caller must cd.
                deny_root_orientation(&task_id);
            }
            ["infer", "orientation", ..] => {
                assert_ne!(cwd, root);
                assert_eq!(Some(&cwd), workspace.as_ref());
                assert!(!cwd.join("implementation.txt").exists());
                let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(value["task_id"], task_id);
                assert_eq!(value["user_goal"], task);
                assert_eq!(value["governed_plan"]["task_binding"], "bound");
                assert_eq!(value["decision_gates"], serde_json::json!([]));
                oriented = true;
            }
            ["rpc", "--op", "context.resolve"] => assert!(oriented),
            _ => {}
        }
    }
    assert!(oriented, "generated instructions must execute orientation");
    // Implementation starts only after the complete documented initialization.
    std::fs::write(cwd.join("implementation.txt"), "workspace-only change\n").unwrap();
    assert!(!root.join("implementation.txt").exists());
    assert_eq!(git(&root, &["rev-parse", "HEAD"]).stdout, initial_head);
    assert!(git(&root, &["status", "--porcelain"]).stdout.is_empty());
}
