//! Real-binary integration tests for #1956 and #1957 recovery workflows.

use decapod::core::db::Connection;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;

fn decapod_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_decapod"))
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {:?} failed in {}:\nstdout:{}\nstderr:{}",
        args,
        dir.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_decapod(
    xdg_root: &Path,
    dir: &Path,
    args: &[&str],
    agent: &str,
    password: Option<&str>,
) -> Output {
    let mut cmd = Command::new(decapod_bin());
    cmd.current_dir(dir)
        .args(args)
        .env("XDG_CONFIG_HOME", xdg_root.join(".xdg-cfg"))
        .env("XDG_DATA_HOME", xdg_root.join(".xdg-data"))
        .env("DECAPOD_AGENT_ID", agent)
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env_remove("DECAPOD_SESSION_PASSWORD");
    if let Some(pw) = password {
        cmd.env("DECAPOD_SESSION_PASSWORD", pw);
    }
    cmd.output().unwrap()
}

fn acquire_session(xdg_root: &Path, dir: &Path, agent: &str) -> String {
    let out = run_decapod(xdg_root, dir, &["session", "acquire"], agent, None);
    assert!(
        out.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| {
            let trimmed = l.trim();
            trimmed
                .strip_prefix("Password:")
                .or_else(|| trimmed.strip_prefix("DECAPOD_SESSION_PASSWORD="))
                .or_else(|| trimmed.strip_prefix("export DECAPOD_SESSION_PASSWORD="))
        })
        .map(|s| s.trim().trim_matches('\'').trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .expect("password in session acquire output")
}

fn extract_json(out: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let idx = stdout
        .find('{')
        .unwrap_or_else(|| panic!("expected JSON stdout, got:\n{stdout}"));
    serde_json::from_str(&stdout[idx..]).unwrap_or_else(|e| panic!("invalid JSON ({e}):\n{stdout}"))
}

#[test]
fn issue_1956_terminal_reflection_uncommitted_recovery_preserves_budget_and_honors_authority() {
    let tmp = tempdir().unwrap();
    let xdg = tmp.path();
    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join("src")).unwrap();
    fs::write(repo.join("README.md"), "# Reflection Recovery Fixture\n").unwrap();
    fs::write(repo.join("src/lib.rs"), "pub fn base() -> u32 { 1 }\n").unwrap();
    git(&repo, &["init", "-b", "master"]);
    git(&repo, &["config", "user.email", "gov@decapod.local"]);
    git(&repo, &["config", "user.name", "Gov Fixture"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "initial"]);

    let init = run_decapod(
        xdg,
        &repo,
        &[
            "init",
            "--force",
            "--no-ci",
            "--no-specs",
            "--no-container-workspaces",
        ],
        "alice",
        None,
    );
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "decapod init"]);

    let pw = acquire_session(xdg, &repo, "alice");
    let t1_json = extract_json(&run_decapod(
        xdg,
        &repo,
        &[
            "todo",
            "add",
            "Blocked workstream task",
            "--scope",
            "src",
            "--format",
            "json",
        ],
        "alice",
        Some(&pw),
    ));
    let t1 = t1_json["id"].as_str().unwrap().to_string();
    assert!(
        run_decapod(
            xdg,
            &repo,
            &["todo", "claim", "--id", &t1, "--format", "json"],
            "alice",
            Some(&pw)
        )
        .status
        .success()
    );
    let b1 = format!("agent/alice/{t1}-blocked");
    let ws1 = extract_json(&run_decapod(
        xdg,
        &repo,
        &["workspace", "ensure", "--branch", &b1],
        "alice",
        Some(&pw),
    ));
    let wt1 = PathBuf::from(ws1["worktree_path"].as_str().unwrap());

    assert!(
        run_decapod(
            xdg,
            &wt1,
            &[
                "govern",
                "plan",
                "init",
                "--title",
                "Initial work",
                "--intent",
                "Update governed runtime safely",
                "--todo-id",
                &t1,
                "--proof-hook",
                "cargo test",
                "--forbidden-path",
                "src/mutations.rs",
                "--forbidden-path",
                "src/secret.rs",
            ],
            "alice",
            Some(&pw),
        )
        .status
        .success()
    );
    assert!(
        run_decapod(
            xdg,
            &wt1,
            &["govern", "plan", "approve"],
            "alice",
            Some(&pw)
        )
        .status
        .success()
    );

    // Round 1 -> reflect on forbidden src/mutations.rs
    let r1 = serde_json::json!({
        "task_id": t1,
        "submission_id": "round-1",
        "op": "change",
        "touched_paths": ["src/mutations.rs"],
        "rationale": "Initial proposal"
    });
    let out1 = run_decapod(
        xdg,
        &wt1,
        &[
            "infer",
            "orientation",
            "--task-id",
            &t1,
            "--reflection",
            &r1.to_string(),
        ],
        "alice",
        Some(&pw),
    );
    assert!(
        out1.status.success(),
        "{}",
        String::from_utf8_lossy(&out1.stderr)
    );
    let refl1 = extract_json(&out1)["reflection"].clone();
    assert_eq!(refl1["disposition"], "reflect", "{refl1}");

    // Round 2 -> expand obligations into src/secret.rs -> terminal blocked: obligations_regressed
    let r2 = serde_json::json!({
        "reflection": {
            "task_id": t1,
            "submission_id": "round-2-regress",
            "expected_token": refl1["token"],
            "op": "change",
            "touched_paths": ["src/mutations.rs", "src/secret.rs"],
            "evidence": refl1["challenge"]["sources"],
            "rationale": "Widen into forbidden secret module"
        }
    });
    let out2 = run_decapod(
        xdg,
        &wt1,
        &[
            "rpc",
            "--op",
            "mentor.obligations",
            "--params",
            &r2.to_string(),
        ],
        "alice",
        Some(&pw),
    );
    assert!(
        out2.status.success(),
        "{}",
        String::from_utf8_lossy(&out2.stderr)
    );
    let refl2 = extract_json(&out2)["result"]["reflection"].clone();
    assert_eq!(refl2["disposition"], "blocked", "{refl2}");
    assert_eq!(refl2["reason"], "obligations_regressed", "{refl2}");
    assert_eq!(refl2["terminal"], true, "{refl2}");

    // Leave terminal reflection receipt in .decapod/governance.json AND authored work uncommitted.
    fs::write(
        wt1.join("src/feature.rs"),
        "pub fn recovered_feature() -> u32 { 42 }\n",
    )
    .unwrap();

    let blocked_exec = run_decapod(
        xdg,
        &wt1,
        &["govern", "plan", "check-execute", "--todo-id", &t1],
        "alice",
        Some(&pw),
    );
    assert!(!blocked_exec.status.success());
    let blocked_err = String::from_utf8_lossy(&blocked_exec.stderr);
    assert!(
        blocked_err.contains("REFLECTION_BLOCKED: obligations_regressed"),
        "{blocked_err}"
    );

    let begin_before_commit = run_decapod(
        xdg,
        &wt1,
        &[
            "govern",
            "artifacts",
            "begin-pr",
            "--id",
            "recovery-pr",
            "--base-branch",
            "master",
        ],
        "alice",
        Some(&pw),
    );
    assert!(!begin_before_commit.status.success());
    assert!(
        String::from_utf8_lossy(&begin_before_commit.stderr).contains("GOVERNANCE_UNARCHIVED_WORK"),
        "{}",
        String::from_utf8_lossy(&begin_before_commit.stderr)
    );

    // 1) Preserve terminal receipt & uncommitted edits on stopped branch without claiming proof.
    git(&wt1, &["add", "--all", "--", ":!.decapod/governance.json"]);
    let ckpt_wip = run_decapod(
        xdg,
        &wt1,
        &[
            "govern",
            "artifacts",
            "checkpoint",
            "--id",
            &format!("{t1}-blocked-wip"),
            "--summary",
            "Preserve terminal reflection receipt and WIP without proof",
        ],
        "alice",
        Some(&pw),
    );
    assert!(
        ckpt_wip.status.success(),
        "{}",
        String::from_utf8_lossy(&ckpt_wip.stderr)
    );
    git(&wt1, &["add", ".decapod/governance.json"]);
    git(&wt1, &["commit", "-m", "archive terminal reflection wip"]);
    let stopped_commit = git(&wt1, &["rev-parse", "HEAD"]);

    // Original branch remains blocked, budget intact, and begin-pr rejects unmerged rotation.
    let still_blocked = run_decapod(
        xdg,
        &wt1,
        &["govern", "plan", "check-execute", "--todo-id", &t1],
        "alice",
        Some(&pw),
    );
    assert!(!still_blocked.status.success());
    assert!(
        String::from_utf8_lossy(&still_blocked.stderr)
            .contains("REFLECTION_BLOCKED: obligations_regressed")
    );
    let begin_after_commit = run_decapod(
        xdg,
        &wt1,
        &[
            "govern",
            "artifacts",
            "begin-pr",
            "--id",
            "recovery-pr",
            "--base-branch",
            "master",
        ],
        "alice",
        Some(&pw),
    );
    assert!(!begin_after_commit.status.success());
    assert!(
        String::from_utf8_lossy(&begin_after_commit.stderr).contains("GOVERNANCE_PR_NOT_ACCEPTED"),
        "{}",
        String::from_utf8_lossy(&begin_after_commit.stderr)
    );

    // 2) Start new accepted workstream from master after subsequent operator authority grant.
    let t2_json = extract_json(&run_decapod(
        xdg,
        &repo,
        &[
            "todo",
            "add",
            "Accepted follow-up workstream",
            "--scope",
            "src",
            "--format",
            "json",
        ],
        "alice",
        Some(&pw),
    ));
    let t2 = t2_json["id"].as_str().unwrap().to_string();
    assert!(
        run_decapod(
            xdg,
            &repo,
            &["todo", "claim", "--id", &t2, "--format", "json"],
            "alice",
            Some(&pw)
        )
        .status
        .success()
    );
    let b2 = format!("agent/alice/{t2}-recover");
    let ws2 = extract_json(&run_decapod(
        xdg,
        &repo,
        &["workspace", "ensure", "--branch", &b2],
        "alice",
        Some(&pw),
    ));
    let wt2 = PathBuf::from(ws2["worktree_path"].as_str().unwrap());

    // Port ONLY non-governance authored edits onto the new branch.
    git(&wt2, &["checkout", &stopped_commit, "--", "src/feature.rs"]);
    assert!(wt2.join("src/feature.rs").exists());
    let wt2_gov: Value =
        serde_json::from_str(&fs::read_to_string(wt2.join(".decapod/governance.json")).unwrap())
            .unwrap();
    assert!(
        wt2_gov["sections"]["plan"].get("reflection").is_none(),
        "new workstream must not copy or mutate stopped branch reflection journal"
    );

    // Honor subsequent operator authority without treating it as proof.
    assert!(
        run_decapod(
            xdg,
            &wt2,
            &[
                "govern",
                "plan",
                "init",
                "--title",
                "Authorized follow-up",
                "--intent",
                "Apply feature under subsequent operator grant",
                "--todo-id",
                &t2,
                "--proof-hook",
                "cargo test",
            ],
            "alice",
            Some(&pw),
        )
        .status
        .success()
    );
    assert!(
        run_decapod(
            xdg,
            &wt2,
            &["govern", "plan", "approve"],
            "alice",
            Some(&pw)
        )
        .status
        .success()
    );
    let wt2_run_id = wt2_gov["sections"]["trajectory"]["run_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        run_decapod(
            xdg,
            &wt2,
            &[
                "govern",
                "trajectory",
                "record",
                "--run-id",
                &wt2_run_id,
                "--modified-file",
                "src/feature.rs",
                "--evidence",
                &format!("Archived stopped terminal reflection commit {stopped_commit}"),
            ],
            "alice",
            Some(&pw),
        )
        .status
        .success()
    );

    let ready = run_decapod(
        xdg,
        &wt2,
        &["govern", "plan", "check-execute", "--todo-id", &t2],
        "alice",
        Some(&pw),
    );
    assert!(
        ready.status.success(),
        "new workstream should admit execution after subsequent authority: {}",
        String::from_utf8_lossy(&ready.stderr)
    );

    // Operator authority is not proof: checkpoint & commit gates stay enforced.
    let uncheckpointed = run_decapod(
        xdg,
        &wt2,
        &[
            "govern",
            "artifacts",
            "verify-checkpoints",
            "--base-branch",
            "master",
        ],
        "alice",
        Some(&pw),
    );
    assert!(
        !uncheckpointed.status.success(),
        "uncommitted/uncheckpointed authored code must not be treated as proof"
    );

    git(&wt2, &["add", "--all", "--", ":!.decapod/governance.json"]);
    assert!(
        run_decapod(
            xdg,
            &wt2,
            &[
                "govern",
                "artifacts",
                "checkpoint",
                "--id",
                &format!("{t2}-recovered"),
                "--summary",
                "Ported feature.rs with operator authority recorded",
                "--proof-ref",
                "check:recovery-test",
            ],
            "alice",
            Some(&pw),
        )
        .status
        .success()
    );
    git(&wt2, &["add", ".decapod/governance.json"]);
    git(&wt2, &["commit", "-m", "recovered workstream"]);

    let proven = run_decapod(
        xdg,
        &wt2,
        &[
            "govern",
            "artifacts",
            "verify-checkpoints",
            "--base-branch",
            "master",
        ],
        "alice",
        Some(&pw),
    );
    assert!(
        proven.status.success(),
        "{}",
        String::from_utf8_lossy(&proven.stderr)
    );

    // Original stopped commit still preserves its terminal reflection receipt verbatim.
    let archived_gov: Value = serde_json::from_str(&git(
        &wt1,
        &[
            "show",
            &format!("{stopped_commit}:.decapod/governance.json"),
        ],
    ))
    .unwrap();
    assert_eq!(
        archived_gov["sections"]["plan"]["reflection"]["receipts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        archived_gov["sections"]["plan"]["reflection"]["receipts"][1]["outcome"]["reason"],
        "obligations_regressed"
    );
}

