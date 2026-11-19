//! Actual-binary CLI + isolated worktree regression suite for Decapod #1469:
//! concurrent authoring without broad source-claim serialization, combined
//! candidate joint-check admission, and target-advancement staleness (`STALE_*`).
#![cfg(any(target_os = "linux", target_os = "macos"))]

use decapod::core::bounded_process::{BoundedCommand, Cancellation};
use decapod::core::governance_document;
use decapod::core::proof::{ProjectProofConfig, ProofDef};
use decapod::core::readiness_evidence::{Conclusion, Observation, SuiteEvidence};
use decapod::core::readiness_profile::ProfileDeclaration;
use decapod::core::repository_profile::{
    self, RepositoryContract, RepositoryExecution, SourceCoverage,
};
use decapod::core::validation_runtime::{self as runtime, Options};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;
use tempfile::TempDir;

const CONFIG: &str = ".decapod/config.toml";
const WORKFLOW: &str = ".github/workflows/decapod-validate.yml";

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
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        let path = parent.join("decapod");
        if path.exists() {
            return path;
        }
    }
    PathBuf::from(cargo_bin)
}

fn git_program() -> PathBuf {
    ["/usr/bin/git", "/usr/local/bin/git"]
        .into_iter()
        .find_map(|p| Path::new(p).canonicalize().ok())
        .unwrap_or_else(|| PathBuf::from("git"))
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new(git_program())
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Concurrent Test")
        .env("GIT_COMMITTER_NAME", "Concurrent Test")
        .env("GIT_AUTHOR_EMAIL", "concurrent@example.invalid")
        .env("GIT_COMMITTER_EMAIL", "concurrent@example.invalid")
        .bounded_output(Duration::from_secs(20))
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed in {}: stdout={} stderr={}",
        root.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn python_runtime() -> (String, BTreeMap<String, String>) {
    let program = ["/usr/bin/python3", "/usr/local/bin/python3"]
        .into_iter()
        .find_map(|path| Path::new(path).canonicalize().ok())
        .expect("integration test requires installed Python 3");
    let output = Command::new(&program)
        .args([
            "-I",
            "-S",
            "-B",
            "-c",
            "import os, encodings, encodings.utf_8; print(os.path.realpath(os.__file__)); print(os.path.realpath(encodings.__file__)); print(os.path.realpath(encodings.utf_8.__file__))",
        ])
        .bounded_output(Duration::from_secs(10))
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let closure = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .enumerate()
        .map(|(index, path)| {
            assert!(
                Path::new(path).is_file(),
                "missing Python runtime file: {path}"
            );
            (format!("python-runtime-{index}"), path.to_owned())
        })
        .collect();
    (program.to_string_lossy().into_owned(), closure)
}

/// Build an actual-binary Decapod CLI invocation with isolated HOME/XDG state
/// and zero ambient DECAPOD_*/GIT_*/CI overrides so Agent B never inherits Agent A's
/// transcript, session, or diagnostic skip environment.
fn decapod_cmd(cwd: &Path, home_root: &Path, agent_id: &str, args: &[&str]) -> Command {
    fs::create_dir_all(home_root.join("home")).unwrap();
    fs::create_dir_all(home_root.join("home/config")).unwrap();
    fs::create_dir_all(home_root.join("home/data")).unwrap();
    fs::create_dir_all(home_root.join("home/cache")).unwrap();
    let mut cmd = Command::new(resolve_decapod_bin());
    for (name, _) in std::env::vars_os() {
        let key = name.to_string_lossy();
        if key.starts_with("DECAPOD_")
            || key.starts_with("GIT_")
            || key.starts_with("BUILDKITE")
            || key.starts_with("GITHUB_")
            || key.starts_with("XDG_")
        {
            cmd.env_remove(name);
        }
    }
    cmd.current_dir(cwd)
        .args(args)
        .env("DECAPOD_AGENT_ID", agent_id)
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1")
        .env("DECAPOD_GROUP_BROKER_REQUEST_BUDGET_MS", "180000")
        .env("HOME", home_root.join("home"))
        .env("XDG_CONFIG_HOME", home_root.join("home/config"))
        .env("XDG_DATA_HOME", home_root.join("home/data"))
        .env("XDG_CACHE_HOME", home_root.join("home/cache"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", agent_id)
        .env("GIT_COMMITTER_NAME", agent_id)
        .env("GIT_AUTHOR_EMAIL", format!("{agent_id}@example.invalid"))
        .env("GIT_COMMITTER_EMAIL", format!("{agent_id}@example.invalid"))
        .env("PATH", "/usr/bin:/bin:/usr/local/bin");
    cmd
}

fn run_decapod(cwd: &Path, home_root: &Path, agent_id: &str, args: &[&str]) -> Output {
    decapod_cmd(cwd, home_root, agent_id, args)
        .bounded_output(Duration::from_secs(120))
        .expect("run decapod CLI")
}

fn run_decapod_ok(
    cwd: &Path,
    home_root: &Path,
    agent_id: &str,
    args: &[&str],
) -> serde_json::Value {
    let out = run_decapod(cwd, home_root, agent_id, args);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "decapod {args:?} as {agent_id} failed in {}: stdout={stdout} stderr={stderr}",
        cwd.display()
    );
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("expected JSON from {args:?}: {e}\nstdout={stdout}\nstderr={stderr}")
    })
}

