use decapod::core::proof::ProofDef;
use decapod::core::readiness_profile::ProfileDeclaration;
use decapod::core::repository_profile::{
    self, RepositoryContract, RepositoryExecution, SourceCoverage,
};
use decapod::core::validation_runtime;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::{TempDir, tempdir};

fn decapod_bin() -> PathBuf {
    let cargo_bin = env!("CARGO_BIN_EXE_decapod");
    if let Ok(path) = Path::new(cargo_bin).canonicalize() {
        return path;
    }
    PathBuf::from(cargo_bin)
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

fn run_decapod(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(decapod_bin());
    cmd.current_dir(dir).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run decapod")
}

fn git(path: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn init_git_repo(path: &Path) {
    git(path, &["init", "-b", "main"]);
    git(path, &["config", "user.email", "test@test.com"]);
    git(path, &["config", "user.name", "test"]);
    std::fs::write(path.join(".gitkeep"), "").unwrap();
    git(path, &["add", "."]);
    git(path, &["commit", "-m", "initial"]);
}

fn extract_json(bytes: &[u8]) -> serde_json::Value {
    let text = String::from_utf8_lossy(bytes);
    let idx = text
        .find('{')
        .unwrap_or_else(|| panic!("expected JSON object in output, got:\n{text}"));
    serde_json::from_str(&text[idx..])
        .unwrap_or_else(|e| panic!("invalid JSON output ({e}):\n{text}"))
}

struct GovernedFixture {
    _tmp: TempDir,
    main_repo: PathBuf,
    wt_path: PathBuf,
    todo_id: String,
    password: String,
    agent_id: String,
}

fn apply_proof_profile(main_repo: &Path, config: &mut toml::Value) {
    let test_bin = find_in_path("test");
    let test_str = test_bin.to_string_lossy().to_string();
    let mut toolchain_files = BTreeMap::new();
    toolchain_files.insert("test".to_string(), test_str.clone());
    let commands = vec![ProofDef {
        name: "smoke".to_string(),
        command: test_str.clone(),
        args: vec!["-f".to_string(), "app.txt".to_string()],
        description: "Verify consumer app marker exists".to_string(),
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
    let mut contract = RepositoryContract {
        canonical_local_target: true,
        reviewed_commands_digest: String::new(),
        source_coverage: vec![SourceCoverage {
            source: ".github/workflows/check.yml".to_string(),
            commands: vec!["smoke".to_string()],
            ci_only: false,
            reason: None,
        }],
        executions,
    };
    contract.reviewed_commands_digest =
        repository_profile::command_contract_digest(&commands, &contract).unwrap();
    let sources = validation_runtime::discover_source_inventory(
        &validation_runtime::Options {
            root: main_repo.canonicalize().unwrap(),
            git: find_in_path("git"),
            repository: "local/ergonomics-test".to_string(),
            base_branch: "main".to_string(),
            accepted_base_ref: "refs/heads/main".to_string(),
            evidence_parent: std::env::temp_dir().canonicalize().unwrap(),
            timeout: std::time::Duration::from_secs(60),
            max_input_bytes: 8 * 1024 * 1024 * 1024,
            max_files: 200_000,
            max_output_bytes: 16 * 1024 * 1024,
        },
        &decapod::core::bounded_process::Cancellation::default(),
    )
    .unwrap();

    let root_tbl = config.as_table_mut().unwrap();
    let repo_tbl = root_tbl["repo"].as_table_mut().unwrap();
    repo_tbl.insert("backend".into(), toml::Value::String("local".into()));
    repo_tbl.insert("base_branch".into(), toml::Value::String("main".into()));
    repo_tbl.insert("canonical_local_target".into(), toml::Value::Boolean(true));
    let proof_tbl = root_tbl
        .entry("proof")
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .unwrap();
    proof_tbl.insert(
        "validation_buildkite_preflights".into(),
        toml::Value::Boolean(false),
    );
    proof_tbl.insert("commands".into(), toml::Value::try_from(&commands).unwrap());
    proof_tbl.insert(
        "repository_contract".into(),
        toml::Value::try_from(&contract).unwrap(),
    );
    proof_tbl.insert(
        "required_profile".into(),
        toml::Value::try_from(&ProfileDeclaration {
            id: repository_profile::REQUIRED_PROFILE.to_string(),
            reviewed_sources_digest: sources.digest().unwrap(),
        })
        .unwrap(),
    );
}

fn setup_governed_worktree(
    agent_id: &str,
    with_proof_profile: bool,
    configure: impl FnOnce(&mut toml::Value),
) -> GovernedFixture {
    let tmp = tempdir().unwrap();
    let main_repo = tmp.path().join("repo");
    std::fs::create_dir_all(&main_repo).unwrap();
    init_git_repo(&main_repo);

    let init = run_decapod(
        &main_repo,
        &[
            "init",
            "--force",
            "--no-container-workspaces",
            "--no-ci",
            "--dir",
            ".",
            "--product-summary",
            "Work Ledger tracks task outcomes and their supporting verification evidence.",
        ],
        &[],
    );
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let intent_path = main_repo.join(".decapod/managed/specs/INTENT.md");
    let intent_raw = std::fs::read_to_string(&intent_path).unwrap();
    std::fs::write(
        &intent_path,
        intent_raw.replace(
            "Define the user-visible outcome in one paragraph.",
            "Verify exact-candidate validation ergonomics and deadlocked-recovery workflows.",
        ),
    )
    .unwrap();
    std::fs::create_dir_all(main_repo.join(".github/workflows")).unwrap();
    std::fs::write(
        main_repo.join(".github/workflows/check.yml"),
        "name: check\non: [push]\njobs:\n  smoke:\n    runs-on: ubuntu-latest\n    steps:\n      - run: test -f app.txt\n",
    )
    .unwrap();
    std::fs::write(main_repo.join("app.txt"), "ready\n").unwrap();
    git(&main_repo, &["add", "-A"]);
    git(&main_repo, &["commit", "-m", "init base"]);

    let config_path = main_repo.join(".decapod/config.toml");
    let mut config: toml::Value =
        toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    let repo_tbl = config["repo"].as_table_mut().unwrap();
    repo_tbl.insert("container_workspaces".into(), toml::Value::Boolean(false));
    if with_proof_profile {
        apply_proof_profile(&main_repo, &mut config);
    }
    configure(&mut config);
    std::fs::write(&config_path, toml::to_string_pretty(&config).unwrap()).unwrap();

    git(&main_repo, &["add", "-A"]);
    git(&main_repo, &["commit", "-m", "configure governed repo"]);

    let session = run_decapod(&main_repo, &["session", "acquire"], &[]);
    assert!(session.status.success());
    let stdout = String::from_utf8_lossy(&session.stdout);
    let password = stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .expect("session password");

    let envs = [
        ("DECAPOD_SESSION_PASSWORD", password.as_str()),
        ("DECAPOD_AGENT_ID", agent_id),
    ];

    let add = run_decapod(
        &main_repo,
        &[
            "todo",
            "add",
            "Governed validation ergonomics task",
            "--format",
            "json",
        ],
        &envs,
    );
    assert!(
        add.status.success(),
        "todo add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let todo_id = extract_json(&add.stdout)["id"]
        .as_str()
        .unwrap()
        .to_string();

    let claim = run_decapod(
        &main_repo,
        &["todo", "claim", "--id", &todo_id, "--lease-seconds", "3600"],
        &envs,
    );
    assert!(
        claim.status.success(),
        "todo claim failed: {}",
        String::from_utf8_lossy(&claim.stderr)
    );

    let branch = format!("agent/{agent_id}/{todo_id}-ergonomics");
    let ensure = run_decapod(
        &main_repo,
        &["workspace", "ensure", "--branch", &branch],
        &envs,
    );
    assert!(
        ensure.status.success(),
        "workspace ensure failed: stdout={} stderr={}",
        String::from_utf8_lossy(&ensure.stdout),
        String::from_utf8_lossy(&ensure.stderr)
    );
    let wt_path = PathBuf::from(
        extract_json(&ensure.stdout)["worktree_path"]
            .as_str()
            .expect("worktree_path"),
    );

    let ops_path = wt_path.join(".decapod/managed/specs/OPERATIONS.md");
    let ops = std::fs::read_to_string(&ops_path).unwrap();
    std::fs::write(
        &ops_path,
        format!("{ops}\n\n## Validation ergonomics note\n\nMaterial authored spec update.\n"),
    )
    .unwrap();
    let plan = serde_json::json!({
        "schema_version": "1.0.0",
        "title": "Plan",
        "intent": "Intent",
        "state": "APPROVED",
        "todo_ids": [todo_id.clone()],
        "proof_hooks": [],
        "unknowns": [],
        "human_questions": [],
        "stop_conditions": [],
        "unresolved_contradictions": [],
        "deferred_questions": [],
        "constraints": {"forbidden_paths": []},
        "updated_at": "2026-06-20T00:00:00Z"
    });
    decapod::core::governance_document::write_section(&wt_path, "plan", &plan).unwrap();

    GovernedFixture {
        _tmp: tmp,
        main_repo,
        wt_path,
        todo_id,
        password,
        agent_id: agent_id.to_string(),
    }
}

#[cfg(unix)]
#[test]
fn rlimit_nofile_diagnosed_in_doctor_and_raised_for_validate_harnesses() {
    let fixture = setup_governed_worktree("rlimit-agent", false, |config| {
        let repo = config["repo"].as_table_mut().unwrap();
        repo.insert(
            "declared_capabilities".into(),
            toml::Value::Array(vec![toml::Value::String("persistent-state".into())]),
        );
        let mut mv = toml::map::Map::new();
        mv.insert("command".into(), toml::Value::String("sh".into()));
        mv.insert(
            "args".into(),
            toml::Value::Array(vec![
                toml::Value::String("-c".into()),
                toml::Value::String("ulimit -Sn".into()),
            ]),
        );
        mv.insert("working_directory".into(), toml::Value::String(".".into()));
        mv.insert("timeout_seconds".into(), toml::Value::Integer(30));
        mv.insert("expected_exit_code".into(), toml::Value::Integer(0));
        repo.insert("migration_validation".into(), toml::Value::Table(mv));
    });
    let exe = decapod_bin();

    let doctor_out = Command::new("sh")
        .current_dir(&fixture.wt_path)
        .args([
            "-c",
            "ulimit -Sn 256 && exec \"$1\" system doctor check --format json",
            "sh",
        ])
        .arg(&exe)
        .env("DECAPOD_SESSION_PASSWORD", &fixture.password)
        .env("DECAPOD_AGENT_ID", &fixture.agent_id)
        .output()
        .unwrap();
    assert!(
        doctor_out.status.success(),
        "doctor failed: {}",
        String::from_utf8_lossy(&doctor_out.stderr)
    );
    let doctor_json = extract_json(&doctor_out.stdout);
    let checks = doctor_json["checks"].as_array().expect("doctor checks");
    let nofile_check = checks
        .iter()
        .find(|c| c["name"] == "Open File Limit (RLIMIT_NOFILE)")
        .expect("RLIMIT_NOFILE check missing");
    assert_eq!(nofile_check["status"], "warn");
    assert!(nofile_check["message"].as_str().unwrap().contains("256"));

    // From a 256-fd shell with a normal hard limit, validation raises soft RLIMIT_NOFILE
    // for the worker and spawned migration_validation harness to >= 4096.
    let val_out = Command::new("sh")
        .current_dir(&fixture.wt_path)
        .args([
            "-c",
            "ulimit -Sn 256 && exec \"$1\" govern project-check --projections --format json",
            "sh",
        ])
        .arg(&exe)
        .env("DECAPOD_SESSION_PASSWORD", &fixture.password)
        .env("DECAPOD_AGENT_ID", &fixture.agent_id)
        .output()
        .unwrap();
    assert!(
        val_out.status.success(),
        "project-check failed: stdout={} stderr={}",
        String::from_utf8_lossy(&val_out.stdout),
        String::from_utf8_lossy(&val_out.stderr)
    );
    let evidence_path = fixture
        .wt_path
        .join(".decapod/managed/artifacts/custody/migration-validation.json");
    let evidence: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&evidence_path).unwrap()).unwrap();
    let observed_nofile = evidence["output_summary"]
        .as_str()
        .unwrap()
        .trim()
        .parse::<u64>()
        .expect("numeric ulimit -Sn output from spawned migration_validation harness");
    assert!(
        observed_nofile >= 4096,
        "expected spawned harness soft RLIMIT_NOFILE >= 4096 even from 256-fd shell, got {observed_nofile}"
    );

    // When the hard limit itself caps RLIMIT_NOFILE below 4096, validate surfaces RLIMIT_NOFILE_LOW.
    let capped_out = Command::new("sh")
        .current_dir(&fixture.wt_path)
        .args([
            "-c",
            "ulimit -Hn 512 && ulimit -Sn 256 && exec \"$1\" govern project-check --format json",
            "sh",
        ])
        .arg(&exe)
        .env("DECAPOD_SESSION_PASSWORD", &fixture.password)
        .env("DECAPOD_AGENT_ID", &fixture.agent_id)
        .output()
        .unwrap();
    assert!(
        capped_out.status.success(),
        "capped project-check failed: stdout={} stderr={}",
        String::from_utf8_lossy(&capped_out.stdout),
        String::from_utf8_lossy(&capped_out.stderr)
    );
    let capped_json = extract_json(&capped_out.stdout);
    let advisories = capped_json["report"]["advisories"]
        .as_array()
        .expect("advisories array when capped at 512");
    assert!(
        advisories.iter().any(|msg| msg
            .as_str()
            .is_some_and(|s| s.contains("RLIMIT_NOFILE_LOW"))),
        "expected RLIMIT_NOFILE_LOW advisory, got: {advisories:?}"
    );
}

fn finalize_and_commit_candidate(
    wt_path: &Path,
    envs: &[(&str, &str)],
    checkpoint_id: &str,
    todo_ids: &[String],
) {
    let refresh = run_decapod(wt_path, &["rpc", "--op", "specs.refresh"], envs);
    assert!(
        refresh.status.success(),
        "specs.refresh failed: stdout={} stderr={}",
        String::from_utf8_lossy(&refresh.stdout),
        String::from_utf8_lossy(&refresh.stderr)
    );

    let gov_raw = std::fs::read_to_string(wt_path.join(".decapod/governance.json")).unwrap();
    let gov_json: serde_json::Value = serde_json::from_str(&gov_raw).unwrap();
    let run_id = gov_json["sections"]["trajectory"]["run_id"]
        .as_str()
        .unwrap()
        .to_string();

    let mut plan_args = vec![
        "govern",
        "plan",
        "update",
        "--title",
        "Validation ergonomics plan",
        "--intent",
        "Exercise governed completion and recovery",
        "--proof-hook",
        "decapod validate",
    ];
    for id in todo_ids {
        plan_args.push("--todo-id");
        plan_args.push(id.as_str());
    }
    let plan_upd = run_decapod(wt_path, &plan_args, envs);
    assert!(
        plan_upd.status.success(),
        "plan update failed: stdout={} stderr={}",
        String::from_utf8_lossy(&plan_upd.stdout),
        String::from_utf8_lossy(&plan_upd.stderr)
    );
    let plan_approve = run_decapod(wt_path, &["govern", "plan", "approve"], envs);
    assert!(
        plan_approve.status.success(),
        "plan approve failed: stdout={} stderr={}",
        String::from_utf8_lossy(&plan_approve.stdout),
        String::from_utf8_lossy(&plan_approve.stderr)
    );

    for (spec, reason) in [
        (
            ".decapod/managed/specs/ARCHITECTURE.md",
            "Architecture unchanged in ergonomics test candidate",
        ),
        (
            ".decapod/managed/specs/INTERFACES.md",
            "Interfaces unchanged in ergonomics test candidate",
        ),
        (
            ".decapod/managed/specs/SECURITY.md",
            "Security unchanged in ergonomics test candidate",
        ),
    ] {
        let rev = run_decapod(
            wt_path,
            &[
                "govern",
                "plan",
                "review-spec",
                "--path",
                spec,
                "--disposition",
                "unchanged-with-reason",
                "--reason",
                reason,
            ],
            envs,
        );
        assert!(
            rev.status.success(),
            "review-spec {spec} failed: stdout={} stderr={}",
            String::from_utf8_lossy(&rev.stdout),
            String::from_utf8_lossy(&rev.stderr)
        );
    }

    let traj = run_decapod(
        wt_path,
        &[
            "govern",
            "trajectory",
            "record",
            "--run-id",
            &run_id,
            "--modified-file",
            ".decapod/managed/specs/OPERATIONS.md",
            "--check",
            "smoke=passed",
        ],
        envs,
    );
    assert!(
        traj.status.success(),
        "trajectory record failed: stdout={} stderr={}",
        String::from_utf8_lossy(&traj.stdout),
        String::from_utf8_lossy(&traj.stderr)
    );

    git(wt_path, &["add", ".decapod/managed/specs/"]);
    let ckpt = run_decapod(
        wt_path,
        &[
            "govern",
            "artifacts",
            "checkpoint",
            "--id",
            checkpoint_id,
            "--summary",
            "material spec update",
            "--proof-ref",
            "smoke=passed",
        ],
        envs,
    );
    assert!(
        ckpt.status.success(),
        "checkpoint failed: stdout={} stderr={}",
        String::from_utf8_lossy(&ckpt.stdout),
        String::from_utf8_lossy(&ckpt.stderr)
    );
    git(wt_path, &["add", "-A"]);
    git(wt_path, &["commit", "-m", "material spec update"]);
}

#[test]
fn unverified_done_todo_recovers_via_qa_verify_regen_without_self_deadlock() {
    let fixture = setup_governed_worktree("regen-agent", true, |_| {});
    let envs = [
        ("DECAPOD_SESSION_PASSWORD", fixture.password.as_str()),
        ("DECAPOD_AGENT_ID", fixture.agent_id.as_str()),
    ];

    let add_done = run_decapod(
        &fixture.wt_path,
        &[
            "todo",
            "add",
            "Merged task closed without validated",
            "--format",
            "json",
        ],
        &envs,
    );
    assert!(add_done.status.success());
    let done_task_id = extract_json(&add_done.stdout)["id"]
        .as_str()
        .unwrap()
        .to_string();

    finalize_and_commit_candidate(
        &fixture.wt_path,
        &envs,
        "ckpt-regen-1",
        &[fixture.todo_id.clone(), done_task_id.clone()],
    );

    let done = run_decapod(
        &fixture.wt_path,
        &["todo", "done", "--id", &done_task_id],
        &envs,
    );
    assert!(
        done.status.success(),
        "todo done failed: {}",
        String::from_utf8_lossy(&done.stderr)
    );
    let done_json = extract_json(&done.stdout);
    assert_eq!(done_json["status"], "ok", "{done_json}");
    assert_eq!(done_json["verification_status"], "CLAIMED", "{done_json}");
    assert!(
        done_json["remediation"]
            .as_str()
            .unwrap()
            .contains("qa verify regen")
    );

    let blocked = run_decapod(&fixture.wt_path, &["validate", "--format", "json"], &envs);
    assert!(!blocked.status.success());
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&blocked.stdout),
        String::from_utf8_lossy(&blocked.stderr)
    );
    assert!(combined.contains("PROOF_HOOK_FAILED"), "{combined}");
    assert!(combined.contains("qa verify regen"), "{combined}");

    let regen = run_decapod(
        &fixture.wt_path,
        &["qa", "verify", "regen", &done_task_id],
        &envs,
    );
    assert!(
        regen.status.success(),
        "qa verify regen self-deadlocked: stdout={} stderr={}",
        String::from_utf8_lossy(&regen.stdout),
        String::from_utf8_lossy(&regen.stderr)
    );

    let unblocked = run_decapod(&fixture.wt_path, &["validate"], &envs);
    assert!(
        unblocked.status.success(),
        "validate after regen failed: stdout={} stderr={}",
        String::from_utf8_lossy(&unblocked.stdout),
        String::from_utf8_lossy(&unblocked.stderr)
    );
}

#[test]
fn todo_done_validated_decouples_broker_and_cleans_handoff() {
    let fixture = setup_governed_worktree("handoff-agent", true, |_| {});
    let envs = [
        ("DECAPOD_SESSION_PASSWORD", fixture.password.as_str()),
        ("DECAPOD_AGENT_ID", fixture.agent_id.as_str()),
    ];
    finalize_and_commit_candidate(
        &fixture.wt_path,
        &envs,
        "ckpt-handoff-1",
        std::slice::from_ref(&fixture.todo_id),
    );

    let done = run_decapod(
        &fixture.wt_path,
        &[
            "todo",
            "done",
            "--id",
            &fixture.todo_id,
            "--validated",
            "--artifact",
            "app.txt",
        ],
        &envs,
    );
    assert!(
        done.status.success(),
        "todo done --validated failed: stdout={} stderr={}",
        String::from_utf8_lossy(&done.stdout),
        String::from_utf8_lossy(&done.stderr)
    );
    let broker_runtime = fixture.main_repo.join(".decapod/data/broker-runtime");
    if broker_runtime.is_dir() {
        let leftover: Vec<_> = std::fs::read_dir(&broker_runtime)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("prepared-done-")
            })
            .collect();
        assert!(
            leftover.is_empty(),
            "prepared-done handoff files were not cleaned up: {leftover:?}"
        );
    }
}
