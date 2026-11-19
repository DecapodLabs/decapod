use decapod::core::init_reconciliation::discover_source_inventory;
use decapod::core::proof::{ProjectProofConfig, ProofDef};
use decapod::core::readiness_profile::ProfileDeclaration;
use decapod::core::repository_profile::{
    self, RepositoryContract, RepositoryExecution, SourceCoverage, command_contract_digest,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const SKIP_GIT: &str = "DECAPOD_VALIDATE_SKIP_GIT_GATES";
const SKIP_TOOLING: &str = "DECAPOD_VALIDATE_SKIP_TOOLING_GATES";
const SKIP_FINGERPRINT: &str = "DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES";
const OVERRIDE_CODE: &str = "READINESS_GATE_OVERRIDE_UNSUPPORTED";

struct TempWorkspace {
    path: PathBuf,
}

impl TempWorkspace {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after unix epoch")
            .as_nanos();
        let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir_name = format!("decapod-1659-{label}-{}-{nanos}-{seq}", std::process::id());
        let path = std::env::temp_dir().join(dir_name);
        fs::create_dir_all(&path).expect("create temp workspace dir");
        Self { path }
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn decapod_bin() -> PathBuf {
    let resolve = |raw: &Path| -> Option<PathBuf> {
        let abs = if raw.is_relative() {
            std::env::current_dir().ok()?.join(raw)
        } else {
            raw.to_path_buf()
        };
        let canon = fs::canonicalize(&abs).unwrap_or(abs);
        canon.is_file().then_some(canon)
    };

    if let Some(bin) = option_env!("CARGO_BIN_EXE_decapod")
        && let Some(found) = resolve(Path::new(bin))
    {
        return found;
    }
    if let Some(bin) = std::env::var_os("CARGO_BIN_EXE_decapod")
        && let Some(found) = resolve(Path::new(&bin))
    {
        return found;
    }
    for key in ["RUNFILES_DIR", "TEST_SRCDIR"] {
        if let Some(root) = std::env::var_os(key) {
            for rel in ["_main/decapod", "__main__/decapod"] {
                let candidate = PathBuf::from(&root).join(rel);
                if let Some(found) = resolve(&candidate) {
                    return found;
                }
            }
        }
    }
    let current = std::env::current_exe().expect("current test exe path");
    if let Some(dir) = current.parent() {
        if let Some(found) = resolve(&dir.join("decapod")) {
            return found;
        }
        if let Some(parent) = dir.parent()
            && let Some(found) = resolve(&parent.join("decapod"))
        {
            return found;
        }
    }
    panic!(
        "could not locate decapod binary via CARGO_BIN_EXE_decapod, runfiles, or current_exe={}",
        current.display()
    );
}

fn sanitized_command(program: &Path, cwd: &Path, home: &Path, envs: &[(&str, &str)]) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(cwd);
    for (key, _) in std::env::vars_os() {
        let s = key.to_string_lossy();
        if s.starts_with("DECAPOD_")
            || s.starts_with("GIT_")
            || s.starts_with("BUILDKITE")
            || s.starts_with("GITHUB_")
            || s.starts_with("XDG_")
            || s == "CI"
        {
            cmd.env_remove(&key);
        }
    }
    let xdg = home.join(".xdg");
    let _ = fs::create_dir_all(&xdg);
    cmd.env("HOME", home)
        .env("XDG_CONFIG_HOME", xdg.join("config"))
        .env("XDG_DATA_HOME", xdg.join("data"))
        .env("XDG_CACHE_HOME", xdg.join("cache"))
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env("DECAPOD_AGENT_ID", "test-gates-1659")
        .env("GIT_AUTHOR_NAME", "Test Agent")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test Agent")
        .env("GIT_COMMITTER_EMAIL", "test@example.com");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd
}

fn run_git(cwd: &Path, home: &Path, args: &[&str]) -> Output {
    let mut cmd = sanitized_command(Path::new("git"), cwd, home, &[]);
    cmd.args(args);
    let out = cmd.output().expect("spawn git");
    assert!(
        out.status.success(),
        "git {:?} failed in {}:\nstdout:\n{}\nstderr:\n{}",
        args,
        cwd.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn run_decapod(cwd: &Path, home: &Path, envs: &[(&str, &str)], args: &[&str]) -> Output {
    let bin = decapod_bin();
    let mut cmd = sanitized_command(&bin, cwd, home, envs);
    cmd.args(args);
    cmd.output().expect("spawn decapod")
}

fn run_decapod_ok(cwd: &Path, home: &Path, envs: &[(&str, &str)], args: &[&str]) -> Output {
    let out = run_decapod(cwd, home, envs, args);
    assert!(
        out.status.success(),
        "decapod {:?} failed (status={:?}) in {}:\nstdout:\n{}\nstderr:\n{}",
        args,
        out.status.code(),
        cwd.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn combined_output(out: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn parse_session_password(stdout: &[u8]) -> String {
    let text = String::from_utf8_lossy(stdout);
    text.lines()
        .find_map(|line| line.strip_prefix("Password:"))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| panic!("missing Password line in session acquire output:\n{text}"))
}

fn write_canonical_local_config(repo: &Path) {
    let cfg_path = repo.join(".decapod").join("config.toml");
    let raw = fs::read_to_string(&cfg_path).expect("read generated config.toml");
    let mut doc: toml::Value = toml::from_str(&raw).expect("parse config.toml");

    let sources = discover_source_inventory(repo).expect("discover source inventory");
    let sources_digest = sources.digest().expect("hash source inventory");

    let true_bin = ["/usr/bin/true", "/bin/true"]
        .into_iter()
        .find(|p| Path::new(p).is_file())
        .expect("locate true binary at /usr/bin/true or /bin/true")
        .to_string();

    let commands = vec![ProofDef {
        name: "local-check".to_string(),
        command: true_bin.clone(),
        args: Vec::new(),
        description: "Canonical local proof check".to_string(),
        required: true,
    }];

    let mut toolchain_files = BTreeMap::new();
    toolchain_files.insert("true".to_string(), true_bin.clone());

    let mut executions = BTreeMap::new();
    executions.insert(
        "local-check".to_string(),
        RepositoryExecution {
            program: true_bin,
            toolchain_files,
            environment: BTreeMap::new(),
            directory: ".".to_string(),
            timeout_seconds: 30,
        },
    );

    let source_coverage: Vec<SourceCoverage> = sources
        .files
        .keys()
        .filter(|path| path.starts_with(".github/workflows/"))
        .map(|path| SourceCoverage {
            source: path.clone(),
            commands: vec!["local-check".to_string()],
            ci_only: false,
            reason: None,
        })
        .collect();
    assert!(
        !source_coverage.is_empty(),
        "expected at least one discovered workflow in {:?}",
        sources.files.keys().collect::<Vec<_>>()
    );

    let mut contract = RepositoryContract {
        canonical_local_target: true,
        reviewed_commands_digest: String::new(),
        source_coverage,
        executions,
        targets: Default::default(),
    };
    contract.reviewed_commands_digest =
        command_contract_digest(&commands, &contract).expect("compute command contract digest");

    let proof_cfg = ProjectProofConfig {
        ci_attested: None,
        validation_buildkite_preflights: false,
        required_profile: Some(ProfileDeclaration {
            id: repository_profile::REQUIRED_PROFILE.to_string(),
            reviewed_sources_digest: sources_digest,
        }),
        repository_contract: Some(contract),
        rust_workers: None,
        commands,
    };

    let tbl = doc.as_table_mut().expect("config root table");
    tbl.insert(
        "proof".to_string(),
        toml::Value::try_from(&proof_cfg).expect("serialize proof config"),
    );
    if let Some(custody) = tbl.get_mut("custody").and_then(|v| v.as_table_mut()) {
        custody.insert(
            "isolation".to_string(),
            toml::Value::String("worktree".to_string()),
        );
    }
    if let Some(repo_tbl) = tbl.get_mut("repo").and_then(|v| v.as_table_mut()) {
        repo_tbl.insert(
            "container_workspaces".to_string(),
            toml::Value::Boolean(false),
        );
        repo_tbl.insert(
            "product_name".to_string(),
            toml::Value::String("gate-skip-override-fixture".to_string()),
        );
        repo_tbl.insert(
            "product_summary".to_string(),
            toml::Value::String(
                "Isolated fixture repository proving readiness gate-skip overrides never establish publication readiness."
                    .to_string(),
            ),
        );
    }

    let rendered = toml::to_string_pretty(&doc).expect("render config.toml");
    fs::write(&cfg_path, rendered).expect("write updated config.toml");

    let intent_path = repo
        .join(".decapod")
        .join("managed")
        .join("specs")
        .join("INTENT.md");
    if let Ok(intent_raw) = fs::read_to_string(&intent_path) {
        let updated_intent = intent_raw.replace(
            "Define the user-visible outcome in one paragraph.",
            "Isolated fixture repository proving readiness gate-skip overrides never establish publication readiness.",
        );
        fs::write(&intent_path, updated_intent).expect("write updated INTENT.md");
    }
}

struct DisposableRepo {
    _root: TempWorkspace,
    home: PathBuf,
    repo: PathBuf,
    worktree: PathBuf,
    session_password: String,
    todo_id: String,
}

impl DisposableRepo {
    fn create(label: &str) -> Self {
        let root = TempWorkspace::new(label);
        let home = root.path.join("home");
        let remote = root.path.join("remote.git");
        let repo = root.path.join("repo");
        fs::create_dir_all(&home).expect("create home");
        fs::create_dir_all(&remote).expect("create remote");
        fs::create_dir_all(&repo).expect("create repo");
        fs::create_dir_all(repo.join(".github").join("workflows")).expect("create workflows dir");

        run_git(&remote, &home, &["init", "--bare", "-b", "master"]);
        run_git(&repo, &home, &["init", "-b", "master"]);
        run_git(&repo, &home, &["config", "user.name", "Test Agent"]);
        run_git(&repo, &home, &["config", "user.email", "test@example.com"]);
        run_git(
            &repo,
            &home,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );

        fs::write(repo.join("README.md"), "# Disposable test repo\n").expect("write README");
        fs::write(repo.join("app.txt"), "initial\n").expect("write app.txt");
        fs::write(
            repo.join(".github").join("workflows").join("ci.yml"),
            "name: ci\non: [pull_request]\njobs:\n  local:\n    runs-on: ubuntu-latest\n    steps:\n      - run: exit 0\n",
        )
        .expect("write ci.yml");

        run_git(&repo, &home, &["add", "."]);
        run_git(&repo, &home, &["commit", "-m", "initial commit"]);
        run_git(&repo, &home, &["push", "-u", "origin", "master"]);

        run_decapod_ok(&repo, &home, &[], &["init", "--proof", "--force"]);

        let sess_out = run_decapod_ok(&repo, &home, &[], &["session", "acquire"]);
        let session_password = parse_session_password(&sess_out.stdout);
        let envs = [("DECAPOD_SESSION_PASSWORD", session_password.as_str())];

        write_canonical_local_config(&repo);

        run_git(&repo, &home, &["add", "-A"]);
        run_git(&repo, &home, &["commit", "-m", "init decapod"]);
        run_git(&repo, &home, &["push", "origin", "master"]);

        let todo_id = Self::add_and_claim_todo(
            &repo,
            &home,
            &session_password,
            "Readiness gate override test",
        );

        let branch = format!("agent/test-gates-1659/{todo_id}-readiness");
        let ws_out = run_decapod_ok(
            &repo,
            &home,
            &envs,
            &["workspace", "ensure", "--branch", &branch],
        );
        let ws_json: serde_json::Value =
            serde_json::from_slice(&ws_out.stdout).expect("parse workspace ensure json");
        let worktree = ws_json["worktree_path"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                panic!(
                    "missing worktree_path in workspace ensure output:\n{}",
                    combined_output(&ws_out)
                )
            });

        let wt_sess = run_decapod_ok(&worktree, &home, &[], &["session", "acquire"]);
        let session_password = parse_session_password(&wt_sess.stdout);
        let wt_envs = [("DECAPOD_SESSION_PASSWORD", session_password.as_str())];

        run_decapod_ok(&worktree, &home, &wt_envs, &["rpc", "--op", "agent.init"]);
        run_decapod_ok(
            &worktree,
            &home,
            &wt_envs,
            &[
                "rpc",
                "--op",
                "constitution.get",
                "--params",
                r#"{"section":"core/DECAPOD"}"#,
            ],
        );
        run_decapod_ok(
            &worktree,
            &home,
            &wt_envs,
            &["rpc", "--op", "context.resolve"],
        );

        Self {
            _root: root,
            home,
            repo,
            worktree,
            session_password,
            todo_id,
        }
    }

    fn add_and_claim_todo(repo: &Path, home: &Path, session_password: &str, title: &str) -> String {
        let envs = [("DECAPOD_SESSION_PASSWORD", session_password)];
        let todo_out = run_decapod_ok(
            repo,
            home,
            &envs,
            &["todo", "add", title, "--format", "json"],
        );
        let todo_json: serde_json::Value =
            serde_json::from_slice(&todo_out.stdout).expect("parse todo add json");
        let id = todo_json["id"]
            .as_str()
            .expect("todo id from todo add")
            .to_string();
        run_decapod_ok(repo, home, &envs, &["todo", "claim", "--id", &id]);
        id
    }

    fn base_envs(&self) -> Vec<(&str, &str)> {
        vec![("DECAPOD_SESSION_PASSWORD", self.session_password.as_str())]
    }

    fn run_in_worktree(&self, extra_envs: &[(&str, &str)], args: &[&str]) -> Output {
        let mut envs = self.base_envs();
        envs.extend_from_slice(extra_envs);
        run_decapod(&self.worktree, &self.home, &envs, args)
    }

    fn run_in_worktree_ok(&self, extra_envs: &[(&str, &str)], args: &[&str]) -> Output {
        let mut envs = self.base_envs();
        envs.extend_from_slice(extra_envs);
        run_decapod_ok(&self.worktree, &self.home, &envs, args)
    }

    fn prepare_clean_commit(&self) {
        fs::write(self.worktree.join("app.txt"), "changed for test\n").expect("update app.txt");

        let specs_dir = self.worktree.join(".decapod").join("managed").join("specs");
        let validation_spec = specs_dir.join("VALIDATION.md");
        let mut content = fs::read_to_string(&validation_spec).expect("read VALIDATION.md");
        content.push_str(
            "\n\n## Issue 1659 Override Isolation Verification\n\n\
             Readiness gate-skip overrides must never authorize full validation, workspace\n\
             publication, or validated task completion.\n",
        );
        fs::write(&validation_spec, content).expect("write VALIDATION.md");

        self.run_in_worktree_ok(&[], &["rpc", "--op", "specs.refresh"]);

        for other in [
            "ARCHITECTURE.md",
            "INTENT.md",
            "INTERFACES.md",
            "OPERATIONS.md",
            "README.md",
            "SECURITY.md",
            "SEMANTICS.md",
        ] {
            let rel = format!(".decapod/managed/specs/{other}");
            if self.worktree.join(&rel).exists() {
                let _ = self.run_in_worktree(
                    &[],
                    &[
                        "govern",
                        "plan",
                        "review-spec",
                        "--path",
                        &rel,
                        "--disposition",
                        "unchanged-with-reason",
                        "--reason",
                        "Unchanged in #1659 override isolation test fixture",
                    ],
                );
            }
        }
        let _ = self.run_in_worktree(
            &[],
            &[
                "govern",
                "plan",
                "review-spec",
                "--path",
                ".decapod/managed/specs/VALIDATION.md",
                "--disposition",
                "updated",
                "--reason",
                "Document #1659 override isolation contract",
            ],
        );
        self.run_in_worktree_ok(&[], &["govern", "plan", "approve"]);

        // Read trajectory run_id from governance.json if initialized by workspace ensure.
        let gov_path = self.worktree.join(".decapod").join("governance.json");
        let gov_raw = fs::read_to_string(&gov_path).expect("read governance.json");
        let gov_json: serde_json::Value =
            serde_json::from_str(&gov_raw).expect("parse governance.json");
        if let Some(run_id) = gov_json["sections"]["trajectory"]["run_id"].as_str() {
            self.run_in_worktree_ok(
                &[],
                &[
                    "govern",
                    "trajectory",
                    "record",
                    "--run-id",
                    run_id,
                    "--inspected-file",
                    "app.txt",
                    "--modified-file",
                    "app.txt",
                    "--modified-file",
                    ".decapod/managed/specs/VALIDATION.md",
                    "--check",
                    "local-check=passed",
                    "--evidence",
                    "app.txt",
                ],
            );
        }

        run_git(&self.worktree, &self.home, &["add", "-A"]);
        self.run_in_worktree_ok(
            &[],
            &[
                "govern",
                "artifacts",
                "checkpoint",
                "--id",
                "test-cp-1",
                "--summary",
                "Test candidate commit",
                "--proof-ref",
                "app.txt",
            ],
        );
        run_git(&self.worktree, &self.home, &["add", "-A"]);
        run_git(
            &self.worktree,
            &self.home,
            &["commit", "-m", "candidate commit"],
        );
    }

    fn head_sha(&self) -> String {
        let out = run_git(&self.worktree, &self.home, &["rev-parse", "HEAD"]);
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn collect_data_files(&self) -> Vec<String> {
        fn walk(root: &Path, base: &Path, out: &mut Vec<String>) {
            let Ok(entries) = fs::read_dir(root) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Ok(rel) = path.strip_prefix(base) {
                        out.push(format!("{}/", rel.to_string_lossy()));
                    }
                    walk(&path, base, out);
                } else if let Ok(rel) = path.strip_prefix(base) {
                    out.push(rel.to_string_lossy().to_string());
                }
            }
        }
        let mut files = Vec::new();
        for dir in [
            self.repo.join(".decapod").join("data"),
            self.worktree.join(".decapod").join("data"),
        ] {
            walk(&dir, &dir, &mut files);
        }
        files
    }
}

fn assert_override_rejected(out: &Output, context: &str) {
    assert!(
        !out.status.success(),
        "expected {context} to fail under override, but exited 0:\n{}",
        combined_output(out)
    );
    let text = combined_output(out);
    assert!(
        text.contains(OVERRIDE_CODE),
        "expected {context} output to contain {OVERRIDE_CODE}, got:\n{text}"
    );
}

#[test]
fn active_overrides_reject_all_readiness_entrypoints() {
    let repo = DisposableRepo::create("active-rejects");
    repo.prepare_clean_commit();
    let head = repo.head_sha();

    // Complete one clean validate and refresh constitutional awareness for `rpc --op workspace.publish`.
    repo.run_in_worktree_ok(&[], &["validate", "--format", "json"]);
    repo.run_in_worktree_ok(&[], &["rpc", "--op", "agent.init"]);
    repo.run_in_worktree_ok(
        &[],
        &[
            "rpc",
            "--op",
            "constitution.get",
            "--params",
            r#"{"section":"core/DECAPOD"}"#,
        ],
    );
    repo.run_in_worktree_ok(&[], &["rpc", "--op", "context.resolve"]);

    let mut truthy_cases: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    truthy_cases.insert(SKIP_GIT, vec!["1", "", "0", "false"]);
    truthy_cases.insert(SKIP_TOOLING, vec!["1", "", "0", "false"]);
    truthy_cases.insert(SKIP_FINGERPRINT, vec!["1", "true", "TRUE", "yes", "YES"]);

    for (var, vals) in &truthy_cases {
        for val in vals {
            let envs = [(*var, *val)];

            let out = repo.run_in_worktree(&envs, &["validate"]);
            assert_override_rejected(&out, &format!("decapod validate with {var}={val:?}"));

            let out = repo.run_in_worktree(&envs, &["v"]);
            assert_override_rejected(&out, &format!("decapod v with {var}={val:?}"));

            let out = repo.run_in_worktree(&envs, &["rpc", "--op", "validate.run"]);
            assert_override_rejected(&out, &format!("rpc validate.run with {var}={val:?}"));

            let out = repo.run_in_worktree(
                &envs,
                &["workspace", "publish", "--title", "t", "--description", "d"],
            );
            assert_override_rejected(&out, &format!("workspace publish with {var}={val:?}"));

            let out = repo.run_in_worktree(
                &envs,
                &[
                    "rpc",
                    "--op",
                    "workspace.publish",
                    "--params",
                    r#"{"title":"t","description":"d"}"#,
                ],
            );
            assert_override_rejected(&out, &format!("rpc workspace.publish with {var}={val:?}"));

            let out = repo.run_in_worktree(
                &envs,
                &["todo", "done", "--id", &repo.todo_id, "--validated"],
            );
            assert_override_rejected(&out, &format!("todo done --validated with {var}={val:?}"));

            let out = repo.run_in_worktree(
                &envs,
                &[
                    "todo",
                    "done",
                    "--id",
                    &repo.todo_id,
                    "--validated",
                    "--commit",
                    &head,
                ],
            );
            assert_override_rejected(
                &out,
                &format!("todo done --validated --commit with {var}={val:?}"),
            );
        }
    }
}

#[test]
fn workspace_publish_and_qa_verify_reject_before_side_effects_or_bypass() {
    let repo = DisposableRepo::create("publish-and-qa-gaps");
    repo.prepare_clean_commit();

    // Unrefreshed spec edit in working tree: workspace publish must fail immediately on the
    // gate-skip override BEFORE running specs.refresh or failing downstream proof checks.
    let marker_spec = repo
        .worktree
        .join(".decapod")
        .join("managed")
        .join("specs")
        .join("VALIDATION.md");
    let before_spec = fs::read_to_string(&marker_spec).expect("read VALIDATION.md");
    let unrefreshed_spec = format!("{before_spec}\n<!-- unrefreshed-marker -->\n");
    fs::write(&marker_spec, &unrefreshed_spec).expect("dirty VALIDATION.md");

    // Test both validation_buildkite_preflights = false (default here) and = true.
    let out_native = repo.run_in_worktree(
        &[(SKIP_GIT, "1")],
        &["workspace", "publish", "--title", "t", "--description", "d"],
    );
    assert_override_rejected(
        &out_native,
        "workspace publish (buildkite_preflights=false) with DECAPOD_VALIDATE_SKIP_GIT_GATES=1",
    );
    let after_spec = fs::read_to_string(&marker_spec).expect("re-read VALIDATION.md");
    assert_eq!(
        unrefreshed_spec, after_spec,
        "workspace publish must reject override before mutating specs"
    );

    let cfg_path = repo.worktree.join(".decapod").join("config.toml");
    let orig_cfg = fs::read_to_string(&cfg_path).expect("read config.toml");
    let bk_cfg = orig_cfg.replace(
        "validation_buildkite_preflights = false",
        "validation_buildkite_preflights = true",
    );
    fs::write(&cfg_path, bk_cfg).expect("enable buildkite_preflights in config.toml");
    let out_bk = repo.run_in_worktree(
        &[(SKIP_TOOLING, "1")],
        &["workspace", "publish", "--title", "t", "--description", "d"],
    );
    assert_override_rejected(
        &out_bk,
        "workspace publish (buildkite_preflights=true) with DECAPOD_VALIDATE_SKIP_TOOLING_GATES=1",
    );
    fs::write(&cfg_path, orig_cfg).expect("restore config.toml");
    fs::write(&marker_spec, &before_spec).expect("restore VALIDATION.md");
}

#[test]
fn qa_verify_rejects_overrides_even_under_ci_env() {
    let repo = DisposableRepo::create("qa-verify-gaps");
    repo.prepare_clean_commit();

    // 1) Complete `repo.todo_id` cleanly with `--validated --artifact app.txt` FIRST (while no
    //    unverified done todo exists) so it carries `verification_artifacts` and `qa verify todo <id>`
    //    reaches `run_validate_and_hash`.
    repo.run_in_worktree_ok(&[], &["validate", "--format", "json"]);
    repo.run_in_worktree_ok(
        &[],
        &[
            "todo",
            "done",
            "--id",
            &repo.todo_id,
            "--validated",
            "--artifact",
            "app.txt",
        ],
    );

    // 2) Add an unverified `done` task so `qa verify` and `qa verify recover` both have an
    //    unverified `done` target and enter `run_validate_and_hash`.
    let unverified_out = repo.run_in_worktree_ok(
        &[],
        &["todo", "add", "Unverified done todo", "--format", "json"],
    );
    let unverified_json: serde_json::Value =
        serde_json::from_slice(&unverified_out.stdout).expect("parse unverified todo add json");
    let unverified_id = unverified_json["id"]
        .as_str()
        .expect("unverified todo id")
        .to_string();
    repo.run_in_worktree_ok(&[], &["todo", "done", "--id", &unverified_id]);

    let recover_arg = format!("{unverified_id}=app.txt");
    // qa verify surfaces must reject skip overrides even when GITHUB_ACTIONS=true would otherwise
    // push nested `decapod validate --format json` into CiLocal scope.
    for qa_args in [
        vec!["qa", "verify"],
        vec!["qa", "verify", "todo", &repo.todo_id],
        vec!["qa", "verify", "recover", "--artifact", &recover_arg],
    ] {
        let out_ci = repo.run_in_worktree(&[(SKIP_GIT, "1"), ("GITHUB_ACTIONS", "true")], &qa_args);
        assert_override_rejected(
            &out_ci,
            &format!("{qa_args:?} with {SKIP_GIT}=1 + GITHUB_ACTIONS=true"),
        );

        let out = repo.run_in_worktree(&[(SKIP_GIT, "1")], &qa_args);
        assert_override_rejected(&out, &format!("{qa_args:?} with {SKIP_GIT}=1"));
    }
}

#[test]
fn ci_local_and_project_check_preserve_partial_scope_and_receipt_is_non_reusable() {
    let repo = DisposableRepo::create("ci-local-and-receipt");
    repo.prepare_clean_commit();
    let head = repo.head_sha();

    // Create a second claimed todo so both historical `--commit` and live `--validated` can be
    // proved cleanly after partial checks fail to establish readiness.
    let live_todo_id = DisposableRepo::add_and_claim_todo(
        &repo.worktree,
        &repo.home,
        &repo.session_password,
        "Live validated completion check",
    );

    // 1. CI-local diagnostics (with and without override) preserve partial scope and publication_ready=false.
    for (ci_env, extra) in [
        (("GITHUB_ACTIONS", "true"), Some((SKIP_GIT, "1"))),
        (("BUILDKITE", "true"), Some((SKIP_TOOLING, "1"))),
        (("GITHUB_ACTIONS", "true"), None),
        (("BUILDKITE", "true"), None),
    ] {
        let mut envs = vec![ci_env];
        if let Some(ov) = extra {
            envs.push(ov);
        }
        let out = repo.run_in_worktree(&envs, &["validate", "--format", "json"]);
        let text = combined_output(&out);
        let json: serde_json::Value = serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("expected JSON from ci-local validate: {e}\n{text}"));
        assert_eq!(
            json["publication_ready"],
            serde_json::Value::Bool(false),
            "ci-local must set publication_ready=false: {text}"
        );
        assert_eq!(
            json["scope"],
            serde_json::Value::String("ci_local".to_string()),
            "ci-local must preserve scope=ci_local: {text}"
        );
    }

    // 2. govern project-check (with and without override) preserves partial scope (publication_ready=false).
    for extra in [vec![(SKIP_GIT, "1")], vec![]] {
        let pc_out = repo.run_in_worktree(&extra, &["govern", "project-check", "--format", "json"]);
        let pc_text = combined_output(&pc_out);
        let json: serde_json::Value = serde_json::from_slice(&pc_out.stdout)
            .unwrap_or_else(|e| panic!("expected JSON from project-check: {e}\n{pc_text}"));
        assert_eq!(
            json["publication_ready"],
            serde_json::Value::Bool(false),
            "govern project-check must set publication_ready=false: {pc_text}"
        );
        assert_eq!(
            json["scope"],
            serde_json::Value::String("project_prerequisites".to_string()),
            "govern project-check must preserve scope=project_prerequisites: {pc_text}"
        );
    }

    // Confirm govern project-check recorded a partial validation receipt in governance.json, then
    // commit the governance update cleanly so subsequent checks test non-reusability of that stored receipt.
    let gov_raw = fs::read_to_string(repo.worktree.join(".decapod").join("governance.json"))
        .expect("read governance.json after project-check");
    let gov_json: serde_json::Value =
        serde_json::from_str(&gov_raw).expect("parse governance.json after project-check");
    assert!(
        !gov_json["sections"]["validation"].is_null(),
        "expected govern project-check to persist sections.validation receipt"
    );
    repo.run_in_worktree_ok(
        &[],
        &[
            "govern",
            "artifacts",
            "checkpoint",
            "--id",
            "test-cp-project-check",
            "--summary",
            "Commit project-check receipt without full readiness",
            "--proof-ref",
            "app.txt",
        ],
    );
    run_git(
        &repo.worktree,
        &repo.home,
        &["add", ".decapod/governance.json"],
    );
    run_git(
        &repo.worktree,
        &repo.home,
        &["commit", "-m", "record project-check receipt"],
    );
    let receipt_head = repo.head_sha();

    // Confirm neither ci-local nor project-check produced full native completion artifacts yet.
    let pre_files = repo.collect_data_files();
    assert!(
        !pre_files
            .iter()
            .any(|f| f.ends_with("historical-proof.json") || f.ends_with("observations.json")),
        "partial/ci-local runs must not produce full validation history artifacts: {pre_files:?}"
    );

    // 3. Removing override after partial/project-check receipt write must NOT allow cached reuse.
    for commit_ref in [&head, &receipt_head] {
        let verify_commit = repo.run_in_worktree(&[], &["qa", "verify", "commit", commit_ref]);
        assert!(
            !verify_commit.status.success(),
            "qa verify commit ({commit_ref}) must fail without prior full validation proof:\n{}",
            combined_output(&verify_commit)
        );

        let done_hist = repo.run_in_worktree(
            &[],
            &[
                "todo",
                "done",
                "--id",
                &repo.todo_id,
                "--validated",
                "--commit",
                commit_ref,
            ],
        );
        assert!(
            !done_hist.status.success(),
            "todo done --validated --commit ({commit_ref}) must fail when only partial/project-check ran:\n{}",
            combined_output(&done_hist)
        );
    }

    let ws_pub_no_proof = repo.run_in_worktree(
        &[],
        &["workspace", "publish", "--title", "t", "--description", "d"],
    );
    assert!(
        !ws_pub_no_proof.status.success(),
        "workspace publish must not reuse project-check receipt as full readiness:\n{}",
        combined_output(&ws_pub_no_proof)
    );

    // 4. Clean full validate without overrides and with falsy fingerprint override values succeeds.
    let clean_empty = repo.run_in_worktree_ok(&[], &["validate", "--format", "json"]);
    let clean_empty_json: serde_json::Value =
        serde_json::from_slice(&clean_empty.stdout).expect("parse clean validate json");
    assert_eq!(
        clean_empty_json["status"],
        serde_json::Value::String("ok".to_string()),
        "clean full validate must succeed:\n{}",
        combined_output(&clean_empty)
    );
    assert_eq!(
        clean_empty_json["readiness"]["ready"],
        serde_json::Value::Bool(true),
        "clean full validate readiness.ready must be true:\n{}",
        combined_output(&clean_empty)
    );
    assert_eq!(
        clean_empty_json["readiness"]["proof_ready"],
        serde_json::Value::Bool(true),
        "clean full validate readiness.proof_ready must be true:\n{}",
        combined_output(&clean_empty)
    );

    for falsy in ["", "0"] {
        let clean_out = repo.run_in_worktree_ok(
            &[(SKIP_FINGERPRINT, falsy)],
            &["validate", "--format", "json"],
        );
        let clean_text = combined_output(&clean_out);
        let clean_json: serde_json::Value =
            serde_json::from_slice(&clean_out.stdout).expect("parse clean validate json");
        assert_eq!(
            clean_json["status"],
            serde_json::Value::String("ok".to_string()),
            "clean full validate with {SKIP_FINGERPRINT}={falsy:?} must succeed:\n{clean_text}",
        );
        assert_eq!(
            clean_json["readiness"]["proof_ready"],
            serde_json::Value::Bool(true),
            "clean full validate with {SKIP_FINGERPRINT}={falsy:?} must have proof_ready=true:\n{clean_text}",
        );
    }

    // Confirm full native validation produced observations.json, completion/, and historical-proof.json
    // in the returned runtime evidence directory.
    let evidence_dir = clean_empty_json["readiness"]["evidence_path"]
        .as_str()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            panic!(
                "expected readiness.evidence_path in clean validate output:\n{}",
                combined_output(&clean_empty)
            )
        });
    assert!(
        evidence_dir.join("observations.json").is_file(),
        "expected observations.json in {}, entries: {:?}",
        evidence_dir.display(),
        fs::read_dir(&evidence_dir)
            .ok()
            .map(|d| d.flatten().map(|e| e.file_name()).collect::<Vec<_>>())
    );
    assert!(
        evidence_dir.join("completion").is_dir(),
        "expected completion/ directory in {}",
        evidence_dir.display()
    );
    assert!(
        evidence_dir.join("historical-proof.json").is_file(),
        "expected historical-proof.json in {}",
        evidence_dir.display()
    );

    // 5. Clean live `todo done --validated`, fast-forward integration to origin/master,
    //    `qa verify commit`, and historical `todo done --validated --commit` all pass!
    repo.run_in_worktree_ok(&[], &["todo", "done", "--id", &live_todo_id, "--validated"]);
    run_git(
        &repo.worktree,
        &repo.home,
        &["push", "origin", "HEAD:master"],
    );
    run_git(
        &repo.repo,
        &repo.home,
        &["merge", "--ff-only", &receipt_head],
    );
    run_git(&repo.worktree, &repo.home, &["fetch", "origin"]);

    repo.run_in_worktree_ok(&[], &["qa", "verify", "commit", &receipt_head]);
    repo.run_in_worktree_ok(
        &[],
        &[
            "todo",
            "done",
            "--id",
            &repo.todo_id,
            "--validated",
            "--commit",
            &receipt_head,
        ],
    );
}

