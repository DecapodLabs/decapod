//! Effective grant and delegation CLI contracts.
#[path = "support/cli_bin.rs"]
mod cli_bin;

fn resolve_decapod_bin() -> std::path::PathBuf {
    cli_bin::resolve_decapod_bin()
}

fn contract_fixture() -> tempfile::TempDir {
    cli_bin::contract_fixture()
}

mod effective_grant_cli_contracts {
    use serde_json::Value;
    use std::path::Path;
    use std::process::{Command, Output};
    use tempfile::TempDir;

    fn decapod_bin() -> std::path::PathBuf {
        super::resolve_decapod_bin()
    }

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Decapod Test")
            .env("GIT_AUTHOR_EMAIL", "test@decapod.local")
            .env("GIT_COMMITTER_NAME", "Decapod Test")
            .env("GIT_COMMITTER_EMAIL", "test@decapod.local")
            .args(args)
            .output()
            .expect("run git");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn run_cli(dir: &Path, agent: &str, session_pw: Option<&str>, args: &[&str]) -> Output {
        let mut cmd = Command::new(decapod_bin());
        cmd.current_dir(dir)
            .env("HOME", dir)
            .env("XDG_CONFIG_HOME", dir.join(".config"))
            .env("DECAPOD_AGENT_ID", agent)
            .env("DECAPOD_STORAGE_OWNER", "1")
            .env("DECAPOD_CLAIM_AUTORUN", "0")
            .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1");
        if let Some(pw) = session_pw {
            cmd.env("DECAPOD_SESSION_PASSWORD", pw);
        } else {
            cmd.env_remove("DECAPOD_SESSION_PASSWORD");
        }
        cmd.args(args).output().expect("run decapod")
    }

