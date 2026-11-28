//! Multi-process lease, custody, and proof fencing consistency suite (#1935, #1892, #1763).
//!
//! Exercises real `decapod` OS subprocesses, competing agents, `SIGSTOP`/`SIGCONT`
//! delayed-owner schedules, monotonic lock budgets, authoritative persisted expiry,
//! duplicate completion CAS races, valid same-generation heartbeat renewal (#1892),
//! and delayed-publication fences after source/base/policy/custody changes (#1763).

use decapod::core::{
    bounded_process::Cancellation,
    broker::DbBroker,
    db::{self, Connection},
    fleet_coord::{self, LeaseLifecycle, LeaseState},
    proof::ProofDef,
    readiness_profile::ProfileDeclaration,
    repository_profile::{self, RepositoryContract, RepositoryExecution, SourceCoverage},
    storage_lock::{StorageLock, StorageLockMode},
    store::{Store, StoreKind},
    todo,
    validation_runtime::{self as runtime},
    workunit,
};
use decapod::plugins::policy;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};
use tempfile::TempDir;

struct CmdResult {
    output: Output,
    combined: String,
}

impl CmdResult {
    fn success(&self) -> bool {
        self.output.status.success()
    }

    fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout)
            .trim()
            .to_string()
    }

    fn json(&self) -> Value {
        let raw = self.stdout_str();
        serde_json::from_str(&raw).unwrap_or_else(|err| {
            panic!(
                "expected JSON on stdout ({err}); combined:\n{}",
                self.combined
            )
        })
    }
}

