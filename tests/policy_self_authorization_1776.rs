#![cfg(unix)]

use decapod::core::bounded_process::{BoundedCommand, Cancellation};
use decapod::core::governance_document;
use decapod::core::proof::{ProjectProofConfig, ProofDef};
use decapod::core::readiness_profile::ProfileDeclaration;
use decapod::core::repository_profile::{
    self, RepositoryContract, RepositoryExecution, SourceCoverage,
};
use decapod::core::validation_runtime as runtime;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;
use tempfile::TempDir;

const CONFIG: &str = ".decapod/config.toml";
const OVERRIDE_PATH: &str = ".decapod/OVERRIDE.md";
const WORKFLOW: &str = ".github/workflows/decapod-validate.yml";

const CLEAR_ENVS: &[&str] = &[
    "GITHUB_ACTIONS",
    "BUILDKITE",
    "GITHUB_EVENT_NAME",
    "GITHUB_WORKSPACE",
    "BUILDKITE_BUILD_CHECKOUT_PATH",
    "CHANGE_EVENT",
    "CHANGE_BASE",
    "CHANGE_HEAD",
    "DECAPOD_VALIDATE_REQUIRE_CHANGE_CONTEXT",
    "DECAPOD_VALIDATE_SKIP_GIT_GATES",
    "DECAPOD_VALIDATE_SKIP_TOOLING_GATES",
    "DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES",
    "DECAPOD_SESSION_PASSWORD",
    "DECAPOD_USER_ID",
    "DECAPOD_ROLE",
    "DECAPOD_AGENT_ID",
    "RUSTC_WRAPPER",
];

fn decapod_bin() -> PathBuf {
    let raw = env!("CARGO_BIN_EXE_decapod");
    Path::new(raw)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(raw))
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("/usr/bin/git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Policy Redteam Fixture")
        .env("GIT_COMMITTER_NAME", "Policy Redteam Fixture")
        .env("GIT_AUTHOR_EMAIL", "redteam@example.invalid")
        .env("GIT_COMMITTER_EMAIL", "redteam@example.invalid")
        .bounded_output(Duration::from_secs(15))
        .expect("git execution");
    assert!(
        out.status.success(),
        "git {args:?} failed in {}: stdout={} stderr={}",
        dir.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn python_runtime() -> (String, BTreeMap<String, String>) {
    let program = ["/usr/bin/python3", "/usr/local/bin/python3"]
        .into_iter()
        .find_map(|p| Path::new(p).canonicalize().ok())
        .expect("python3 required for proof contract fixture");
    let out = Command::new(&program)
        .args([
            "-I",
            "-S",
            "-B",
            "-c",
            "import os, encodings, encodings.utf_8; print(os.path.realpath(os.__file__)); print(os.path.realpath(encodings.__file__)); print(os.path.realpath(encodings.utf_8.__file__))",
        ])
        .bounded_output(Duration::from_secs(10))
        .unwrap();
    assert!(out.status.success());
    let files = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .enumerate()
        .map(|(i, p)| (format!("python-runtime-{i}"), p.to_string()))
        .collect();
    (program.to_string_lossy().into_owned(), files)
}

struct SessionCreds {
    agent_id: String,
    password: String,
}

struct TestRepo {
    _temp: TempDir,
    home: PathBuf,
    xdg: PathBuf,
    root: PathBuf,
    #[allow(dead_code)]
    upstream: PathBuf,
    operator: SessionCreds,
}

fn ensure_table<'a>(
    root: &'a mut toml::Value,
    key: &str,
) -> &'a mut toml::map::Map<String, toml::Value> {
    let map = root.as_table_mut().expect("toml table");
    if !map.get(key).is_some_and(|v| v.is_table()) {
        map.insert(key.into(), toml::Value::Table(toml::map::Map::new()));
    }
    map.get_mut(key).unwrap().as_table_mut().unwrap()
}

fn set_db_maintenance(doc: &mut toml::Value, enabled: bool, allowed: Option<Vec<&str>>) {
    let gov = ensure_table(doc, "governance");
    let mut maint = toml::map::Map::new();
    maint.insert("enabled".into(), enabled.into());
    if let Some(list) = allowed {
        maint.insert(
            "allowed".into(),
            toml::Value::Array(
                list.into_iter()
                    .map(|s| toml::Value::String(s.into()))
                    .collect(),
            ),
        );
    }
    gov.insert("db_maintenance".into(), toml::Value::Table(maint));
}