    fn setup_repo_with_session() -> (TempDir, String) {
        let fixture = super::contract_fixture();
        let root = fixture.path();
        git(root, &["init", "-b", "master"]);
        let init = run_cli(root, "coordinator", None, &["init", "--force"]);
        assert!(
            init.status.success(),
            "init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );
        std::fs::create_dir_all(root.join("src/api")).unwrap();
        std::fs::write(root.join("src/api/routes.rs"), "pub fn ok() {}\n").unwrap();
        std::fs::create_dir_all(root.join(".github/workflows")).unwrap();
        std::fs::write(root.join(".github/workflows/ci.yml"), "name: ci\n").unwrap();

        let cfg = r#"schema_version = "1.0.0"

[init]
specs = true
ci = false
diagram_style = "ascii"
entrypoints = ["AGENTS.md"]

[repo]
name = "cli-grant-test"
summary = "Effective grant CLI test"
base_branch = "master"

[governance]
protected_paths = [".github/workflows/**"]
approval_categories = ["destructive_operations"]
"#;
        std::fs::write(root.join(".decapod/config.toml"), cfg).unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-m", "init"]);
        git(root, &["checkout", "-b", "agent/coordinator/grant-test"]);

        let sess = run_cli(root, "coordinator", None, &["session", "acquire"]);
        assert!(
            sess.status.success(),
            "session acquire failed: {}",
            String::from_utf8_lossy(&sess.stderr)
        );
        let sess_stdout = String::from_utf8_lossy(&sess.stdout);
        let pw = sess_stdout
            .lines()
            .find_map(|line| {
                let t = line.trim();
                t.strip_prefix("Password:")
                    .or_else(|| t.strip_prefix("DECAPOD_SESSION_PASSWORD="))
                    .or_else(|| t.strip_prefix("export DECAPOD_SESSION_PASSWORD="))
            })
            .map(|s| s.trim().trim_matches('\'').trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| panic!("session password line missing in:\n{sess_stdout}"));
        (fixture, pw)
    }

    #[test]
    fn red_green_5_cli_rpc_grant_refusal_lawful_repair_and_fresh_authorization() {
        let (fixture, pw) = setup_repo_with_session();
        let root = fixture.path();

        let parent_out = run_cli(
            root,
            "coordinator",
            Some(&pw),
            &[
                "todo",
                "add",
                "Parent grant test",
                "--scope",
                "allow:[src/**];deny:[.github/workflows/**]",
                "--dir",
                root.to_str().unwrap(),
                "--format",
                "json",
            ],
        );
        assert!(parent_out.status.success());
        let parent_id = serde_json::from_slice::<Value>(&parent_out.stdout).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        let claim_p = run_cli(
            root,
            "coordinator",
            Some(&pw),
            &["todo", "claim", "--id", &parent_id, "--format", "json"],
        );
        assert!(claim_p.status.success());

        let child_out = run_cli(
            root,
            "coordinator",
            Some(&pw),
            &[
                "todo",
                "add",
                "Child API task",
                "--parent",
                &parent_id,
                "--scope",
                "allow:[src/api/**]",
                "--dir",
                root.to_str().unwrap(),
                "--format",
                "json",
            ],
        );
        assert!(child_out.status.success());
        let child_id = serde_json::from_slice::<Value>(&child_out.stdout).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        let claim_c = run_cli(
            root,
            "coordinator",
            Some(&pw),
            &["todo", "claim", "--id", &child_id, "--format", "json"],
        );
        assert!(claim_c.status.success());

        // 1) RPC policy.grant.explain denies .github/workflows/ci.yml via parent denial + narrow scope.
        let rpc_params = serde_json::json!({
            "command": "workspace.publish",
            "path": ".github/workflows/ci.yml",
            "task_id": child_id,
        })
        .to_string();
        let rpc_out = run_cli(
            root,
            "coordinator",
            Some(&pw),
            &[
                "rpc",
                "--op",
                "policy.grant.explain",
                "--params",
                &rpc_params,
            ],
        );
        assert!(
            rpc_out.status.success(),
            "rpc policy.grant.explain failed: {}",
            String::from_utf8_lossy(&rpc_out.stderr)
        );
        let rpc_json: Value = serde_json::from_slice(&rpc_out.stdout).unwrap();
        assert_eq!(
            rpc_json["success"], true,
            "rpc policy.grant.explain failed: {rpc_json:#}"
        );
        assert_eq!(rpc_json["result"]["decision"], "denied");
        assert!(rpc_json["result"]["prestate"]["policy_revision"].is_string());

        // 2) Operator acquires fixture session; basic agent sees denied trust floor and cannot self-approve,
        // while operator sees unproven before approval and todo.archive refuses until approved.
        let basic_grant = run_cli(
            root,
            "coordinator",
            Some(&pw),
            &[
                "govern",
                "policy",
                "grant",
                "--command",
                "task.archive",
                "--path",
                &child_id,
                "--task-id",
                &child_id,
                "--format",
                "json",
            ],
        );
        assert!(basic_grant.status.success());
        let g_basic: Value = serde_json::from_slice(&basic_grant.stdout).unwrap();
        assert_eq!(g_basic["decision"], "denied");

        let op_sess = run_cli(root, "operator", None, &["session", "acquire"]);
        assert!(op_sess.status.success());
        let op_stdout = String::from_utf8_lossy(&op_sess.stdout);
        let op_pw = op_stdout
            .lines()
            .find_map(|line| {
                let t = line.trim();
                t.strip_prefix("Password:")
                    .or_else(|| t.strip_prefix("DECAPOD_SESSION_PASSWORD="))
                    .or_else(|| t.strip_prefix("export DECAPOD_SESSION_PASSWORD="))
            })
            .map(|s| s.trim().trim_matches('\'').trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| panic!("operator session password missing in:\n{op_stdout}"));

        let op_child_out = run_cli(
            root,
            "operator",
            Some(&op_pw),
            &[
                "todo",
                "add",
                "Operator child task",
                "--parent",
                &parent_id,
                "--scope",
                "allow:[src/api/**]",
                "--dir",
                root.to_str().unwrap(),
                "--format",
                "json",
            ],
        );
        assert!(op_child_out.status.success());
        let op_child_id = serde_json::from_slice::<Value>(&op_child_out.stdout).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            run_cli(
                root,
                "operator",
                Some(&op_pw),
                &["todo", "claim", "--id", &op_child_id, "--format", "json"],
            )
            .status
            .success()
        );

        let grant_before = run_cli(
            root,
            "operator",
            Some(&op_pw),
            &[
                "govern",
                "policy",
                "grant",
                "--command",
                "task.archive",
                "--path",
                &op_child_id,
                "--task-id",
                &op_child_id,
                "--format",
                "json",
            ],
        );
        assert!(grant_before.status.success());
        let g_before: Value = serde_json::from_slice(&grant_before.stdout).unwrap();
        assert_eq!(g_before["decision"], "unproven");
        assert_eq!(g_before["approval_required"], true);
        let fp = g_before["target_fingerprint"].as_str().unwrap().to_string();

        let archive_refused = run_cli(
            root,
            "operator",
            Some(&op_pw),
            &["todo", "archive", "--id", &op_child_id],
        );
        assert!(
            !archive_refused.status.success(),
            "unapproved archive must fail"
        );

        // 3) Agent cannot self-approve; lawful repair requires operator session in the fixture.
        let self_approve = run_cli(
            root,
            "coordinator",
            Some(&pw),
            &[
                "govern", "policy", "approve", "--id", &fp, "--scope", "global",
            ],
        );
        assert!(
            !self_approve.status.success(),
            "basic agent must not self-approve policy actions"
        );

        let approve = run_cli(
            root,
            "operator",
            Some(&op_pw),
            &[
                "govern", "policy", "approve", "--id", &fp, "--scope", "global",
            ],
        );
        assert!(
            approve.status.success(),
            "operator policy approve failed: {}",
            String::from_utf8_lossy(&approve.stderr)
        );

        let grant_after = run_cli(
            root,
            "operator",
            Some(&op_pw),
            &[
                "govern",
                "policy",
                "grant",
                "--command",
                "task.archive",
                "--path",
                &op_child_id,
                "--task-id",
                &op_child_id,
                "--format",
                "json",
            ],
        );
        assert!(grant_after.status.success());
        let g_after: Value = serde_json::from_slice(&grant_after.stdout).unwrap();
        assert_eq!(g_after["decision"], "allowed", "{g_after:?}");
        assert!(
            !g_after["approval_ids_matched"]
                .as_array()
                .unwrap()
                .is_empty()
        );

        let archive_ok = run_cli(
            root,
            "operator",
            Some(&op_pw),
            &["todo", "archive", "--id", &op_child_id, "--format", "json"],
        );
        assert!(
            archive_ok.status.success(),
            "archive after approval failed: {}",
            String::from_utf8_lossy(&archive_ok.stderr)
        );
    }