fn scrub_env(cmd: &mut Command, home: Option<&Path>, xdg: Option<[&Path; 4]>) {
    let keys: Vec<String> = std::env::vars()
        .map(|(k, _)| k)
        .filter(|k| {
            k.starts_with("DECAPOD_")
                || k.starts_with("GIT_")
                || k.starts_with("GITHUB_")
                || k.starts_with("BUILDKITE")
                || k.starts_with("CHANGE_")
                || k.starts_with("SOURCE_CI_")
                || k.starts_with("XDG_")
        })
        .collect();
    for k in keys {
        cmd.env_remove(k);
    }
    cmd.env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0");
    if let Some(h) = home {
        cmd.env("HOME", h);
    }
    if let Some([cfg, data, state, cache]) = xdg {
        cmd.env("XDG_CONFIG_HOME", cfg)
            .env("XDG_DATA_HOME", data)
            .env("XDG_STATE_HOME", state)
            .env("XDG_CACHE_HOME", cache);
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let mut cmd = Command::new("git");
    scrub_env(&mut cmd, None, None);
    let out = cmd
        .current_dir(dir)
        .args([
            "-c",
            "user.name=Fencing Test",
            "-c",
            "user.email=fencing@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} in {} failed:\nstdout:\n{}\nstderr:\n{}",
        dir.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

struct FencingHarness {
    _temp: TempDir,
    home: PathBuf,
    xdg: [PathBuf; 4],
    repo_root: PathBuf,
}

fn find_in_path(binary: &str) -> PathBuf {
    let path_var = std::env::var_os("PATH").expect("PATH must be set");
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return candidate.canonicalize().unwrap_or(candidate);
        }
    }
    panic!("could not find {binary} in PATH");
}

impl FencingHarness {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().join("home");
        let xdg_cfg = temp.path().join("xdg_config");
        let xdg_data = temp.path().join("xdg_data");
        let xdg_state = temp.path().join("xdg_state");
        let xdg_cache = temp.path().join("xdg_cache");
        for d in [&home, &xdg_cfg, &xdg_data, &xdg_state, &xdg_cache] {
            fs::create_dir_all(d).unwrap();
        }
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).unwrap();
        git(&repo_root, &["init", "--initial-branch=master"]);
        fs::create_dir_all(repo_root.join(".github/workflows")).unwrap();
        fs::write(
            repo_root.join(".github/workflows/ci.yml"),
            "name: ci\non: [push, pull_request]\n",
        )
        .unwrap();
        fs::write(repo_root.join("README.md"), "# fencing fixture\n").unwrap();
        git(
            &repo_root,
            &["add", "README.md", ".github/workflows/ci.yml"],
        );
        git(&repo_root, &["commit", "-m", "init"]);

        let h = Self {
            _temp: temp,
            home,
            xdg: [xdg_cfg, xdg_data, xdg_state, xdg_cache],
            repo_root,
        };
        let init = h.run_as(
            "bootstrap",
            None,
            &h.repo_root,
            &["init", "--force", "--no-container-workspaces", "--no-ci"],
        );
        assert!(init.success(), "decapod init failed:\n{}", init.combined);
        let intent_path = h.repo_root.join(".decapod/managed/specs/INTENT.md");
        if let Ok(intent_raw) = fs::read_to_string(&intent_path) {
            let _ = fs::write(
                &intent_path,
                intent_raw.replace(
                    "Define the user-visible outcome in one paragraph.",
                    "Multi-process lease, custody, and proof fencing consistency fixture.",
                ),
            );
        }

        let config_path = h.repo_root.join(".decapod/config.toml");
        let existing = fs::read_to_string(&config_path).expect("read config.toml");
        let mut config: toml::Value = toml::from_str(&existing).expect("parse config.toml");
        let root_tbl = config.as_table_mut().expect("config root table");
        let repo_tbl = root_tbl
            .entry("repo")
            .or_insert_with(|| toml::Value::Table(Default::default()))
            .as_table_mut()
            .expect("repo table");
        repo_tbl.insert("backend".into(), toml::Value::String("local".into()));
        repo_tbl.insert("base_branch".into(), toml::Value::String("master".into()));
        repo_tbl.insert("canonical_local_target".into(), toml::Value::Boolean(true));
        repo_tbl.insert("container_workspaces".into(), toml::Value::Boolean(false));

        let custody_tbl = root_tbl
            .entry("custody")
            .or_insert_with(|| toml::Value::Table(Default::default()))
            .as_table_mut()
            .expect("custody table");
        custody_tbl.insert("isolation".into(), toml::Value::String("worktree".into()));

        let test_str = find_in_path("test").to_string_lossy().to_string();
        let mut toolchain_files = BTreeMap::new();
        toolchain_files.insert("test".to_string(), test_str.clone());
        let commands = vec![ProofDef {
            name: "smoke".to_string(),
            command: test_str.clone(),
            args: vec!["-f".to_string(), "README.md".to_string()],
            description: "Verify README exists".to_string(),
            required: true,
        }];
        let mut executions = BTreeMap::new();
        executions.insert(
            "smoke".to_string(),
            RepositoryExecution {
                program: test_str,
                toolchain_files,
                platforms: BTreeMap::new(),
                environment: BTreeMap::new(),
                directory: ".".to_string(),
                timeout_seconds: 30,
            },
        );
        let coverage = vec![SourceCoverage {
            source: ".github/workflows/ci.yml".to_string(),
            commands: vec!["smoke".to_string()],
            ci_only: false,
            reason: None,
        }];
        let mut contract = RepositoryContract {
            canonical_local_target: true,
            reviewed_commands_digest: String::new(),
            source_coverage: coverage,
            executions,
            targets: Default::default(),
        };
        contract.reviewed_commands_digest =
            repository_profile::command_contract_digest(&commands, &contract)
                .expect("compute reviewed_commands_digest");

        git(&h.repo_root, &["add", "-A"]);
        git(&h.repo_root, &["commit", "-m", "chore: stage init"]);

        let sources = runtime::discover_source_inventory(
            &runtime::Options {
                root: h.repo_root.canonicalize().expect("canonicalize repo_root"),
                git: find_in_path("git"),
                repository: "local/fencing-1935".to_string(),
                base_branch: "master".to_string(),
                accepted_base_ref: "refs/heads/master".to_string(),
                evidence_parent: std::env::temp_dir().canonicalize().expect("temp_dir"),
                timeout: Duration::from_secs(60),
                max_input_bytes: 8 * 1024 * 1024 * 1024,
                max_files: 200_000,
                max_output_bytes: 16 * 1024 * 1024,
                target: None,
            },
            &Cancellation::default(),
        )
        .expect("discover source inventory");

        let proof_tbl = root_tbl
            .entry("proof")
            .or_insert_with(|| toml::Value::Table(Default::default()))
            .as_table_mut()
            .expect("proof table");
        proof_tbl.insert(
            "validation_buildkite_preflights".into(),
            toml::Value::Boolean(false),
        );
        proof_tbl.insert(
            "validation_github_actions_preflights".into(),
            toml::Value::Boolean(false),
        );
        proof_tbl.insert(
            "commands".into(),
            toml::Value::try_from(&commands).expect("serialize commands"),
        );
        proof_tbl.insert(
            "repository_contract".into(),
            toml::Value::try_from(&contract).expect("serialize contract"),
        );
        proof_tbl.insert(
            "required_profile".into(),
            toml::Value::try_from(&ProfileDeclaration {
                id: repository_profile::REQUIRED_PROFILE.to_string(),
                reviewed_sources_digest: sources.digest().expect("sources digest"),
            })
            .expect("serialize required_profile"),
        );
        fs::write(&config_path, toml::to_string_pretty(&config).unwrap()).unwrap();
        git(&h.repo_root, &["add", "-A"]);
        git(
            &h.repo_root,
            &["commit", "-m", "chore: configure proof profile"],
        );
        h
    }

    fn xdg_refs(&self) -> [&Path; 4] {
        [&self.xdg[0], &self.xdg[1], &self.xdg[2], &self.xdg[3]]
    }

    fn acquire_session(&self, agent: &str) -> String {
        let out = self.run_as(agent, None, &self.repo_root, &["session", "acquire"]);
        assert!(
            out.success(),
            "session acquire for {agent} failed:\n{}",
            out.combined
        );
        out.combined
            .lines()
            .find_map(|line| line.strip_prefix("Password: "))
            .unwrap_or_else(|| panic!("missing Password line in:\n{}", out.combined))
            .trim()
            .to_string()
    }

    fn run_as(&self, agent: &str, pw: Option<&str>, dir: &Path, args: &[&str]) -> CmdResult {
        self.run_as_env(agent, pw, dir, args, &[])
    }

    fn run_as_env(
        &self,
        agent: &str,
        pw: Option<&str>,
        dir: &Path,
        args: &[&str],
        extra: &[(&str, &str)],
    ) -> CmdResult {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
        scrub_env(&mut cmd, Some(&self.home), Some(self.xdg_refs()));
        cmd.current_dir(dir)
            .args(args)
            .env("DECAPOD_AGENT_ID", agent)
            .env("DECAPOD_CLAIM_AUTORUN", "0")
            .env("DECAPOD_GROUP_BROKER_REQUEST_BUDGET_MS", "180000");
        if let Some(p) = pw {
            cmd.env("DECAPOD_SESSION_PASSWORD", p);
        }
        for (k, v) in extra {
            cmd.env(k, v);
        }
        let output = cmd.output().expect("run decapod");
        let combined = format!(
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        CmdResult { output, combined }
    }

    fn store_root(&self) -> PathBuf {
        self.repo_root.join(".decapod/data")
    }

    fn read_task_row(&self, task_id: &str) -> (String, String, i64, String, Option<String>, i64) {
        let conn = Connection::open(todo::todo_db_path(&self.store_root())).expect("open todo db");
        conn.query_row(
            "SELECT status, COALESCE(assigned_to,''), COALESCE(lease_generation,0),
                    COALESCE(lease_lifecycle,''), lease_expires_at, revision
             FROM tasks WHERE id = ?1",
            [task_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .expect("query task row")
    }
}

#[test]
fn authoritative_lease_ordering_claim_heartbeat_renew_handoff_release_reclaim() {
    let h = FencingHarness::new();
    let pw_a = h.acquire_session("agent-a");
    let pw_b = h.acquire_session("agent-b");

    let add = h.run_as(
        "agent-a",
        Some(&pw_a),
        &h.repo_root,
        &["todo", "add", "Ordering fence task", "--format", "json"],
    );
    assert!(add.success(), "{}", add.combined);
    let task_id = add.json()["id"].as_str().unwrap().to_string();

    // 1. Initial exclusive claim -> gen=1, lifecycle=claimed.
    let claim_a = h.run_as(
        "agent-a",
        Some(&pw_a),
        &h.repo_root,
        &[
            "todo",
            "claim",
            "--id",
            &task_id,
            "--lease-seconds",
            "600",
            "--format",
            "json",
        ],
    );
    assert!(claim_a.success(), "{}", claim_a.combined);
    assert_eq!(claim_a.json()["result"]["lease_generation"], 1);
    let (_, assignee, lease_gen, lifecycle, exp1, rev1) = h.read_task_row(&task_id);
    assert_eq!(assignee, "agent-a");
    assert_eq!(lease_gen, 1);
    assert_eq!(lifecycle, "claimed");

    // 2. Competing agent-b claim must be rejected while gen=1 is active.
    let claim_b = h.run_as(
        "agent-b",
        Some(&pw_b),
        &h.repo_root,
        &["todo", "claim", "--id", &task_id, "--format", "json"],
    );
    assert!(!claim_b.success() || claim_b.json()["status"] == "conflict");
    assert_eq!(h.read_task_row(&task_id).1, "agent-a");

    // 3. Same-owner heartbeat renewal extends expiry without bumping generation or revision.
    let hb = h.run_as(
        "agent-a",
        Some(&pw_a),
        &h.repo_root,
        &[
            "todo",
            "renew",
            "--heartbeat",
            "--id",
            &task_id,
            "--lease-seconds",
            "3600",
            "--format",
            "json",
        ],
    );
    assert!(hb.success(), "{}", hb.combined);
    assert_eq!(hb.json()["result"]["lease_generation"], 1);
    assert_eq!(hb.json()["result"]["extended"], true);
    let (_, assignee2, gen2, _, exp2, rev2) = h.read_task_row(&task_id);
    assert_eq!(assignee2, "agent-a");
    assert_eq!(gen2, 1);
    assert_eq!(rev2, rev1);
    assert_ne!(exp2, exp1);

    // 4. Non-heartbeat renew rotates lease_generation 1 -> 2 and increments revision.
    let renew = h.run_as(
        "agent-a",
        Some(&pw_a),
        &h.repo_root,
        &[
            "todo",
            "renew",
            "--id",
            &task_id,
            "--lease-seconds",
            "3600",
            "--format",
            "json",
        ],
    );
    assert!(renew.success(), "{}", renew.combined);
    assert_eq!(renew.json()["result"]["lease_generation"], 2);
    let (_, _, gen3, lifecycle3, _, rev3) = h.read_task_row(&task_id);
    assert_eq!(gen3, 2);
    assert_eq!(lifecycle3, "extended");
    assert!(rev3 > rev2);

    // 5. Handoff enforces verified trust + exact one-use policy approval, then advances gen 2 -> 3.
    let unverified_handoff = h.run_as(
        "agent-a",
        Some(&pw_a),
        &h.repo_root,
        &[
            "todo",
            "handoff",
            "--id",
            &task_id,
            "--to",
            "agent-b",
            "--summary",
            "unverified handoff must fail",
            "--format",
            "json",
        ],
    );
    assert!(!unverified_handoff.success(), "basic trust must be denied");

    DbBroker::new(&h.store_root())
        .with_conn(
            &todo::todo_db_path(&h.store_root()),
            "operator",
            None,
            "test.1935.promote_verified",
            |conn| {
                conn.execute(
                    "INSERT INTO agents(agent_id, trust_level, status, updated_at)
                     VALUES('agent-a', 'verified', 'active', '1Z')
                     ON CONFLICT(agent_id) DO UPDATE SET trust_level = 'verified'",
                    db::params![],
                )?;
                Ok(())
            },
        )
        .unwrap();
    let expiry_ts = format!(
        "{}Z",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 900
    );
    let store = Store {
        kind: StoreKind::Repo,
        root: h.store_root(),
    };
    policy::approve_action_with_expiry(
        &store,
        "todo.handoff",
        Some(&task_id),
        "operator",
        "global",
        Some(&expiry_ts),
    )
    .unwrap();

    let handoff = h.run_as(
        "agent-a",
        Some(&pw_a),
        &h.repo_root,
        &[
            "todo",
            "handoff",
            "--id",
            &task_id,
            "--to",
            "agent-b",
            "--summary",
            "transfer custody",
            "--format",
            "json",
        ],
    );
    assert!(handoff.success(), "{}", handoff.combined);
    let (_, assignee4, gen4, lifecycle4, _, _) = h.read_task_row(&task_id);
    assert_eq!(assignee4, "agent-b");
    assert_eq!(gen4, 3);
    assert_eq!(lifecycle4, "claimed");

    // 6. Stale agent-a cannot heartbeat, renew, or release after handoff.
    for args in [
        vec![
            "todo",
            "renew",
            "--heartbeat",
            "--id",
            &task_id,
            "--format",
            "json",
        ],
        vec!["todo", "renew", "--id", &task_id, "--format", "json"],
        vec!["todo", "release", "--id", &task_id, "--format", "json"],
    ] {
        let res = h.run_as("agent-a", Some(&pw_a), &h.repo_root, &args);
        let rejected = !res.success() || res.json()["status"] != "ok";
        assert!(
            rejected,
            "stale agent-a succeeded for {args:?}: {}",
            res.combined
        );
    }
    assert_eq!(h.read_task_row(&task_id).1, "agent-b");
    assert_eq!(h.read_task_row(&task_id).2, 3);

    // 7. agent-b releases; agent-a claims again -> generation stays >= 3 and claimed.
    let rel = h.run_as(
        "agent-b",
        Some(&pw_b),
        &h.repo_root,
        &["todo", "release", "--id", &task_id, "--format", "json"],
    );
    assert!(rel.success(), "{}", rel.combined);
    let (_, assignee5, _, lifecycle5, _, _) = h.read_task_row(&task_id);
    assert_eq!(assignee5, "");
    assert_eq!(lifecycle5, "released");

    let reclaim = h.run_as(
        "agent-a",
        Some(&pw_a),
        &h.repo_root,
        &["todo", "claim", "--id", &task_id, "--format", "json"],
    );
    assert!(reclaim.success(), "{}", reclaim.combined);
    let (_, assignee6, gen6, lifecycle6, _, _) = h.read_task_row(&task_id);
    assert_eq!(assignee6, "agent-a");
    assert!(gen6 >= 3);
    assert_eq!(lifecycle6, "claimed");

    let verify = h.run_as(
        "agent-a",
        Some(&pw_a),
        &h.repo_root,
        &["todo", "journal", "verify", "--format", "json"],
    );
    assert!(verify.success(), "{}", verify.combined);
    assert_eq!(verify.json()["journal_valid"], true);
}

#[test]
fn delayed_owner_after_expiry_and_reclaim_is_fenced_across_os_processes() {
    let h = FencingHarness::new();
    let pw_a = h.acquire_session("agent-a");
    let pw_b = h.acquire_session("agent-b");

    let add = h.run_as(
        "agent-a",
        Some(&pw_a),
        &h.repo_root,
        &["todo", "add", "Expiry reclaim task", "--format", "json"],
    );
    let task_id = add.json()["id"].as_str().unwrap().to_string();
    let claim_a = h.run_as(
        "agent-a",
        Some(&pw_a),
        &h.repo_root,
        &[
            "todo",
            "claim",
            "--id",
            &task_id,
            "--lease-seconds",
            "1",
            "--format",
            "json",
        ],
    );
    assert!(claim_a.success(), "{}", claim_a.combined);

    let exp_secs: u64 = h
        .read_task_row(&task_id)
        .4
        .unwrap()
        .trim_end_matches('Z')
        .parse()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        < exp_secs
    {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for 1s lease expiry"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    // agent-b reclaims expired lease -> gen=2, lifecycle=reclaimed.
    let reclaim_b = h.run_as(
        "agent-b",
        Some(&pw_b),
        &h.repo_root,
        &[
            "todo",
            "claim",
            "--id",
            &task_id,
            "--lease-seconds",
            "1800",
            "--format",
            "json",
        ],
    );
    assert!(reclaim_b.success(), "reclaim_b:\n{}", reclaim_b.combined);
    assert_eq!(reclaim_b.json()["result"]["lease_generation"], 2);
    assert_eq!(reclaim_b.json()["result"]["lease_lifecycle"], "reclaimed");

    // Delayed agent-a attempts heartbeat, renew, release, and plain done -> all fenced.
    let stale_hb = h.run_as(
        "agent-a",
        Some(&pw_a),
        &h.repo_root,
        &[
            "todo",
            "renew",
            "--heartbeat",
            "--id",
            &task_id,
            "--format",
            "json",
        ],
    );
    assert!(!stale_hb.success(), "{}", stale_hb.combined);
    assert!(
        stale_hb.combined.contains("LEASE_HEARTBEAT_STALE"),
        "{}",
        stale_hb.combined
    );

    let stale_renew = h.run_as(
        "agent-a",
        Some(&pw_a),
        &h.repo_root,
        &["todo", "renew", "--id", &task_id, "--format", "json"],
    );
    assert!(!stale_renew.success() || stale_renew.json()["status"] == "conflict");

    // Verify persisted custody still belongs exclusively to agent-b at gen=2.
    let (status, assignee, lease_gen, lifecycle, _, _) = h.read_task_row(&task_id);
    assert_eq!(status, "open");
    assert_eq!(assignee, "agent-b");
    assert_eq!(lease_gen, 2);
    assert_eq!(lifecycle, "reclaimed");
}

#[cfg(unix)]
#[test]
fn suspended_process_sigstop_sigcont_cannot_corrupt_reclaimed_custody() {
    let h = FencingHarness::new();
    let pw_a = h.acquire_session("agent-a");
    let pw_b = h.acquire_session("agent-b");

    let add = h.run_as(
        "agent-a",
        Some(&pw_a),
        &h.repo_root,
        &["todo", "add", "SIGSTOP delayed owner", "--format", "json"],
    );
    let task_id = add.json()["id"].as_str().unwrap().to_string();
    let ready_flag = h._temp.path().join("agent_a_claimed");
    let go_flag = h._temp.path().join("agent_a_go");

    // Child OS process claims as agent-a, signals ready_flag, waits for go_flag
    // (while suspended via SIGSTOP and resumed via SIGCONT), then tries heartbeat.
    let bin = env!("CARGO_BIN_EXE_decapod");
    let script = format!(
        "set -e\n\
         \"{bin}\" todo claim --id \"{task_id}\" --lease-seconds 1 --format json >/dev/null\n\
         touch \"{}\"\n\
         while [ ! -f \"{}\" ]; do sleep 0.05; done\n\
         \"{bin}\" todo renew --heartbeat --id \"{task_id}\" --lease-seconds 1200 --format json\n",
        ready_flag.display(),
        go_flag.display()
    );
    let mut cmd = Command::new("/bin/sh");
    scrub_env(&mut cmd, Some(&h.home), Some(h.xdg_refs()));
    cmd.current_dir(&h.repo_root)
        .args(["-c", &script])
        .env("DECAPOD_AGENT_ID", "agent-a")
        .env("DECAPOD_SESSION_PASSWORD", &pw_a)
        .env("DECAPOD_CLAIM_AUTORUN", "0");
    let child = cmd.spawn().expect("spawn delayed agent-a");
    let pid = child.id() as libc::pid_t;

    // Wait for agent-a to finish claim.
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ready_flag.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        ready_flag.exists(),
        "agent-a did not complete initial claim"
    );

    // Suspend agent-a with SIGSTOP.
    assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);

    // Wait for agent-a's 1s lease to genuinely expire while suspended, then agent-b reclaims.
    let exp_secs: u64 = h
        .read_task_row(&task_id)
        .4
        .unwrap()
        .trim_end_matches('Z')
        .parse()
        .unwrap();
    let exp_deadline = Instant::now() + Duration::from_secs(10);
    while std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        < exp_secs
    {
        assert!(
            Instant::now() < exp_deadline,
            "timed out waiting for stopped owner lease expiry"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    let reclaim_b = h.run_as(
        "agent-b",
        Some(&pw_b),
        &h.repo_root,
        &[
            "todo",
            "claim",
            "--id",
            &task_id,
            "--lease-seconds",
            "1800",
            "--format",
            "json",
        ],
    );
    assert!(reclaim_b.success(), "{}", reclaim_b.combined);

    // Resume agent-a with SIGCONT and allow its delayed heartbeat to execute.
    fs::write(&go_flag, b"go").unwrap();
    assert_eq!(unsafe { libc::kill(pid, libc::SIGCONT) }, 0);
    let out = child.wait_with_output().expect("wait delayed child");
    assert!(
        !out.status.success(),
        "resumed stale agent-a must fail heartbeat after reclaim"
    );

    let (_, assignee, lease_gen, lifecycle, _, _) = h.read_task_row(&task_id);
    assert_eq!(assignee, "agent-b");
    assert_eq!(lease_gen, 2);
    assert_eq!(lifecycle, "reclaimed");

    let conn = Connection::open(todo::todo_db_path(&h.store_root())).unwrap();
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
}

#[test]
fn clock_skew_monotonic_budgets_and_authoritative_expiry_boundaries() {
    // 1. Authoritative persisted expiry boundary (no client wall-clock trust).
    assert_eq!(
        fleet_coord::lease_state(Some("1000Z"), "999Z"),
        LeaseState::Active
    );
    assert_eq!(
        fleet_coord::lease_state(Some("1000Z"), "1000Z"),
        LeaseState::Expired
    );
    assert_eq!(
        fleet_coord::lease_state(Some("1000Z"), "1500Z"),
        LeaseState::Expired
    );
    assert!(!fleet_coord::lease_allows_reclaim(Some("1000Z"), "999Z"));
    assert!(fleet_coord::lease_allows_reclaim(Some("1000Z"), "1000Z"));
    assert_eq!(
        fleet_coord::lease_state(None, "1000Z"),
        LeaseState::Unspecified
    );
    assert_eq!(
        fleet_coord::lease_state(Some("not-a-timestamp"), "1000Z"),
        LeaseState::Unspecified
    );

    // 2. Generation monotonicity prevents backward-clock ABA revival.
    assert_eq!(fleet_coord::next_lease_generation(0, false), 1);
    assert_eq!(fleet_coord::next_lease_generation(1, true), 2);
    assert_eq!(fleet_coord::extended_generation(2), 3);
    assert_eq!(fleet_coord::handoff_lease_generation(3), 4);
    assert!(LeaseLifecycle::Claimed.holds_exclusive_custody());
    assert!(LeaseLifecycle::Extended.holds_exclusive_custody());
    assert!(LeaseLifecycle::Reclaimed.holds_exclusive_custody());
    assert!(!LeaseLifecycle::Expired.holds_exclusive_custody());
    assert!(!LeaseLifecycle::Released.holds_exclusive_custody());
    assert!(!LeaseLifecycle::Yielded.holds_exclusive_custody());

    // 3. Monotonic StorageLock elapsed timeout bounds contention cleanly across OS threads.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("decapod.db");
    fs::write(&db_path, b"").unwrap();
    let lock2 = StorageLock::acquire(
        &db_path,
        StorageLockMode::Exclusive,
        Duration::from_millis(200),
    )
    .unwrap();
    let db_clone = db_path.clone();
    let handle = std::thread::spawn(move || {
        let t0 = Instant::now();
        let e = StorageLock::acquire(
            &db_clone,
            StorageLockMode::Exclusive,
            Duration::from_millis(80),
        )
        .unwrap_err()
        .to_string();
        (e, t0.elapsed())
    });
    let (thread_err, thread_elapsed) = handle.join().unwrap();
    drop(lock2);
    assert!(thread_err.contains("STORAGE_LOCK_TIMEOUT"), "{thread_err}");
    assert!(
        thread_elapsed >= Duration::from_millis(60) && thread_elapsed < Duration::from_secs(5),
        "unexpected monotonic timeout elapsed: {thread_elapsed:?}"
    );
}

#[test]
fn duplicate_completion_race_across_competing_processes_admits_at_most_once() {
    let h = FencingHarness::new();
    let pw = h.acquire_session("agent-race");
    let add = h.run_as(
        "agent-race",
        Some(&pw),
        &h.repo_root,
        &[
            "todo",
            "add",
            "Duplicate completion race",
            "--format",
            "json",
        ],
    );
    let task_id = add.json()["id"].as_str().unwrap().to_string();

    // 1. Atomic CAS duplicate-completion race at exact expected revision/status admits strictly 1 winner.
    let store = Store {
        kind: StoreKind::Repo,
        root: h.store_root(),
    };
    let n = 4usize;
    let barrier = Arc::new(Barrier::new(n));
    let mut cas_handles = Vec::new();
    for _ in 0..n {
        let b = Arc::clone(&barrier);
        let st = store.clone();
        let tid = task_id.clone();
        cas_handles.push(std::thread::spawn(move || {
            b.wait();
            todo::update_status(
                &st,
                &tid,
                0,
                "open",
                "done",
                "task.done",
                json!({"actor": "agent-race"}),
            )
            .unwrap()
        }));
    }
    let cas_results: Vec<Value> = cas_handles
        .into_iter()
        .map(|jh| jh.join().unwrap())
        .collect();
    let ok_count = cas_results.iter().filter(|v| v["status"] == "ok").count();
    let conflict_count = cas_results
        .iter()
        .filter(|v| v["status"] == "conflict" && v["reason"] == "stale_task_revision")
        .count();
    assert_eq!(ok_count, 1, "expected 1 CAS winner, got: {cas_results:?}");
    assert_eq!(
        conflict_count,
        n - 1,
        "expected n-1 CAS conflicts, got: {cas_results:?}"
    );

    let conn = Connection::open(todo::todo_db_path(&h.store_root())).unwrap();
    let done_events: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE stream='todo' AND subject_id=?1 AND event_type='task.done'",
            [&task_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(done_events, 1, "duplicate task.done event emitted");
    assert_eq!(h.read_task_row(&task_id).0, "done");

    // 2. Competing OS processes racing `decapod todo release` on an exclusively claimed task
    //    admits only the custody holder, and non-owners cannot call `todo done` either.
    let add_b = h.run_as(
        "agent-race",
        Some(&pw),
        &h.repo_root,
        &[
            "todo",
            "add",
            "Exclusive completion race",
            "--format",
            "json",
        ],
    );
    let task_b = add_b.json()["id"].as_str().unwrap().to_string();
    assert!(
        h.run_as(
            "agent-race",
            Some(&pw),
            &h.repo_root,
            &[
                "todo",
                "claim",
                "--id",
                &task_b,
                "--lease-seconds",
                "1800",
                "--format",
                "json"
            ],
        )
        .success()
    );
    let pw_c1 = h.acquire_session("agent-c1");
    let pw_c2 = h.acquire_session("agent-c2");
    let pw_c3 = h.acquire_session("agent-c3");

    // Non-owner `todo done` while exclusively claimed is fenced.
    let non_owner_done = h.run_as(
        "agent-c1",
        Some(&pw_c1),
        &h.repo_root,
        &["todo", "done", "--id", &task_b, "--format", "json"],
    );
    assert!(
        !non_owner_done.success() || non_owner_done.json()["status"] != "ok",
        "non-owner must be fenced from completing claimed task: {}",
        non_owner_done.combined
    );

    let proc_barrier = Arc::new(Barrier::new(3));
    let racers = [
        ("agent-c1", pw_c1),
        ("agent-c2", pw_c2),
        ("agent-c3", pw_c3),
    ];
    let mut handles = Vec::new();
    for (agent, actor_pw) in racers {
        let b = Arc::clone(&proc_barrier);
        let root = h.repo_root.clone();
        let home = h.home.clone();
        let xdg = h.xdg.clone();
        let tid = task_b.clone();
        handles.push(std::thread::spawn(move || {
            b.wait();
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
            scrub_env(
                &mut cmd,
                Some(&home),
                Some([&xdg[0], &xdg[1], &xdg[2], &xdg[3]]),
            );
            cmd.current_dir(&root)
                .args(["todo", "release", "--id", &tid, "--format", "json"])
                .env("DECAPOD_AGENT_ID", agent)
                .env("DECAPOD_SESSION_PASSWORD", &actor_pw)
                .env("DECAPOD_CLAIM_AUTORUN", "0");
            let out = cmd.output().expect("race todo release");
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            (agent, out.status.success(), stdout)
        }));
    }
    let results: Vec<_> = handles.into_iter().map(|jh| jh.join().unwrap()).collect();
    for (agent, ok, out) in &results {
        let parsed: Value = serde_json::from_str(out).unwrap_or_default();
        assert!(
            !*ok || parsed["status"] == "conflict",
            "non-owner {agent} must be rejected releasing active lease: {out}"
        );
    }
    let owner_release = h.run_as(
        "agent-race",
        Some(&pw),
        &h.repo_root,
        &["todo", "release", "--id", &task_b, "--format", "json"],
    );
    assert!(owner_release.success(), "{}", owner_release.combined);
    assert_eq!(owner_release.json()["status"], "ok");

    // Now that owner released, exact CAS completion admits once at post-release revision.
    let rev_released = h.read_task_row(&task_b).5;
    let first_done = todo::update_status(
        &store,
        &task_b,
        rev_released,
        "open",
        "done",
        "task.done",
        json!({"actor": "agent-race"}),
    )
    .unwrap();
    assert_eq!(first_done["status"], "ok");
    let dup_done = todo::update_status(
        &store,
        &task_b,
        rev_released,
        "open",
        "done",
        "task.done",
        json!({"actor": "agent-race"}),
    )
    .unwrap();
    assert_eq!(dup_done["status"], "conflict");
    let done_events_b: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE stream='todo' AND subject_id=?1 AND event_type='task.done'",
            [&task_b],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(done_events_b, 1);

    // Workunit operation_id idempotency vs conflicting-bytes fencing.
    workunit::init_workunit(&h.repo_root, "wu_dup", "intent:dup").unwrap();
    let m1 = workunit::load_workunit(&h.repo_root, "wu_dup").unwrap();
    let mut m2 = m1.clone();
    m2.spec_refs.push("specs/ARCHITECTURE.md".into());
    workunit::write_workunit_checked(&h.repo_root, &m2, "op-same-1", 1).unwrap();
    let mut m3 = m2.clone();
    m3.spec_refs.push("specs/INTERFACES.md".into());
    let conflict = workunit::write_workunit_checked(&h.repo_root, &m3, "op-same-1", 1)
        .unwrap_err()
        .to_string();
    assert!(
        conflict.contains("WORKUNIT_OPERATION_CONFLICT"),
        "{conflict}"
    );
}

#[test]
fn valid_heartbeat_renewal_and_delayed_publication_fences_1892_and_1763() {
    let h = FencingHarness::new();
    let pw = h.acquire_session("agent-pub");

    let add = h.run_as(
        "agent-pub",
        Some(&pw),
        &h.repo_root,
        &["todo", "add", "Publish fenced task", "--format", "json"],
    );
    let task_id = add.json()["id"].as_str().unwrap().to_string();
    let claim = h.run_as(
        "agent-pub",
        Some(&pw),
        &h.repo_root,
        &[
            "todo",
            "claim",
            "--id",
            &task_id,
            "--lease-seconds",
            "120",
            "--format",
            "json",
        ],
    );
    assert!(claim.success(), "{}", claim.combined);

    let branch = format!("agent/agent-pub/{task_id}-fence");
    let ensure = h.run_as(
        "agent-pub",
        Some(&pw),
        &h.repo_root,
        &["workspace", "ensure", "--branch", &branch],
    );
    assert!(ensure.success(), "{}", ensure.combined);
    let wt_path = PathBuf::from(ensure.json()["worktree_path"].as_str().unwrap());

    // Bind plan, existing workspace trajectory, and material spec update inside the worktree.
    fs::write(wt_path.join("README.md"), "# fencing fixture updated\n").unwrap();
    let arch_path = wt_path.join(".decapod/managed/specs/ARCHITECTURE.md");
    let arch_orig = fs::read_to_string(&arch_path).unwrap();
    fs::write(&arch_path, format!("{arch_orig}\n- Fence test note.\n")).unwrap();
    assert!(
        h.run_as(
            "agent-pub",
            Some(&pw),
            &wt_path,
            &["rpc", "--op", "specs.refresh"]
        )
        .success()
    );

    let gov_raw: Value =
        serde_json::from_slice(&fs::read(wt_path.join(".decapod/governance.json")).unwrap())
            .unwrap();
    let branch_run_id = gov_raw["sections"]["trajectory"]["run_id"]
        .as_str()
        .unwrap_or(&format!("branch_{task_id}"))
        .to_string();
    let plan_up = h.run_as(
        "agent-pub",
        Some(&pw),
        &wt_path,
        &[
            "govern",
            "plan",
            "update",
            "--title",
            "Fence test",
            "--intent",
            "Verify valid renewal and delayed publication refusal",
            "--todo-id",
            &task_id,
            "--proof-hook",
            "decapod validate",
        ],
    );
    assert!(plan_up.success(), "{}", plan_up.combined);
    assert!(
        h.run_as(
            "agent-pub",
            Some(&pw),
            &wt_path,
            &["govern", "plan", "approve"]
        )
        .success()
    );
    for spec in [
        ".decapod/managed/specs/ARCHITECTURE.md",
        ".decapod/managed/specs/INTERFACES.md",
        ".decapod/managed/specs/SECURITY.md",
    ] {
        let disp = if spec.ends_with("ARCHITECTURE.md") {
            "updated"
        } else {
            "unchanged-with-reason"
        };
        let rev = h.run_as(
            "agent-pub",
            Some(&pw),
            &wt_path,
            &[
                "govern",
                "plan",
                "review-spec",
                "--path",
                spec,
                "--disposition",
                disp,
                "--reason",
                "Reviewed for fence fixture",
            ],
        );
        assert!(rev.success(), "review-spec {spec}:\n{}", rev.combined);
    }
    let traj = h.run_as(
        "agent-pub",
        Some(&pw),
        &wt_path,
        &[
            "govern",
            "trajectory",
            "record",
            "--run-id",
            &branch_run_id,
            "--modified-file",
            "README.md",
            "--modified-file",
            ".decapod/managed/specs/ARCHITECTURE.md",
            "--check",
            "smoke=passed",
        ],
    );
    assert!(traj.success(), "{}", traj.combined);

    git(&wt_path, &["add", "README.md", ".decapod/managed"]);
    let ckpt = h.run_as(
        "agent-pub",
        Some(&pw),
        &wt_path,
        &[
            "govern",
            "artifacts",
            "checkpoint",
            "--id",
            "ckpt-1",
            "--summary",
            "Ready candidate",
            "--proof-ref",
            "smoke=passed",
        ],
    );
    assert!(ckpt.success(), "{}", ckpt.combined);
    git(&wt_path, &["add", "."]);
    git(&wt_path, &["commit", "-m", "feat: candidate c1"]);
    let c1 = git(&wt_path, &["rev-parse", "HEAD"]);

    // 1. Valid active heartbeat renewal (#1892) with DECAPOD_VALIDATE_LEASE_TTL=renew
    //    must allow `decapod validate` to pass and record completion for C1 at gen=1.
    let pre_status = git(
        &wt_path,
        &["status", "--porcelain", "--untracked-files=normal"],
    );
    let val = h.run_as_env(
        "agent-pub",
        Some(&pw),
        &wt_path,
        &["validate", "--format", "json"],
        &[("DECAPOD_VALIDATE_LEASE_TTL", "renew")],
    );
    let post_status = git(
        &wt_path,
        &["status", "--porcelain", "--untracked-files=normal"],
    );
    assert!(
        val.success(),
        "validate with renew failed (pre_status={pre_status:?}, post_status={post_status:?}):\n{}",
        val.combined
    );
    let val_json = val.json();
    assert_eq!(val_json["status"], "ok", "{}", val.combined);
    assert_eq!(
        h.read_task_row(&task_id).2,
        1,
        "heartbeat must preserve gen=1"
    );

    // Verify commit proof was recorded for C1.
    let commit_lookup = h.run_as(
        "agent-pub",
        Some(&pw),
        &wt_path,
        &["qa", "verify", "commit", &c1, "--json"],
    );
    assert!(commit_lookup.success(), "{}", commit_lookup.combined);
    assert_eq!(commit_lookup.json()["task"]["lease_generation"], 1);

    // 1b. Initialize workunit bound to authoritative trajectory intent, validate, renew via --heartbeat
    //     AFTER validation, and promote workunit to VERIFIED -> must succeed with active heartbeat (#1892)!
    let intent_id = gov_raw["sections"]["trajectory"]["intent_id"]
        .as_str()
        .expect("intent_id in governance.json")
        .to_string();
    assert!(
        h.run_as(
            "agent-pub",
            Some(&pw),
            &wt_path,
            &[
                "govern",
                "capsule",
                "query",
                "--topic",
                "fencing-1935",
                "--scope",
                "interfaces",
                "--task-id",
                &task_id,
                "--write",
            ],
        )
        .success()
    );
    assert!(
        h.run_as(
            "agent-pub",
            Some(&pw),
            &wt_path,
            &[
                "govern",
                "workunit",
                "init",
                "--task-id",
                &task_id,
                "--intent-ref",
                &intent_id,
            ],
        )
        .success()
    );
    assert!(
        h.run_as(
            "agent-pub",
            Some(&pw),
            &wt_path,
            &[
                "govern",
                "workunit",
                "attach-spec",
                "--task-id",
                &task_id,
                "--ref",
                ".decapod/managed/specs/ARCHITECTURE.md",
            ],
        )
        .success()
    );
    assert!(
        h.run_as(
            "agent-pub",
            Some(&pw),
            &wt_path,
            &[
                "govern",
                "workunit",
                "attach-state",
                "--task-id",
                &task_id,
                "--ref",
                &format!(".decapod/managed/context/{task_id}.json"),
            ],
        )
        .success()
    );
    assert!(
        h.run_as(
            "agent-pub",
            Some(&pw),
            &wt_path,
            &[
                "govern",
                "workunit",
                "set-proof-plan",
                "--task-id",
                &task_id,
                "--gate",
                "validate_passes",
            ],
        )
        .success()
    );
    for to in ["executing", "claimed"] {
        assert!(
            h.run_as(
                "agent-pub",
                Some(&pw),
                &wt_path,
                &[
                    "govern",
                    "workunit",
                    "transition",
                    "--task-id",
                    &task_id,
                    "--to",
                    to,
                ],
            )
            .success()
        );
    }
    let wu_ckpt = h.run_as(
        "agent-pub",
        Some(&pw),
        &wt_path,
        &[
            "govern",
            "workunit",
            "checkpoint",
            "--task-id",
            &task_id,
            "--operation-id",
            "wu-ckpt-1",
        ],
    );
    assert!(wu_ckpt.success(), "{}", wu_ckpt.combined);
    let smoke_art = ".decapod/data/smoke-proof-1935.txt";
    fs::write(wt_path.join(smoke_art), "smoke pass\n").unwrap();
    let rec_proof = h.run_as(
        "agent-pub",
        Some(&pw),
        &wt_path,
        &[
            "govern",
            "workunit",
            "record-proof",
            "--task-id",
            &task_id,
            "--gate",
            "validate_passes",
            "--status",
            "pass",
            "--artifact",
            smoke_art,
        ],
    );
    assert!(rec_proof.success(), "{}", rec_proof.combined);
    let val_wu = h.run_as(
        "agent-pub",
        Some(&pw),
        &wt_path,
        &["validate", "--format", "json"],
    );
    assert!(val_wu.success(), "{}", val_wu.combined);
    let post_val_hb = h.run_as(
        "agent-pub",
        Some(&pw),
        &wt_path,
        &[
            "todo",
            "renew",
            "--heartbeat",
            "--id",
            &task_id,
            "--lease-seconds",
            "7200",
            "--format",
            "json",
        ],
    );
    assert!(post_val_hb.success(), "{}", post_val_hb.combined);
    let promote = h.run_as(
        "agent-pub",
        Some(&pw),
        &wt_path,
        &[
            "govern",
            "workunit",
            "transition",
            "--task-id",
            &task_id,
            "--to",
            "verified",
        ],
    );
    assert!(
        promote.success(),
        "workunit transition --to verified must succeed after active heartbeat renewal:\n{}",
        promote.combined
    );

    // 2. Delayed publication after source HEAD advances (C1 -> C2): old receipt is stale,
    //    `qa verify commit C1` still reflects C1, while C2 has no recorded proof yet.
    fs::write(wt_path.join("README.md"), "# fencing fixture c2\n").unwrap();
    git(&wt_path, &["add", "README.md"]);
    git(&wt_path, &["commit", "-m", "feat: candidate c2"]);
    let c2 = git(&wt_path, &["rev-parse", "HEAD"]);
    let c2_lookup = h.run_as(
        "agent-pub",
        Some(&pw),
        &wt_path,
        &["qa", "verify", "commit", &c2, "--json"],
    );
    assert!(!c2_lookup.success(), "C2 must not inherit C1's proof");

    // 3. Delayed publication after canonical accepted base (`master`) advances (#1763):
    //    C1 proof is no longer current against the new base; publish must refuse.
    fs::write(h.repo_root.join("UPSTREAM.txt"), "base moved\n").unwrap();
    git(&h.repo_root, &["add", "UPSTREAM.txt"]);
    git(&h.repo_root, &["commit", "-m", "chore: advance master"]);
    let pub_plan = h.run_as(
        "agent-pub",
        Some(&pw),
        &wt_path,
        &["workspace", "publish", "--dry-run"],
    );
    assert_eq!(
        pub_plan.json()["action"],
        "blocked",
        "{}",
        pub_plan.combined
    );

    let pub_after_base = h.run_as("agent-pub", Some(&pw), &wt_path, &["workspace", "publish"]);
    assert!(
        !pub_after_base.success(),
        "delayed workspace publish must refuse after source/base drift:\n{}",
        pub_after_base.combined
    );

    let done_after_base_move = h.run_as(
        "agent-pub",
        Some(&pw),
        &h.repo_root,
        &[
            "todo",
            "done",
            "--id",
            &task_id,
            "--validated",
            "--commit",
            &c1,
            "--format",
            "json",
        ],
    );
    assert!(
        !done_after_base_move.success(),
        "C1 proof must be rejected once accepted base master moves:\n{}",
        done_after_base_move.combined
    );

    // 4. Delayed publication after policy change + custody rotation (#1763 / #1892):
    //    Rotate lease_generation 1 -> 2 via non-heartbeat `todo renew`, then attempt
    //    `todo done --validated --commit C1` -> MUST refuse gen-1 proof!
    let rotate = h.run_as(
        "agent-pub",
        Some(&pw),
        &h.repo_root,
        &["todo", "renew", "--id", &task_id, "--format", "json"],
    );
    assert!(rotate.success(), "{}", rotate.combined);
    assert_eq!(h.read_task_row(&task_id).2, 2);

    let done_stale = h.run_as(
        "agent-pub",
        Some(&pw),
        &h.repo_root,
        &[
            "todo",
            "done",
            "--id",
            &task_id,
            "--validated",
            "--commit",
            &c1,
            "--format",
            "json",
        ],
    );
    assert!(
        !done_stale.success(),
        "stale gen-1 proof must fail after generation rotation:\n{}",
        done_stale.combined
    );
}

#[test]
fn fencing_1935_clock_anomaly_preserves_monotonic_fences_and_cloud_custody_rejects_stale_generation()
 {
    use decapod::core::cloud_workspace::{CloudCustodyRecord, CloudCustodyStore};
    use decapod::core::fleet_coord::{
        LeaseState, format_epoch_z, lease_allows_reclaim, lease_expires_at, lease_state,
        next_lease_generation,
    };

    // 1. Clock-anomaly & wall-clock skew invariants:
    //    - lease_expires_at clamps lease duration and advances monotonically from now_secs.
    //    - Backward wall-clock jumps (`1700000000Z`) and unparseable/corrupt timestamps never
    //      allow premature reclaim; only `now >= exp` transitions to Expired + increments generation.
    let exp = lease_expires_at(1_800_000_000, 100);
    assert_eq!(exp, "1800000100Z");

    assert_eq!(
        lease_state(Some(&exp), &format_epoch_z(1_700_000_000)),
        LeaseState::Active
    );
    assert!(!lease_allows_reclaim(Some(&exp), "1700000000Z"));
    assert_eq!(lease_state(Some(&exp), "1800000099Z"), LeaseState::Active);
    assert!(!lease_allows_reclaim(Some(&exp), "1800000099Z"));

    // Unparseable clock string fails closed to Unspecified (never grants reclaim).
    assert_eq!(
        lease_state(Some("corrupt-ts"), "1800000200Z"),
        LeaseState::Unspecified
    );
    assert!(!lease_allows_reclaim(Some("corrupt-ts"), "1800000200Z"));

    // Exact boundary expiry transitions Expired and monotonically advances generation.
    assert_eq!(lease_state(Some(&exp), "1800000100Z"), LeaseState::Expired);
    assert!(lease_allows_reclaim(Some(&exp), "1800000100Z"));
    assert_eq!(next_lease_generation(1, true), 2);

    // 2. Cloud custody store records monotonic fencing_generation / task_revision without SQLite.
    let tmp = TempDir::new().unwrap();
    let repo_root = tmp.path().join("cloud-repo");
    let wt_path = repo_root.join(".decapod/workspaces/wt-fenced");
    fs::create_dir_all(&wt_path).unwrap();
    let store = CloudCustodyStore::at(tmp.path().join("cloud-custody"));
    let gen1 = CloudCustodyRecord {
        version: 1,
        origin: "https://propodus.example.invalid".into(),
        project_id: "proj_1".into(),
        task_id: "t_fenced".into(),
        task_title: "Fenced cloud task".into(),
        native_task_version: 3,
        workspace_id: "ws-fenced".into(),
        worktree_path: wt_path.clone(),
        branch: "agent/cloud/fenced".into(),
        actor: "agent-a".into(),
        custody_id: "cust_gen1".into(),
        pending_operation_id: None,
        pending_custody_id: None,
        pending_native_task_version: None,
        task_revision: Some(3),
        fencing_generation: Some(1),
        expires_at: None,
        invocation_id: None,
        container_expected: false,
        container_runtime: None,
        container_name: None,
        container_id: None,
    };
    store.save(&repo_root, &gen1).unwrap();
    let loaded = store.load(&repo_root, &wt_path).unwrap().unwrap();
    assert_eq!(loaded.fencing_generation, Some(1));
    assert_ne!(loaded.fencing_generation, Some(2));
    assert!(
        !repo_root.join(".decapod/data/decapod.db").exists(),
        "cloud custody fencing must never touch decapod.db"
    );
}

#[test]
fn model_check_competing_agents_and_clock_anomalies_preserve_custody_and_proof_fences() {
    use decapod::core::fleet_coord::{
        LeaseLifecycle, LeaseState, lease_allows_reclaim, lease_state,
    };

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct LeaseModel {
        assigned_to: Option<String>,
        generation: i64,
        lifecycle: LeaseLifecycle,
        expires_at_secs: Option<i64>,
        revision: i64,
        done: bool,
    }

    let h = FencingHarness::new();
    let pw_a = h.acquire_session("agent-a");
    let pw_b = h.acquire_session("agent-b");
    let pw_c = h.acquire_session("agent-c");
    let creds = [("agent-a", pw_a), ("agent-b", pw_b), ("agent-c", pw_c)];

    let add = h.run_as(
        "agent-a",
        Some(&creds[0].1),
        &h.repo_root,
        &["todo", "add", "Model check 1935 task", "--format", "json"],
    );
    assert!(add.success(), "{}", add.combined);
    let task_id = add.json()["id"].as_str().unwrap().to_string();

    let mut model = LeaseModel {
        assigned_to: None,
        generation: 0,
        lifecycle: LeaseLifecycle::Claimed, // unused while assigned_to is None
        expires_at_secs: None,
        revision: 0,
        done: false,
    };

    // Deterministic Xorshift64 schedule over 48 adversarial steps combining competing
    // claims, same-gen heartbeats with forward/backward clock jumps, generation-rotating
    // renewals, yields, releases, and stale-owner completion attempts.
    let mut seed: u64 = 0x1935_cafe_babe_d00d;
    let mut next_u64 = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    let mut wall_clock: i64 = 1_800_000_000;
    let store = Store {
        kind: StoreKind::Repo,
        root: h.store_root(),
    };

    for step in 0..48usize {
        let actor_idx = (next_u64() as usize) % creds.len();
        let (actor, actor_pw) = (&creds[actor_idx].0, &creds[actor_idx].1);
        let op = (next_u64() % 6) as usize;

        // Exercise adversarial clock anomalies: backward skew (-200s), tiny forward (+1s),
        // and large forward (+700s) jumps.
        let skew_mode = next_u64() % 4;
        if skew_mode == 0 {
            wall_clock = (wall_clock - 200).max(1_700_000_000);
        } else if skew_mode == 1 {
            wall_clock += 1;
        } else {
            wall_clock += 700;
        }
        let now_ts = format!("{wall_clock}Z");

        match op {
            // 0: Competing `todo claim` via real CLI (uses system clock so use >= 1s lease or check outcome).
            0 => {
                let res = h.run_as(
                    actor,
                    Some(actor_pw),
                    &h.repo_root,
                    &[
                        "todo",
                        "claim",
                        "--id",
                        &task_id,
                        "--lease-seconds",
                        "600",
                        "--format",
                        "json",
                    ],
                );
                let (_, db_assignee, db_gen, db_lifecycle, db_exp, db_rev) =
                    h.read_task_row(&task_id);
                let live_now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                let model_exp_str = model.expires_at_secs.map(|s| format!("{s}Z"));
                let can_claim = match model.assigned_to.as_deref() {
                    None => true,
                    Some(cur) if cur == *actor => true,
                    Some(_) => {
                        !model.lifecycle.holds_exclusive_custody()
                            || lease_allows_reclaim(
                                model_exp_str.as_deref(),
                                &format!("{live_now}Z"),
                            )
                    }
                };
                if can_claim {
                    assert!(
                        res.success(),
                        "step {step} ({actor}) expected claim to succeed: {}",
                        res.combined
                    );
                    assert_eq!(db_assignee, *actor);
                    assert!(db_gen >= model.generation);
                    assert!(db_rev >= model.revision);
                    model.assigned_to = Some((*actor).to_string());
                    model.generation = db_gen;
                    model.lifecycle = LeaseLifecycle::parse(Some(&db_lifecycle));
                    model.expires_at_secs = db_exp
                        .as_deref()
                        .and_then(|s| s.trim_end_matches('Z').parse::<i64>().ok());
                    model.revision = db_rev;
                } else {
                    assert!(
                        !res.success() || res.json()["status"] == "conflict",
                        "step {step} ({actor}) competing claim bypassed live owner {:?}: {}",
                        model.assigned_to,
                        res.combined
                    );
                    assert_eq!(db_assignee, model.assigned_to.as_deref().unwrap_or(""));
                    assert_eq!(db_gen, model.generation);
                    assert_eq!(db_rev, model.revision);
                }
            }
            // 1: Same-generation heartbeat under synthetic clock anomaly (`heartbeat_claim_lease_at`).
            1 => {
                let model_exp_str = model.expires_at_secs.map(|s| format!("{s}Z"));
                let active_holder = model.assigned_to.as_deref() == Some(*actor)
                    && model.generation > 0
                    && model.lifecycle.holds_exclusive_custody()
                    && lease_state(model_exp_str.as_deref(), &now_ts) == LeaseState::Active;
                // Keep thread-local / env agent aligned only if calling directly? `heartbeat_claim_lease_at`
                // checks `crate::current_agent_id()`; test via `todo renew --heartbeat` CLI for real OS process fencing
                // and verify `lease_state` / `lease_allows_reclaim` invariants directly against model clock.
                if let Some(exp_secs) = model.expires_at_secs {
                    let expected_active = wall_clock < exp_secs;
                    assert_eq!(
                        lease_state(Some(&format!("{exp_secs}Z")), &now_ts) == LeaseState::Active,
                        expected_active
                    );
                    assert_eq!(
                        lease_allows_reclaim(Some(&format!("{exp_secs}Z")), &now_ts),
                        wall_clock >= exp_secs
                    );
                }
                let hb = h.run_as(
                    actor,
                    Some(actor_pw),
                    &h.repo_root,
                    &[
                        "todo",
                        "renew",
                        "--heartbeat",
                        "--id",
                        &task_id,
                        "--lease-seconds",
                        "600",
                        "--format",
                        "json",
                    ],
                );
                let live_now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                let real_active = model.assigned_to.as_deref() == Some(*actor)
                    && model.generation > 0
                    && model.lifecycle.holds_exclusive_custody()
                    && model.expires_at_secs.is_some_and(|e| e > live_now);
                let (_, db_assignee, db_gen, _, db_exp, db_rev) = h.read_task_row(&task_id);
                if real_active {
                    assert!(
                        hb.success(),
                        "step {step} heartbeat failed: {}",
                        hb.combined
                    );
                    assert_eq!(
                        db_gen, model.generation,
                        "heartbeat must not rotate generation"
                    );
                    assert_eq!(db_rev, model.revision, "heartbeat must not bump revision");
                    model.expires_at_secs = db_exp
                        .as_deref()
                        .and_then(|s| s.trim_end_matches('Z').parse::<i64>().ok());
                } else {
                    assert!(
                        !hb.success(),
                        "step {step} stale/non-owner heartbeat unexpectedly succeeded (active_holder={active_holder}): {}",
                        hb.combined
                    );
                    assert_eq!(db_assignee, model.assigned_to.as_deref().unwrap_or(""));
                    assert_eq!(db_gen, model.generation);
                }
            }
            // 2: Generation-rotating `todo renew`.
            2 => {
                let renew = h.run_as(
                    actor,
                    Some(actor_pw),
                    &h.repo_root,
                    &[
                        "todo",
                        "renew",
                        "--id",
                        &task_id,
                        "--lease-seconds",
                        "600",
                        "--format",
                        "json",
                    ],
                );
                let live_now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                let real_active = model.assigned_to.as_deref() == Some(*actor)
                    && model.generation > 0
                    && model.lifecycle.holds_exclusive_custody()
                    && model.expires_at_secs.is_some_and(|e| e > live_now);
                let (_, _, db_gen, db_lifecycle, db_exp, db_rev) = h.read_task_row(&task_id);
                if real_active {
                    assert!(
                        renew.success(),
                        "step {step} renew failed: {}",
                        renew.combined
                    );
                    assert!(db_gen > model.generation, "renew must rotate generation");
                    assert!(db_rev > model.revision, "renew must increment revision");
                    model.generation = db_gen;
                    model.lifecycle = LeaseLifecycle::parse(Some(&db_lifecycle));
                    model.expires_at_secs = db_exp
                        .as_deref()
                        .and_then(|s| s.trim_end_matches('Z').parse::<i64>().ok());
                    model.revision = db_rev;
                } else {
                    assert!(
                        !renew.success() || renew.json()["status"] == "conflict",
                        "step {step} stale renew succeeded: {}",
                        renew.combined
                    );
                    assert_eq!(db_gen, model.generation);
                }
            }
            // 3: `todo release` or `todo yield`.
            3 | 4 => {
                let use_yield = op == 4;
                let args: Vec<&str> = if use_yield {
                    vec![
                        "todo",
                        "yield",
                        "--id",
                        &task_id,
                        "--reason",
                        "model-check-yield",
                        "--format",
                        "json",
                    ]
                } else {
                    vec!["todo", "release", "--id", &task_id, "--format", "json"]
                };
                let rel = h.run_as(actor, Some(actor_pw), &h.repo_root, &args);
                let is_owner = model.assigned_to.as_deref() == Some(*actor)
                    && model.lifecycle.holds_exclusive_custody();
                let (_, db_assignee, db_gen, db_lifecycle, db_exp, db_rev) =
                    h.read_task_row(&task_id);
                if is_owner {
                    assert!(
                        rel.success(),
                        "step {step} release/yield failed: {}",
                        rel.combined
                    );
                    model.assigned_to = if db_assignee.is_empty() {
                        None
                    } else {
                        Some(db_assignee)
                    };
                    model.generation = db_gen;
                    model.lifecycle = LeaseLifecycle::parse(Some(&db_lifecycle));
                    model.expires_at_secs = db_exp
                        .as_deref()
                        .and_then(|s| s.trim_end_matches('Z').parse::<i64>().ok());
                    model.revision = db_rev;
                    assert!(!model.lifecycle.holds_exclusive_custody());
                } else if !use_yield && model.assigned_to.is_none() {
                    // `todo release` is idempotent on an already-unclaimed task and must not alter
                    // generation, revision, or assignee.
                    assert!(
                        rel.success()
                            && rel.json()["result"]["message"]
                                .as_str()
                                .is_some_and(|m| m.contains("already released")),
                        "step {step} idempotent release on unclaimed task unexpected: {}",
                        rel.combined
                    );
                    assert_eq!(db_assignee, "");
                    assert_eq!(db_gen, model.generation);
                    assert_eq!(db_rev, model.revision);
                } else {
                    assert!(
                        !rel.success() || rel.json()["status"] != "ok",
                        "step {step} non-owner release/yield mutated state: {}",
                        rel.combined
                    );
                    assert_eq!(db_assignee, model.assigned_to.as_deref().unwrap_or(""));
                    assert_eq!(db_gen, model.generation);
                    assert_eq!(db_rev, model.revision);
                }
            }
            // 5: Stale CAS completion attempt at `revision - 1` must always be fenced.
            _ => {
                let stale_rev = model.revision.saturating_sub(1);
                if model.revision > 0 {
                    let res = todo::update_status(
                        &store,
                        &task_id,
                        stale_rev,
                        "open",
                        "done",
                        "task.done",
                        json!({"actor": actor}),
                    )
                    .unwrap();
                    assert_eq!(res["status"], "conflict");
                }
            }
        }

        // Invariant: journal verification passes cleanly after every single schedule step.
        let jv = todo::task_journal_verify(&h.store_root()).expect("journal verify");
        assert_eq!(jv["status"], "ok", "step {step} broke journal verify: {jv}");
        assert!(!model.done);
    }
}