#[test]
fn issue_1957_machine_only_completed_task_preserves_unverified_history_while_unblocking_code_candidate()
 {
    let tmp = tempdir().unwrap();
    let xdg = tmp.path();
    let main_repo = tmp.path().join("repo");
    fs::create_dir_all(main_repo.join("src")).unwrap();
    fs::write(main_repo.join("README.md"), "# Machine Recovery Fixture\n").unwrap();
    fs::write(
        main_repo.join("src/lib.rs"),
        "pub fn ok() -> bool { true }\n",
    )
    .unwrap();
    git(&main_repo, &["init", "-b", "master"]);
    git(&main_repo, &["config", "user.email", "ops@decapod.local"]);
    git(&main_repo, &["config", "user.name", "Ops Fixture"]);
    git(&main_repo, &["add", "."]);
    git(&main_repo, &["commit", "-m", "initial"]);

    let init = run_decapod(
        xdg,
        &main_repo,
        &[
            "init",
            "--dir",
            ".",
            "--product-summary",
            "Fixture verifying machine-only operational task recovery without fabricated files.",
        ],
        "operator",
        None,
    );
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let config_path = main_repo.join(".decapod").join("config.toml");
    let cfg = fs::read_to_string(&config_path).unwrap().replace(
        "container_workspaces = true",
        "container_workspaces = false",
    );
    fs::write(&config_path, cfg).unwrap();
    git(&main_repo, &["add", "-A"]);
    git(&main_repo, &["commit", "-m", "decapod init"]);

    let op_pw = acquire_session(xdg, &main_repo, "operator");
    let ops_todo = extract_json(&run_decapod(
        xdg,
        &main_repo,
        &[
            "todo",
            "add",
            "Host machine cgroup delegation repair",
            "--dir",
            ".",
            "--format",
            "json",
        ],
        "operator",
        Some(&op_pw),
    ));
    let ops_id = ops_todo["id"].as_str().unwrap().to_string();
    // Completed machine-only operational task without --validated (no repo file artifact).
    let done_ops = run_decapod(
        xdg,
        &main_repo,
        &["todo", "done", "--id", &ops_id],
        "operator",
        Some(&op_pw),
    );
    assert!(
        done_ops.status.success(),
        "todo done failed: {}",
        String::from_utf8_lossy(&done_ops.stderr)
    );
    let done_json = extract_json(&done_ops);
    assert_eq!(done_json["status"], "ok", "{done_json}");

    let db_path = main_repo.join(".decapod/data/decapod.db");
    let conn = Connection::open(&db_path).unwrap();
    let (pre_status, pre_completed_at): (String, Option<String>) = conn
        .query_row(
            "SELECT status, completed_at FROM tasks WHERE id = ?1",
            [&ops_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(pre_status, "done");
    assert!(pre_completed_at.is_some());

    // Separate code candidate hits PROOF_HOOK_FAILED and qa verify recover requests a repo file.
    let code_todo = extract_json(&run_decapod(
        xdg,
        &main_repo,
        &[
            "todo",
            "add",
            "Separate code candidate task",
            "--dir",
            ".",
            "--format",
            "json",
        ],
        "operator",
        Some(&op_pw),
    ));
    let code_id = code_todo["id"].as_str().unwrap().to_string();
    assert!(
        run_decapod(
            xdg,
            &main_repo,
            &["todo", "claim", "--id", &code_id, "--format", "json"],
            "operator",
            Some(&op_pw),
        )
        .status
        .success()
    );
    let code_branch = format!("agent/operator/{code_id}-code");
    let code_ws = extract_json(&run_decapod(
        xdg,
        &main_repo,
        &["workspace", "ensure", "--branch", &code_branch],
        "operator",
        Some(&op_pw),
    ));
    let wt_path = PathBuf::from(code_ws["worktree_path"].as_str().unwrap());
    assert!(
        run_decapod(
            xdg,
            &wt_path,
            &["govern", "plan", "approve"],
            "operator",
            Some(&op_pw),
        )
        .status
        .success()
    );

    let blocked = run_decapod(
        xdg,
        &wt_path,
        &["validate", "--format", "json"],
        "operator",
        Some(&op_pw),
    );
    assert!(!blocked.status.success());
    let blocked_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&blocked.stdout),
        String::from_utf8_lossy(&blocked.stderr)
    );
    assert!(blocked_text.contains("PROOF_HOOK_FAILED"), "{blocked_text}");
    assert!(blocked_text.contains(&ops_id), "{blocked_text}");

    let recover_req = run_decapod(
        xdg,
        &wt_path,
        &["qa", "verify", "recover", "--json"],
        "operator",
        Some(&op_pw),
    );
    assert!(!recover_req.status.success());
    assert!(
        String::from_utf8_lossy(&recover_req.stderr).contains("--artifact"),
        "{}",
        String::from_utf8_lossy(&recover_req.stderr)
    );

    // Record machine-only operational evidence and archive via explicit task.archive policy approval.
    assert!(
        run_decapod(
            xdg,
            &main_repo,
            &[
                "todo",
                "comment",
                "--id",
                &ops_id,
                "--comment",
                "Machine-only cgroup delegation applied on host; historically unverified without repo artifact"
            ],
            "operator",
            Some(&op_pw),
        )
        .status
        .success()
    );
    let grant = extract_json(&run_decapod(
        xdg,
        &main_repo,
        &[
            "govern",
            "policy",
            "grant",
            "--command",
            "task.archive",
            "--path",
            &ops_id,
            "--task-id",
            &ops_id,
            "--format",
            "json",
        ],
        "operator",
        Some(&op_pw),
    ));
    let fp = grant["target_fingerprint"].as_str().unwrap();
    assert!(
        run_decapod(
            xdg,
            &main_repo,
            &[
                "govern", "policy", "approve", "--id", fp, "--scope", "global",
            ],
            "operator",
            Some(&op_pw),
        )
        .status
        .success()
    );
    let archived = run_decapod(
        xdg,
        &main_repo,
        &["todo", "archive", "--id", &ops_id],
        "operator",
        Some(&op_pw),
    );
    assert!(
        archived.status.success(),
        "todo archive failed: {}",
        String::from_utf8_lossy(&archived.stderr)
    );

    // Preserves completed_at and unverified status while removing ops_id from unverified done blocker.
    let (post_status, post_completed_at, closed_at): (String, Option<String>, Option<String>) =
        conn.query_row(
            "SELECT status, completed_at, closed_at FROM tasks WHERE id = ?1",
            [&ops_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(post_status, "archived");
    assert_eq!(post_completed_at, pre_completed_at);
    assert!(closed_at.is_some());

    let verif_status: Option<String> = conn
        .query_row(
            "SELECT last_verified_status FROM task_verification WHERE todo_id = ?1",
            [&ops_id],
            |r| r.get(0),
        )
        .unwrap_or(None);
    assert_ne!(
        verif_status.as_deref(),
        Some("pass"),
        "machine-only archive must preserve task as historically unverified"
    );

    let after_validate = run_decapod(
        xdg,
        &wt_path,
        &["validate", "--format", "json"],
        "operator",
        Some(&op_pw),
    );
    let after_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&after_validate.stdout),
        String::from_utf8_lossy(&after_validate.stderr)
    );
    assert!(
        !after_text.contains("PROOF_HOOK_FAILED"),
        "PROOF_HOOK_FAILED should be cleared after archiving machine-only ops task: {after_text}"
    );
}
