//! Whole-command regression for a newly created ordinary ref at the accepted tip.
//! Git, the action shell, and both production CLIs are real. Only the installed
//! Ajv/Node toolchain and provider event metadata are controlled fixture doubles.
#![cfg(unix)]

use decapod::core::bounded_process::BoundedCommand;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;
use tempfile::TempDir;

const ACTION: &str = include_str!("../.github/actions/ci-context/action.yml");
const WORKFLOW: &str = include_str!("../.github/workflows/constitution-sync.yml");
const ACCEPTED: &str = "refs/remotes/origin/master";
const ORDINARY_REF: &str = "refs/heads/release-plz-fixture";
const SCHEMA_STEP: &str = "Validate Constitution against Schema";
const CONFIRM_STEP: &str = "Confirm accepted branch remained unchanged";

fn output(command: &mut Command) -> Output {
    command.bounded_output(Duration::from_secs(30)).unwrap()
}
fn passed(result: &Output) {
    assert!(result.status.success(), "{result:?}");
}
fn values(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .map(|line| {
            let (key, value) = line.split_once('=').unwrap();
            (key.to_owned(), value.to_owned())
        })
        .collect()
}
fn step(name: &str) -> &str {
    WORKFLOW
        .split_once(&format!("      - name: {name}\n"))
        .unwrap()
        .1
        .split("      - name:")
        .next()
        .unwrap()
}
fn script(name: &str) -> String {
    step(name)
        .split_once("        run: |\n")
        .unwrap()
        .1
        .lines()
        .map(|line| line.strip_prefix("          ").unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}
fn action_script() -> String {
    let script = ACTION
        .split_once("      run: |\n")
        .unwrap()
        .1
        .lines()
        .map(|line| line.strip_prefix("        ").unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    // Bazel supplies the actual compiled helper in relocated runfiles. Replace
    // only compilation/preparation, never the action's verification or routing.
    let compile = "rustc +1.96.1 --edition=2024 -D warnings \\\n  \"$GITHUB_ACTION_PATH/../../../assets/tools/ci_event_context.rs\" \\\n  -o \"$private_dir/ci-event-context\"";
    assert_eq!(script.matches(compile).count(), 1);
    script.replace(
        compile,
        "cp -- \"$CONTEXT_FIXTURE\" \"$private_dir/ci-event-context\"",
    )
}
fn executable(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

struct Tools {
    _owned: TempDir,
    context: PathBuf,
    profile: PathBuf,
}
impl Tools {
    fn new() -> Self {
        let owned = tempfile::tempdir().unwrap();
        let locate = |packaged: Option<&str>, source: &str, name: &str| {
            if let Some(packaged) = packaged.filter(|p| !p.is_empty()) {
                let mut candidates = vec![PathBuf::from(packaged)];
                for variable in ["RUNFILES_DIR", "TEST_SRCDIR"] {
                    if let Some(root) = std::env::var_os(variable) {
                        candidates.push(
                            PathBuf::from(root)
                                .join("_main")
                                .join(packaged.trim_start_matches("./")),
                        );
                    }
                }
                return candidates
                    .into_iter()
                    .find_map(|path| path.canonicalize().ok())
                    .expect("Bazel must supply its declared real CLI fixture");
            }
            // Cargo's local test route compiles the same dependency-free source.
            let path = owned.path().join(name);
            let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
            let result = Command::new(compiler)
                .args(["--edition=2024", "-D", "warnings"])
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join(source))
                .arg("-o")
                .arg(&path)
                .bounded_output(Duration::from_secs(90))
                .unwrap();
            passed(&result);
            path
        };
        let context = locate(
            option_env!("DECAPOD_CREATED_CONTEXT_TEST_FIXTURE"),
            "assets/tools/ci_event_context.rs",
            "ci-event-context",
        );
        let profile = locate(
            option_env!("DECAPOD_CREATED_PROFILE_TEST_FIXTURE"),
            "assets/tools/source_ci_profile.rs",
            "source-ci-profile",
        );
        Self {
            _owned: owned,
            context,
            profile,
        }
    }
}

struct Fixture<'a> {
    owned: TempDir,
    tools: &'a Tools,
    repo: PathBuf,
    runner: PathBuf,
    base: String,
    head: String,
}
impl<'a> Fixture<'a> {
    fn new(tools: &'a Tools) -> Self {
        let owned = tempfile::tempdir().unwrap();
        let repo = owned.path().join("repo");
        let runner = owned.path().join("runner");
        for dir in [
            &repo,
            &runner,
            &owned.path().join("bin"),
            &owned.path().join("ajv-package"),
        ] {
            fs::create_dir(dir).unwrap();
        }
        fs::create_dir_all(repo.join("assets/schemas")).unwrap();
        fs::create_dir(repo.join(".decapod")).unwrap();
        fs::write(
            repo.join(".decapod/config.toml"),
            "[repo]\nbase_branch = 'master'\n",
        )
        .unwrap();
        fs::write(repo.join("assets/constitution.json"), "{}\n").unwrap();
        fs::write(repo.join("assets/schemas/constitution.schema.json"), "{}\n").unwrap();
        fs::write(owned.path().join("github-env"), "").unwrap();
        executable(
            &owned.path().join("bin/node"),
            "case \"$1\" in\n  --version) printf 'v20.0.0\\n';;\n  -e) printf '%s\\n5.0.0\\n' \"$FIXTURE_PACKAGE\";;\n  *) exit 99;;\nesac",
        );
        fs::write(
            owned.path().join("ajv-package/package.json"),
            "{\"name\":\"ajv-cli\",\"version\":\"5.0.0\"}\n",
        )
        .unwrap();
        executable(
            &owned.path().join("ajv-package/ajv"),
            "if [ \"$1\" = help ]; then printf 'Ajv fixture\\n'; exit 0; fi\nprintf '%s\\n' \"$@\" >> \"$FIXTURE_CALLS\"\nif [ \"${FIXTURE_DIRTY:-false}\" = true ]; then printf 'changed during schema\\n' > assets/constitution.json; fi\nexit \"${FIXTURE_EXIT:-0}\"",
        );
        fs::copy(&tools.profile, runner.join("source-ci-profile")).unwrap();
        let mut fixture = Self {
            owned,
            tools,
            repo,
            runner,
            base: String::new(),
            head: String::new(),
        };
        fixture.git(&["init", "-q", "-b", "master"]);
        fixture.git(&["config", "user.name", "Created branch fixture"]);
        fixture.git(&["config", "user.email", "created-branch@example.invalid"]);
        fixture.git(&["add", "."]);
        fixture.git(&["commit", "-qm", "base"]);
        fixture.base = fixture.git(&["rev-parse", "HEAD"]);
        fixture.git(&["commit", "-qm", "accepted tip", "--allow-empty"]);
        fixture.head = fixture.git(&["rev-parse", "HEAD"]);
        fixture.git(&["update-ref", ACCEPTED, &fixture.head]);
        fixture
    }
    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(program);
        let mut paths = vec![
            self.owned.path().join("ajv-package"),
            self.owned.path().join("bin"),
        ];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        command
            .env_clear()
            .current_dir(&self.repo)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("HOME", self.owned.path())
            .env("CARGO_HOME", self.owned.path().join("cargo-home"))
            .env("LANG", "C")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("RUNNER_TEMP", &self.runner)
            .env("GITHUB_WORKSPACE", &self.repo)
            .env("GITHUB_ACTION_PATH", "/unused-precompiled-action")
            .env("GITHUB_ENV", self.owned.path().join("github-env"))
            .env("GITHUB_RUN_ID", "1234")
            .env("GITHUB_RUN_ATTEMPT", "1")
            .env("GITHUB_JOB", "validate-constitution")
            .env("CONTEXT_FIXTURE", &self.tools.context)
            .env("FIXTURE_PACKAGE", self.owned.path().join("ajv-package"))
            .env("FIXTURE_CALLS", self.owned.path().join("calls"));
        command
    }
    fn git(&self, args: &[&str]) -> String {
        let result = output(
            self.command("git")
                .args([
                    "-c",
                    "commit.gpgsign=false",
                    "-c",
                    "core.hooksPath=/dev/null",
                ])
                .args(args),
        );
        passed(&result);
        String::from_utf8(result.stdout).unwrap().trim().into()
    }
    fn context(
        &self,
        name: &str,
        event: &str,
        reference: &str,
        created: &str,
        before: &str,
        head: &str,
    ) -> (Output, BTreeMap<String, String>) {
        let file = self.owned.path().join(format!("{name}-output"));
        fs::write(&file, "").unwrap();
        let result = output(
            self.command("bash")
                .args(["-c", &action_script()])
                .env("GITHUB_OUTPUT", &file)
                .env("RAW_EVENT", event)
                .env("RAW_REF", reference)
                .env("EVENT_CREATED", created)
                .env("EVENT_BASE", before)
                .env("EVENT_HEAD", head)
                .env("DISPATCH_PRE_PR", "false")
                .env("DISPATCH_BASE", "")
                .env("DISPATCH_HEAD", ""),
        );
        let emitted = fs::read_to_string(file).unwrap();
        if !result.status.success() {
            assert!(emitted.is_empty(), "partial failed context: {emitted}");
        }
        (result, values(&emitted))
    }
    fn creation(&self, name: &str) -> (Output, BTreeMap<String, String>) {
        self.context(
            name,
            "push",
            ORDINARY_REF,
            "true",
            &"0".repeat(40),
            &self.head,
        )
    }
    fn run_schema(
        &self,
        original: &BTreeMap<String, String>,
        rechecked: &BTreeMap<String, String>,
        exit: &str,
        dirty: bool,
    ) -> Output {
        let verified_env =
            values(&fs::read_to_string(self.owned.path().join("github-env")).unwrap());
        output(
            self.command("bash")
                .args(["-c", &script(SCHEMA_STEP)])
                .envs(verified_env)
                .env("NO_CHANGE", &original["no_change"])
                .env("SOURCE_CI_NO_CHANGE", "true")
                .env(
                    "RECHECK_NO_CHANGE",
                    rechecked.get("no_change").map(String::as_str).unwrap_or(""),
                )
                .env("ORIGINAL_CONTEXT", identity(original))
                .env("RECHECK_CONTEXT", identity(rechecked))
                .env("FIXTURE_EXIT", exit)
                .env("FIXTURE_DIRTY", dirty.to_string()),
        )
    }
    fn confirm(
        &self,
        original: &BTreeMap<String, String>,
        rechecked: &BTreeMap<String, String>,
    ) -> Output {
        output(
            self.command("bash")
                .args(["-c", &script(CONFIRM_STEP)])
                .env(
                    "RECHECK_NO_CHANGE",
                    rechecked.get("no_change").map(String::as_str).unwrap_or(""),
                )
                .env("ORIGINAL_CONTEXT", identity(original))
                .env("RECHECK_CONTEXT", identity(rechecked)),
        )
    }
    fn no_receipt(&self) {
        assert!(!self.runner.join("source-ci-evidence").exists());
        assert!(
            !fs::read_to_string(self.owned.path().join("github-env"))
                .unwrap()
                .contains("SOURCE_CI_CUSTODY_ARTIFACT=")
        );
    }
    fn exact_ajv_call(&self) {
        assert_eq!(
            fs::read_to_string(self.owned.path().join("calls")).unwrap(),
            "validate\n-s\nassets/schemas/constitution.schema.json\n-d\nassets/constitution.json\n--strict=false\n"
        );
    }
}
fn identity(context: &BTreeMap<String, String>) -> String {
    ["base", "head", "checkout_tree"]
        .map(|key| context.get(key).map(String::as_str).unwrap_or(""))
        .join(":")
}