impl TestRepo {
    fn new(container_workspaces: bool) -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().join("home");
        let xdg = temp.path().join("xdg");
        let root = temp.path().join("repo");
        let upstream = temp.path().join("upstream.git");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&xdg).unwrap();
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&upstream).unwrap();

        fs::write(root.join("README.md"), "# Policy Test Repo\n").unwrap();
        fs::write(
            root.join(".gitignore"),
            ".decapod/data/\n.decapod/managed/governance-document.lock\n.decapod/backups/\n",
        )
        .unwrap();
        fs::create_dir_all(root.join(".github/workflows")).unwrap();
        fs::write(
            root.join(WORKFLOW),
            "name: Validate\non: [pull_request]\njobs:\n  validate:\n    runs-on: ubuntu-latest\n    steps:\n      - run: decapod validate\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::write(root.join("scripts/check.py"), "print('ok')\n").unwrap();
        fs::create_dir_all(root.join("kernel")).unwrap();
        fs::write(
            root.join("kernel/critical_invariant.txt"),
            "immutable = true\n",
        )
        .unwrap();

        git(&root, &["init", "-b", "master"]);
        git(&upstream, &["init", "--bare"]);
        git(
            &root,
            &["remote", "add", "origin", upstream.to_str().unwrap()],
        );

        let mut init_args = vec![
            "init",
            "--proof",
            "--no-ci",
            "--product-name",
            "policy-fixture",
            "--product-summary",
            "Adversarial policy self-authorization fixture",
            "--architecture-direction",
            "Local CLI governed repository",
            "--backend",
            "local",
        ];
        if !container_workspaces {
            init_args.push("--no-container-workspaces");
        }
        let repo = Self {
            _temp: temp,
            home,
            xdg,
            root: root.canonicalize().unwrap(),
            upstream: upstream.canonicalize().unwrap(),
            operator: SessionCreds {
                agent_id: "operator".into(),
                password: String::new(),
            },
        };
        repo.run_ok(&repo.root, &init_args, None);

        // Pin base_branch = "master" cleanly in initial config.
        repo.update_toml(&repo.root, |doc| {
            doc["repo"]["base_branch"] = toml::Value::String("master".into());
        });

        git(&repo.root, &["add", "-A"]);
        git(
            &repo.root,
            &["commit", "-m", "chore: accept baseline master policy"],
        );
        git(&repo.root, &["push", "-u", "origin", "master"]);
        git(&upstream, &["symbolic-ref", "HEAD", "refs/heads/master"]);
        git(&repo.root, &["remote", "set-head", "origin", "master"]);

        let creds = repo.acquire_session("operator");
        Self {
            operator: creds,
            ..repo
        }
    }

    fn cmd(&self, dir: &Path, args: &[&str], creds: Option<&SessionCreds>) -> Output {
        let mut cmd = Command::new(decapod_bin());
        for var in CLEAR_ENVS {
            cmd.env_remove(var);
        }
        cmd.current_dir(dir)
            .args(args)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.xdg)
            .env("DECAPOD_CLAIM_AUTORUN", "0")
            .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1");
        if let Some(c) = creds {
            cmd.env("DECAPOD_AGENT_ID", &c.agent_id);
            if !c.password.is_empty() {
                cmd.env("DECAPOD_SESSION_PASSWORD", &c.password);
            }
        } else {
            cmd.env("DECAPOD_AGENT_ID", &self.operator.agent_id);
            if !self.operator.password.is_empty() {
                cmd.env("DECAPOD_SESSION_PASSWORD", &self.operator.password);
            }
        }
        cmd.bounded_output(Duration::from_secs(90))
            .expect("run decapod command")
    }

    fn run_ok(&self, dir: &Path, args: &[&str], creds: Option<&SessionCreds>) -> String {
        let out = self.cmd(dir, args, creds);
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        assert!(
            out.status.success(),
            "decapod {args:?} failed in {}:\nstdout={stdout}\nstderr={stderr}",
            dir.display()
        );
        stdout
    }

    fn acquire_session(&self, agent_id: &str) -> SessionCreds {
        let mut cmd = Command::new(decapod_bin());
        for var in CLEAR_ENVS {
            cmd.env_remove(var);
        }
        let out = cmd
            .current_dir(&self.root)
            .args(["session", "acquire"])
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", &self.xdg)
            .env("DECAPOD_AGENT_ID", agent_id)
            .env("DECAPOD_CLAIM_AUTORUN", "0")
            .env("DECAPOD_GROUP_BROKER_IDLE_SECS", "1")
            .bounded_output(Duration::from_secs(30))
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "session acquire failed for {agent_id}: stdout={stdout}\nstderr={stderr}"
        );
        let password = stdout
            .lines()
            .chain(stderr.lines())
            .find_map(|line| {
                let line = line.trim();
                line.strip_prefix("Password:")
                    .or_else(|| line.strip_prefix("export DECAPOD_SESSION_PASSWORD="))
                    .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
            })
            .unwrap_or_else(|| {
                panic!("missing session password in output:\nstdout={stdout}\nstderr={stderr}")
            });
        SessionCreds {
            agent_id: agent_id.to_string(),
            password,
        }
    }

    fn update_toml(&self, repo_dir: &Path, f: impl FnOnce(&mut toml::Value)) {
        let path = repo_dir.join(CONFIG);
        let raw = fs::read_to_string(&path).unwrap();
        let mut val: toml::Value = toml::from_str(&raw).unwrap();
        f(&mut val);
        fs::write(&path, toml::to_string_pretty(&val).unwrap()).unwrap();
    }

    fn add_and_claim_todo(&self, title: &str, scope: &str, creds: &SessionCreds) -> String {
        let out = self.run_ok(
            &self.root,
            &["todo", "add", title, "--scope", scope, "--format", "json"],
            Some(creds),
        );
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let id = v["id"].as_str().unwrap().to_string();
        for attempt in 0..3 {
            let claim = self.cmd(&self.root, &["todo", "claim", "--id", &id], Some(creds));
            if claim.status.success() {
                return id;
            }
            let msg = combined(&claim);
            if msg.contains("BROKER_UNKNOWN") && attempt < 2 {
                // Follow documented BROKER_UNKNOWN recovery: inspect persisted state before retrying.
                let get = self.cmd(
                    &self.root,
                    &["todo", "get", "--id", &id, "--format", "json"],
                    Some(creds),
                );
                if get.status.success()
                    && let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(&get.stdout)
                    && parsed["owner"].as_str() == Some(creds.agent_id.as_str())
                {
                    return id;
                }
                continue;
            }
            panic!("decapod [todo claim --id {id}] failed: {msg}");
        }
        id
    }

    fn ensure_worktree(&self, branch: &str, creds: &SessionCreds) -> PathBuf {
        let out = self.run_ok(
            &self.root,
            &["workspace", "ensure", "--branch", branch],
            Some(creds),
        );
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        PathBuf::from(v["worktree_path"].as_str().unwrap())
    }

    fn prepare_governed_worktree(
        &self,
        worktree: &Path,
        task_id: &str,
        creds: &SessionCreds,
        security_note: &str,
    ) {
        self.run_ok(
            worktree,
            &["infer", "orientation", "--task-id", task_id],
            Some(creds),
        );
        self.run_ok(worktree, &["govern", "plan", "approve"], Some(creds));
        for (spec, heading) in [
            ("INTENT", "Governed policy test intent"),
            ("SECURITY", "Governed policy test slice"),
        ] {
            let p = worktree.join(format!(".decapod/managed/specs/{spec}.md"));
            let existing = fs::read_to_string(&p).unwrap();
            fs::write(
                &p,
                format!("{existing}\n\n## {heading}\n\n{security_note}\n"),
            )
            .unwrap();
        }
        self.run_ok(worktree, &["rpc", "--op", "specs.refresh"], Some(creds));
        for spec in ["ARCHITECTURE", "INTERFACES"] {
            self.run_ok(
                worktree,
                &[
                    "govern",
                    "plan",
                    "review-spec",
                    "--path",
                    &format!(".decapod/managed/specs/{spec}.md"),
                    "--disposition",
                    "unchanged-with-reason",
                    "--reason",
                    "Test slice isolates security and policy behavior without altering architecture or interfaces",
                ],
                Some(creds),
            );
        }
    }

    fn checkpoint_and_commit(
        &self,
        worktree: &Path,
        id: &str,
        summary: &str,
        creds: &SessionCreds,
    ) {
        git(worktree, &["add", "-A"]);
        self.run_ok(
            worktree,
            &[
                "govern",
                "artifacts",
                "checkpoint",
                "--id",
                id,
                "--summary",
                summary,
            ],
            Some(creds),
        );
        git(worktree, &["add", governance_document::GOVERNANCE_PATH]);
        git(worktree, &["commit", "-m", summary]);
    }

    fn commit_on_master(&self, msg: &str, f: impl FnOnce(&Path)) {
        let cur = git(&self.root, &["rev-parse", "--abbrev-ref", "HEAD"]);
        if cur != "master" {
            git(&self.root, &["checkout", "master"]);
        }
        f(&self.root);
        git(&self.root, &["add", "-A"]);
        git(&self.root, &["commit", "-m", msg]);
        git(&self.root, &["push", "origin", "master"]);
    }

    fn install_canonical_local_proof_on_master(&self, require_laya: bool) -> ProjectProofConfig {
        let (program, toolchain_files) = python_runtime();
        let def = ProofDef {
            name: "check-script".into(),
            command: program.clone(),
            args: vec![
                "-I".into(),
                "-S".into(),
                "-B".into(),
                "scripts/check.py".into(),
            ],
            description: "Run committed source check script".into(),
            required: true,
        };
        let mut contract = RepositoryContract {
            canonical_local_target: true,
            reviewed_commands_digest: String::new(),
            source_coverage: vec![SourceCoverage {
                source: WORKFLOW.into(),
                commands: vec![def.name.clone()],
                ci_only: false,
                reason: None,
            }],
            executions: BTreeMap::from([(
                def.name.clone(),
                RepositoryExecution {
                    program,
                    toolchain_files,
                    environment: BTreeMap::new(),
                    directory: ".".into(),
                    timeout_seconds: 15,
                },
            )]),
            targets: BTreeMap::new(),
        };
        let profile_id = if require_laya {
            "decapod.pr-readiness.v1"
        } else {
            "repo.pr-readiness.v1"
        };
        let mut proof = ProjectProofConfig {
            commands: vec![def.clone()],
            required_profile: Some(ProfileDeclaration {
                id: profile_id.into(),
                reviewed_sources_digest: String::new(),
            }),
            repository_contract: Some(contract.clone()),
            ..ProjectProofConfig::default()
        };
        let opts = runtime::Options {
            root: self.root.clone(),
            git: Path::new("/usr/bin/git").canonicalize().unwrap(),
            repository: "fixture/policy".into(),
            base_branch: "master".into(),
            accepted_base_ref: "refs/remotes/origin/master".into(),
            evidence_parent: self._temp.path().join("evidence"),
            timeout: Duration::from_secs(30),
            max_input_bytes: 32 * 1024 * 1024,
            max_files: 5000,
            max_output_bytes: 512 * 1024,
            target: None,
        };
        fs::create_dir_all(&opts.evidence_parent).unwrap();

        let source_digest = runtime::discover_source_inventory(&opts, &Cancellation::default())
            .unwrap()
            .digest()
            .unwrap();
        proof
            .required_profile
            .as_mut()
            .unwrap()
            .reviewed_sources_digest = source_digest;
        contract.reviewed_commands_digest =
            repository_profile::command_contract_digest(&[def], &contract).unwrap();
        proof.repository_contract = Some(contract);

        self.commit_on_master("chore: accept canonical local proof policy", |root| {
            self.update_toml(root, |doc| {
                doc.as_table_mut()
                    .unwrap()
                    .insert("proof".into(), toml::Value::try_from(&proof).unwrap());
                if require_laya {
                    ensure_table(doc, "laya").insert("mode".into(), "required".into());
                }
            });
        });
        proof
    }
}