#[test]
fn broker_handoff_isolates_override_tainted_leader_and_rejects_tainted_client() {
    let repo = DisposableRepo::create("broker-isolation");
    repo.prepare_clean_commit();
    let head = repo.head_sha();

    // Establish clean full validation proof (also proving falsy SKIP_FINGERPRINT="false" is accepted).
    let clean_out = repo.run_in_worktree_ok(
        &[(SKIP_FINGERPRINT, "false")],
        &["validate", "--format", "json"],
    );
    let clean_json: serde_json::Value =
        serde_json::from_slice(&clean_out.stdout).expect("parse clean validate json");
    assert_eq!(
        clean_json["readiness"]["proof_ready"],
        serde_json::Value::Bool(true),
        "expected clean validate with SKIP_FINGERPRINT=false to succeed:\n{}",
        combined_output(&clean_out)
    );

    // Integrate validated commit into accepted origin/master so `--commit <head>` historical completion applies.
    run_git(
        &repo.worktree,
        &repo.home,
        &["push", "origin", "HEAD:master"],
    );
    run_git(&repo.repo, &repo.home, &["merge", "--ff-only", &head]);
    run_git(&repo.worktree, &repo.home, &["fetch", "origin"]);

    // Spawn an override-tainted broker leader via a non-readiness todo command with idle timeout
    // so the broker process stays resident with DECAPOD_VALIDATE_SKIP_GIT_GATES=1 in its own env.
    let bin = decapod_bin();
    let leader_envs = [
        ("DECAPOD_SESSION_PASSWORD", repo.session_password.as_str()),
        ("DECAPOD_GROUP_BROKER_IDLE_SECS", "10"),
        (SKIP_GIT, "1"),
    ];
    let mut leader_cmd = sanitized_command(&bin, &repo.worktree, &repo.home, &leader_envs);
    leader_cmd
        .args(["todo", "list", "--format", "json"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut leader: Child = leader_cmd.spawn().expect("spawn broker leader");

    // Wait briefly so the leader binds the group broker socket before client runs.
    thread::sleep(Duration::from_millis(700));

    // An override-tainted client must fail with READINESS_GATE_OVERRIDE_UNSUPPORTED before mutating todo state.
    let tainted_client = repo.run_in_worktree(
        &[(SKIP_GIT, "1")],
        &[
            "todo",
            "done",
            "--id",
            &repo.todo_id,
            "--validated",
            "--commit",
            &head,
        ],
    );
    assert_override_rejected(
        &tainted_client,
        "override-tainted client todo done --validated --commit under broker",
    );

    // Verify todo is still open/claimed (not completed by the rejected override attempt).
    let get_out = repo.run_in_worktree_ok(
        &[],
        &["todo", "get", "--id", &repo.todo_id, "--format", "json"],
    );
    let get_json: serde_json::Value =
        serde_json::from_slice(&get_out.stdout).expect("parse todo get json");
    let status_val = get_json["status"]
        .as_str()
        .or_else(|| get_json["todo"]["status"].as_str())
        .unwrap_or_default();
    assert_ne!(
        status_val,
        "done",
        "rejected override client must not mark todo done: {}",
        combined_output(&get_out)
    );

    // A clean client running against the override-tainted leader must NOT inherit the leader's
    // SKIP_GIT env var and succeeds cleanly!
    let start = Instant::now();
    let clean_client = repo.run_in_worktree_ok(
        &[("DECAPOD_GROUP_BROKER_IDLE_SECS", "10")],
        &[
            "todo",
            "done",
            "--id",
            &repo.todo_id,
            "--validated",
            "--commit",
            &head,
        ],
    );
    assert!(
        start.elapsed() < Duration::from_secs(60),
        "clean broker handoff took unexpectedly long: {:?}",
        start.elapsed()
    );
    assert!(clean_client.status.success());

    let _ = leader.kill();
    let _ = leader.wait();
}