#[test]
fn no_change_workflow_checks_the_same_recipe_and_brackets_it_with_verified_context() {
    for name in [
        "Recheck accepted branch before schema validation",
        "Recheck accepted branch after schema validation",
    ] {
        assert!(step(name).contains("uses: ./.github/actions/ci-context"));
        assert!(step(name).contains("steps.context.outputs.no_change == 'true'"));
    }
    assert!(step("Recheck accepted branch after schema validation").contains("if: always()"));
    assert!(step(CONFIRM_STEP).contains("if: always()"));
    assert!(step(SCHEMA_STEP).contains("NO_CHANGE: ${{ steps.context.outputs.no_change }}"));
    assert!(
        step(SCHEMA_STEP)
            .contains("RECHECK_NO_CHANGE: ${{ steps.before-schema.outputs.no_change }}")
    );
    assert!(
        step(CONFIRM_STEP)
            .contains("RECHECK_NO_CHANGE: ${{ steps.after-schema.outputs.no_change }}")
    );
    for (name, recheck) in [
        (SCHEMA_STEP, "before-schema"),
        (CONFIRM_STEP, "after-schema"),
    ] {
        assert!(step(name).contains("ORIGINAL_CONTEXT: ${{ steps.context.outputs.base }}:${{ steps.context.outputs.head }}:${{ steps.context.outputs.checkout_tree }}"));
        assert!(step(name).contains(&format!(
            "RECHECK_CONTEXT: ${{{{ steps.{recheck}.outputs.base }}}}:${{{{ steps.{recheck}.outputs.head }}}}:${{{{ steps.{recheck}.outputs.checkout_tree }}}}"
        )));
    }
    assert!(ACTION.contains("value: ${{ steps.context.outputs.no_change }}"));
    assert!(!WORKFLOW.contains("continue-on-error"));
    let before = WORKFLOW
        .find("- name: Recheck accepted branch before")
        .unwrap();
    let check = WORKFLOW.find(&format!("- name: {SCHEMA_STEP}")).unwrap();
    let after = WORKFLOW
        .find("- name: Recheck accepted branch after")
        .unwrap();
    assert!(before < check && check < after);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn real_created_branch_action_and_schema_commands_preserve_execution_and_custody_boundaries() {
    let tools = Tools::new();
    for (exit, dirty) in [("0", false), ("17", false), ("0", true)] {
        let f = Fixture::new(&tools);
        let (initial, original) = f.creation("initial");
        passed(&initial);
        assert_eq!(original["no_change"], "true");
        assert_eq!(original["pr_like"], "false");
        let (before, rechecked) = f.creation("before");
        passed(&before);
        let result = f.run_schema(&original, &rechecked, exit, dirty);
        assert_eq!(result.status.success(), exit == "0", "{result:?}");
        let result_json = String::from_utf8_lossy(&result.stdout);
        assert!(result_json.contains("\"schema\":\"decapod.source-ci.v1\""));
        assert!(!result_json.contains("decapod.source-ci.execution"));
        if exit != "0" {
            assert!(result_json.contains("\"status\":\"failed\""));
            assert!(result_json.contains("\"exit_code\":17"));
        }
        f.exact_ajv_call();
        let (after, final_context) = f.creation("after");
        if dirty {
            assert!(!after.status.success());
            assert!(!f.confirm(&original, &final_context).status.success());
        } else {
            passed(&after);
            passed(&f.confirm(&original, &final_context));
        }
        f.no_receipt();
    }
    // Rechecks must not silently accept a ref that moved backward into a real
    // range, nor an altered HEAD/index, before or after schema execution.
    for phase in ["before", "after"] {
        for mutation in ["accepted-ref", "head", "dirty", "hidden-index"] {
            let f = Fixture::new(&tools);
            let (initial, original) = f.creation("initial");
            passed(&initial);
            if phase == "after" {
                passed(&f.run_schema(&original, &original, "0", false));
            }
            match mutation {
                "accepted-ref" => {
                    f.git(&["update-ref", ACCEPTED, &f.base]);
                }
                "head" => {
                    f.git(&["checkout", "-q", "--detach", &f.base]);
                }
                "dirty" => fs::write(f.repo.join("assets/constitution.json"), "changed\n").unwrap(),
                "hidden-index" => {
                    f.git(&[
                        "update-index",
                        "--assume-unchanged",
                        "assets/constitution.json",
                    ]);
                    fs::write(f.repo.join("assets/constitution.json"), "hidden\n").unwrap();
                }
                _ => unreachable!(),
            }
            let (recheck, context) = f.creation("recheck");
            if mutation == "accepted-ref" {
                passed(&recheck);
                assert_eq!(context["no_change"], "false");
            } else {
                assert!(!recheck.status.success());
            }
            let result = if phase == "before" {
                f.run_schema(&original, &context, "0", false)
            } else {
                f.confirm(&original, &context)
            };
            assert!(
                !result.status.success(),
                "accepted {phase}/{mutation}: {result:?}"
            );
            if phase == "before" {
                assert!(!f.owned.path().join("calls").exists());
            }
            f.no_receipt();
        }
    }
    // Missing/malformed dispositions fail closed rather than skipping both
    // routes. Distinct source ranges cannot reuse an earlier no-change marker.
    for disposition in ["", "TRUE", "true\nfalse"] {
        let f = Fixture::new(&tools);
        let (_, mut context) = f.creation("initial");
        context.insert("no_change".into(), disposition.into());
        assert!(
            !f.run_schema(&context, &BTreeMap::new(), "0", false)
                .status
                .success()
        );
        assert!(!f.owned.path().join("calls").exists());
        f.no_receipt();
    }
    // Ordinary PR/push/workbranch B==H still fail before publishing any context.
    for (event, reference, created, before) in [
        ("pull_request", "refs/pull/1/merge", "false", "same"),
        ("push", ORDINARY_REF, "false", "same"),
        ("push", "refs/heads/agent/task/candidate", "true", "zero"),
    ] {
        let f = Fixture::new(&tools);
        let before = if before == "same" {
            f.head.clone()
        } else {
            "0".repeat(40)
        };
        let (result, _) = f.context("equal", event, reference, created, &before, &f.head);
        assert!(!result.status.success(), "{event}/{reference}: {result:?}");
        f.no_receipt();
    }
    // All real candidate routes retain measured preparation and execution,
    // even if a caller attempts to inject a no-change environment hint.
    for (event, reference, created) in [
        ("pull_request", "refs/pull/1/merge", "false"),
        ("push", ORDINARY_REF, "false"),
        ("push", "refs/heads/agent/task/candidate", "true"),
        ("push", ORDINARY_REF, "true"),
    ] {
        let f = Fixture::new(&tools);
        fs::write(f.repo.join("candidate-source"), "new source\n").unwrap();
        f.git(&["add", "."]);
        f.git(&["commit", "-qm", "authored candidate"]);
        let head = f.git(&["rev-parse", "HEAD"]);
        let before = if created == "true" {
            "0".repeat(40)
        } else {
            f.head.clone()
        };
        let (result, context) = f.context("candidate", event, reference, created, &before, &head);
        passed(&result);
        assert_eq!(context["no_change"], "false");
        let mut forged = context.clone();
        forged.insert("no_change".into(), "true".into());
        assert!(!f.run_schema(&forged, &context, "0", false).status.success());
        f.no_receipt();
        passed(&f.run_schema(&context, &BTreeMap::new(), "0", false));
        f.exact_ajv_call();
        let evidence = f.runner.join("source-ci-evidence/constitution-schema");
        assert!(evidence.join("preparation/preparation.json").is_file());
        let execution = fs::read_to_string(evidence.join("execution/execution.json")).unwrap();
        assert!(execution.contains("\"status\":\"passed\""), "{execution}");
        assert!(execution.contains("\"complete_pr_profile\":false"));
    }
    // The explicit measured CLI has no no-change override or equal-endpoint
    // receipt path; reject it independently of the workflow branch selection.
    let f = Fixture::new(&tools);
    let (initial, _) = f.creation("initial");
    passed(&initial);
    let verified = values(&fs::read_to_string(f.owned.path().join("github-env")).unwrap());
    let result = output(
        f.command(&tools.profile)
            .args(["run-ci", "--check", "constitution-schema"])
            .envs(verified)
            .env("SOURCE_CI_NO_CHANGE", "true"),
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("distinct immutable B/H"));
    assert!(!f.owned.path().join("calls").exists());
    assert!(
        !f.runner
            .join("source-ci-evidence/constitution-schema/execution")
            .exists()
    );
    let tree = f.git(&["rev-parse", "HEAD^{tree}"]);
    let digest = format!("sha256:{}", "0".repeat(64));
    for mode in ["prepare-evidence", "run-evidence"] {
        let evidence = f.runner.join(mode);
        let result = output(
            f.command(&tools.profile)
                .args([
                    mode,
                    "--check",
                    "constitution-schema",
                    "--base",
                    &f.head,
                    "--head",
                    &f.head,
                    "--snapshot",
                    &f.head,
                    "--validation-head",
                    &f.head,
                    "--tree",
                    &tree,
                    "--provider",
                    "github",
                    "--run-id",
                    "1234",
                    "--run-attempt",
                    "1",
                    "--job-id",
                    "validate-constitution",
                    "--expected-profile-sha256",
                    &digest,
                    "--expected-invocation-sha256",
                    &digest,
                    "--evidence-dir",
                    evidence.to_str().unwrap(),
                ])
                .env("SOURCE_CI_NO_CHANGE", "true"),
        );
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("distinct immutable B/H"));
        assert!(!evidence.exists());
    }
}