fn combined_diag(out: &Output) -> String {
    format!(
        "status={:?}\nstdout={}\nstderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

struct ConsumerRepo {
    _tmp: TempDir,
    root: PathBuf,
    _upstream: PathBuf,
    home_a: PathBuf,
    home_b: PathBuf,
}

impl ConsumerRepo {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let root_raw = tmp.path().join("repo");
        let upstream_raw = tmp.path().join("upstream.git");
        let home_a = tmp.path().join("actor-a");
        let home_b = tmp.path().join("actor-b");
        fs::create_dir_all(root_raw.join("scripts")).unwrap();
        fs::create_dir_all(root_raw.join(".github/workflows")).unwrap();
        fs::create_dir_all(&upstream_raw).unwrap();
        fs::create_dir_all(&home_a).unwrap();
        fs::create_dir_all(&home_b).unwrap();

        fs::write(
            root_raw.join("README.md"),
            "# Concurrent Consumer\n\nProducer and consumer modules share behavioral and interface contracts.\n",
        )
        .unwrap();
        // Baseline: `producer.compute(x, factor=2)` and `consumer.run()` calls `compute(3)`.
        // Budget invariant: `producer.LEFT_DELTA + consumer.RIGHT_DELTA <= 1`.
        fs::write(
            root_raw.join("producer.py"),
            "LEFT_DELTA = 0\n\ndef compute(x, factor=2):\n    return x * factor\n",
        )
        .unwrap();
        fs::write(
            root_raw.join("consumer.py"),
            "RIGHT_DELTA = 0\nfrom producer import compute\n\ndef run():\n    return compute(3)\n",
        )
        .unwrap();
        fs::write(
            root_raw.join("scripts/check_budget.py"),
            "import os, sys\nsys.path.insert(0, os.getcwd())\nimport producer, consumer\ntotal = producer.LEFT_DELTA + consumer.RIGHT_DELTA\nassert total <= 1, f'combined concurrency budget exceeded: {total}'\nprint('budget-ok')\n",
        )
        .unwrap();
        fs::write(
            root_raw.join("scripts/check_interface.py"),
            "import os, sys\nsys.path.insert(0, os.getcwd())\nimport consumer\nres = consumer.run()\nassert res == 6, f'unexpected result: {res}'\nprint('interface-ok')\n",
        )
        .unwrap();
        fs::write(
            root_raw.join(WORKFLOW),
            "name: Governed consumer\non: [pull_request]\njobs:\n  validate:\n    runs-on: ubuntu-latest\n    steps:\n      - run: decapod validate\n",
        )
        .unwrap();
        fs::write(
            root_raw.join(".gitignore"),
            ".decapod/data/\n.decapod/managed/governance-document.lock\n.decapod/workspaces/\n__pycache__/\n*.pyc\n",
        )
        .unwrap();

        git(&root_raw, &["init", "-b", "main"]);
        git(&upstream_raw, &["init", "--bare"]);
        let root = root_raw.canonicalize().unwrap();
        let upstream = upstream_raw.canonicalize().unwrap();
        git(
            &root,
            &["remote", "add", "origin", upstream.to_str().unwrap()],
        );

        let init_out = run_decapod(
            &root,
            &home_a,
            "bootstrap",
            &[
                "init",
                "--force",
                "--backend",
                "local",
                "--no-container-workspaces",
            ],
        );
        assert!(init_out.status.success(), "{}", combined_diag(&init_out));

        // Configure canonical_local_target = true with two real independent Python checks.
        let (program, toolchain_files) = python_runtime();
        let mut commands = vec![
            ProofDef {
                name: "check-budget".into(),
                command: program.clone(),
                args: ["-S", "-B", "scripts/check_budget.py"]
                    .map(str::to_owned)
                    .into(),
                description: "Verify combined shared-behavior budget".into(),
                required: true,
            },
            ProofDef {
                name: "check-interface".into(),
                command: program.clone(),
                args: ["-S", "-B", "scripts/check_interface.py"]
                    .map(str::to_owned)
                    .into(),
                description: "Verify producer/consumer interface compatibility".into(),
                required: true,
            },
        ];
        commands.sort_by(|a, b| a.name.cmp(&b.name));
        let execution = RepositoryExecution {
            program,
            toolchain_files,
            environment: BTreeMap::new(),
            directory: ".".into(),
            timeout_seconds: 15,
        };
        let mut contract = RepositoryContract {
            canonical_local_target: true,
            reviewed_commands_digest: String::new(),
            source_coverage: vec![SourceCoverage {
                source: WORKFLOW.into(),
                commands: commands.iter().map(|c| c.name.clone()).collect(),
                ci_only: false,
                reason: None,
            }],
            executions: commands
                .iter()
                .map(|c| (c.name.clone(), execution.clone()))
                .collect(),
        };
        contract.reviewed_commands_digest =
            repository_profile::command_contract_digest(&commands, &contract).unwrap();

        let opts = Options {
            root: root.clone(),
            git: git_program(),
            repository: "consumer/concurrent".into(),
            base_branch: "main".into(),
            accepted_base_ref: "refs/heads/main".into(),
            evidence_parent: tmp.path().join("evidence-init"),
            timeout: Duration::from_secs(30),
            max_input_bytes: 64 * 1024 * 1024,
            max_files: 10_000,
            max_output_bytes: 1024 * 1024,
        };
        fs::create_dir_all(&opts.evidence_parent).unwrap();
        let sources = runtime::discover_source_inventory(&opts, &Cancellation::default()).unwrap();
        let proof = ProjectProofConfig {
            validation_buildkite_preflights: false,
            required_profile: Some(ProfileDeclaration {
                id: repository_profile::REQUIRED_PROFILE.into(),
                reviewed_sources_digest: sources.digest().unwrap(),
            }),
            repository_contract: Some(contract),
            rust_workers: None,
            ci_attested: None,
            commands,
        };
        let mut cfg: toml::Value =
            toml::from_str(&fs::read_to_string(root.join(CONFIG)).unwrap()).unwrap();
        cfg["repo"]["base_branch"] = "main".into();
        cfg["proof"] = toml::Value::try_from(&proof).unwrap();
        fs::write(root.join(CONFIG), toml::to_string_pretty(&cfg).unwrap()).unwrap();

        git(&root, &["add", "-A"]);
        git(
            &root,
            &["commit", "-m", "accept baseline consumer contract"],
        );
        git(&root, &["push", "origin", "main"]);

        Self {
            _tmp: tmp,
            root,
            _upstream: upstream,
            home_a,
            home_b,
        }
    }
}

/// Prepare governed workbranch specs, spec reviews, checkpoint, and clean commit
/// inside `wt` so `decapod validate --format json` evaluates the actual committed tree.
fn govern_and_commit_workbranch(
    wt: &Path,
    home: &Path,
    agent_id: &str,
    task_id: &str,
    spec_update: (&str, &str),
    checkpoint_id: &str,
    commit_msg: &str,
) -> String {
    let (spec_rel_path, spec_addition) = spec_update;
    run_decapod_ok(
        wt,
        home,
        agent_id,
        &["infer", "orientation", "--task-id", task_id],
    );
    run_decapod_ok(wt, home, agent_id, &["govern", "plan", "approve"]);

    let spec_file = wt.join(spec_rel_path);
    let existing = fs::read_to_string(&spec_file).unwrap();
    fs::write(&spec_file, format!("{existing}\n{spec_addition}\n")).unwrap();

    run_decapod_ok(wt, home, agent_id, &["rpc", "--op", "specs.refresh"]);
    for name in ["ARCHITECTURE", "INTERFACES", "SECURITY"] {
        let path = format!(".decapod/managed/specs/{name}.md");
        let disposition = if path == spec_rel_path {
            "updated"
        } else {
            "unchanged-with-reason"
        };
        let reason = if path == spec_rel_path {
            "Updated living spec for concurrent workbranch change"
        } else {
            "Unchanged spec reviewed against current concurrent candidate"
        };
        run_decapod_ok(
            wt,
            home,
            agent_id,
            &[
                "govern",
                "plan",
                "review-spec",
                "--path",
                &path,
                "--disposition",
                disposition,
                "--reason",
                reason,
            ],
        );
    }

    git(wt, &["add", "-A"]);
    run_decapod_ok(
        wt,
        home,
        agent_id,
        &[
            "govern",
            "artifacts",
            "checkpoint",
            "--id",
            checkpoint_id,
            "--summary",
            commit_msg,
        ],
    );
    git(wt, &["add", governance_document::GOVERNANCE_PATH]);
    git(wt, &["commit", "-m", commit_msg]);
    assert!(git(wt, &["status", "--porcelain"]).is_empty());
    git(wt, &["rev-parse", "HEAD"])
}

fn refresh_reviews_checkpoint_and_commit(
    wt: &Path,
    home: &Path,
    agent_id: &str,
    checkpoint_id: &str,
    commit_msg: &str,
    updated_spec_rel_path: Option<&str>,
) -> String {
    run_decapod_ok(wt, home, agent_id, &["rpc", "--op", "specs.refresh"]);
    for name in ["ARCHITECTURE", "INTERFACES", "SECURITY"] {
        let path = format!(".decapod/managed/specs/{name}.md");
        let disposition = if updated_spec_rel_path == Some(path.as_str()) {
            "updated"
        } else {
            "unchanged-with-reason"
        };
        run_decapod_ok(
            wt,
            home,
            agent_id,
            &[
                "govern",
                "plan",
                "review-spec",
                "--path",
                &path,
                "--disposition",
                disposition,
                "--reason",
                "Re-reviewed specs for updated combined/rebased candidate",
            ],
        );
    }
    git(wt, &["add", "-A"]);
    run_decapod_ok(
        wt,
        home,
        agent_id,
        &[
            "govern",
            "artifacts",
            "checkpoint",
            "--id",
            checkpoint_id,
            "--summary",
            commit_msg,
        ],
    );
    git(wt, &["add", governance_document::GOVERNANCE_PATH]);
    git(wt, &["commit", "-m", commit_msg]);
    assert!(git(wt, &["status", "--porcelain"]).is_empty());
    git(wt, &["rev-parse", "HEAD"])
}

#[test]
fn scenario_1_agent_b_without_transcript_observes_live_claim_selects_permitted_or_yields() {
    let repo = ConsumerRepo::new();

    // Agent A creates and claims task_a on `shared_contract` / `producer.py`, then binds wt_a.
    let add_a = run_decapod_ok(
        &repo.root,
        &repo.home_a,
        "agent-a",
        &[
            "todo",
            "add",
            "--format",
            "json",
            "--scope",
            r#"paths:["producer.py"]"#,
            "Producer API refactor",
        ],
    );
    let task_a = add_a["id"].as_str().unwrap().to_string();
    let claim_a = run_decapod_ok(
        &repo.root,
        &repo.home_a,
        "agent-a",
        &[
            "todo",
            "claim",
            "--id",
            &task_a,
            "--lease-seconds",
            "3600",
            "--format",
            "json",
        ],
    );
    assert_eq!(claim_a["status"], "ok", "{claim_a}");
    let branch_a = format!("agent/agent-a/{task_a}-producer");
    let ws_a = run_decapod_ok(
        &repo.root,
        &repo.home_a,
        "agent-a",
        &["workspace", "ensure", "--branch", &branch_a],
    );
    assert_eq!(ws_a["can_work"], true, "{ws_a}");
    let wt_a = PathBuf::from(ws_a["worktree_path"].as_str().unwrap());

    // Two additional tasks exist:
    // - task_b_overlap shares the exact same path scope as task_a (advisory overlap, NOT blocking);
    // - task_b_disjoint touches `consumer.py` (disjoint path scope).
    let add_b_overlap = run_decapod_ok(
        &repo.root,
        &repo.home_a,
        "agent-a",
        &[
            "todo",
            "add",
            "--format",
            "json",
            "--scope",
            r#"paths:["producer.py"]"#,
            "Overlapping producer follow-up",
        ],
    );
    let task_b_overlap = add_b_overlap["id"].as_str().unwrap().to_string();

    let add_b_disjoint = run_decapod_ok(
        &repo.root,
        &repo.home_a,
        "agent-a",
        &[
            "todo",
            "add",
            "--format",
            "json",
            "--scope",
            r#"paths:["consumer.py"]"#,
            "Consumer migration",
        ],
    );
    let task_b_disjoint = add_b_disjoint["id"].as_str().unwrap().to_string();

    // Also create a shared integration reservation held by A, and a competing reservation.
    let add_int_a = run_decapod_ok(
        &repo.root,
        &repo.home_a,
        "agent-a",
        &[
            "todo",
            "add",
            "--format",
            "json",
            "--scope",
            "resource:integration:main",
            "Integration owner slot",
        ],
    );
    let task_int_a = add_int_a["id"].as_str().unwrap().to_string();
    assert_eq!(
        run_decapod_ok(
            &repo.root,
            &repo.home_a,
            "agent-a",
            &["todo", "claim", "--id", &task_int_a, "--format", "json"]
        )["status"],
        "ok"
    );
    let add_int_b = run_decapod_ok(
        &repo.root,
        &repo.home_a,
        "agent-a",
        &[
            "todo",
            "add",
            "--format",
            "json",
            "--scope",
            "resource:integration:main",
            "Competing integration slot",
        ],
    );
    let task_int_b = add_int_b["id"].as_str().unwrap().to_string();

    // Fresh Agent B starts with NO transcript/session/env from Agent A (`repo.home_b`, `agent-b`).
    // B observes canonical live claim state via `todo fleet` and `todo list`.
    let fleet = run_decapod_ok(
        &repo.root,
        &repo.home_b,
        "agent-b",
        &["todo", "fleet", "--format", "json"],
    );
    let active_claims = fleet["fleet"]["active_claims"].as_array().unwrap();
    let seen_a = active_claims
        .iter()
        .find(|c| c["task_id"] == task_a)
        .expect("Agent B must observe A's live claim in fleet projection");
    assert_eq!(seen_a["agent_id"], "agent-a");
    assert_eq!(seen_a["lease_state"], "active");
    assert_eq!(seen_a["lease_lifecycle"], "claimed");
    assert!(seen_a["lease_generation"].as_u64().unwrap() >= 1);

    let list = run_decapod_ok(
        &repo.root,
        &repo.home_b,
        "agent-b",
        &["todo", "list", "--format", "json"],
    );
    let item_a = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == task_a)
        .unwrap();
    assert_eq!(item_a["assigned_to"], "agent-a");

    // Negative controls: B CANNOT grant itself A's custody or bind A's worktree.
    let hijack_claim = run_decapod(
        &repo.root,
        &repo.home_b,
        "agent-b",
        &["todo", "claim", "--id", &task_a, "--format", "json"],
    );
    let hijack_json: serde_json::Value =
        serde_json::from_slice(&hijack_claim.stdout).expect("conflict claim returns JSON");
    assert_eq!(hijack_json["status"], "conflict", "{hijack_json}");
    assert_eq!(
        hijack_json["result"]["assigned_to"], "agent-a",
        "{hijack_json}"
    );
    assert_eq!(
        hijack_json["result"]["lease_state"], "active",
        "{hijack_json}"
    );

    let hijack_branch = format!("agent/agent-b/{task_a}-hijack");
    let hijack_ws = run_decapod(
        &repo.root,
        &repo.home_b,
        "agent-b",
        &["workspace", "ensure", "--branch", &hijack_branch],
    );
    assert!(
        !hijack_ws.status.success(),
        "B must not create a worktree for A's live task: {}",
        combined_diag(&hijack_ws)
    );
    assert!(
        combined_diag(&hijack_ws).contains("WORKSPACE_BRANCH_TODO_CLAIM_CONFLICT"),
        "{}",
        combined_diag(&hijack_ws)
    );

    // B running `workspace status` inside A's checkout cannot author (`can_work == false`, `AUTHORING_CUSTODY_STALE`).
    let status_b_in_a = run_decapod_ok(&wt_a, &repo.home_b, "agent-b", &["workspace", "status"]);
    assert_eq!(
        status_b_in_a["workspace"]["can_work"], false,
        "{status_b_in_a}"
    );
    assert!(
        status_b_in_a["workspace"]["blockers"]
            .to_string()
            .contains("AUTHORING_CUSTODY_STALE"),
        "{status_b_in_a}"
    );

    // Competing shared integration reservation yields with typed blocker (`conflict_class=shared_resource`).
    let int_conflict = run_decapod_ok(
        &repo.root,
        &repo.home_b,
        "agent-b",
        &["todo", "claim", "--id", &task_int_b, "--format", "json"],
    );
    assert_eq!(int_conflict["status"], "conflict", "{int_conflict}");
    assert_eq!(int_conflict["result"]["conflict_class"], "shared_resource");
    assert_eq!(int_conflict["result"]["predicate"], "resource_reservation");
    assert_eq!(
        int_conflict["result"]["resolution"],
        "wait_for_resource_custody"
    );
    assert_eq!(
        int_conflict["result"]["authoring_permission"],
        "isolated_source_authoring_unaffected"
    );

    // B selects a separate permitted source task EVEN WITH OVERLAPPING `paths:["producer.py"]`:
    // advisory overlap is reported, NOT falsely serialized!
    let claim_b_overlap = run_decapod_ok(
        &repo.root,
        &repo.home_b,
        "agent-b",
        &["todo", "claim", "--id", &task_b_overlap, "--format", "json"],
    );
    assert_eq!(claim_b_overlap["status"], "ok", "{claim_b_overlap}");
    assert_eq!(
        claim_b_overlap["result"]["integration_readiness"],
        "combined_candidate_reconciliation_required"
    );
    let overlaps = claim_b_overlap["result"]["advisory_overlaps"]
        .as_array()
        .unwrap();
    assert!(!overlaps.is_empty(), "{claim_b_overlap}");
    assert!(
        overlaps.iter().all(|o| o["enforced"] == false),
        "path overlap must remain advisory: {claim_b_overlap}"
    );
    let branch_b_overlap = format!("agent/agent-b/{task_b_overlap}-overlap");
    let ws_b_overlap = run_decapod_ok(
        &repo.root,
        &repo.home_b,
        "agent-b",
        &["workspace", "ensure", "--branch", &branch_b_overlap],
    );
    assert_eq!(ws_b_overlap["can_work"], true, "{ws_b_overlap}");

    // B also claims a disjoint source task (`paths:["consumer.py"]`) with zero advisory overlap
    // and binds its own isolated worktree concurrently while A's integration slot is still held.
    let claim_b_disjoint = run_decapod_ok(
        &repo.root,
        &repo.home_b,
        "agent-b",
        &[
            "todo",
            "claim",
            "--id",
            &task_b_disjoint,
            "--format",
            "json",
        ],
    );
    assert_eq!(claim_b_disjoint["status"], "ok", "{claim_b_disjoint}");
    assert!(
        claim_b_disjoint["result"]
            .get("advisory_overlaps")
            .is_none(),
        "disjoint paths + integration resource reservation must not emit bogus overlaps: {claim_b_disjoint}"
    );
    let branch_b_disjoint = format!("agent/agent-b/{task_b_disjoint}-consumer");
    let ws_b_disjoint = run_decapod_ok(
        &repo.root,
        &repo.home_b,
        "agent-b",
        &["workspace", "ensure", "--branch", &branch_b_disjoint],
    );
    assert_eq!(ws_b_disjoint["can_work"], true, "{ws_b_disjoint}");
    let wt_b = PathBuf::from(ws_b_disjoint["worktree_path"].as_str().unwrap());
    assert_ne!(wt_a, wt_b);

    // Both A and B remain concurrently admissible in their own isolated worktrees.
    let status_a = run_decapod_ok(&wt_a, &repo.home_a, "agent-a", &["workspace", "status"]);
    let status_b = run_decapod_ok(&wt_b, &repo.home_b, "agent-b", &["workspace", "status"]);
    assert_eq!(status_a["workspace"]["can_work"], true, "{status_a}");
    assert_eq!(status_b["workspace"]["can_work"], true, "{status_b}");
}