fn combined(out: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn self_weaken_worktree_and_unaccepted_root_cannot_grant_maintenance_rbac_shared_claims_or_schema_bypass()
 {
    let repo = TestRepo::new(false);
    let attacker = repo.acquire_session("attacker");
    let primary = repo.acquire_session("primary");

    let task_id = repo.add_and_claim_todo("Self-weaken target", "root", &attacker);
    let worktree = repo.ensure_worktree(
        &format!("agent/{}/{task_id}-self-weaken", attacker.agent_id),
        &attacker,
    );
    repo.prepare_governed_worktree(&worktree, &task_id, &attacker, "Attacker worktree test");

    // 1. Worktree edits attempting to self-grant db_maintenance, RBAC bypass, and basic shared claims.
    repo.update_toml(&worktree, |doc| {
        {
            let gov = ensure_table(doc, "governance");
            gov.insert(
                "allow_basic_shared_claims".into(),
                toml::Value::Boolean(true),
            );
            gov.insert("approval_categories".into(), toml::Value::Array(vec![]));

            let mut roles = toml::map::Map::new();
            roles.insert(
                "attacker".into(),
                toml::Value::Array(vec!["maintainer".into()]),
            );
            let mut rbac = toml::map::Map::new();
            rbac.insert("actor_roles".into(), toml::Value::Table(roles));
            rbac.insert(
                "journal_repair_roles".into(),
                toml::Value::Array(vec!["maintainer".into()]),
            );
            rbac.insert(
                "validation_bypass_roles".into(),
                toml::Value::Array(vec!["maintainer".into()]),
            );
            gov.insert("rbac".into(), toml::Value::Table(rbac));
        }
        set_db_maintenance(
            doc,
            true,
            Some(vec!["db.inspect", "db.verify", "db.backup", "db.recover"]),
        );
    });
    repo.checkpoint_and_commit(
        &worktree,
        "attacker-self-grant",
        "Attempt worktree self-grant",
        &attacker,
    );

    let caps_out = repo.cmd(&worktree, &["data", "db", "capabilities"], Some(&attacker));
    let caps_txt = combined(&caps_out);
    assert!(
        !caps_out.status.success()
            || serde_json::from_str::<serde_json::Value>(&caps_txt)
                .ok()
                .and_then(|v| v["policy"]["enabled"].as_bool())
                == Some(false),
        "worktree must not enable db_maintenance capabilities: {caps_txt}"
    );
    let verify_out = repo.cmd(
        &worktree,
        &["data", "db", "maintain", "verify", "--actor", "attacker"],
        Some(&attacker),
    );
    assert!(
        !verify_out.status.success(),
        "worktree maintain verify must deny: {}",
        combined(&verify_out)
    );

    let recover_out = repo.cmd(
        &worktree,
        &["data", "db", "maintain", "recover", "--actor", "attacker"],
        Some(&attacker),
    );
    assert!(
        !recover_out.status.success(),
        "worktree maintain recover must deny: {}",
        combined(&recover_out)
    );

    let bypass_out = repo.cmd(
        &worktree,
        &[
            "govern",
            "project-check",
            "--bypass-validation",
            "--bypass-reason",
            "self-authorized worktree bypass",
        ],
        Some(&attacker),
    );
    assert!(
        !bypass_out.status.success(),
        "worktree validation bypass must deny: {}",
        combined(&bypass_out)
    );

    let shared_task = repo.add_and_claim_todo("Primary owned task", "root", &primary);
    let add_owner = repo.cmd(
        &worktree,
        &[
            "todo",
            "add-owner",
            "--id",
            &shared_task,
            "--owner",
            "attacker",
        ],
        Some(&attacker),
    );
    assert!(
        !add_owner.status.success(),
        "worktree allow_basic_shared_claims=true must not authorize shared claim: {}",
        combined(&add_owner)
    );

    // 2. Unaccepted main-root feature branch attempting the exact same self-grant + journal restore.
    git(&repo.root, &["checkout", "-b", "proposal/self-grant"]);
    repo.update_toml(&repo.root, |doc| {
        {
            let gov = ensure_table(doc, "governance");
            let mut roles = toml::map::Map::new();
            roles.insert(
                "attacker".into(),
                toml::Value::Array(vec!["maintainer".into()]),
            );
            let mut rbac = toml::map::Map::new();
            rbac.insert("actor_roles".into(), toml::Value::Table(roles));
            rbac.insert(
                "journal_repair_roles".into(),
                toml::Value::Array(vec!["maintainer".into()]),
            );
            rbac.insert(
                "validation_bypass_roles".into(),
                toml::Value::Array(vec!["maintainer".into()]),
            );
            gov.insert("rbac".into(), toml::Value::Table(rbac));
        }
        set_db_maintenance(doc, true, Some(vec!["db.verify", "db.backup"]));
    });
    git(&repo.root, &["add", CONFIG]);
    git(
        &repo.root,
        &["commit", "-m", "proposal: unaccepted root grant"],
    );

    let root_maint = repo.cmd(
        &worktree,
        &["data", "db", "maintain", "verify", "--actor", "attacker"],
        Some(&attacker),
    );
    assert!(
        !root_maint.status.success(),
        "unaccepted main-root branch must not enable db_maintenance: {}",
        combined(&root_maint)
    );
    let root_bypass = repo.cmd(
        &worktree,
        &[
            "govern",
            "project-check",
            "--bypass-validation",
            "--bypass-reason",
            "unaccepted root rbac",
        ],
        Some(&attacker),
    );
    assert!(
        !root_bypass.status.success(),
        "unaccepted main-root branch must not authorize --bypass-validation: {}",
        combined(&root_bypass)
    );
    let journal_recover = repo.cmd(
        &worktree,
        &[
            "todo",
            "journal",
            "restore",
            "--expected-current-digest",
            "0000000000000000000000000000000000000000000000000000000000000000",
        ],
        Some(&attacker),
    );
    assert!(
        !journal_recover.status.success(),
        "unaccepted main-root branch must not authorize journal restore: {}",
        combined(&journal_recover)
    );

    // 3. Unknown privilege flag `main_merge = true` must be rejected by strict schema validation.
    git(&repo.root, &["checkout", "master"]);
    repo.update_toml(&worktree, |doc| {
        ensure_table(doc, "governance").insert("main_merge".into(), toml::Value::Boolean(true));
    });
    let schema_out = repo.cmd(&worktree, &["govern", "project-check"], Some(&attacker));
    let schema_txt = combined(&schema_out);
    assert!(
        !schema_out.status.success(),
        "main_merge=true must fail: {schema_txt}"
    );
    assert!(
        schema_txt.contains("INVALID_CONFIG_SCHEMA"),
        "expected INVALID_CONFIG_SCHEMA for unknown privilege key main_merge: {schema_txt}"
    );
}

#[test]
fn candidate_base_branch_self_selection_cannot_bypass_accepted_gates_or_proof() {
    let repo = TestRepo::new(false);
    let accepted_proof = repo.install_canonical_local_proof_on_master(true);
    let attacker = repo.acquire_session("attacker");

    let task_id = repo.add_and_claim_todo("Base branch self-select", "root", &attacker);
    let branch = format!("agent/{}/{task_id}-base-bypass", attacker.agent_id);
    let worktree = repo.ensure_worktree(&branch, &attacker);
    repo.prepare_governed_worktree(&worktree, &task_id, &attacker, "Base branch attack slice");

    // Candidate sets repo.base_branch to its OWN branch, disables Laya, replaces required_profile & proof with `true`,
    // and recomputes candidate digests to impersonate an accepted target!
    let (program, toolchain_files) = python_runtime();
    let noop_def = ProofDef {
        name: "noop".into(),
        command: program.clone(),
        args: vec![
            "-I".into(),
            "-S".into(),
            "-B".into(),
            "-c".into(),
            "print('fake')".into(),
        ],
        description: "Weakened candidate command".into(),
        required: true,
    };
    let mut noop_contract = RepositoryContract {
        canonical_local_target: true,
        reviewed_commands_digest: String::new(),
        source_coverage: vec![SourceCoverage {
            source: WORKFLOW.into(),
            commands: vec![noop_def.name.clone()],
            ci_only: false,
            reason: None,
        }],
        executions: BTreeMap::from([(
            noop_def.name.clone(),
            RepositoryExecution {
                program,
                toolchain_files,
                environment: BTreeMap::new(),
                directory: ".".into(),
                timeout_seconds: 15,
            },
        )]),
        targets: BTreeMap::new(),
    };
    let opts = runtime::Options {
        root: worktree.clone(),
        git: Path::new("/usr/bin/git").canonicalize().unwrap(),
        repository: "fixture/policy".into(),
        base_branch: branch.clone(),
        accepted_base_ref: format!("refs/heads/{branch}"),
        evidence_parent: repo._temp.path().join("evidence"),
        timeout: Duration::from_secs(30),
        max_input_bytes: 32 * 1024 * 1024,
        max_files: 5000,
        max_output_bytes: 512 * 1024,
        target: None,
    };
    let src_digest = runtime::discover_source_inventory(&opts, &Cancellation::default())
        .unwrap()
        .digest()
        .unwrap();
    noop_contract.reviewed_commands_digest = repository_profile::command_contract_digest(
        std::slice::from_ref(&noop_def),
        &noop_contract,
    )
    .unwrap();
    let mut weakened_proof = accepted_proof.clone();
    weakened_proof.commands = vec![noop_def];
    weakened_proof.required_profile = Some(ProfileDeclaration {
        id: "repo.pr-readiness.v1".into(),
        reviewed_sources_digest: src_digest,
    });
    weakened_proof.repository_contract = Some(noop_contract);

    repo.update_toml(&worktree, |doc| {
        ensure_table(doc, "repo").insert("base_branch".into(), toml::Value::String(branch.clone()));
        ensure_table(doc, "laya").insert("mode".into(), toml::Value::String("disabled".into()));
        doc.as_table_mut().unwrap().insert(
            "proof".into(),
            toml::Value::try_from(&weakened_proof).unwrap(),
        );
    });
    repo.run_ok(
        &worktree,
        &["rpc", "--op", "specs.refresh"],
        Some(&attacker),
    );
    repo.checkpoint_and_commit(
        &worktree,
        "self-select-base",
        "Attempt self-selected base_branch",
        &attacker,
    );

    let proj_out = repo.cmd(
        &worktree,
        &["govern", "project-check", "--format", "json"],
        Some(&attacker),
    );
    assert!(
        !proj_out.status.success(),
        "candidate self-selecting repo.base_branch + weakening Laya/proof MUST fail project-check: {}",
        combined(&proj_out)
    );

    let val_out = repo.cmd(
        &worktree,
        &["validate", "--format", "json"],
        Some(&attacker),
    );
    assert!(
        !val_out.status.success(),
        "candidate self-selecting repo.base_branch + weakening Laya/proof MUST fail validate: {}",
        combined(&val_out)
    );
}

#[test]
fn candidate_protected_paths_removal_cannot_bypass_gatekeeper_or_project_check() {
    let repo = TestRepo::new(false);
    repo.commit_on_master("policy: protect kernel/critical_invariant.txt", |root| {
        repo.update_toml(root, |doc| {
            doc["governance"]["protected_paths"] = toml::Value::Array(vec![toml::Value::String(
                "kernel/critical_invariant.txt".into(),
            )]);
        });
    });

    let attacker = repo.acquire_session("attacker");
    let task_id = repo.add_and_claim_todo("Protected path erasure", "root", &attacker);
    let worktree = repo.ensure_worktree(
        &format!("agent/{}/{task_id}-erase-protected", attacker.agent_id),
        &attacker,
    );
    repo.prepare_governed_worktree(
        &worktree,
        &task_id,
        &attacker,
        "Protected path erasure attempt",
    );

    // Mutate protected file AND erase `protected_paths` in candidate `.decapod/config.toml`.
    fs::write(
        worktree.join("kernel/critical_invariant.txt"),
        "immutable = false # tampered\n",
    )
    .unwrap();
    repo.update_toml(&worktree, |doc| {
        doc["governance"]["protected_paths"] = toml::Value::Array(vec![]);
    });
    repo.checkpoint_and_commit(
        &worktree,
        "erase-protected",
        "Mutate protected path while erasing policy",
        &attacker,
    );

    // Check against staged change AND project-check against base.
    let gk_out = repo.cmd(
        &worktree,
        &[
            "govern",
            "gatekeeper",
            "check",
            "--paths",
            "kernel/critical_invariant.txt",
        ],
        Some(&attacker),
    );
    assert!(
        !gk_out.status.success(),
        "gatekeeper must enforce accepted master's protected_paths even when candidate removes them: {}",
        combined(&gk_out)
    );

    let proj_out = repo.cmd(
        &worktree,
        &["govern", "project-check", "--format", "json"],
        Some(&attacker),
    );
    assert!(
        !proj_out.status.success(),
        "project-check must fail when candidate modifies accepted protected_path after erasing protected_paths: {}",
        combined(&proj_out)
    );
}

#[test]
fn unaccepted_branch_or_worktree_cannot_weaken_container_workspaces_or_custody_isolation() {
    let repo = TestRepo::new(true);
    let attacker = repo.acquire_session("attacker");
    let task_id = repo.add_and_claim_todo("Weaken container isolation", "root", &attacker);
    let worktree = repo.ensure_worktree(
        &format!("agent/{}/{task_id}-weaken-container", attacker.agent_id),
        &attacker,
    );
    repo.prepare_governed_worktree(
        &worktree,
        &task_id,
        &attacker,
        "Attempt container isolation downgrade",
    );

    // 1. Candidate worktree tries disabling container_workspaces and custody.isolation.
    repo.update_toml(&worktree, |doc| {
        ensure_table(doc, "repo")
            .insert("container_workspaces".into(), toml::Value::Boolean(false));
        ensure_table(doc, "custody")
            .insert("isolation".into(), toml::Value::String("worktree".into()));
    });
    repo.checkpoint_and_commit(
        &worktree,
        "wt-disable-container",
        "Disable container_workspaces in worktree",
        &attacker,
    );
    let wt_out = repo.cmd(
        &worktree,
        &["govern", "project-check", "--format", "json"],
        Some(&attacker),
    );
    assert!(
        !wt_out.status.success(),
        "candidate worktree cannot disable container_workspaces: {}",
        combined(&wt_out)
    );

    // 2. Unaccepted main-root feature branch tries setting container_workspaces = false and custody.isolation = worktree.
    git(
        &repo.root,
        &["checkout", "-b", "attacker/disable-container"],
    );
    repo.update_toml(&repo.root, |doc| {
        ensure_table(doc, "repo")
            .insert("container_workspaces".into(), toml::Value::Boolean(false));
        ensure_table(doc, "custody")
            .insert("isolation".into(), toml::Value::String("worktree".into()));
    });
    git(&repo.root, &["add", CONFIG]);
    git(
        &repo.root,
        &["commit", "-m", "unaccepted: disable container workspaces"],
    );

    let root_branch_out = repo.cmd(
        &worktree,
        &["govern", "project-check", "--format", "json"],
        Some(&attacker),
    );
    assert!(
        !root_branch_out.status.success(),
        "unaccepted main-root feature branch cannot disable container_workspaces: {}",
        combined(&root_branch_out)
    );

    // 3. TOML comment trick `# container_workspaces = false` on master must NOT trigger raw substring check.
    git(&repo.root, &["checkout", "master"]);
    repo.commit_on_master("docs: add misleading comment to config.toml", |root| {
        let cfg = fs::read_to_string(root.join(CONFIG)).unwrap();
        fs::write(
            root.join(CONFIG),
            format!("# Do not set container_workspaces = false here\n{cfg}"),
        )
        .unwrap();
    });
    // Sync worktree config to master's commented config and add a code file so is_non_code=false.
    git(&worktree, &["checkout", "master", "--", CONFIG]);
    fs::create_dir_all(worktree.join("src")).unwrap();
    fs::write(worktree.join("src/lib.rs"), "pub fn code_change() {}\n").unwrap();
    let comment_out = repo.cmd(
        &worktree,
        &["govern", "project-check", "--format", "json"],
        Some(&attacker),
    );
    let comment_txt = combined(&comment_out);
    assert!(
        !comment_out.status.success()
            && !comment_txt.contains("Container workspace requirement disabled"),
        "comment `# container_workspaces = false` must NOT bypass container workspace enforcement: {comment_txt}"
    );
}

#[test]
fn candidate_worktree_cannot_switch_cloud_backend_to_local() {
    let repo = TestRepo::new(false);
    repo.commit_on_master("policy: enforce cloud backend", |root| {
        repo.update_toml(root, |doc| {
            doc["repo"]["backend"] = toml::Value::String("cloud".into());
        });
    });

    // Ensure workspace directly from git worktree (since cloud mode has no local todo CLI).
    let worktree = repo._temp.path().join("cloud-attack-wt");
    git(
        &repo.root,
        &[
            "worktree",
            "add",
            "-b",
            "agent/attacker/cloud-bypass",
            worktree.to_str().unwrap(),
            "master",
        ],
    );
    repo.update_toml(&worktree, |doc| {
        doc["repo"]["backend"] = toml::Value::String("local".into());
    });

    let todo_out = repo.cmd(&worktree, &["todo", "add", "local bypass todo"], None);
    let todo_txt = combined(&todo_out);
    assert!(
        !todo_out.status.success(),
        "worktree switching repo.backend=local when master requires cloud MUST fail closed: {todo_txt}"
    );
    assert!(
        !worktree.join(".decapod/data/decapod.db").exists(),
        "worktree must not initialize local decapod.db when master requires cloud"
    );

    let proj_out = repo.cmd(
        &worktree,
        &["govern", "project-check", "--format", "json"],
        None,
    );
    assert!(
        !proj_out.status.success(),
        "project-check in worktree attempting cloud->local switch must fail: {}",
        combined(&proj_out)
    );
}

#[test]
fn unaccepted_override_md_hitl_disable_cannot_bypass_approval() {
    let repo = TestRepo::new(false);
    let attacker = repo.acquire_session("attacker");
    let task_id = repo.add_and_claim_todo("OVERRIDE HITL attack", "root", &attacker);
    let worktree = repo.ensure_worktree(
        &format!("agent/{}/{task_id}-override-hitl", attacker.agent_id),
        &attacker,
    );

    // Baseline before OVERRIDE attack must require HITL for policy.purge.
    let baseline_eval = repo.run_ok(
        &worktree,
        &[
            "govern",
            "policy",
            "eval",
            "--command",
            "policy.purge",
            "--path",
            ".ruff.toml",
        ],
        Some(&attacker),
    );
    assert!(
        baseline_eval.contains("Human-in-the-loop Required: true"),
        "baseline policy.purge on .ruff.toml must require HITL: {baseline_eval}"
    );

    // 1. Candidate worktree attempts HITL_DISABLE inside plugins/POLICY.
    let wt_ovr = fs::read_to_string(worktree.join(OVERRIDE_PATH)).unwrap();
    let patched_wt_ovr = wt_ovr.replace(
        "### plugins/POLICY\n````markdown\nReplace this line with this directive's override. Use Markdown or any documentation style you prefer.\n````",
        "### plugins/POLICY\n````markdown\nHITL_DISABLE scope=global max_risk=critical\nHITL: I don't want human in the loop\n````",
    );
    assert_ne!(
        wt_ovr, patched_wt_ovr,
        "OVERRIDE.md fixture must contain ### plugins/POLICY"
    );
    fs::write(worktree.join(OVERRIDE_PATH), patched_wt_ovr).unwrap();
    repo.run_ok(&worktree, &["docs", "override"], Some(&attacker));
    let wt_eval_out = repo.run_ok(
        &worktree,
        &[
            "govern",
            "policy",
            "eval",
            "--command",
            "policy.purge",
            "--path",
            ".ruff.toml",
        ],
        Some(&attacker),
    );
    assert!(
        wt_eval_out.contains("Human-in-the-loop Required: true"),
        "worktree OVERRIDE.md must NOT disable HITL approval requirement: {wt_eval_out}"
    );

    // 2. Unaccepted main-root feature branch attempts HITL_DISABLE inside plugins/POLICY.
    git(&repo.root, &["checkout", "-b", "attacker/disable-hitl"]);
    let root_ovr = fs::read_to_string(repo.root.join(OVERRIDE_PATH)).unwrap();
    let patched_root_ovr = root_ovr.replace(
        "### plugins/POLICY\n````markdown\nReplace this line with this directive's override. Use Markdown or any documentation style you prefer.\n````",
        "### plugins/POLICY\n````markdown\nHITL_DISABLE scope=global max_risk=critical\nHITL: I don't want human in the loop\n````",
    );
    fs::write(repo.root.join(OVERRIDE_PATH), patched_root_ovr).unwrap();
    repo.run_ok(&repo.root, &["docs", "override"], Some(&attacker));
    git(&repo.root, &["add", OVERRIDE_PATH]);
    git(
        &repo.root,
        &[
            "commit",
            "-m",
            "attacker: disable HITL on unaccepted root branch",
        ],
    );

    let eval_root_store = repo.run_ok(
        &repo.root,
        &[
            "govern",
            "policy",
            "eval",
            "--command",
            "policy.purge",
            "--path",
            ".ruff.toml",
        ],
        Some(&attacker),
    );
    assert!(
        eval_root_store.contains("Human-in-the-loop Required: true"),
        "unaccepted root branch OVERRIDE.md must NOT disable HITL approval requirement on root store: {eval_root_store}"
    );

    let eval_out = repo.run_ok(
        &worktree,
        &[
            "govern",
            "policy",
            "eval",
            "--command",
            "policy.purge",
            "--path",
            ".ruff.toml",
        ],
        Some(&attacker),
    );
    assert!(
        eval_out.contains("Human-in-the-loop Required: true"),
        "unaccepted root branch OVERRIDE.md must NOT disable HITL approval requirement: {eval_out}"
    );
}

#[test]
fn unaccepted_override_md_and_validation_rules_cannot_weaken_hitl_or_evidence_gates() {
    let repo = TestRepo::new(false);
    let attacker = repo.acquire_session("attacker");
    let task_id = repo.add_and_claim_todo("OVERRIDE.md attack", "root", &attacker);
    let worktree = repo.ensure_worktree(
        &format!("agent/{}/{task_id}-override-weaken", attacker.agent_id),
        &attacker,
    );
    repo.prepare_governed_worktree(
        &worktree,
        &task_id,
        &attacker,
        "Attempt OVERRIDE.md weakening",
    );

    // 1. Test NonOverridable rule override rejection in OVERRIDE.md (in worktree)
    let base_override = fs::read_to_string(worktree.join(OVERRIDE_PATH)).unwrap();
    fs::write(
        worktree.join(OVERRIDE_PATH),
        format!(
            "{base_override}\n\n## VALIDATION_RULES Overrides\n\n### decapod.scope.protected_paths\n````toml\nseverity = \"advisory\"\n````\n"
        ),
    )
    .unwrap();
    repo.run_ok(&worktree, &["docs", "override"], Some(&attacker));
    repo.checkpoint_and_commit(
        &worktree,
        "override-nonoverridable",
        "Try overriding NonOverridable rule",
        &attacker,
    );

    let non_ovr_out = repo.cmd(
        &worktree,
        &["govern", "project-check", "--format", "json"],
        Some(&attacker),
    );
    let non_ovr_txt = combined(&non_ovr_out);
    assert!(
        !non_ovr_out.status.success() && non_ovr_txt.contains("OVERRIDE_NOT_PERMITTED"),
        "NonOverridable rule override in OVERRIDE.md must reject with OVERRIDE_NOT_PERMITTED: {non_ovr_txt}"
    );

    // 2. Test ConfigurableOnly `decapod.evidence.required_and_fresh` weakening attempt in OVERRIDE.md:
    // attacker tries lowering severity -> advisory and inflating evidence_max_age_seconds -> 999999999!
    fs::write(
        worktree.join(OVERRIDE_PATH),
        format!(
            "{base_override}\n\n## VALIDATION_RULES Overrides\n\n### decapod.evidence.required_and_fresh\n````toml\nseverity = \"advisory\"\nevidence_max_age_seconds = 999999999\n````\n"
        ),
    )
    .unwrap();
    repo.run_ok(&worktree, &["docs", "override"], Some(&attacker));
    repo.checkpoint_and_commit(
        &worktree,
        "override-weaken-evidence",
        "Try weakening evidence freshness rule",
        &attacker,
    );

    let ev_out = repo.cmd(
        &worktree,
        &["govern", "project-check", "--format", "json"],
        Some(&attacker),
    );
    assert!(
        !ev_out.status.success(),
        "OVERRIDE.md must not permit weakening decapod.evidence.required_and_fresh severity/max_age: {}",
        combined(&ev_out)
    );
}

#[test]
fn concurrent_proposal_and_toctou_policy_change_blocks_stale_receipts_and_widening() {
    let repo = TestRepo::new(false);
    let agent_a = repo.acquire_session("agent-a");
    let agent_b = repo.acquire_session("agent-b");

    // Step A: Master grants bounded db_maintenance initially (verify + backup).
    repo.commit_on_master("policy: grant bounded db_maintenance", |root| {
        repo.update_toml(root, |doc| {
            set_db_maintenance(
                doc,
                true,
                Some(vec!["db.inspect", "db.verify", "db.backup"]),
            );
        });
    });

    let task_a = repo.add_and_claim_todo("Concurrent proposal A", "tests/a.txt", &agent_a);
    let wt_a = repo.ensure_worktree(
        &format!("agent/{}/{task_a}-prop", agent_a.agent_id),
        &agent_a,
    );
    let task_b = repo.add_and_claim_todo("Concurrent exec B", "tests/b.txt", &agent_b);
    let wt_b = repo.ensure_worktree(
        &format!("agent/{}/{task_b}-exec", agent_b.agent_id),
        &agent_b,
    );

    // Agent B captures policy hash H1.
    let caps_json: serde_json::Value =
        serde_json::from_str(&repo.run_ok(&wt_b, &["data", "db", "capabilities"], Some(&agent_b)))
            .unwrap();
    let h1 = caps_json["policy"]["canonical_policy_sha256"]
        .as_str()
        .expect("policy.canonical_policy_sha256")
        .to_string();

    // Concurrently, Agent A writes a much wider policy in worktree A AND checks out an unaccepted proposal in root.
    repo.update_toml(&wt_a, |doc| {
        set_db_maintenance(
            doc,
            true,
            Some(vec!["db.inspect", "db.verify", "db.backup", "db.recover"]),
        );
    });
    let agent_a_recover = repo.cmd(
        &wt_a,
        &["data", "db", "maintain", "recover", "--actor", "agent-a"],
        Some(&agent_a),
    );
    assert!(
        !agent_a_recover.status.success(),
        "concurrent proposal by agent A must not grant agent A recover: {}",
        combined(&agent_a_recover)
    );

    // Master rotates/revokes db_maintenance to H2 before Agent B executes using H1.
    repo.commit_on_master("policy: revoke db_maintenance", |root| {
        repo.update_toml(root, |doc| {
            set_db_maintenance(doc, false, None);
        });
    });

    let stale_backup = repo.cmd(
        &wt_b,
        &[
            "data",
            "db",
            "maintain",
            "backup",
            "--destination",
            ".decapod/backups/snap-stale.db",
            "--expected-policy-sha256",
            &h1,
            "--actor",
            "agent-b",
        ],
        Some(&agent_b),
    );
    let stale_txt = combined(&stale_backup);
    assert!(
        !stale_backup.status.success(),
        "stale expected_policy_sha256 after master policy rotation MUST fail: {stale_txt}"
    );
    assert!(
        stale_txt.contains("DB_MAINTENANCE_DISABLED")
            || stale_txt.contains("POLICY_SNAPSHOT_STALE"),
        "expected policy change or disabled denial: {stale_txt}"
    );

    // Policy eval remains fail-closed / HITL-required when unapproved.
    let eval_before = repo.run_ok(
        &wt_b,
        &[
            "govern",
            "policy",
            "eval",
            "--command",
            "policy.purge",
            "--path",
            ".ruff.toml",
        ],
        Some(&agent_b),
    );
    assert!(
        eval_before.contains("Human-in-the-loop Required: true"),
        "HITL approval must stay enforced: {eval_before}"
    );

    // Proof capture TOCTOU: canonical local proof passes on master M1, then master policy rotates M2.
    repo.install_canonical_local_proof_on_master(false);
    let task_c = repo.add_and_claim_todo("Validate TOCTOU check", "root", &agent_b);
    let branch_c = format!("agent/{}/{task_c}-val-toctou", agent_b.agent_id);
    let wt_c = repo.ensure_worktree(&branch_c, &agent_b);
    repo.prepare_governed_worktree(
        &wt_c,
        &task_c,
        &agent_b,
        "Initial valid local readiness baseline",
    );
    repo.checkpoint_and_commit(&wt_c, "valid-base", "Valid change against M1", &agent_b);

    let first_val = repo.run_ok(&wt_c, &["validate", "--format", "json"], Some(&agent_b));
    let first_json: serde_json::Value = serde_json::from_str(&first_val).unwrap();
    assert_eq!(
        first_json["readiness"]["ready"].as_bool(),
        Some(true),
        "baseline canonical local validation must pass: {first_val}"
    );
    let old_head = git(&wt_c, &["rev-parse", "HEAD"]);

    // Policy/target rotates on master (M2); old commit validation is no longer current for new target!
    repo.commit_on_master(
        "policy: rotate master target + add protected path",
        |root| {
            repo.update_toml(root, |doc| {
                doc["governance"]["protected_paths"] =
                    toml::Value::Array(vec![toml::Value::String("README.md".into())]);
            });
        },
    );

    // Revalidating wt_c without syncing/updating against M2 must NOT report current readiness on stale target/policy!
    let reval = repo.cmd(&wt_c, &["validate", "--format", "json"], Some(&agent_b));
    let reval_txt = combined(&reval);
    assert!(
        !reval.status.success(),
        "stale candidate against rotated accepted master policy MUST fail validation: {reval_txt}"
    );
    // Attempting `todo done --validated --commit <old_head>` must not bypass rotated master target.
    let done_stale = repo.cmd(
        &wt_c,
        &[
            "todo",
            "done",
            "--id",
            &task_c,
            "--validated",
            "--commit",
            &old_head,
        ],
        Some(&agent_b),
    );
    assert!(
        !done_stale.status.success(),
        "todo done against rotated master target must reject stale proof: {}",
        combined(&done_stale)
    );
    let _qa_cmd = repo.cmd(
        &wt_c,
        &["qa", "verify", "commit", &old_head],
        Some(&agent_b),
    );
}

#[test]
fn legitimate_rotation_revocation_audit_and_ordinary_recovery_work() {
    let repo = TestRepo::new(false);
    let maintainer = repo.acquire_session("maintainer-a");
    let intruder = repo.acquire_session("intruder");

    let task_id = repo.add_and_claim_todo("Legit maintenance lifecycle", "root", &maintainer);
    let wt = repo.ensure_worktree(
        &format!("agent/{}/{task_id}-legit-maint", maintainer.agent_id),
        &maintainer,
    );

    // Phase 1: Initially denied (default-deny).
    let denied_init = repo.cmd(
        &wt,
        &[
            "data",
            "db",
            "maintain",
            "backup",
            "--destination",
            ".decapod/backups/snap-00.db",
            "--actor",
            "maintainer-a",
        ],
        Some(&maintainer),
    );
    assert!(
        !denied_init.status.success(),
        "default-deny backup must fail"
    );

    // Phase 2: Accepted rotation on master enables bounded verify + backup.
    repo.commit_on_master("policy: accept bounded db_maintenance rotation", |root| {
        repo.update_toml(root, |doc| {
            set_db_maintenance(
                doc,
                true,
                Some(vec!["db.inspect", "db.verify", "db.backup"]),
            );
        });
    });

    let caps_ok: serde_json::Value =
        serde_json::from_str(&repo.run_ok(&wt, &["data", "db", "capabilities"], Some(&maintainer)))
            .unwrap();
    let policy_sha = caps_ok["policy"]["canonical_policy_sha256"]
        .as_str()
        .expect("policy.canonical_policy_sha256")
        .to_string();
    assert_eq!(
        caps_ok["policy"]["enabled"].as_bool(),
        Some(true),
        "accepted policy rotation should enable db_maintenance: {caps_ok}"
    );

    let ok_verify = repo.run_ok(
        &wt,
        &[
            "data",
            "db",
            "maintain",
            "verify",
            "--expected-policy-sha256",
            &policy_sha,
            "--actor",
            "maintainer-a",
        ],
        Some(&maintainer),
    );
    assert!(
        ok_verify.contains("ok") || ok_verify.contains("verify"),
        "{ok_verify}"
    );

    let ok_backup = repo.run_ok(
        &wt,
        &[
            "data",
            "db",
            "maintain",
            "backup",
            "--destination",
            ".decapod/backups/snap-01.db",
            "--expected-policy-sha256",
            &policy_sha,
            "--actor",
            "maintainer-a",
        ],
        Some(&maintainer),
    );
    assert!(ok_backup.contains("backup"), "{ok_backup}");

    // Wrong-actor custody mismatch (`intruder` inside maintainer-a's claimed worktree) and `recover` remain denied.
    let denied_intruder = repo.cmd(
        &wt,
        &[
            "data",
            "db",
            "maintain",
            "backup",
            "--destination",
            ".decapod/backups/snap-02.db",
            "--actor",
            "intruder",
        ],
        Some(&intruder),
    );
    assert!(
        !denied_intruder.status.success(),
        "intruder without worktree custody must deny"
    );
    let denied_recover = repo.cmd(
        &wt,
        &[
            "data",
            "db",
            "maintain",
            "recover",
            "--actor",
            "maintainer-a",
        ],
        Some(&maintainer),
    );
    assert!(!denied_recover.status.success());

    // Phase 3: Revocation on master restores denial while keeping audit history.
    repo.commit_on_master("policy: revoke db_maintenance", |root| {
        repo.update_toml(root, |doc| {
            set_db_maintenance(doc, false, None);
        });
    });
    let denied_after_revoke = repo.cmd(
        &wt,
        &[
            "data",
            "db",
            "maintain",
            "backup",
            "--destination",
            ".decapod/backups/snap-03.db",
            "--actor",
            "maintainer-a",
        ],
        Some(&maintainer),
    );
    assert!(!denied_after_revoke.status.success());

    let audit_out = repo.run_ok(&wt, &["data", "db", "audit"], Some(&maintainer));
    assert!(
        audit_out.contains("maintainer-a")
            && audit_out.contains("intruder")
            && audit_out.contains("CANONICAL_LOCAL_OPT_IN_ALLOWED")
            && audit_out.contains("WORKTREE_CUSTODY_MISMATCH")
            && audit_out.contains("POLICY_OPT_IN_DISABLED"),
        "audit history must remain intact after revocation: {audit_out}"
    );

    // Phase 4: Ordinary config error recovery + stricter task-local choices remain usable.
    repo.update_toml(&wt, |doc| {
        doc["schema_version"] = toml::Value::String("99.0.0".into());
    });
    let broken_out = repo.cmd(&wt, &["todo", "list"], Some(&maintainer));
    let broken_txt = combined(&broken_out);
    assert!(
        !broken_out.status.success(),
        "unsupported schema_version must fail: {broken_txt}"
    );
    assert!(
        broken_txt.contains("INVALID_CONFIG_SCHEMA"),
        "expected INVALID_CONFIG_SCHEMA diagnostic: {broken_txt}"
    );

    // Read-only docs & schema commands still succeed despite broken config.toml!
    let doc_recovery = repo.run_ok(
        &wt,
        &[
            "docs",
            "show",
            "embedded/docs/agent/config-schema.md",
            "--source",
            "embedded",
        ],
        Some(&maintainer),
    );
    assert!(doc_recovery.contains("INVALID_CONFIG_SCHEMA"));
    let schema_recovery = repo.run_ok(
        &wt,
        &["data", "schema", "--deterministic"],
        Some(&maintainer),
    );
    assert!(schema_recovery.contains("command_registry"));

    // Repair config AND add a stricter task-local protected path.
    repo.update_toml(&wt, |doc| {
        doc["schema_version"] = toml::Value::String("1.0.0".into());
        doc["governance"]["protected_paths"] = toml::Value::Array(vec![toml::Value::String(
            "kernel/stricter_local_guard.txt".into(),
        )]);
    });
    let repaired_list = repo.run_ok(&wt, &["todo", "list"], Some(&maintainer));
    assert!(repaired_list.contains(&task_id) || !repaired_list.is_empty());

    // Stricter task-local protected path is honored by gatekeeper alongside any accepted paths.
    fs::create_dir_all(wt.join("kernel")).unwrap();
    fs::write(wt.join("kernel/stricter_local_guard.txt"), "tightened\n").unwrap();
    let stricter_gk = repo.cmd(
        &wt,
        &[
            "govern",
            "gatekeeper",
            "check",
            "--paths",
            "kernel/stricter_local_guard.txt",
        ],
        Some(&maintainer),
    );
    assert!(
        !stricter_gk.status.success(),
        "stricter task-local protected_path should enforce gatekeeper block: {}",
        combined(&stricter_gk)
    );
}
