//! CI storage isolation must preserve the source checkout and exact Git proof.
#[allow(dead_code)]
#[path = "../assets/tools/source_ci_profile.rs"]
mod source_ci_profile;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const ACTION: &str = include_str!("../.github/actions/validation-checkout/action.yml");
const CI: &str = include_str!("../.github/workflows/ci.yml");
const VALIDATE: &str = include_str!("../.github/workflows/decapod-validate.yml");
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "decapod-ci-checkout-{}-{} with spaces",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {:?}", out);
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

fn script() -> String {
    ACTION
        .split_once("      run: |\n")
        .unwrap()
        .1
        .lines()
        .map(|line| line.strip_prefix("        ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(unix)]
#[test]
fn private_clone_preserves_exact_commit_and_does_not_copy_or_chmod_existing_state() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let root = &fixture.0;
    git(root, &["init", "-q", "--initial-branch=master"]);
    git(
        root,
        &[
            "remote",
            "add",
            "origin",
            "https://example.invalid/repository.git",
        ],
    );
    fs::create_dir_all(root.join(".decapod/managed/specs")).unwrap();
    fs::write(root.join(".decapod/config.toml"), "[repo]\n").unwrap();
    fs::write(
        root.join(".decapod/managed/specs/INTENT.md"),
        "Authored proof\n",
    )
    .unwrap();
    fs::write(root.join(".gitignore"), ".decapod/data/\n").unwrap();
    git(root, &["add", "."]);
    git(
        root,
        &[
            "-c",
            "user.name=CI Fixture",
            "-c",
            "user.email=ci@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    fs::create_dir_all(root.join(".decapod/data")).unwrap();
    fs::write(
        root.join(".decapod/data/local-state"),
        "untouched runtime state",
    )
    .unwrap();
    fs::set_permissions(root.join(".decapod"), fs::Permissions::from_mode(0o777)).unwrap();
    let head = git(root, &["rev-parse", "HEAD"]);
    let tree = git(root, &["rev-parse", "HEAD^{tree}"]);
    let output_file = root.join("action-output");
    let out = Command::new("bash")
        .args(["-c", &format!("umask 000; {}", script())])
        .current_dir(root)
        .env("GITHUB_OUTPUT", &output_file)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let output = fs::read_to_string(output_file).unwrap();
    let path = output.trim().strip_prefix("path=").unwrap();
    assert!(path.starts_with(".ci-validation."));
    assert!(!path.contains('/'));
    let clone = root.join(path);
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&clone, &["rev-parse", "HEAD^{tree}"]), tree);
    assert_eq!(
        git(&clone, &["remote", "get-url", "origin"]),
        "https://example.invalid/repository.git"
    );
    for relative in [
        "",
        ".git",
        ".decapod",
        ".decapod/managed",
        ".decapod/managed/specs",
    ] {
        assert_eq!(
            fs::metadata(clone.join(relative))
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0,
            "{relative}"
        );
    }
    assert!(!clone.join(".decapod/data").exists());
    assert_eq!(
        fs::read_to_string(root.join(".decapod/data/local-state")).unwrap(),
        "untouched runtime state"
    );
    assert_eq!(
        fs::metadata(root.join(".decapod"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o777
    );
    assert!(!clone.join(".git/objects/info/alternates").exists());
}

#[test]
fn action_does_not_relax_storage_policy_or_reuse_existing_state() {
    assert!(ACTION.contains("umask 077"));
    assert!(ACTION.contains("mktemp -d"));
    assert!(ACTION.contains("--no-local --no-hardlinks"));
    assert!(
        !script()
            .lines()
            .any(|line| line.trim_start().starts_with("chmod "))
    );
    assert!(!ACTION.contains("DECAPOD_STORAGE_SHARED_GROUP"));
    assert!(!ACTION.contains("DECAPOD_VALIDATE_SKIP"));
}

#[test]
fn all_direct_governance_steps_use_private_checkout() {
    for (workflow, id) in [
        (CI, "health"),
        (CI, "governance-artifacts"),
        (VALIDATE, "standalone-validation"),
    ] {
        assert!(workflow.contains(&format!("run-ci --check {id}")));
        let check = source_ci_profile::find_check(id).unwrap();
        let script = source_ci_profile::workflow_owned::script_sources(check);
        assert!(
            script
                .iter()
                .any(|(path, _)| path.ends_with("private-checkout.sh"))
        );
        let primary = script
            .iter()
            .find(|(path, _)| !path.ends_with("private-checkout.sh"))
            .unwrap();
        let body = std::str::from_utf8(primary.1).unwrap();
        assert!(body.contains("source_private_checkout"));
        assert!(body.contains("cd \"$SOURCE_PRIVATE_CHECKOUT\""));
    }
    assert!(!CI.contains("path: ${{ steps.validation-checkout.outputs.path }}"));
}

#[test]
fn source_ci_requests_only_read_access_to_repository_contents() {
    let permissions = CI.split_once("permissions:\n").unwrap().1;
    let permissions = permissions.split_once("\n\n").unwrap().0;
    assert!(permissions.contains("  contents: read"));
    assert!(!permissions.contains("contents: write"));
    assert!(!CI.contains("git push"));
    assert!(!CI.contains("gh release"));
}

// The shared helper owns Supabase/Docker process setup; executable helper
// regressions live in source_ci_profile_policy. This remaining shell preamble
// belongs to the already-shared integration artifact runner.
const PRIVATE_TEST_STEPS: [(&str, &str, &str); 1] = [(
    "test-integration",
    "Run selective tests",
    "\"$RUNNER_TEMP/integration-tests/integration-shards\" run",
)];

fn test_step_preamble(job: &str, step: &str, command: &str) -> String {
    assert_eq!((job, step, command), PRIVATE_TEST_STEPS[0]);
    let script = include_str!("../assets/tools/source_ci/integration-worker.sh");
    assert!(script.contains("set -euo pipefail\numask 077"));
    "set -euo pipefail\numask 077".into()
}

#[test]
fn only_fresh_ci_test_processes_set_private_creation_permissions() {
    for (job, step, command) in PRIVATE_TEST_STEPS {
        let expected = if job == "supabase-consumer" {
            "set -euo pipefail\numask 077\nunset GIT_CONFIG GIT_CONFIG_PARAMETERS\nexport GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_COUNT=0"
        } else {
            "set -euo pipefail\numask 077"
        };
        assert_eq!(
            test_step_preamble(job, step, command),
            expected,
            "{job} must set creation permissions before spawning test processes"
        );
    }
    // Do not replace this scoped boundary with an inherited storage-policy
    // opt-out, a global workflow default, or changes to existing installations.
    assert!(!CI.contains("DECAPOD_STORAGE_SHARED_GROUP"));
    assert!(!CI.contains("\ndefaults:\n"));
}

#[test]
fn offline_git_fixture_isolation_preserves_local_config_and_provider_settings() {
    let fixture = Fixture::new();
    let root = &fixture.0;
    git(root, &["init", "-q"]);
    git(root, &["config", "--local", "user.name", "Fixture owner"]);
    let authored = "git@github.com:DecapodLabs/decapod.git";
    git(root, &["remote", "add", "origin", authored]);
    let provider_config = root.join("provider-gitconfig");
    let provider_body = "[url \"https://provider.invalid/\"]\n\tinsteadOf = git@github.com:\n[user]\n\tname = Provider owner\n";
    fs::write(&provider_config, provider_body).unwrap();
    let local_before = fs::read(root.join(".git/config")).unwrap();
    let check = source_ci_profile::find_check("supabase-consumer").unwrap();
    let recipe = source_ci_profile::commands(check, &[]).unwrap();
    assert!(CI.contains("run-ci --check supabase-consumer"));
    // Each inherited source can independently poison fixtures. No process-wide
    // environment mutations or real user/provider configuration are needed.
    for layer in ["global", "system", "count", "parameters", "config"] {
        let configure = |command: &mut Command| {
            command
                .current_dir(root)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_COUNT", "0")
                .env_remove("GIT_CONFIG")
                .env_remove("GIT_CONFIG_PARAMETERS");
            match layer {
                "global" => {
                    command.env("GIT_CONFIG_GLOBAL", &provider_config);
                }
                "system" => {
                    command
                        .env("GIT_CONFIG_NOSYSTEM", "0")
                        .env("GIT_CONFIG_SYSTEM", &provider_config);
                }
                "count" => {
                    command
                        .env("GIT_CONFIG_COUNT", "1")
                        .env(
                            "GIT_CONFIG_KEY_0",
                            "url.https://provider.invalid/.insteadOf",
                        )
                        .env("GIT_CONFIG_VALUE_0", "git@github.com:");
                }
                "parameters" => {
                    command.env(
                        "GIT_CONFIG_PARAMETERS",
                        "'url.https://provider.invalid/.insteadOf=git@github.com:'",
                    );
                }
                "config" => {
                    command.env("GIT_CONFIG", &provider_config);
                }
                _ => unreachable!(),
            }
        };
        let baseline = || {
            let mut command = Command::new("git");
            configure(&mut command);
            if layer == "config" {
                command.args(["config", "--get", "user.name"]);
            } else {
                command.args(["remote", "get-url", "origin"]);
            }
            let output = command.output().unwrap();
            assert!(output.status.success(), "{layer}: {output:?}");
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        };
        let expected_poison = if layer == "config" {
            "Provider owner"
        } else {
            "https://provider.invalid/DecapodLabs/decapod.git"
        };
        assert_eq!(
            baseline(),
            expected_poison,
            "{layer} baseline must detect the contamination"
        );
        let mut command = Command::new("bash");
        configure(&mut command);
        // Apply the exact typed environment that actual CI/native execution
        // consumes, not a copied shell preamble.
        for (key, value) in &recipe[0].env {
            if let Some(value) = value {
                command.env(key, value);
            } else {
                command.env_remove(key);
            }
        }
        command.args([
            "-c",
            "git remote get-url origin\ngit config --get user.name",
        ]);
        let output = command.output().unwrap();
        assert!(output.status.success(), "{layer}: {output:?}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("{authored}\nFixture owner\n"),
            "{layer}"
        );
        assert_eq!(fs::read_to_string(&provider_config).unwrap(), provider_body);
        assert_eq!(fs::read(root.join(".git/config")).unwrap(), local_before);
        assert_eq!(
            baseline(),
            expected_poison,
            "{layer}: isolation leaked outside the child process"
        );
    }
    assert!(!CI.contains("GIT_CONFIG_GLOBAL=/dev/null"));
    assert!(!CI.contains("unset GIT_CONFIG GIT_CONFIG_PARAMETERS"));
    assert!(recipe.iter().all(|command| {
        command
            .env
            .contains(&("GIT_CONFIG_GLOBAL", Some("/dev/null")))
    }));
}

#[test]
fn private_creation_child_fixture() {
    let Some(root) = std::env::var_os("DECAPOD_CI_CREATION_FIXTURE") else {
        return;
    };
    let root = PathBuf::from(root);
    fs::create_dir_all(root.join("new/state/data")).unwrap();
    fs::write(root.join("new/state/data/file"), "fresh private state").unwrap();
}

#[cfg(unix)]
#[test]
fn test_step_umask_is_inherited_by_children_without_changing_shared_state() {
    use std::os::unix::fs::PermissionsExt;
    for (job, step, command) in PRIVATE_TEST_STEPS {
        let fixture = Fixture::new();
        let root = &fixture.0;
        fs::create_dir(root.join("existing-shared")).unwrap();
        fs::write(root.join("existing-shared/file"), "unchanged shared state").unwrap();
        fs::set_permissions(
            root.join("existing-shared"),
            fs::Permissions::from_mode(0o777),
        )
        .unwrap();
        fs::set_permissions(
            root.join("existing-shared/file"),
            fs::Permissions::from_mode(0o666),
        )
        .unwrap();
        // Change only the new shell's mask, never the parallel Rust test
        // process. The actual CI preamble must reach a spawned Rust process.
        let output = Command::new("bash")
            .args([
                "-c",
                &format!(
                    "umask 000\n(\n{}\n\"$1\" --exact private_creation_child_fixture --nocapture\n)\ntest \"$(umask)\" = 0000",
                    test_step_preamble(job, step, command)
                ),
                "ci-private-creation",
            ])
            .arg(std::env::current_exe().unwrap())
            .env("DECAPOD_CI_CREATION_FIXTURE", root)
            .output()
            .unwrap();
        assert!(output.status.success(), "{job}: {output:?}");
        for relative in ["new", "new/state", "new/state/data"] {
            assert_eq!(
                fs::metadata(root.join(relative))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700,
                "{job}: {relative}"
            );
        }
        assert_eq!(
            fs::metadata(root.join("new/state/data/file"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600,
            "{job}"
        );
        assert_eq!(
            fs::metadata(root.join("existing-shared"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o777,
            "{job}"
        );
        assert_eq!(
            fs::metadata(root.join("existing-shared/file"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o666,
            "{job}"
        );
        assert_eq!(
            fs::read_to_string(root.join("existing-shared/file")).unwrap(),
            "unchanged shared state",
            "{job}"
        );
    }
}

#[test]
fn actual_validation_receives_complete_immutable_release_evidence() {
    let context_action = include_str!("../.github/actions/ci-context/action.yml");
    for (workflow, recipe) in [
        (CI, include_str!("../assets/tools/source_ci/health.sh")),
        (
            VALIDATE,
            include_str!("../assets/tools/source_ci/standalone-validation.sh"),
        ),
    ] {
        assert!(workflow.contains("uses: ./.github/actions/ci-context"));
        assert!(workflow.contains("fetch-depth: 0"));
        for field in ["EVENT", "BASE", "HEAD"] {
            assert!(context_action.contains(&format!("SOURCE_CI_{field}=%s")));
            assert!(recipe.contains(&format!("export CHANGE_{field}=\"${{SOURCE_CI_{field}")));
        }
        assert!(recipe.contains("decapod govern project-check --refresh-specs"));
        assert!(!recipe.contains("decapod validate --refresh-specs"));
    }
}

#[test]
fn private_clone_preserves_remote_only_base_for_detached_ci_checkout() {
    let fixture = Fixture::new();
    let source = fixture.0.join("source");
    fs::create_dir(&source).unwrap();
    git(&source, &["init", "-q", "--initial-branch=master"]);
    git(&source, &["config", "user.name", "Detached CI fixture"]);
    git(&source, &["config", "user.email", "ci@example.invalid"]);
    git(
        &source,
        &[
            "remote",
            "add",
            "origin",
            "https://example.invalid/repository.git",
        ],
    );
    fs::write(source.join("source.txt"), "accepted base\n").unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-qm", "accepted base"]);
    let base = git(&source, &["rev-parse", "HEAD"]);
    git(
        &source,
        &["update-ref", "refs/remotes/origin/master", &base],
    );
    git(
        &source,
        &["update-ref", "refs/remotes/origin/accepted-base", &base],
    );
    fs::write(source.join("source.txt"), "candidate tree\n").unwrap();
    git(&source, &["commit", "-qam", "candidate"]);
    let head = git(&source, &["rev-parse", "HEAD"]);
    git(&source, &["checkout", "-q", "--detach", &head]);
    git(&source, &["branch", "-D", "master"]);
    assert!(
        git(
            &source,
            &["for-each-ref", "--format=%(refname)", "refs/heads/"]
        )
        .is_empty()
    );
    let before = git(&source, &["show-ref"]);
    let output = fixture.0.join("output");
    let result = Command::new("bash")
        .args(["-c", &script()])
        .current_dir(&source)
        .env("GITHUB_OUTPUT", &output)
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
    let path = fs::read_to_string(output).unwrap();
    let clone = source.join(path.trim().strip_prefix("path=").unwrap());
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]), head);
    for reference in [
        "refs/remotes/origin/master",
        "refs/remotes/origin/accepted-base",
    ] {
        assert_eq!(
            git(&clone, &["rev-parse", "--verify", reference]),
            base,
            "configured base history must survive detached checkout isolation"
        );
    }
    assert_eq!(
        git(&source, &["show-ref"]),
        before,
        "source refs must remain untouched"
    );
    assert!(!clone.join(".git/objects/info/alternates").exists());
}

#[test]
fn standalone_uses_strict_release_proof_and_never_label_or_branch_bypasses() {
    let recipe = include_str!("../assets/tools/source_ci/standalone-validation.sh");
    assert!(VALIDATE.contains("run-ci --check standalone-validation"));
    // Keep the dev profile and its runtime checks; only debug info may be omitted.
    let build_commands: Vec<_> = recipe
        .lines()
        .filter(|line| line.starts_with("cargo "))
        .collect();
    assert_eq!(
        build_commands,
        [
            "cargo build --locked --quiet --config profile.dev.debug=0 --features build-tools --bin decapod --bin decapod-release-projection-drift"
        ]
    );
    assert!(recipe.contains("./target/debug/decapod-release-projection-drift"));
    assert!(recipe.contains("SOURCE_CI_RELEASE_ONLY"));
    assert!(
        include_str!("../assets/tools/source_ci_workflow_owned.rs")
            .contains("release_classifier::classify(repo, event, base, head)")
    );
    for shortcut in [
        "SKIP_FINGERPRINT_GATES",
        "pull_request.labels",
        "startsWith(github.head_ref",
    ] {
        assert!(!VALIDATE.contains(shortcut));
        assert!(!recipe.contains(shortcut));
    }
    assert!(recipe.contains("set -euo pipefail"));
}

fn ordinary_drift_script(workflow: &str) -> String {
    let recipe = if workflow == CI {
        include_str!("../assets/tools/source_ci/health.sh")
    } else {
        include_str!("../assets/tools/source_ci/standalone-validation.sh")
    };
    let block = recipe
        .split_once("# Ensure no spec or entrypoint drift\n")
        .unwrap()
        .1;
    format!("{}\n)", block.split_once("\n)\n").unwrap().0)
}

#[test]
fn ordinary_drift_checks_execute_against_head_and_reject_hidden_staged_edits() {
    for workflow in [CI, VALIDATE] {
        for kind in ["clean", "unstaged", "staged", "staged-reverted"] {
            let fixture = Fixture::new();
            let root = &fixture.0;
            git(root, &["init", "-q"]);
            git(root, &["config", "user.name", "Drift fixture"]);
            git(root, &["config", "user.email", "fixture@example.invalid"]);
            fs::write(root.join("source.rs"), "accepted\n").unwrap();
            git(root, &["add", "."]);
            git(root, &["commit", "-qm", "baseline"]);
            if kind != "clean" {
                fs::write(root.join("source.rs"), "changed\n").unwrap();
            }
            if kind.starts_with("staged") {
                git(root, &["add", "source.rs"]);
            }
            if kind == "staged-reverted" {
                fs::write(root.join("source.rs"), "accepted\n").unwrap();
            }
            let output = Command::new("bash")
                .args(["-c", &ordinary_drift_script(workflow)])
                .current_dir(root)
                .env("RELEASE_ONLY", "false")
                .env("SOURCE_CI_EVENT", "pull_request")
                .env("SOURCE_CI_BASE", "1".repeat(40))
                .env("SOURCE_CI_HEAD", "2".repeat(40))
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                kind == "clean",
                "{kind}: {output:?}"
            );
        }
    }
}

#[test]
fn bazel_version_and_lockfile_are_frozen_before_source_checks() {
    assert_eq!(include_str!("../.bazelversion").trim(), "9.3.0");
    assert!(
        include_str!("../.config/build/bazelrc")
            .lines()
            .any(|line| line == "common --lockfile_mode=error")
    );
}

#[cfg(unix)]
#[test]
fn measured_private_checkout_keeps_source_clean_and_retains_detached_proof() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let source = fixture.0.join("source");
    let scratch = fixture.0.join("provider temp");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&scratch).unwrap();
    git(&source, &["init", "-q", "--initial-branch=master"]);
    git(&source, &["config", "user.name", "Private fixture"]);
    git(&source, &["config", "user.email", "ci@example.invalid"]);
    git(
        &source,
        &[
            "remote",
            "add",
            "origin",
            "https://example.invalid/repository.git",
        ],
    );
    fs::create_dir_all(source.join(".decapod/managed")).unwrap();
    fs::write(source.join(".decapod/managed/proof.txt"), "base\n").unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-qm", "accepted base"]);
    let base = git(&source, &["rev-parse", "HEAD"]);
    git(
        &source,
        &["update-ref", "refs/remotes/origin/master", &base],
    );
    fs::write(source.join("candidate.txt"), "candidate\n").unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-qm", "candidate"]);
    let head = git(&source, &["rev-parse", "HEAD"]);
    let tree = git(&source, &["rev-parse", "HEAD^{tree}"]);
    git(&source, &["checkout", "-q", "--detach", &head]);
    git(&source, &["branch", "-D", "master"]);
    let refs = git(&source, &["show-ref"]);
    fs::set_permissions(source.join(".decapod"), fs::Permissions::from_mode(0o777)).unwrap();
    let helper = include_str!("../assets/tools/source_ci/private-checkout.sh");
    let run = |temp: &Path| {
        Command::new("bash")
            .args(["-c", &format!("umask 000\n{helper}\nsource_private_checkout\nprintf '%s' \"$SOURCE_PRIVATE_CHECKOUT\"\n")])
            .current_dir(&source)
            .env("RUNNER_TEMP", temp)
            .output().unwrap()
    };
    let output = run(&scratch);
    assert!(output.status.success(), "{output:?}");
    let clone = PathBuf::from(String::from_utf8(output.stdout).unwrap());
    assert!(clone.starts_with(fs::canonicalize(&scratch).unwrap()));
    assert!(!clone.starts_with(&source));
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&clone, &["rev-parse", "HEAD^{tree}"]), tree);
    assert_eq!(git(&clone, &["rev-parse", "origin/master"]), base);
    assert_eq!(
        git(&clone, &["remote", "get-url", "origin"]),
        "https://example.invalid/repository.git"
    );
    assert!(!clone.join(".git/objects/info/alternates").exists());
    for relative in ["", ".git", ".decapod", ".decapod/managed"] {
        assert_eq!(
            fs::metadata(clone.join(relative))
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0,
            "{relative}"
        );
    }
    assert_eq!(
        fs::metadata(source.join(".decapod"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o777
    );
    assert_eq!(git(&source, &["show-ref"]), refs);
    assert_eq!(
        git(&source, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    // Misconfigured provider scratch must fail rather than make source output
    // invisible to custody. No source path, even ignored, is an allowed clone.
    let denied = run(&source);
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("outside the source checkout"));
    assert_eq!(
        git(&source, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
}

#[cfg(unix)]
fn governance_recipe_case(case: &str) {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let source = fixture.0.join("source");
    let scratch = fixture.0.join("provider temp");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&scratch).unwrap();
    git(&source, &["init", "-q", "--initial-branch=master"]);
    git(&source, &["config", "user.name", "Governance fixture"]);
    git(&source, &["config", "user.email", "ci@example.invalid"]);
    git(
        &source,
        &[
            "remote",
            "add",
            "origin",
            "https://example.invalid/repository.git",
        ],
    );
    let recipes = source.join("assets/tools/source_ci");
    fs::create_dir_all(&recipes).unwrap();
    fs::write(
        recipes.join("private-checkout.sh"),
        include_str!("../assets/tools/source_ci/private-checkout.sh"),
    )
    .unwrap();
    fs::write(
        recipes.join("governance-artifacts.sh"),
        include_str!("../assets/tools/source_ci/governance-artifacts.sh"),
    )
    .unwrap();
    fs::write(source.join(".gitignore"), "target/\n").unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-qm", "accepted base"]);
    let base = git(&source, &["rev-parse", "HEAD"]);
    git(
        &source,
        &["update-ref", "refs/remotes/origin/master", &base],
    );
    fs::write(source.join("candidate.txt"), "authored candidate\n").unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-qm", "candidate"]);
    let head = git(&source, &["rev-parse", "HEAD"]);
    let tree = git(&source, &["rev-parse", "HEAD^{tree}"]);
    let unrelated = git(&source, &["commit-tree", &tree, "-m", "unrelated"]);
    let merge = git(
        &source,
        &[
            "commit-tree",
            &tree,
            "-p",
            &base,
            "-p",
            &head,
            "-m",
            "provider merge",
        ],
    );
    let checkout = match case {
        "head" => head.clone(),
        "unrelated" | "replacement" => unrelated.clone(),
        "wrong-order" => git(
            &source,
            &[
                "commit-tree",
                &tree,
                "-p",
                &head,
                "-p",
                &base,
                "-m",
                "wrong order",
            ],
        ),
        "wrong-base" => git(
            &source,
            &[
                "commit-tree",
                &tree,
                "-p",
                &unrelated,
                "-p",
                &head,
                "-m",
                "wrong base",
            ],
        ),
        "wrong-head" => git(
            &source,
            &[
                "commit-tree",
                &tree,
                "-p",
                &base,
                "-p",
                &unrelated,
                "-m",
                "wrong head",
            ],
        ),
        "extra-parent" => git(
            &source,
            &[
                "commit-tree",
                &tree,
                "-p",
                &base,
                "-p",
                &head,
                "-p",
                &unrelated,
                "-m",
                "extra parent",
            ],
        ),
        "different-tree" => {
            let base_tree = git(&source, &["rev-parse", &format!("{base}^{{tree}}")]);
            git(
                &source,
                &[
                    "commit-tree",
                    &base_tree,
                    "-p",
                    &base,
                    "-p",
                    &head,
                    "-m",
                    "changed tree",
                ],
            )
        }
        _ => merge.clone(),
    };
    git(&source, &["checkout", "-q", "--detach", &checkout]);
    if case == "replacement" {
        git(&source, &["replace", &unrelated, &merge]);
    }
    let refs = git(&source, &["show-ref"]);
    let config = fs::read(source.join(".git/config")).unwrap();
    let record = scratch.join("verifier-call");
    fs::create_dir_all(source.join("target/debug")).unwrap();
    // This recorder tests the wrapper's exact command/checkout boundary, not
    // governance acceptance. The real Rust publication/checkpoint gates stay
    // unchanged and must still be run by the normal CI validation profile.
    fs::write(
        source.join("target/debug/decapod"),
        r#"#!/usr/bin/env bash
set -euo pipefail
test "$(git rev-parse HEAD)" = "$EXPECTED_HEAD"
test "$(git rev-parse HEAD^{tree})" = "$EXPECTED_TREE"
test "$PWD" != "$EXPECTED_SOURCE"
test "$#" = 10
test "$1 $2 $3" = 'govern artifacts verify-checkpoints'
test "$4" = --base-branch && test "$5" = "$SOURCE_CI_BASE"
test "$6" = --head-ref && test "$7" = "$SOURCE_CI_HEAD"
test "$8 $9 ${10}" = '--target-base master --publication-material'
printf '%s\n' "$PWD" > "$RECIPE_RECORD"
exit "${RECIPE_EXIT_CODE:-0}"
"#,
    )
    .unwrap();
    let mut command = Command::new("bash");
    command
        .arg(recipes.join("governance-artifacts.sh"))
        .current_dir(&source)
        // This fixture supplies its own reviewed recipes. A checkout-less CI
        // shard can inherit a provider executor root absent from its runfiles.
        // Bind only this child; production still resolves helpers from R.
        .env("SOURCE_CI_EXECUTOR_ROOT", &source)
        .env("RUNNER_TEMP", &scratch)
        .env(
            "SOURCE_CI_BASE",
            if case == "base-ref" { "master" } else { &base },
        )
        .env(
            "SOURCE_CI_HEAD",
            match case {
                "head-ref" => "HEAD",
                "tree-object" => &tree,
                "same-base-head" => &base,
                _ => &head,
            },
        )
        .env(
            "SOURCE_CI_EVENT",
            if case == "wrong-event" {
                "push"
            } else {
                "pull_request"
            },
        )
        .env("EXPECTED_HEAD", &head)
        .env("EXPECTED_TREE", &tree)
        .env("EXPECTED_SOURCE", &source)
        .env("RECIPE_RECORD", &record);
    if case == "inherited-git" {
        // Every value would redirect or break the recipe if inherited by Git.
        command
            .env("GIT_DIR", scratch.join("missing.git"))
            .env("GIT_WORK_TREE", &scratch)
            .env("GIT_INDEX_FILE", scratch.join("wrong-index"))
            .env("GIT_OBJECT_DIRECTORY", scratch.join("wrong-objects"))
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "core.bare")
            .env("GIT_CONFIG_VALUE_0", "true")
            .env("GIT_NO_REPLACE_OBJECTS", "0");
    }
    if case == "verifier-failure" {
        command.env("RECIPE_EXIT_CODE", "23");
    }
    if matches!(case, "clone-race" | "source-race") {
        let real_git = Command::new("bash")
            .args(["-c", "command -v git"])
            .output()
            .unwrap();
        assert!(real_git.status.success());
        let shim = scratch.join("bin");
        fs::create_dir(&shim).unwrap();
        fs::write(
            shim.join("git"),
            r#"#!/usr/bin/env bash
set -euo pipefail
"$REAL_GIT" "$@"
if [ "$#" = 4 ] && [ "$1 $2 $3" = 'checkout --quiet --detach' ] && [ "$4" = "$EXPECTED_HEAD" ]; then
  if [ "$RACE_KIND" = clone-race ]; then
    "$REAL_GIT" update-ref HEAD "$ORIGINAL_CHECKOUT"
  else
    "$REAL_GIT" -C "$EXPECTED_SOURCE" update-ref HEAD "$EXPECTED_HEAD"
  fi
fi
"#,
        )
        .unwrap();
        fs::set_permissions(shim.join("git"), fs::Permissions::from_mode(0o700)).unwrap();
        command
            .env(
                "PATH",
                format!("{}:{}", shim.display(), std::env::var("PATH").unwrap()),
            )
            .env(
                "REAL_GIT",
                String::from_utf8(real_git.stdout).unwrap().trim(),
            )
            .env("RACE_KIND", case)
            .env("ORIGINAL_CHECKOUT", &checkout);
    }
    let output = command.output().unwrap();
    let reaches_verifier = matches!(
        case,
        "head" | "merge" | "inherited-git" | "verifier-failure"
    );
    assert_eq!(record.exists(), reaches_verifier, "{case}: {output:?}");
    assert_eq!(
        output.status.success(),
        reaches_verifier && case != "verifier-failure",
        "{case}: {output:?}"
    );
    if case == "verifier-failure" {
        assert_eq!(output.status.code(), Some(23));
    }
    if reaches_verifier {
        let clone = PathBuf::from(fs::read_to_string(&record).unwrap().trim());
        assert!(clone.starts_with(&scratch));
        assert_eq!(git(&clone, &["rev-parse", "HEAD"]), head);
        assert_eq!(git(&clone, &["rev-parse", "HEAD^{tree}"]), tree);
        assert_eq!(git(&clone, &["rev-parse", "origin/master"]), base);
        assert_eq!(git(&clone, &["status", "--porcelain"]), "");
        assert!(!clone.join(".git/objects/info/alternates").exists());
    }
    assert_eq!(git(&source, &["show-ref"]), refs);
    assert_eq!(fs::read(source.join(".git/config")).unwrap(), config);
    if case != "source-race" {
        assert_eq!(
            git(&source, &["--no-replace-objects", "rev-parse", "HEAD"]),
            checkout
        );
        assert_eq!(
            git(&source, &["--no-replace-objects", "status", "--porcelain"]),
            ""
        );
    }
}

#[cfg(unix)]
#[test]
fn governance_recipe_normalizes_only_exact_same_tree_pr_merge() {
    for case in ["head", "merge", "inherited-git", "verifier-failure"] {
        governance_recipe_case(case);
    }
}

#[cfg(unix)]
#[test]
fn governance_recipe_fixture_isolates_ambient_executor_root() {
    const CHILD_RECORD: &str = "GOVERNANCE_RECIPE_EXECUTOR_FIXTURE_RECORD";
    if let Some(record) = std::env::var_os(CHILD_RECORD) {
        governance_recipe_case("merge");
        fs::write(record, "fixture verified\n").unwrap();
        return;
    }

    // Reproduce the provider's inherited root in a separate test process;
    // never mutate the environment of concurrently running tests.
    let fixture = Fixture::new();
    let missing_executor = fixture.0.join("absent provider checkout");
    assert!(!missing_executor.exists());
    let record = fixture.0.join("child-record");
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "governance_recipe_fixture_isolates_ambient_executor_root",
            "--nocapture",
        ])
        .env("SOURCE_CI_EXECUTOR_ROOT", missing_executor)
        .env(CHILD_RECORD, &record)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(fs::read_to_string(record).unwrap(), "fixture verified\n");
}

#[cfg(unix)]
#[test]
fn governance_recipe_rejects_unproven_normalization_and_checkout_races() {
    for case in [
        "unrelated",
        "wrong-order",
        "wrong-base",
        "wrong-head",
        "extra-parent",
        "different-tree",
        "wrong-event",
        "head-ref",
        "base-ref",
        "tree-object",
        "same-base-head",
        "replacement",
        "clone-race",
        "source-race",
    ] {
        governance_recipe_case(case);
    }
}