#[test]
fn scenario_2_clean_merge_failing_interface_and_behavior_blocks_admission_until_repaired() {
    let repo = ConsumerRepo::new();
    let base_0 = git(&repo.root, &["rev-parse", "main"]);

    // Provision two isolated tasks + worktrees (disjoint source files: `producer.py` vs `consumer.py`).
    let task_a = run_decapod_ok(
        &repo.root,
        &repo.home_a,
        "agent-a",
        &[
            "todo",
            "add",
            "--format",
            "json",
            "--scope",
            r#"paths:["producer.py"]"#,
            "Producer positional-only + multiplier signature",
        ],
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        run_decapod_ok(
            &repo.root,
            &repo.home_a,
            "agent-a",
            &["todo", "claim", "--id", &task_a, "--format", "json"]
        )["status"],
        "ok"
    );
    let branch_a = format!("agent/agent-a/{task_a}-producer");
    let wt_a = PathBuf::from(
        run_decapod_ok(
            &repo.root,
            &repo.home_a,
            "agent-a",
            &["workspace", "ensure", "--branch", &branch_a],
        )["worktree_path"]
            .as_str()
            .unwrap(),
    );

    let task_b = run_decapod_ok(
        &repo.root,
        &repo.home_b,
        "agent-b",
        &[
            "todo",
            "add",
            "--format",
            "json",
            "--scope",
            r#"paths:["consumer.py"]"#,
            "Consumer explicit factor keyword call",
        ],
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        run_decapod_ok(
            &repo.root,
            &repo.home_b,
            "agent-b",
            &["todo", "claim", "--id", &task_b, "--format", "json"]
        )["status"],
        "ok"
    );
    let branch_b = format!("agent/agent-b/{task_b}-consumer");
    let wt_b = PathBuf::from(
        run_decapod_ok(
            &repo.root,
            &repo.home_b,
            "agent-b",
            &["workspace", "ensure", "--branch", &branch_b],
        )["worktree_path"]
            .as_str()
            .unwrap(),
    );

    // 1. Agent A edits `producer.py` (`LEFT_DELTA = 1`, `def compute(x, /, multiplier=2)`).
    // Locally green: budget 1+0 <= 1, consumer still calls `compute(3)`.
    fs::write(
        wt_a.join("producer.py"),
        "LEFT_DELTA = 1\n\ndef compute(x, /, multiplier=2):\n    return x * multiplier\n",
    )
    .unwrap();
    let head_a = govern_and_commit_workbranch(
        &wt_a,
        &repo.home_a,
        "agent-a",
        &task_a,
        (
            ".decapod/managed/specs/ARCHITECTURE.md",
            "## Producer Concurrent Slice\n\nProducer updates `compute(x, /, multiplier=2)` with `LEFT_DELTA = 1`.",
        ),
        "cp-alpha-isolated",
        "feat(producer): isolate positional-only multiplier API",
    );
    let val_a = run_decapod_ok(
        &wt_a,
        &repo.home_a,
        "agent-a",
        &["validate", "--format", "json"],
    );
    assert_eq!(val_a["readiness"]["ready"], true, "{val_a}");

    // 2. Agent B edits `consumer.py` (`RIGHT_DELTA = 1`, `compute(3, factor=2)`).
    // Locally green: budget 0+1 <= 1, baseline producer still accepts `factor=2`.
    fs::write(
        wt_b.join("consumer.py"),
        "RIGHT_DELTA = 1\nfrom producer import compute\n\ndef run():\n    return compute(3, factor=2)\n",
    )
    .unwrap();
    let head_b = govern_and_commit_workbranch(
        &wt_b,
        &repo.home_b,
        "agent-b",
        &task_b,
        (
            ".decapod/managed/specs/INTERFACES.md",
            "## Consumer Concurrent Slice\n\nConsumer passes explicit `factor=2` with `RIGHT_DELTA = 1`.",
        ),
        "cp-beta-isolated",
        "feat(consumer): pass explicit factor keyword",
    );
    let val_b = run_decapod_ok(
        &wt_b,
        &repo.home_b,
        "agent-b",
        &["validate", "--format", "json"],
    );
    assert_eq!(val_b["readiness"]["ready"], true, "{val_b}");
    assert_ne!(head_a, head_b);

    // 3. Clean Git merge of `branch_b` into `wt_a` (`producer.py` and `consumer.py` merge with ZERO conflict).
    // Retain `wt_a`'s active governance journal (`--ours .decapod/governance.json`) and refresh projections.
    let merge_out = Command::new(git_program())
        .current_dir(&wt_a)
        .args(["merge", "--no-commit", "--no-ff", &branch_b])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .bounded_output(Duration::from_secs(20))
        .unwrap();
    if !merge_out.status.success() {
        // Only governance/living-spec projection metadata may conflict; application
        // source files (`producer.py`, `consumer.py`) must merge cleanly without conflict!
        let unmerged = git(&wt_a, &["diff", "--name-only", "--diff-filter=U"]);
        for path in unmerged.lines().filter(|l| !l.is_empty()) {
            assert!(
                path == ".decapod/governance.json" || path.starts_with(".decapod/managed/specs/"),
                "unexpected source conflict during clean merge: {unmerged}"
            );
            let side = if path == ".decapod/managed/specs/INTERFACES.md" {
                "--theirs"
            } else {
                "--ours"
            };
            git(&wt_a, &["checkout", side, path]);
            git(&wt_a, &["add", path]);
        }
    }
    let combined_broken_head = refresh_reviews_checkpoint_and_commit(
        &wt_a,
        &repo.home_a,
        "agent-a",
        "cp-combined-broken",
        "merge: combine producer and consumer slices",
        None,
    );

    // Executed joint checks on the clean-merged combined candidate MUST fail BOTH
    // (a) producer/consumer interface mismatch AND (b) contradictory shared-behavior budget!
    let val_broken = run_decapod(
        &wt_a,
        &repo.home_a,
        "agent-a",
        &["validate", "--format", "json"],
    );
    assert!(
        !val_broken.status.success(),
        "broken combined candidate must fail validate: {}",
        combined_diag(&val_broken)
    );
    let val_broken_json: serde_json::Value = serde_json::from_slice(&val_broken.stdout).unwrap();
    assert_eq!(
        val_broken_json["readiness"]["ready"], false,
        "{val_broken_json}"
    );
    assert_eq!(val_broken_json["publication_ready"], false);
    let broken_evidence = PathBuf::from(
        val_broken_json["readiness"]["evidence_path"]
            .as_str()
            .unwrap(),
    );
    assert!(
        !broken_evidence
            .join("completion/native-readiness-receipt.json")
            .exists(),
        "failed combined candidate must not emit a readiness receipt"
    );
    let broken_obs: SuiteEvidence =
        serde_json::from_slice(&fs::read(broken_evidence.join("observations.json")).unwrap())
            .unwrap();
    // `native_proof::run_native` executes planned checks in sorted order and halts at the first
    // failing check (`check-budget`), recording its failure; Step 4 below repairs `check-budget`
    // and proves `check-interface` also fails on the combined candidate.
    assert_eq!(broken_obs.results.len(), 1, "{broken_obs:?}");
    assert_eq!(broken_obs.results[0].key.job, "check-budget");
    assert!(
        matches!(
            broken_obs.results[0].observation,
            Observation::Executed {
                conclusion: Conclusion::Failure,
                ..
            }
        ),
        "check-budget (behavior) must fail on combined tree: {broken_obs:?}"
    );

    // Admission (`todo done --validated`) and `workspace publish` MUST refuse the broken combined head.
    let done_broken = run_decapod(
        &wt_a,
        &repo.home_a,
        "agent-a",
        &[
            "todo",
            "done",
            "--id",
            &task_a,
            "--validated",
            "--commit",
            &combined_broken_head,
        ],
    );
    assert!(!done_broken.status.success());
    let pub_broken = run_decapod(&wt_a, &repo.home_a, "agent-a", &["workspace", "publish"]);
    assert!(!pub_broken.status.success());

    // 4. Repair ONLY (b) contradictory shared-behavior (`RIGHT_DELTA = 0`), leaving (a) interface mismatch broken:
    //    `check-budget` now passes, while `check-interface` STILL fails -> admission STILL refuses!
    fs::write(
        wt_a.join("consumer.py"),
        "RIGHT_DELTA = 0\nfrom producer import compute\n\ndef run():\n    return compute(3, factor=2)\n",
    )
    .unwrap();
    let partial_head = refresh_reviews_checkpoint_and_commit(
        &wt_a,
        &repo.home_a,
        "agent-a",
        "cp-combined-partial",
        "fix(budget): reconcile shared delta budget only",
        None,
    );
    let val_partial = run_decapod(
        &wt_a,
        &repo.home_a,
        "agent-a",
        &["validate", "--format", "json"],
    );
    assert!(!val_partial.status.success());
    let val_partial_json: serde_json::Value = serde_json::from_slice(&val_partial.stdout).unwrap();
    assert_eq!(
        val_partial_json["readiness"]["ready"], false,
        "{val_partial_json}"
    );
    let partial_evidence = PathBuf::from(
        val_partial_json["readiness"]["evidence_path"]
            .as_str()
            .unwrap(),
    );
    assert!(
        !partial_evidence
            .join("completion/native-readiness-receipt.json")
            .exists()
    );
    let partial_obs: SuiteEvidence =
        serde_json::from_slice(&fs::read(partial_evidence.join("observations.json")).unwrap())
            .unwrap();
    assert!(partial_obs.results.iter().any(|r| {
        r.key.job == "check-budget"
            && matches!(
                r.observation,
                Observation::Executed {
                    conclusion: Conclusion::Success,
                    ..
                }
            )
    }));
    assert!(partial_obs.results.iter().any(|r| {
        r.key.job == "check-interface"
            && matches!(
                r.observation,
                Observation::Executed {
                    conclusion: Conclusion::Failure,
                    ..
                }
            )
    }));
    let done_partial = run_decapod(
        &wt_a,
        &repo.home_a,
        "agent-a",
        &[
            "todo",
            "done",
            "--id",
            &task_a,
            "--validated",
            "--commit",
            &partial_head,
        ],
    );
    assert!(!done_partial.status.success());

    // 5. Repair (a) incompatible producer/consumer interface (`compute(3, multiplier=2)`) ->
    //    revalidation on exact combined head & accepted base SUCCEEDS and binds exact head/base!
    fs::write(
        wt_a.join("consumer.py"),
        "RIGHT_DELTA = 0\nfrom producer import compute\n\ndef run():\n    return compute(3, multiplier=2)\n",
    )
    .unwrap();
    let repaired_head = refresh_reviews_checkpoint_and_commit(
        &wt_a,
        &repo.home_a,
        "agent-a",
        "cp-combined-repaired",
        "fix(interface): align consumer with positional-only multiplier",
        None,
    );
    let val_repaired = run_decapod_ok(
        &wt_a,
        &repo.home_a,
        "agent-a",
        &["validate", "--format", "json"],
    );
    assert_eq!(val_repaired["readiness"]["ready"], true, "{val_repaired}");
    let repaired_evidence =
        PathBuf::from(val_repaired["readiness"]["evidence_path"].as_str().unwrap());
    let receipt: serde_json::Value = serde_json::from_slice(
        &fs::read(repaired_evidence.join("completion/native-readiness-receipt.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["material"]["head_revision"], repaired_head);
    assert_eq!(receipt["material"]["accepted_base_revision"], base_0);

    // Land the repaired combined candidate onto `main` and complete `task_a` with `--validated --commit`.
    git(&repo.root, &["merge", "--ff-only", &branch_a]);
    let done_ok = run_decapod_ok(
        &wt_a,
        &repo.home_a,
        "agent-a",
        &[
            "todo",
            "done",
            "--id",
            &task_a,
            "--validated",
            "--commit",
            &repaired_head,
            "--format",
            "json",
        ],
    );
    assert_eq!(done_ok["status"], "ok", "{done_ok}");
}

#[test]
fn scenario_3_target_advance_invalidates_b_receipt_with_stale_until_rebased_and_revalidated() {
    let repo = ConsumerRepo::new();
    let base_0 = git(&repo.root, &["rev-parse", "main"]);

    let task_a = run_decapod_ok(
        &repo.root,
        &repo.home_a,
        "agent-a",
        &[
            "todo",
            "add",
            "--format",
            "json",
            "--scope",
            r#"paths:["producer.py"]"#,
            "Producer delta update",
        ],
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        run_decapod_ok(
            &repo.root,
            &repo.home_a,
            "agent-a",
            &["todo", "claim", "--id", &task_a, "--format", "json"]
        )["status"],
        "ok"
    );
    let branch_a = format!("agent/agent-a/{task_a}-land-first");
    let wt_a = PathBuf::from(
        run_decapod_ok(
            &repo.root,
            &repo.home_a,
            "agent-a",
            &["workspace", "ensure", "--branch", &branch_a],
        )["worktree_path"]
            .as_str()
            .unwrap(),
    );

    let task_b = run_decapod_ok(
        &repo.root,
        &repo.home_b,
        "agent-b",
        &[
            "todo",
            "add",
            "--format",
            "json",
            "--scope",
            r#"paths:["consumer.py"]"#,
            "Consumer compatible update",
        ],
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        run_decapod_ok(
            &repo.root,
            &repo.home_b,
            "agent-b",
            &["todo", "claim", "--id", &task_b, "--format", "json"]
        )["status"],
        "ok"
    );
    let branch_b = format!("agent/agent-b/{task_b}-land-second");
    let wt_b = PathBuf::from(
        run_decapod_ok(
            &repo.root,
            &repo.home_b,
            "agent-b",
            &["workspace", "ensure", "--branch", &branch_b],
        )["worktree_path"]
            .as_str()
            .unwrap(),
    );

    // 1. Both A and B commit compatible changes and pass `decapod validate` against `base_0`.
    fs::write(
        wt_a.join("producer.py"),
        "LEFT_DELTA = 1\n\ndef compute(x, factor=2):\n    return x * factor\n",
    )
    .unwrap();
    let head_a = govern_and_commit_workbranch(
        &wt_a,
        &repo.home_a,
        "agent-a",
        &task_a,
        (
            ".decapod/managed/specs/ARCHITECTURE.md",
            "## Producer Delta Landed\n\nSets `LEFT_DELTA = 1` cleanly.",
        ),
        "cp-a-first",
        "feat(producer): set LEFT_DELTA = 1",
    );
    let val_a = run_decapod_ok(
        &wt_a,
        &repo.home_a,
        "agent-a",
        &["validate", "--format", "json"],
    );
    assert_eq!(val_a["readiness"]["ready"], true, "{val_a}");

    fs::write(
        wt_b.join("consumer.py"),
        "RIGHT_DELTA = 0\nfrom producer import compute\n\ndef run():\n    return compute(3)\n\ndef healthcheck():\n    return run() == 6\n",
    )
    .unwrap();
    let head_b = govern_and_commit_workbranch(
        &wt_b,
        &repo.home_b,
        "agent-b",
        &task_b,
        (
            ".decapod/managed/specs/INTERFACES.md",
            "## Consumer Healthcheck\n\nAdds `healthcheck()` helper with `RIGHT_DELTA = 0`.",
        ),
        "cp-b-before-advance",
        "feat(consumer): add healthcheck helper",
    );
    let val_b_before = run_decapod_ok(
        &wt_b,
        &repo.home_b,
        "agent-b",
        &["validate", "--format", "json"],
    );
    assert_eq!(val_b_before["readiness"]["ready"], true, "{val_b_before}");
    let evidence_b_before =
        PathBuf::from(val_b_before["readiness"]["evidence_path"].as_str().unwrap());
    let receipt_b_before: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence_b_before.join("completion/native-readiness-receipt.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt_b_before["material"]["head_revision"], head_b);
    assert_eq!(
        receipt_b_before["material"]["accepted_base_revision"],
        base_0
    );

    // 2. Agent A lands first onto shared target `main`, advancing `refs/heads/main` to `base_1 = head_a`.
    git(&repo.root, &["merge", "--ff-only", &branch_a]);
    let base_1 = git(&repo.root, &["rev-parse", "main"]);
    assert_eq!(base_1, head_a);
    assert_ne!(base_1, base_0);
    let done_a = run_decapod_ok(
        &wt_a,
        &repo.home_a,
        "agent-a",
        &[
            "todo",
            "done",
            "--id",
            &task_a,
            "--validated",
            "--commit",
            &head_a,
            "--format",
            "json",
        ],
    );
    assert_eq!(done_a["status"], "ok", "{done_a}");

    // 3. B's earlier receipt is now STALE against the advanced shared target (`STALE_ACCEPTED_BASE`),
    //    and B is refused at validate, todo-completion, and publish boundaries!
    let val_b_stale = run_decapod(
        &wt_b,
        &repo.home_b,
        "agent-b",
        &["validate", "--format", "json"],
    );
    assert!(
        !val_b_stale.status.success(),
        "stale target must block validation: {}",
        combined_diag(&val_b_stale)
    );
    let stale_diag = combined_diag(&val_b_stale);
    assert!(
        stale_diag.contains("STALE_ACCEPTED_BASE"),
        "expected STALE_ACCEPTED_BASE blocker after A landed: {stale_diag}"
    );
    let val_b_stale_json: serde_json::Value = serde_json::from_slice(&val_b_stale.stdout).unwrap();
    assert_eq!(val_b_stale_json["readiness"]["ready"], false);
    assert_eq!(val_b_stale_json["publication_ready"], false);

    let done_b_stale = run_decapod(
        &wt_b,
        &repo.home_b,
        "agent-b",
        &[
            "todo",
            "done",
            "--id",
            &task_b,
            "--validated",
            "--commit",
            &head_b,
        ],
    );
    assert!(
        !done_b_stale.status.success(),
        "stale B receipt must not authorize todo completion"
    );

    let pub_b_stale = run_decapod(&wt_b, &repo.home_b, "agent-b", &["workspace", "publish"]);
    assert!(
        !pub_b_stale.status.success(),
        "stale B candidate must refuse publish"
    );

    // 4. B synchronizes `origin/main` to `base_1`, merges/rebases `main` cleanly onto `branch_b`,
    //    refreshes governance, and revalidates -> fresh receipt binds `(head_b_rebased, base_1)`!
    git(&repo.root, &["push", "origin", "main"]);
    git(&wt_b, &["fetch", "origin", "main"]);
    let merge_b = Command::new(git_program())
        .current_dir(&wt_b)
        .args(["merge", "--no-commit", "--no-ff", "origin/main"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .bounded_output(Duration::from_secs(20))
        .unwrap();
    if !merge_b.status.success() {
        let unmerged = git(&wt_b, &["diff", "--name-only", "--diff-filter=U"]);
        for path in unmerged.lines().filter(|l| !l.is_empty()) {
            assert!(
                path == ".decapod/governance.json" || path.starts_with(".decapod/managed/specs/"),
                "unexpected conflict rebasing onto A's landed main: {unmerged}"
            );
            let side = if path == ".decapod/managed/specs/ARCHITECTURE.md" {
                "--theirs"
            } else {
                "--ours"
            };
            git(&wt_b, &["checkout", side, path]);
            git(&wt_b, &["add", path]);
        }
    }
    let head_b_rebased = refresh_reviews_checkpoint_and_commit(
        &wt_b,
        &repo.home_b,
        "agent-b",
        "cp-b-rebased",
        "merge: rebase consumer branch onto landed main",
        Some(".decapod/managed/specs/INTERFACES.md"),
    );

    let val_b_rebased = run_decapod_ok(
        &wt_b,
        &repo.home_b,
        "agent-b",
        &["validate", "--format", "json"],
    );
    assert_eq!(val_b_rebased["readiness"]["ready"], true, "{val_b_rebased}");
    let evidence_b_rebased = PathBuf::from(
        val_b_rebased["readiness"]["evidence_path"]
            .as_str()
            .unwrap(),
    );
    let receipt_b_rebased: serde_json::Value = serde_json::from_slice(
        &fs::read(evidence_b_rebased.join("completion/native-readiness-receipt.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        receipt_b_rebased["material"]["head_revision"],
        head_b_rebased
    );
    assert_eq!(
        receipt_b_rebased["material"]["accepted_base_revision"],
        base_1
    );

    // Land B onto `main` and complete `task_b` with `--validated --commit`.
    git(&repo.root, &["merge", "--ff-only", &branch_b]);
    let done_b_ok = run_decapod_ok(
        &wt_b,
        &repo.home_b,
        "agent-b",
        &[
            "todo",
            "done",
            "--id",
            &task_b,
            "--validated",
            "--commit",
            &head_b_rebased,
            "--format",
            "json",
        ],
    );
    assert_eq!(done_b_ok["status"], "ok", "{done_b_ok}");
}
