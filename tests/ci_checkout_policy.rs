//! CI storage isolation must preserve the source checkout and exact Git proof.
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
    for (workflow, step) in [
        (VALIDATE, "Build Decapod from checkout"),
        (VALIDATE, "Start validation trajectory"),
        (VALIDATE, "Decapod Validate"),
        (VALIDATE, "Ensure no spec or entrypoint drift"),
        (
            CI,
            "Verify normalized governance and every authored checkpoint",
        ),
        (CI, "Start validation trajectory"),
        (CI, "Initialize Decapod"),
        (CI, "Acquire Decapod session"),
        (CI, "Run validation harness"),
        (CI, "Ensure no spec or entrypoint drift"),
        (CI, "Run watcher"),
        (CI, "Check health summary"),
    ] {
        assert!(workflow.contains(&format!("      - name: {step}\n        working-directory: ${{{{ steps.validation-checkout.outputs.path }}}}\n")), "{step}");
    }
    assert_eq!(
        CI.matches("uses: ./.github/actions/validation-checkout")
            .count(),
        2
    );
    assert_eq!(
        VALIDATE
            .matches("uses: ./.github/actions/validation-checkout")
            .count(),
        1
    );
    assert!(CI.contains(
        "\"$GITHUB_WORKSPACE/target/debug/decapod\" govern artifacts verify-checkpoints"
    ));
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

const PRIVATE_TEST_STEPS: [(&str, &str, &str); 3] = [
    (
        "supabase-consumer",
        "Supabase HTTP and existing cloud boundaries",
        "cargo test --locked --features supabase-cloud --lib",
    ),
    (
        "workspace-lifecycle-docker",
        "Prove interrupted caller and owned container recovery",
        "cargo test --locked --lib",
    ),
    (
        "test-integration",
        "Run selective tests",
        "\"$RUNNER_TEMP/integration-tests/integration-shards\" run",
    ),
];

fn test_step_preamble(job: &str, step: &str, command: &str) -> String {
    let job = CI.split_once(&format!("\n  {job}:\n")).unwrap().1;
    let step = job
        .split_once(&format!("      - name: {step}\n"))
        .unwrap()
        .1;
    let body = step.split_once("        run: |\n").unwrap().1;
    let prefix = body.split_once(&format!("          {command}")).unwrap().0;
    prefix
        .lines()
        .map(|line| line.strip_prefix("          ").unwrap())
        .collect::<Vec<_>>()
        .join("\n")
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
    let (job, step, cargo) = PRIVATE_TEST_STEPS[0];
    let preamble = test_step_preamble(job, step, cargo);
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
        command.args([
            "-c",
            &format!("{preamble}\ngit remote get-url origin\ngit config --get user.name"),
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
    assert_eq!(CI.matches("GIT_CONFIG_GLOBAL=/dev/null").count(), 1);
    assert_eq!(
        CI.matches("unset GIT_CONFIG GIT_CONFIG_PARAMETERS").count(),
        1
    );
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