    #[test]
    fn cli_cumulative_delegation_budget_rejects_sibling_overflow() {
        let (fixture, pw) = setup_repo_with_session();
        let root = fixture.path();

        let p_out = run_cli(
            root,
            "coordinator",
            Some(&pw),
            &[
                "todo",
                "add",
                "Shared parent budget",
                "--scope",
                "allow:[src/**]",
                "--dir",
                root.to_str().unwrap(),
                "--format",
                "json",
            ],
        );
        let parent_id = serde_json::from_slice::<Value>(&p_out.stdout).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            run_cli(
                root,
                "coordinator",
                Some(&pw),
                &["todo", "claim", "--id", &parent_id, "--format", "json"],
            )
            .status
            .success()
        );

        let mut children = Vec::new();
        for (label, cap) in [("c1", "2"), ("c2", "2"), ("c3", "1")] {
            let out = run_cli(
                root,
                "coordinator",
                Some(&pw),
                &[
                    "todo",
                    "add",
                    label,
                    "--parent",
                    &parent_id,
                    "--scope",
                    "allow:[src/api/**]",
                    "--dir",
                    root.to_str().unwrap(),
                    "--format",
                    "json",
                ],
            );
            let cid = serde_json::from_slice::<Value>(&out.stdout).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string();
            let dlg = run_cli(
                root,
                "coordinator",
                Some(&pw),
                &[
                    "todo",
                    "delegate",
                    "--parent",
                    &parent_id,
                    "--child",
                    &cid,
                    "--allow",
                    "src/api/**",
                    "--budget-total",
                    "3",
                    "--budget-child",
                    cap,
                    "--depth",
                    "1",
                    "--format",
                    "json",
                ],
            );
            assert!(
                dlg.status.success(),
                "delegate {label} failed: {}",
                String::from_utf8_lossy(&dlg.stderr)
            );
            children.push(cid);
        }

        // First claim reserves 2/3 -> succeeds.
        assert!(
            run_cli(
                root,
                "worker-1",
                Some(&pw),
                &["todo", "claim", "--id", &children[0], "--format", "json"],
            )
            .status
            .success()
        );
        // Second claim wants 2 more -> 4 > 3 -> fails atomically.
        let c2_claim = run_cli(
            root,
            "worker-2",
            Some(&pw),
            &["todo", "claim", "--id", &children[1], "--format", "json"],
        );
        assert!(!c2_claim.status.success());
        assert!(
            String::from_utf8_lossy(&c2_claim.stderr)
                .contains("CUMULATIVE_DELEGATION_BUDGET_EXCEEDED")
        );
        // Third claim wants 1 -> 2 + 1 = 3 <= 3 -> succeeds.
        assert!(
            run_cli(
                root,
                "worker-3",
                Some(&pw),
                &["todo", "claim", "--id", &children[2], "--format", "json"],
            )
            .status
            .success()
        );
    }

    #[test]
    fn guidance_surfaces_exclude_maintainer_entrypoint_drift_setting() {
        let (fixture, pw) = setup_repo_with_session();
        let root = fixture.path();
        let needle = "_allow_entrypoint_version_drift";

        for args in [
            vec!["--help"],
            vec!["validate", "--help"],
            vec!["init", "--help"],
            vec!["govern", "--help"],
            vec!["docs", "ingest"],
            vec!["rpc", "--op", "context.resolve", "--params", "{}"],
        ] {
            let out = run_cli(root, "coordinator", Some(&pw), &args);
            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(
                !combined.contains(needle),
                "decapod {:?} leaked maintainer setting {needle}",
                args
            );
        }
    }
}
