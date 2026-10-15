//! Finite source workflow inventory and shared direct-argv execution contract.
#![allow(dead_code)]
#[path = "../assets/tools/source_ci_profile.rs"]
mod profile;

use profile::{CHECKS, Platform, commands, find_check, inventory, selected};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const CI: &str = include_str!("../.github/workflows/ci.yml");
const DOCS: &str = include_str!("../.github/workflows/docs.yml");
const CONSTITUTION: &str = include_str!("../.github/workflows/constitution-sync.yml");
const VALIDATE: &str = include_str!("../.github/workflows/decapod-validate.yml");
const DOCS_SYNC: &str = include_str!("../.github/workflows/docs_sync.yml");
const PREPARE: &str = include_str!("../.github/actions/source-ci-profile/action.yml");
const SOURCE: &str = include_str!("../assets/tools/source_ci_profile.rs");

#[test]
fn constitution_schema_checks_cover_branches_and_prs_without_release_tag_ranges() {
    let triggers = CONSTITUTION.split_once("\njobs:\n").unwrap().0;
    assert!(triggers.contains("  push:\n    branches: [\"**\"]\n"));
    assert!(triggers.contains("  pull_request:\n"));
    assert!(!triggers.contains("tags:"));
    assert!(!triggers.contains("tags-ignore:"));
    let validation = job(CONSTITUTION, "validate-constitution");
    assert!(validation.contains("uses: ./.github/actions/ci-context"));
    assert!(validation.contains("run-ci --check constitution-schema"));
    assert!(validation.contains("run --check constitution-schema"));
    // An accepted-tip branch creation still checks the schema, while the
    // existing immutable-context verifier continues rejecting tag contexts.
    assert!(validation.contains("Recheck accepted branch after schema validation"));
}

#[test]
fn docs_deployment_requires_positive_native_owner_before_job_scheduling() {
    let build = job(DOCS, "build");
    let deploy = job(DOCS, "deploy");
    assert!(build.contains("native-owner: ${{ steps.owner.outputs.native_owner }}"));
    assert!(build.contains("run-ci --check docs-build"));
    assert!(build.contains(
        "if: github.ref == 'refs/heads/master' && steps.owner.outputs.native_owner == 'true'"
    ));
    assert!(deploy.contains(
        "if: github.ref == 'refs/heads/master' && needs.build.outputs.native-owner == 'true'"
    ));
    assert!(deploy.contains("needs: build"));
    assert!(deploy.contains("uses: actions/deploy-pages@v4"));
    assert!(!deploy.contains("BUILDKITE_BUILD_ID"));
    assert!(!build.contains("pages: write"));
    assert!(
        !DOCS
            .split_once("\njobs:\n")
            .unwrap()
            .0
            .contains("pages: write")
    );
}

#[test]
fn docs_owner_selector_rejects_sanitized_importers_and_incomplete_identity() {
    let script = job(DOCS, "build")
        .split_once("        run: |\n")
        .unwrap()
        .1
        .split_once("      - uses: actions/checkout@v4")
        .unwrap()
        .0
        .lines()
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    for (case, github, runner, buildkite, build_id, expected) in [
        ("native", "true", "github-hosted", "", "", "true"),
        ("sanitized importer", "true", "self-hosted", "", "", "false"),
        ("missing runner", "true", "", "", "", "false"),
        ("unknown runner", "true", "other", "", "", "false"),
        ("missing github", "", "github-hosted", "", "", "false"),
        ("false github", "false", "github-hosted", "", "", "false"),
        (
            "buildkite marker",
            "true",
            "github-hosted",
            "true",
            "",
            "false",
        ),
        (
            "buildkite id",
            "true",
            "github-hosted",
            "",
            "fixture",
            "false",
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let output_file = temp.path().join("output");
        // Empty PATH makes the selector fail if it attempts provider calls or
        // credential-bearing tools. Identity selection is shell-only.
        let result = Command::new("/bin/bash")
            .args(["--noprofile", "--norc", "-c", &script])
            .env_clear()
            .env("PATH", "")
            .env("GITHUB_ACTIONS", github)
            .env("RUNNER_ENVIRONMENT", runner)
            .env("BUILDKITE", buildkite)
            .env("BUILDKITE_BUILD_ID", build_id)
            .env("GITHUB_OUTPUT", &output_file)
            .output()
            .unwrap();
        assert!(result.status.success(), "{case}: {result:?}");
        assert_eq!(
            fs::read_to_string(output_file).unwrap(),
            format!("native_owner={expected}\n"),
            "{case}"
        );
    }
}

#[test]
fn dependency_tool_preparation_keeps_both_fresh_measured_checks() {
    let lint = job(CI, "deps-lint");
    assert!(lint.contains("needs: release-metadata"));
    assert!(lint.contains("if: needs.release-metadata.outputs.release_only != 'true'"));
    assert!(!lint.contains("cargo install"));
    assert!(!lint.contains("command -v cargo-"));
    assert!(!lint.contains("actions/cache"));
    let installation = lint
        .find("- name: Install verified dependency lint tools")
        .unwrap();
    for check in ["dependencies-machete", "dependencies-deny"] {
        let marker = format!("run-ci --check {check}");
        assert_eq!(lint.matches(&marker).count(), 1);
        assert!(installation < lint.find(&marker).unwrap());
    }
    let machete = commands(find_check("dependencies-machete").unwrap(), &[]).unwrap();
    let deny = commands(find_check("dependencies-deny").unwrap(), &[]).unwrap();
    assert_eq!(machete[0].program, "cargo");
    assert_eq!(machete[0].args, ["machete"]);
    assert_eq!(deny[0].program, "cargo");
    assert_eq!(
        deny[0].args,
        ["deny", "--config", ".config/deny.toml", "check"]
    );
}

#[cfg(target_os = "linux")]
mod dependency_tool_installation {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const MACHETE_SHA: &str = "48200087f54c55aabcd4db4af1e25742b49846c02a1b1bfa134711945b35b2e9";
    const DENY_SHA: &str = "9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f";

    fn installer() -> String {
        job(CI, "deps-lint")
            .split_once("      - name: Install verified dependency lint tools\n        run: |\n")
            .unwrap()
            .1
            .split_once("      - name: Prepare shared source-CI profile")
            .unwrap()
            .0
            .lines()
            .map(|line| line.strip_prefix("          ").unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn executable(path: &Path, contents: &str) {
        fs::write(path, contents).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn fixture(version_override: Option<&str>) -> (tempfile::TempDir, String) {
        let root = tempfile::tempdir().unwrap();
        let mock = root.path().join("mock-bin");
        fs::create_dir(&mock).unwrap();
        executable(
            &mock.join("uname"),
            "#!/bin/sh\ncase \"$1\" in -s) echo Linux;; -m) echo x86_64;; *) exit 9;; esac\n",
        );
        executable(
            &mock.join("curl"),
            "#!/bin/sh\n[ \"${FAIL_DOWNLOAD:-0}\" = 0 ] || exit 22\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = --output ]; then shift; output=$1; fi\n  shift\ndone\ncp \"$TOOL_FIXTURES/$(basename \"$output\")\" \"$output\"\n",
        );
        for tool in ["cargo-machete", "cargo-deny"] {
            executable(
                &mock.join(tool),
                "#!/bin/sh\necho ambient >> \"$TOOL_FIXTURES/ambient-executed\"\nexit 97\n",
            );
        }
        fs::write(root.path().join("github-path"), "").unwrap();
        fs::write(root.path().join("github-env"), "").unwrap();
        let mut script = installer();
        for (tool, tag, version, pin) in [
            ("cargo-machete", "v0.9.2", "0.9.2", MACHETE_SHA),
            ("cargo-deny", "0.20.2", "cargo-deny 0.20.2", DENY_SHA),
        ] {
            let package = format!("{tool}-{tag}-x86_64-unknown-linux-musl");
            fs::create_dir(root.path().join(&package)).unwrap();
            // Match the actual tagged upstream CLIs: machete prints the bare
            // version, while deny's clap command prints its name and version.
            let reported = version_override.unwrap_or(version);
            executable(
                &root.path().join(&package).join(tool),
                &format!(
                    "#!/bin/sh\necho {tool} >> \"$TOOL_FIXTURES/executed\"\nprintf '%s\\n' '{reported}'\n"
                ),
            );
            let archive = root.path().join(format!("{tool}.tar.gz"));
            let packed = Command::new("tar")
                .args(["-czf"])
                .arg(&archive)
                .arg("-C")
                .arg(root.path())
                .arg(&package)
                .status()
                .unwrap();
            assert!(packed.success());
            let digest = Command::new("sha256sum").arg(&archive).output().unwrap();
            assert!(digest.status.success());
            let digest = String::from_utf8(digest.stdout).unwrap();
            // Test fixtures replace the checked-in trust values only in this
            // private shell string. Production always uses the official pins.
            script = script.replace(pin, digest.split_whitespace().next().unwrap());
        }
        (root, script)
    }

    fn run(root: &Path, script: &str, fail_download: bool) -> std::process::Output {
        Command::new("bash")
            .args(["-c", script])
            .current_dir(root)
            .env("RUNNER_TEMP", root)
            .env("GITHUB_PATH", root.join("github-path"))
            .env("GITHUB_ENV", root.join("github-env"))
            .env("CARGO_HOME", root.join("cargo-home"))
            .env("TOOL_FIXTURES", root)
            .env("FAIL_DOWNLOAD", if fail_download { "1" } else { "0" })
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    root.join("mock-bin").display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .output()
            .unwrap()
    }

    #[test]
    fn official_pins_are_fixed_and_authentication_precedes_extraction_and_execution() {
        let script = installer();
        assert_eq!(script.matches(MACHETE_SHA).count(), 1);
        assert_eq!(script.matches(DENY_SHA).count(), 1);
        assert!(script.contains("--proto '=https' --proto-redir '=https'"));
        assert!(script.contains("--max-time 90 --retry 2 --retry-max-time 180"));
        assert!(script.find("sha256sum --check").unwrap() < script.find("tar -xzOf").unwrap());
        assert!(!script.contains("sha256sum --check --status"));
        assert!(script.contains("cargo-machete '0.9.2'"));
        assert!(script.contains("cargo-deny 'cargo-deny 0.20.2'"));
        assert!(script.find("tar -xzOf").unwrap() < script.find("--version").unwrap());
        let (root, script) = fixture(None);
        let result = run(root.path(), &script, false);
        assert!(result.status.success(), "{result:?}");
        for tool in ["cargo-machete", "cargo-deny"] {
            let installed = root.path().join("verified-dependency-tools").join(tool);
            assert!(fs::symlink_metadata(&installed).unwrap().is_file());
            assert_eq!(
                fs::metadata(installed).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
        assert!(!root.path().join("ambient-executed").exists());
        assert_eq!(
            fs::read_to_string(root.path().join("executed")).unwrap(),
            "cargo-machete\ncargo-deny\n"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("github-path"))
                .unwrap()
                .trim(),
            root.path()
                .join("verified-dependency-tools")
                .to_str()
                .unwrap()
        );
        assert!(
            fs::read_to_string(root.path().join("github-env"))
                .unwrap()
                .starts_with(&format!(
                    "PATH={}/verified-dependency-tools:{}/cargo-home/bin:",
                    root.path().display(),
                    root.path().display()
                ))
        );
    }

    #[test]
    fn wrong_digest_wrong_version_and_failed_download_publish_no_tools_or_path() {
        for failure in ["digest", "version", "wrong-format", "download"] {
            let (root, script) = fixture(match failure {
                "version" => Some("0.9.1"),
                "wrong-format" => Some("cargo-machete 0.9.2"),
                _ => None,
            });
            let script = if failure == "digest" {
                installer()
            } else {
                script
            };
            let result = run(root.path(), &script, failure == "download");
            assert!(!result.status.success(), "{failure}: {result:?}");
            assert!(!root.path().join("verified-dependency-tools").exists());
            assert!(!root.path().join("ambient-executed").exists());
            assert!(
                fs::read_to_string(root.path().join("github-path"))
                    .unwrap()
                    .is_empty()
            );
            assert!(
                fs::read_to_string(root.path().join("github-env"))
                    .unwrap()
                    .is_empty()
            );
            let diagnostic = String::from_utf8_lossy(&result.stderr);
            if failure == "digest" {
                assert!(diagnostic.contains("cargo-machete archive SHA256 verification failed"));
            }
            if matches!(failure, "version" | "wrong-format") {
                assert!(diagnostic.contains("cargo-machete version mismatch: expected 0.9.2, got"));
            } else {
                assert!(!root.path().join("executed").exists());
            }
        }
    }
}

fn paths(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}
fn full_paths() -> Vec<String> {
    paths(&[
        "src/decapod/core/sql/new.sql",
        "src/decapod/core/todo.rs",
        "src/decapod/plugins/todo.rs",
        "src/main.rs",
        "tests/gatling.rs",
        "Cargo.toml",
        ".github/workflows/ci.yml",
    ])
}

#[test]
fn setup_proves_release_stable_inputs_before_the_strict_shared_build() {
    let check = find_check("setup-bazel").unwrap();
    assert!(!check.skipped_for_release_only);
    assert!(check.capabilities.contains(&"python3"));
    let recipe = commands(check, &full_paths()).unwrap();
    assert_eq!(recipe.len(), 2);
    assert_eq!(recipe[0].program, "bazelisk");
    assert_eq!(recipe[0].args, ["test", "//:cargo_inputs_policy"]);
    assert_eq!(recipe[1].program, "bazelisk");
    assert_eq!(
        recipe[1].args,
        ["build", "//:decapod", "//:release_projection_drift"]
    );
    assert!(recipe.iter().all(|command| {
        command
            .args
            .iter()
            .all(|arg| !arg.starts_with("--lockfile_mode"))
    }));
}

#[cfg(unix)]
#[test]
fn release_upload_stages_exact_binary_bytes_as_a_regular_file() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let release = job(CI, "release-build");
    let staging = release
        .split_once("      - name: Stage regular release binary\n        run: |\n")
        .unwrap()
        .1
        .split_once("      - name: Upload binary")
        .unwrap()
        .0
        .lines()
        .map(|line| line.strip_prefix("          ").unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(release.contains("path: ${{ env.RELEASE_BINARY }}"));
    assert!(!release.contains("path: bazel-bin/decapod"));
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("bazel-bin")).unwrap();
    let executable = root.path().join("actual-bazel-binary");
    fs::write(&executable, b"exact compiled binary bytes\0\xff").unwrap();
    symlink(&executable, root.path().join("bazel-bin/decapod")).unwrap();
    let environment = root.path().join("github-env");
    fs::write(&environment, "").unwrap();
    let output = Command::new("bash")
        .args(["-c", &staging])
        .current_dir(root.path())
        .env("GITHUB_ENV", &environment)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let export = fs::read_to_string(&environment).unwrap();
    let staged = PathBuf::from(export.trim().strip_prefix("RELEASE_BINARY=").unwrap());
    assert!(staged.is_relative());
    assert!(staged.starts_with("target/buildkite-artifacts"));
    let staged = root.path().join(staged);
    let metadata = fs::symlink_metadata(&staged).unwrap();
    assert!(metadata.is_file() && !metadata.file_type().is_symlink());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o755);
    assert_eq!(fs::read(staged).unwrap(), fs::read(executable).unwrap());

    fs::remove_file(root.path().join("bazel-bin/decapod")).unwrap();
    fs::write(&environment, "").unwrap();
    let missing = Command::new("bash")
        .args(["-c", &staging])
        .current_dir(root.path())
        .env("GITHUB_ENV", &environment)
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(fs::read_to_string(environment).unwrap().is_empty());
}

fn workflow(name: &str) -> &'static str {
    match name {
        "ci.yml" => CI,
        "docs.yml" => DOCS,
        "constitution-sync.yml" => CONSTITUTION,
        "decapod-validate.yml" => VALIDATE,
        "docs_sync.yml" => DOCS_SYNC,
        _ => panic!("unknown workflow: {name}"),
    }
}
fn jobs(workflow: &str) -> Vec<&str> {
    workflow
        .split_once("\njobs:\n")
        .unwrap()
        .1
        .lines()
        .filter(|line| {
            line.starts_with("  ")
                && !line.starts_with("   ")
                && line.ends_with(':')
                && !line.trim_start().starts_with('#')
        })
        .map(|line| line.trim().strip_suffix(':').unwrap())
        .collect()
}
fn job<'a>(workflow: &'a str, id: &str) -> &'a str {
    let start = workflow.find(&format!("\n  {id}:\n")).unwrap();
    let rest = &workflow[start + 1..];
    let mut length = rest.lines().next().unwrap().len() + 1;
    for line in rest.lines().skip(1) {
        if line.starts_with("  ") && !line.starts_with("   ") && line.ends_with(':') {
            break;
        }
        length += line.len() + 1;
    }
    &rest[..length.min(rest.len())]
}

fn repository_root() -> PathBuf {
    match (
        std::env::var_os("TEST_SRCDIR"),
        std::env::var_os("TEST_WORKSPACE"),
    ) {
        (Some(runfiles), Some(workspace)) => Path::new(&runfiles).join(workspace),
        _ => {
            let source = Path::new(file!());
            if source.is_absolute() {
                source.parent().unwrap().parent().unwrap().to_path_buf()
            } else {
                std::env::current_dir().unwrap()
            }
        }
    }
}

#[test]
fn any_new_workflow_requires_explicit_inventory_review() {
    let actual: BTreeSet<_> = fs::read_dir(repository_root().join(".github/workflows"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".yml") || name.ends_with(".yaml"))
        .collect();
    let expected: BTreeSet<_> = [
        "ci.yml",
        "constitution-sync.yml",
        "decapod-validate.yml",
        "docs.yml",
        "docs_autosync.yml",
        "docs_sync.yml",
        "nix-flake.yml",
        "release.yml",
        "security-audit.yml",
    ]
    .map(String::from)
    .into_iter()
    .collect();
    assert_eq!(
        actual, expected,
        "new, renamed or removed workflows require source-PR inventory review"
    );
}

#[test]
fn finite_inventory_exactly_covers_pr_jobs_and_explicitly_excludes_other_jobs() {
    let full = inventory(&full_paths(), false);
    assert_eq!(full.len(), 37);
    assert_eq!(full.iter().filter(|j| j.workflow == "ci.yml").count(), 33);
    assert!(full.iter().all(|j| j.applicable));
    let excluded: BTreeMap<&str, &[&str]> = BTreeMap::from([
        (
            "ci.yml",
            &["propodus-live", "test-master", "release-build"][..],
        ),
        ("docs.yml", &["deploy"][..]),
        ("constitution-sync.yml", &[][..]),
        ("decapod-validate.yml", &[][..]),
        ("docs_sync.yml", &[][..]),
    ]);
    for (file, excluded) in excluded {
        let actual: BTreeSet<_> = jobs(workflow(file)).into_iter().collect();
        let scheduled: BTreeSet<_> = full
            .iter()
            .filter(|j| j.workflow == file)
            .map(|j| j.job)
            .chain(excluded.iter().copied())
            .collect();
        assert_eq!(actual, scheduled, "inventory drift in {file}");
    }
    assert!(job(CI, "propodus-live").contains("github.event_name == 'workflow_dispatch'"));
    assert!(job(CI, "test-master").contains("needs.release-metadata.outputs.event == 'push'"));
    assert!(job(CI, "release-build").contains("github.ref == 'refs/heads/master'"));
    assert!(job(DOCS, "deploy").contains("github.ref == 'refs/heads/master'"));
    let release = include_str!("../.github/workflows/release.yml");
    assert!(job(release, "plan").contains(
        "github.event_name == 'pull_request' && startsWith(github.head_ref, 'release-plz-')"
    ));
    assert!(
        job(release, "release-publish").contains(
            "github.ref == 'refs/heads/master' || github.event_name == 'workflow_dispatch'"
        )
    );
    assert!(
        job(release, "publish-decapod-image")
            .contains("github.event_name == 'push' && github.ref != 'refs/heads/master'")
    );
    for file in [
        include_str!("../.github/workflows/docs_autosync.yml"),
        include_str!("../.github/workflows/security-audit.yml"),
        include_str!("../.github/workflows/nix-flake.yml"),
    ] {
        let events = file
            .split_once("\non:\n")
            .unwrap()
            .1
            .split("\njobs:\n")
            .next()
            .unwrap();
        assert!(
            !events
                .lines()
                .any(|line| line.trim_start().starts_with("pull_request:")),
            "a newly PR-triggered workflow requires inventory review"
        );
    }
}

#[test]
fn each_matrix_member_is_present_exactly_once() {
    let full = inventory(&full_paths(), false);
    let keys: BTreeSet<_> = full.iter().map(|j| (j.workflow, j.job, j.matrix)).collect();
    assert_eq!(keys.len(), full.len());
    assert!(job(CI, "test").contains("target: [core, plugins, cli]"));
    assert!(job(CI, "state-commit-golden-vectors").contains("os: [ubuntu-latest, macos-latest]"));
    let musl = job(CI, "musl-publication");
    let targets: Vec<_> = musl
        .lines()
        .filter_map(|line| line.strip_prefix("          - target: "))
        .collect();
    assert_eq!(
        targets,
        ["x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"]
    );
    let integration = job(CI, "test-integration");
    let matrix = integration
        .split_once("      matrix:\n        shard:\n")
        .unwrap()
        .1
        .split_once("    steps:\n")
        .unwrap()
        .0;
    let actual: Vec<_> = matrix
        .lines()
        .map(|line| line.strip_prefix("          - ").unwrap())
        .collect();
    let expected: Vec<_> = full
        .iter()
        .filter(|j| j.job == "test-integration")
        .map(|j| j.matrix.unwrap())
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn every_shared_check_is_called_by_the_actual_workflow_job() {
    let ids: BTreeSet<_> = CHECKS.iter().map(|c| c.id).collect();
    assert_eq!(ids.len(), CHECKS.len());
    for check in CHECKS {
        let body = job(workflow(check.workflow), check.job);
        let reference = if check.id.starts_with("selective-") {
            "run-ci --check \"selective-${{ matrix.target }}\"".to_string()
        } else if check.job == "test-integration" {
            "run-ci --check integration-${{ matrix.shard }}".to_string()
        } else {
            format!("run-ci --check {}", check.id)
        };
        assert!(
            (body.contains(&reference)
                || (check.id == "docs-drift"
                    && body.contains("uses: ./.github/actions/docs-drift"))),
            "{} must call its shared implementation",
            check.id
        );
        assert!(
            (body.contains("uses: ./.github/actions/source-ci-profile")
                || profile::workflow_owned::artifact_worker(check)),
            "{} must compile the same helper",
            check.id
        );
        let recipe = commands(check, &full_paths()).unwrap();
        assert!(!recipe.is_empty(), "{} lacks execution", check.id);
        if let Some(script) = profile::workflow_owned::script(check) {
            assert_eq!(recipe.len(), 1);
            assert_eq!(recipe[0].program, "bash");
            assert_eq!(recipe[0].args, [script]);
            assert!(!profile::workflow_owned::script_sources(check).is_empty());
        } else {
            assert!(
                recipe
                    .iter()
                    .all(|c| !["sh", "bash", "eval", "python", "python3"].contains(&c.program))
            );
        }
        assert!(body.contains(profile::custody::UPLOAD_STEP_NAME));
        assert!(body.contains("if: always() && env.SOURCE_CI_CUSTODY_ARTIFACT != ''"));
        assert!(body.contains("name: ${{ env.SOURCE_CI_CUSTODY_ARTIFACT }}"));
    }
    assert!(PREPARE.contains("rustc --edition=2024 -O assets/tools/source_ci_profile.rs"));
    assert!(!SOURCE.contains("Command::new(\"sh\")"));
    assert!(!SOURCE.contains("Command::new(\"bash\")"));
    assert!(!SOURCE.contains("serde_yaml"));
}

#[test]
fn grouped_runtime_checks_keep_each_recipe_receipt_and_applicability() {
    let ids = ["rpc-suite", "chaos-replay", "daemonless-lifecycle"];
    let full = inventory(&full_paths(), false);
    let grouped: Vec<_> = full.iter().filter(|j| j.job == "test-rpc-suite").collect();
    assert_eq!(grouped.len(), 1);
    assert_eq!(grouped[0].checks, ids);
    assert!(grouped[0].applicable);
    assert!(
        !inventory(&full_paths(), true)
            .iter()
            .find(|j| j.job == "test-rpc-suite")
            .unwrap()
            .applicable
    );
    let scheduled: BTreeSet<_> = full.iter().flat_map(|j| j.checks.iter().copied()).collect();
    assert_eq!(scheduled, CHECKS.iter().map(|c| c.id).collect());
    for (id, target) in ids.into_iter().zip([
        "//:agent_rpc_suite",
        "//:chaos_replay",
        "//:daemonless_lifecycle",
    ]) {
        let check = find_check(id).unwrap();
        assert_eq!(check.job, "test-rpc-suite");
        assert_eq!(check.platform, Platform::LinuxX86_64);
        assert_eq!(check.selection, profile::Selection::Always);
        assert!(check.matrix.is_none());
        assert_eq!(check.capabilities, ["bazelisk"]);
        assert!(selected(check, &[], false));
        assert!(!selected(check, &full_paths(), true));
        let recipe = commands(check, &[]).unwrap();
        assert_eq!(recipe.len(), 1);
        assert_eq!(recipe[0].program, "bazelisk");
        assert_eq!(
            recipe[0].args,
            ["test", target, "--test_arg=--test-threads=1"]
        );
    }
    let body = job(CI, "test-rpc-suite");
    assert!(body.contains("needs: release-metadata"));
    assert!(body.contains("needs.release-metadata.outputs.release_only != 'true'"));
    assert_eq!(body.matches("uses: actions/checkout@v4").count(), 1);
    assert_eq!(
        body.matches("uses: ./.github/actions/source-ci-profile")
            .count(),
        1
    );
    assert_eq!(body.matches(profile::custody::UPLOAD_STEP_NAME).count(), 1);
    assert!(body.contains("if: always() && env.SOURCE_CI_CUSTODY_ARTIFACT != ''"));
    assert!(!body.contains("continue-on-error"));
    assert!(!body.contains("actions/cache"));
    assert!(!body.contains("download-artifact"));
    assert!(!CI.contains("\n  chaos-replay:\n"));
    assert!(!CI.contains("\n  daemonless-lifecycle:\n"));
    assert!(job(CI, "release-build").contains("test-rpc-suite"));
}

#[cfg(unix)]
#[test]
fn grouped_runtime_failure_never_skips_a_check_or_becomes_success() {
    use std::os::unix::fs::PermissionsExt;
    let body = job(CI, "test-rpc-suite");
    let script = body
        .split_once(
            "      - name: Run independent RPC, chaos and daemonless checks\n        run: |\n",
        )
        .unwrap()
        .1
        .split_once("      - name: Upload measured source-CI custody")
        .unwrap()
        .0
        .lines()
        .map(|line| line.strip_prefix("          ").unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    for failures in 0..8 {
        let root = tempfile::tempdir().unwrap();
        let executor = root.path().join("source-ci-profile");
        fs::write(
            &executor,
            r#"#!/usr/bin/env bash
set -euo pipefail
[[ $# == 3 && "$1" == run-ci && "$2" == --check ]]
printf '%s\n' "$3" >> "$RUNNER_TEMP/calls"
case "$3" in
  rpc-suite) bit=1; code=23;;
  chaos-replay) bit=2; code=37;;
  daemonless-lifecycle) bit=4; code=43;;
  *) exit 99;;
esac
if (( FAILURES & bit )); then exit "$code"; fi
"#,
        )
        .unwrap();
        fs::set_permissions(&executor, fs::Permissions::from_mode(0o755)).unwrap();
        let output = Command::new("bash")
            .args(["-e", "-c", &script])
            .env("RUNNER_TEMP", root.path())
            .env("FAILURES", failures.to_string())
            .output()
            .unwrap();
        let expected = if failures & 4 != 0 {
            43
        } else if failures & 2 != 0 {
            37
        } else if failures & 1 != 0 {
            23
        } else {
            0
        };
        assert_eq!(
            output.status.code(),
            Some(expected),
            "failure mask {failures}: {output:?}"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("calls")).unwrap(),
            "rpc-suite\nchaos-replay\ndaemonless-lifecycle\n"
        );
    }
}

#[test]
fn immutable_path_consumers_share_one_read_only_diff_implementation() {
    for id in [
        "detect-migration-script-changes",
        "test",
        "integration-prepare",
        "migration-script-tests",
    ] {
        let body = job(CI, id);
        assert!(body.contains("uses: ./.github/actions/ci-context"));
        assert!(body.contains("run-ci --check"));
        assert!(!body.contains("HEAD^"));
        assert!(!body.contains("origin/$base_ref"));
    }
    assert!(
        include_str!("../assets/tools/source_ci/migration-detection.sh")
            .contains("migration-changed --base \"$CHANGE_BASE\" --head \"$CHANGE_HEAD\"")
    );
    assert!(
        include_str!("../assets/tools/source_ci/integration-prepare.sh")
            .contains("paths --base \"$CHANGE_BASE\" --head \"$CHANGE_HEAD\"")
    );
    assert!(
        include_str!("../assets/tools/source_ci/integration-prepare.sh")
            .contains("integration-shards\" plan --paths")
    );
    assert!(
        include_str!("../assets/tools/source_ci/integration-worker.sh")
            .contains("integration-tests/integration-shards\" run")
    );
}

#[test]
fn selectors_preserve_unanchored_source_patterns_and_exact_migration_path() {
    for (id, matching, nonmatching) in [
        (
            "selective-core",
            "tests/fixtures/src/decapod/core/hash.rs",
            "src/decapod/plugins/hash.rs",
        ),
        (
            "selective-plugins",
            "tests/fixtures/src/decapod/plugins/todo.rs",
            "src/decapod/core/todo.rs",
        ),
        (
            "selective-cli",
            "tests/fixtures/src/main.rs",
            "src/bin/other.rs",
        ),
        (
            "migration-tests",
            "src/decapod/core/sql/new.sql",
            "tests/fixtures/src/decapod/core/sql/new.sql",
        ),
    ] {
        let c = find_check(id).unwrap();
        assert!(selected(c, &paths(&[matching]), false));
        assert!(!selected(c, &paths(&[nonmatching]), false));
        assert!(commands(c, &paths(&[nonmatching])).unwrap().is_empty());
    }
    let core = find_check("selective-core").unwrap();
    assert_eq!(
        commands(core, &paths(&["src/decapod/core/todo.rs"]))
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        commands(core, &paths(&["src/decapod/core/hash.rs"]))
            .unwrap()
            .len(),
        1
    );
    assert!(profile::core_full(&paths(&[
        "src/decapod/core/workspace_lifecycle.rs"
    ])));
    assert!(!profile::docs_sync_changed(&paths(&[
        "src/not-rust.txt",
        "docs/not-markdown.txt"
    ])));
    assert!(profile::docs_sync_changed(&paths(&["docs/top-level.md"])));
    assert!(profile::docs_sync_changed(&paths(&["src/main.rs"])));
}

#[test]
fn release_only_keeps_build_validation_constitution_and_docs_obligations() {
    let jobs = inventory(&full_paths(), true);
    let active: BTreeSet<_> = jobs
        .iter()
        .filter(|j| j.applicable)
        .map(|j| (j.workflow, j.job))
        .collect();
    assert_eq!(
        active,
        BTreeSet::from([
            ("ci.yml", "release-metadata"),
            ("ci.yml", "health"),
            ("ci.yml", "setup"),
            ("ci.yml", "musl-publication"),
            ("constitution-sync.yml", "validate-constitution"),
            ("decapod-validate.yml", "validate"),
            ("docs.yml", "build"),
            ("docs_sync.yml", "sync-docs")
        ])
    );
    assert!(selected(
        find_check("musl-build-aarch64").unwrap(),
        &[],
        true
    ));
    assert!(!selected(
        find_check("musl-test-aarch64").unwrap(),
        &[],
        true
    ));
    assert!(!selected(find_check("fmt").unwrap(), &[], true));
    assert!(selected(find_check("docs-build").unwrap(), &[], true));
}

#[test]
fn platform_and_actual_docker_requirements_cannot_disappear_from_the_plan() {
    assert!(!Platform::MacosAarch64.matches("linux", "x86_64"));
    assert!(!Platform::MacosAarch64.matches("macos", "x86_64"));
    assert!(Platform::MacosAarch64.matches("macos", "aarch64"));
    assert!(!Platform::LinuxX86_64.matches("linux", "aarch64"));
    assert!(!Platform::LinuxX86_64.matches("macos", "x86_64"));
    let jobs = inventory(&[], false);
    assert!(
        jobs.iter()
            .any(|j| j.matrix == Some("macos-latest") && j.applicable)
    );
    let docker = find_check("workspace-lifecycle-docker").unwrap();
    assert!(docker.capabilities.contains(&"docker-daemon"));
    let recipe = commands(docker, &[]).unwrap();
    assert_eq!(recipe[0].program, "docker");
    assert_eq!(recipe[0].args, ["pull", "busybox:1.37"]);
    assert!(recipe[1].args.contains(&"--ignored"));
    assert!(
        recipe[1]
            .env
            .contains(&("DECAPOD_RUN_DOCKER_LIFECYCLE_TEST", Some("1")))
    );
    let arm = commands(find_check("musl-test-aarch64").unwrap(), &[]).unwrap();
    assert_eq!(arm[0].program, "cross");
    assert!(arm[0].args.contains(&"test"));
    assert!(!arm[0].args.contains(&"--no-run"));
}

#[test]
fn offline_fixture_git_isolation_and_private_creation_are_scoped_to_children() {
    let check = find_check("supabase-consumer").unwrap();
    assert!(profile::requires_private_umask(check));
    for command in commands(check, &[]).unwrap() {
        for expected in [
            ("GIT_CONFIG", None),
            ("GIT_CONFIG_PARAMETERS", None),
            ("GIT_CONFIG_GLOBAL", Some("/dev/null")),
            ("GIT_CONFIG_NOSYSTEM", Some("1")),
            ("GIT_CONFIG_COUNT", Some("0")),
        ] {
            assert!(command.env.contains(&expected));
        }
    }
    assert!(!profile::requires_private_umask(find_check("fmt").unwrap()));
    assert!(
        commands(find_check("fmt").unwrap(), &[]).unwrap()[0]
            .env
            .iter()
            .all(|(k, _)| !k.starts_with("GIT_"))
    );
}

#[test]
fn malformed_paths_and_mutable_revisions_are_rejected() {
    for bad in [
        b"../escape".as_slice(),
        b"/absolute",
        b"-option",
        b"src/a\rb",
        b"a\0../b\0",
        b"\xff",
    ] {
        assert!(profile::parse_paths(bad).is_err());
    }
    assert_eq!(
        profile::parse_paths(b"src/a\0src/a\0src/b\0").unwrap(),
        paths(&["src/a", "src/b"])
    );
    for invalid in [
        "HEAD",
        "origin/master",
        "abc123",
        "0000000000000000000000000000000000000000",
        "--help",
    ] {
        assert!(!profile::immutable_sha(invalid));
    }
    assert!(profile::immutable_sha(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    ));
}

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "source-ci-policy-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_COUNT", "0")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn immutable_diff_remains_bound_after_branch_moves_and_ignores_replace_refs() {
    let temp = Temp::new();
    let root = &temp.0;
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "fixture@example.invalid"]);
    git(root, &["config", "user.name", "Fixture"]);
    fs::write(root.join("base"), "base").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]);
    fs::create_dir_all(root.join("src/decapod/core/sql")).unwrap();
    fs::write(root.join("src/decapod/core/sql/new.sql"), "select 1").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "source"]);
    let head = git(root, &["rev-parse", "HEAD"]);
    let expected = paths(&["src/decapod/core/sql/new.sql"]);
    assert_eq!(
        profile::changed_paths(root, &base, &head).unwrap(),
        expected
    );
    fs::write(root.join("later"), "later").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "later"]);
    let later = git(root, &["rev-parse", "HEAD"]);
    git(root, &["replace", &head, &later]);
    assert_eq!(
        profile::changed_paths(root, &base, &head).unwrap(),
        expected
    );
    assert!(profile::changed_paths(root, "HEAD", &head).is_err());
    assert!(
        profile::changed_paths(root, &base, "1111111111111111111111111111111111111111").is_err()
    );
}

#[test]
fn no_shared_subset_success_can_masquerade_as_complete_readiness() {
    assert!(SOURCE.contains("\\\"complete_pr_profile\\\":false"));
    for (workflow, job, _) in profile::WORKFLOW_OWNED_OBLIGATIONS {
        assert!(
            inventory(&full_paths(), false)
                .iter()
                .any(|j| j.workflow == *workflow && j.job == *job)
        );
    }
    assert!(profile::WORKFLOW_OWNED_OBLIGATIONS.is_empty());
    assert!(find_check("health").is_ok());
    assert!(find_check("standalone-validation").is_ok());
    assert!(
        include_str!("../assets/tools/source_ci_custody.rs").contains("authenticated\\\":false")
    );
}

#[test]
fn isolated_profile_execution_child() {
    let Some(root) = std::env::var_os("SOURCE_CI_FIXTURE_ROOT") else {
        return;
    };
    let id = std::env::var("SOURCE_CI_FIXTURE_CHECK").unwrap();
    let success = profile::run_check(
        Path::new(&root),
        find_check(&id).unwrap(),
        &full_paths(),
        "fixture-base",
        "fixture-head",
    )
    .unwrap();
    std::process::exit(if success { 0 } else { 1 });
}

#[cfg(unix)]
fn fake_tool(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
fn execute_fixture(root: &Path, id: &str, code: i32) -> std::process::Output {
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    fake_tool(&bin.join("rustc"), "printf 'rustc 1.96.1 (fixture)\\n'");
    fake_tool(&bin.join("rustfmt"), "exit 0");
    fake_tool(
        &bin.join("cargo"),
        r#"
printf 'CALL\n' >> "$SOURCE_CI_FIXTURE_ROOT/calls"
printf '%s\n' "$@" >> "$SOURCE_CI_FIXTURE_ROOT/calls"
printf '%s\n' "${GIT_CONFIG-unset}" "${GIT_CONFIG_PARAMETERS-unset}" "$GIT_CONFIG_GLOBAL" "$GIT_CONFIG_NOSYSTEM" "$GIT_CONFIG_COUNT" > "$SOURCE_CI_FIXTURE_ROOT/environment"
/bin/mkdir -p "$SOURCE_CI_FIXTURE_ROOT/new/state"
printf 'new private file' > "$SOURCE_CI_FIXTURE_ROOT/new/state/file"
exit "$SOURCE_CI_FIXTURE_EXIT"
"#,
    );
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "isolated_profile_execution_child", "--nocapture"])
        .env("PATH", &bin)
        .env("SOURCE_CI_FIXTURE_ROOT", root)
        .env("SOURCE_CI_FIXTURE_CHECK", id)
        .env("SOURCE_CI_FIXTURE_EXIT", code.to_string())
        .env("GIT_CONFIG", "poison")
        .env("GIT_CONFIG_PARAMETERS", "poison")
        .env("GIT_CONFIG_GLOBAL", "poison")
        .env("GIT_CONFIG_NOSYSTEM", "0")
        .env("GIT_CONFIG_COUNT", "7")
        .output()
        .unwrap()
}

#[cfg(unix)]
#[test]
fn actual_runner_emits_failed_exit_metadata_and_stops_before_second_command() {
    let temp = Temp::new();
    let output = execute_fixture(&temp.0, "supabase-consumer", 19);
    assert!(!output.status.success());
    let metadata = String::from_utf8(output.stdout).unwrap();
    assert!(metadata.contains("\"status\":\"failed\""), "{metadata}");
    assert!(metadata.contains("\"exit_code\":19"), "{metadata}");
    assert_eq!(
        fs::read_to_string(temp.0.join("calls"))
            .unwrap()
            .matches("CALL")
            .count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn actual_runner_preserves_private_permissions_and_exact_fixture_git_environment() {
    use std::os::unix::fs::PermissionsExt;
    let temp = Temp::new();
    fs::write(temp.0.join("existing"), "keep").unwrap();
    fs::set_permissions(temp.0.join("existing"), fs::Permissions::from_mode(0o666)).unwrap();
    let output = execute_fixture(&temp.0, "supabase-consumer", 0);
    assert!(output.status.success(), "{output:?}");
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("\"status\":\"passed\"")
    );
    assert_eq!(
        fs::read_to_string(temp.0.join("calls"))
            .unwrap()
            .matches("CALL")
            .count(),
        2
    );
    assert_eq!(
        fs::read_to_string(temp.0.join("environment")).unwrap(),
        "unset\nunset\n/dev/null\n1\n0\n"
    );
    assert_eq!(
        fs::metadata(temp.0.join("new/state"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(temp.0.join("new/state/file"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(temp.0.join("existing"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o666
    );
}

#[cfg(target_os = "linux")]
#[test]
fn actual_runner_rejects_missing_macos_instead_of_omitting_the_matrix() {
    let temp = Temp::new();
    let output = execute_fixture(&temp.0, "golden-vectors-macos", 0);
    assert!(!output.status.success());
    let metadata = String::from_utf8(output.stdout).unwrap();
    assert!(
        metadata.contains("\"status\":\"unavailable\""),
        "{metadata}"
    );
    assert!(metadata.contains("required platform macos"));
    assert!(!temp.0.join("calls").exists());
}

#[cfg(unix)]
#[test]
fn actual_runner_rejects_absent_tools_without_emitting_success() {
    let temp = Temp::new();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "isolated_profile_execution_child", "--nocapture"])
        .env("PATH", &temp.0)
        .env("SOURCE_CI_FIXTURE_ROOT", &temp.0)
        .env("SOURCE_CI_FIXTURE_CHECK", "fmt")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let metadata = String::from_utf8(output.stdout).unwrap();
    assert!(
        metadata.contains("\"status\":\"unavailable\""),
        "{metadata}"
    );
    assert!(metadata.contains("missing required tool: cargo"));
}

#[test]
fn custody_environment_policy_rejects_unknown_and_executable_overrides() {
    let spec = profile::custody::declared_toolchain::ENVIRONMENT_KEYS;
    let mut environment = spec
        .iter()
        .map(|k| (k.to_string(), None))
        .collect::<BTreeMap<_, _>>();
    assert!(profile::custody::declared_toolchain::validate_environment(&environment, &[]).is_ok());
    environment.insert("RUSTC_WRAPPER".into(), Some("unknown-wrapper".into()));
    assert!(profile::custody::declared_toolchain::validate_environment(&environment, &[]).is_err());
    environment.insert("RUSTC_WRAPPER".into(), None);
    environment.insert("UNKNOWN".into(), None);
    assert!(profile::custody::declared_toolchain::validate_environment(&environment, &[]).is_err());
    assert_eq!(
        profile::custody::artifact_name("test", Some("core"), "2").unwrap(),
        "source-ci-custody-test-core-attempt-2"
    );
    assert!(profile::custody::artifact_name("../test", None, "2").is_err());
}

#[test]
fn health_checks_auto_acquired_session_without_update_or_secret_output() {
    let health = include_str!("../assets/tools/source_ci/health.sh");
    assert!(health.contains("$(decapod session status)"));
    assert!(health.contains("== \"Session active\""));
    assert!(health.contains("unset session_status"));
    assert!(!health.contains("decapod session acquire"));
    assert!(!health.contains("decapod validate"));
}

#[test]
fn synthetic_release_manifest_is_materialized_only_by_its_fixture_loader() {
    let root = repository_root();
    assert!(
        !root
            .join("tests/fixtures/derived-release/Cargo.toml")
            .exists()
    );
    let fixture = include_str!("fixtures/derived-release/Cargo.toml.fixture");
    assert!(fixture.contains("[package]"));
    let loader = include_str!("../src/decapod/core/derived_release_profile_tests.rs");
    assert!(loader.contains("\"Cargo.toml\""));
    assert!(loader.contains("tests/fixtures/derived-release/Cargo.toml.fixture"));
    // Actual dependency lint still inspects the entire repository source tree.
    assert_eq!(
        commands(find_check("dependencies-machete").unwrap(), &[]).unwrap()[0].args,
        ["machete"]
    );
}

#[test]
fn golden_recipes_check_immutable_vectors_and_run_their_regressions() {
    for id in ["golden-vectors-linux", "golden-vectors-macos"] {
        let recipe = commands(find_check(id).unwrap(), &[]).unwrap();
        assert!(recipe.iter().any(|command| command.program == "cargo"
            && command.args == ["test", "--release"]
            && command.cwd == "tests/golden_vectors"));
        assert!(recipe.iter().any(|command| command.program == "git"
            && command.args == ["add", "initial.txt", "target.txt"]));
        let generator = recipe.last().unwrap();
        assert_eq!(generator.program, "./target/release/golden_vectors");
        assert_eq!(generator.args, ["--check"]);
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn execute_setup_fixture(root: &Path, failure: &str, include_tool: bool) -> std::process::Output {
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    fake_tool(&bin.join("python3"), "exit 0");
    if include_tool {
        // Only the tool is controlled. The production finite recipe and runner
        // choose commands, order, failure propagation and subprocess execution.
        fake_tool(
            &bin.join("bazelisk"),
            r#"
printf '%s\n' "$*" >> "$SOURCE_CI_FIXTURE_ROOT/calls"
case "$*" in
  'test //:cargo_inputs_policy')
    [ "$SOURCE_CI_FIXTURE_FAILURE" != policy ] || exit 21
    printf 'checked\n' > "$SOURCE_CI_FIXTURE_ROOT/policy-checked"
    ;;
  'build //:decapod //:release_projection_drift')
    [ -f "$SOURCE_CI_FIXTURE_ROOT/policy-checked" ] || exit 90
    [ "$SOURCE_CI_FIXTURE_FAILURE" != build ] || exit 22
    /bin/mkdir -p "$SOURCE_CI_FIXTURE_ROOT/bazel-bin"
    printf 'current candidate bytes\n' > "$SOURCE_CI_FIXTURE_ROOT/bazel-bin/decapod"
    printf 'current drift checker bytes\n' > "$SOURCE_CI_FIXTURE_ROOT/bazel-bin/release_projection_drift"
    ;;
  *) exit 91 ;;
esac
"#,
        );
    }
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "isolated_profile_execution_child", "--nocapture"])
        .env("PATH", bin)
        .env("SOURCE_CI_FIXTURE_ROOT", root)
        .env("SOURCE_CI_FIXTURE_CHECK", "setup-bazel")
        .env("SOURCE_CI_FIXTURE_FAILURE", failure)
        .output()
        .unwrap()
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn validation_recipe_executes_check_then_build_without_a_prebuilt_candidate() {
    let root = Temp::new();
    assert!(!root.0.join("bazel-bin/decapod").exists());
    let output = execute_setup_fixture(&root.0, "none", true);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(root.0.join("calls")).unwrap(),
        "test //:cargo_inputs_policy\nbuild //:decapod //:release_projection_drift\n"
    );
    assert_eq!(
        fs::read(root.0.join("bazel-bin/decapod")).unwrap(),
        b"current candidate bytes\n"
    );
    let metadata = String::from_utf8(output.stdout).unwrap();
    assert!(metadata.contains("\"status\":\"passed\""), "{metadata}");
    assert_eq!(metadata.matches("\"exit_code\":0").count(), 2);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn validation_recipe_fails_closed_on_failed_prerequisite_build_or_missing_tool() {
    for failure in ["policy", "build", "missing_tool"] {
        let root = Temp::new();
        let output = execute_setup_fixture(&root.0, failure, failure != "missing_tool");
        assert!(!output.status.success(), "{failure}: {output:?}");
        assert!(!root.0.join("bazel-bin/decapod").exists());
        let metadata = String::from_utf8(output.stdout).unwrap();
        if failure == "missing_tool" {
            assert!(
                metadata.contains("\"status\":\"unavailable\""),
                "{metadata}"
            );
            assert!(metadata.contains("missing required tool: bazelisk"));
            assert!(!root.0.join("calls").exists());
        } else {
            assert!(metadata.contains("\"status\":\"failed\""), "{metadata}");
            let calls = fs::read_to_string(root.0.join("calls")).unwrap();
            assert_eq!(
                calls.lines().count(),
                if failure == "policy" { 1 } else { 2 }
            );
            assert!(metadata.contains(if failure == "policy" {
                "\"exit_code\":21"
            } else {
                "\"exit_code\":22"
            }));
        }
    }
}

#[test]
fn dual_root_context_is_explicit_versioned_and_all_or_none() {
    use profile::custody::{DUAL_ROOT_SCHEMA, ExecutorContext};
    let flags = BTreeMap::from([
        ("--execution-context".into(), DUAL_ROOT_SCHEMA.into()),
        ("--executor-root".into(), "/independent/executor".into()),
        ("--executor-revision".into(), "1".repeat(40)),
        ("--executor-tree".into(), "2".repeat(40)),
    ]);
    assert!(
        ExecutorContext::from_flags(&BTreeMap::new())
            .unwrap()
            .is_none()
    );
    let context = ExecutorContext::from_flags(&flags).unwrap().unwrap();
    assert_eq!(context.revision, "1".repeat(40));
    for key in flags.keys() {
        let mut incomplete = flags.clone();
        incomplete.remove(key);
        assert!(ExecutorContext::from_flags(&incomplete).is_err(), "{key}");
    }
    for (key, bad) in [
        ("--execution-context", "decapod.source-ci.dual-root.v999"),
        ("--executor-root", ""),
        ("--executor-revision", "HEAD"),
        ("--executor-tree", "refs/heads/master"),
    ] {
        let mut invalid = flags.clone();
        invalid.insert(key.into(), bad.into());
        assert!(ExecutorContext::from_flags(&invalid).is_err(), "{key}");
    }
}

fn dual_root_fixture() -> (Temp, profile::custody::ExecutorContext, PathBuf) {
    let temp = Temp::new();
    let executor = temp.0.join("executor");
    let subject = temp.0.join("subject");
    fs::create_dir(&executor).unwrap();
    fs::create_dir(&subject).unwrap();
    for (name, bytes) in profile::custody::profile_sources() {
        let path = executor.join(profile::custody::profile_source_path(name));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    git(&executor, &["init", "-q"]);
    git(&executor, &["config", "user.name", "Executor fixture"]);
    git(
        &executor,
        &["config", "user.email", "executor@example.invalid"],
    );
    git(&executor, &["config", "commit.gpgsign", "false"]);
    git(&executor, &["config", "core.hooksPath", "/dev/null"]);
    git(&executor, &["add", "."]);
    git(&executor, &["commit", "-qm", "reviewed executor closure"]);
    let context = profile::custody::ExecutorContext {
        revision: git(&executor, &["rev-parse", "HEAD"]),
        tree: git(&executor, &["rev-parse", "HEAD^{tree}"]),
        root: executor,
    };
    (temp, context, subject)
}

#[test]
fn dual_root_verifies_complete_executor_closure_without_treating_candidate_recipes_as_authority() {
    use profile::custody::{profile_digest, verify_executor_root};
    let (_temp, context, subject) = dual_root_fixture();
    let candidate_recipe = subject.join("assets/tools/source_ci/integration-prepare.sh");
    fs::create_dir_all(candidate_recipe.parent().unwrap()).unwrap();
    fs::write(&candidate_recipe, "candidate helper must not run\n").unwrap();
    let actual = verify_executor_root(&subject, &context).unwrap();
    assert_eq!(actual.closure_digest, profile_digest().unwrap());
    let identity: serde_json::Value =
        serde_json::from_str(&actual.context_json(&"3".repeat(40), &"4".repeat(40))).unwrap();
    assert_eq!(identity["schema"], "decapod.source-ci.dual-root.v1");
    assert_eq!(identity["executor_revision"], context.revision);
    assert_eq!(identity["executor_tree"], context.tree);
    assert_eq!(identity["executor_profile_digest"], actual.closure_digest);
    assert_eq!(identity["executor_closure_digest"], actual.closure_digest);
    assert_eq!(identity["subject_revision"], "3".repeat(40));
    assert_eq!(identity["subject_tree"], "4".repeat(40));
    assert!(!identity.to_string().contains(actual.root.to_str().unwrap()));
    assert_eq!(
        fs::read_to_string(&candidate_recipe).unwrap(),
        "candidate helper must not run\n"
    );
    for id in [
        "integration-prepare",
        "integration-gatling-0",
        "integration-status",
    ] {
        let recipe = commands(find_check(id).unwrap(), &full_paths()).unwrap();
        let args = profile::custody::configured_arguments(&recipe[0], &actual.root);
        assert!(Path::new(&args[0]).starts_with(&actual.root));
    }
    let recipe = commands(find_check("setup-bazel").unwrap(), &[]).unwrap();
    for command in &recipe {
        let args = profile::custody::configured_arguments(command, &actual.root);
        assert_eq!(
            args,
            command
                .args
                .iter()
                .map(|arg| std::ffi::OsString::from(*arg))
                .collect::<Vec<_>>()
        );
        assert_eq!(command.cwd, ".");
    }
}

#[test]
fn dual_root_rejects_aliases_missing_or_altered_closure_and_identity_drift() {
    use profile::custody::verify_executor_root;
    let (_temp, context, subject) = dual_root_fixture();
    assert!(verify_executor_root(&context.root, &context).is_err());
    let nested = context.root.join("subject");
    fs::create_dir(&nested).unwrap();
    assert!(verify_executor_root(&nested, &context).is_err());
    fs::remove_dir(&nested).unwrap();
    let mut wrong = context.clone();
    wrong.revision = "1".repeat(40);
    assert!(verify_executor_root(&subject, &wrong).is_err());
    wrong = context.clone();
    wrong.tree = "2".repeat(40);
    assert!(verify_executor_root(&subject, &wrong).is_err());
    for relative in [
        "assets/tools/source_ci/integration-worker.sh",
        "assets/tools/integration_shards.rs",
        "assets/tools/integration_custody.rs",
        "assets/tools/source_ci/private-checkout.sh",
        "assets/tools/docs_sync.rs",
        "src/decapod/core/release_metadata.rs",
    ] {
        let file = context.root.join(relative);
        let original = fs::read(&file).unwrap();
        fs::remove_file(&file).unwrap();
        assert!(
            verify_executor_root(&subject, &context).is_err(),
            "missing {relative}"
        );
        fs::write(&file, b"changed authority\n").unwrap();
        assert!(
            verify_executor_root(&subject, &context).is_err(),
            "changed {relative}"
        );
        fs::write(&file, original).unwrap();
    }
    verify_executor_root(&subject, &context).unwrap();
    let hidden = "assets/tools/integration_shards.rs";
    git(
        &context.root,
        &["update-index", "--assume-unchanged", hidden],
    );
    assert!(verify_executor_root(&subject, &context).is_err());
    git(
        &context.root,
        &["update-index", "--no-assume-unchanged", hidden],
    );
    fs::write(context.root.join(hidden), "different committed executor\n").unwrap();
    git(&context.root, &["add", hidden]);
    git(&context.root, &["commit", "-qm", "altered closure"]);
    assert!(verify_executor_root(&subject, &context).is_err(), "moved R");
    wrong.revision = git(&context.root, &["rev-parse", "HEAD"]);
    wrong.tree = git(&context.root, &["rev-parse", "HEAD^{tree}"]);
    assert!(
        verify_executor_root(&subject, &wrong).is_err(),
        "compiled closure mismatch even at clean R"
    );
}

#[cfg(unix)]
#[test]
fn dual_root_rejects_root_and_transitive_helper_symlinks() {
    use profile::custody::verify_executor_root;
    use std::os::unix::fs::symlink;
    let (temp, context, subject) = dual_root_fixture();
    let alias = temp.0.join("alias");
    symlink(&context.root, &alias).unwrap();
    let mut wrong = context.clone();
    wrong.root = alias;
    assert!(verify_executor_root(&subject, &wrong).is_err());
    let subject_alias = temp.0.join("subject-alias");
    symlink(&subject, &subject_alias).unwrap();
    assert!(verify_executor_root(&subject_alias, &context).is_err());
    let helpers = context.root.join("assets/tools/source_ci");
    let saved = temp.0.join("external-helpers");
    fs::rename(&helpers, &saved).unwrap();
    symlink(&saved, &helpers).unwrap();
    assert!(verify_executor_root(&subject, &context).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn integration_preparation_uses_and_packages_executor_helpers_while_writing_subject_outputs() {
    let temp = Temp::new();
    let subject = temp.0.join("subject");
    let executor = temp.0.join("executor");
    let scratch = temp.0.join("scratch");
    let bin = temp.0.join("bin");
    for path in [&subject, &executor, &scratch, &bin] {
        fs::create_dir(path).unwrap();
        fs::create_dir_all(path.join("assets/tools/source_ci")).unwrap();
    }
    for name in ["integration-worker.sh", "integration-status.sh"] {
        fs::write(
            executor.join("assets/tools/source_ci").join(name),
            format!("executor {name}\n"),
        )
        .unwrap();
        fs::write(
            subject.join("assets/tools/source_ci").join(name),
            format!("candidate {name}\n"),
        )
        .unwrap();
    }
    for name in ["integration_shards.rs", "integration_custody.rs"] {
        fs::write(
            executor.join("assets/tools").join(name),
            "executor helper\n",
        )
        .unwrap();
        fs::write(
            subject.join("assets/tools").join(name),
            "candidate helper must not compile\n",
        )
        .unwrap();
    }
    fake_tool(
        &executor.join("profile"),
        "[ \"$1\" = paths ] || exit 19; printf 'src/example.rs\\0'",
    );
    fake_tool(&scratch.join("source-ci-profile"), "exit 99");
    fake_tool(
        &bin.join("jq"),
        "case \"$1\" in -c) printf '[]\\n';; *) printf '0\\n';; esac",
    );
    fake_tool(
        &bin.join("rustc"),
        r#"
set -eu
printf '%s\n' "$@" >> "$MOCK_LOG"
source=''
output=''
while [ "$#" -gt 0 ]; do
  case "$1" in *.rs) source="$1";; -o) shift; output="$1";; esac
  shift
done
case "$source" in "$SOURCE_CI_EXECUTOR_ROOT"/assets/tools/*.rs) ;; *) exit 94;; esac
case "$source" in
  */integration_shards.rs)
    printf '#!/bin/sh\n[ "$1" = plan ] || exit 92\nprintf '\''{"include":[]}\\n'\''\n' > "$output";;
  */integration_custody.rs)
    cat > "$output" <<'HELPER'
#!/bin/sh
case "$1" in
  context) printf 'context=fixture\n';;
  digest) sha256sum "$3" | cut -d ' ' -f1;;
  describe-preparation) printf 'fixture descriptor\n';;
  *) exit 93;;
esac
HELPER
    ;;
  *) exit 95;;
esac
chmod +x "$output"
"#,
    );
    let output = Command::new("bash")
        .args([
            "-c",
            include_str!("../assets/tools/source_ci/integration-prepare.sh"),
        ])
        .current_dir(&subject)
        .env("RUNNER_TEMP", &scratch)
        .env("GITHUB_OUTPUT", scratch.join("outputs"))
        .env("CHANGE_BASE", "base")
        .env("CUSTODY_BASE", "base")
        .env("CHANGE_HEAD", "head")
        .env("CUSTODY_HEAD", "head")
        .env("SOURCE_CI_EXECUTOR_ROOT", &executor)
        .env("SOURCE_CI_PROFILE_EXECUTABLE", executor.join("profile"))
        .env("MOCK_LOG", scratch.join("compiler-inputs"))
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let compiler_inputs = fs::read_to_string(scratch.join("compiler-inputs")).unwrap();
    for name in ["integration_shards.rs", "integration_custody.rs"] {
        assert!(
            compiler_inputs.contains(executor.join("assets/tools").join(name).to_str().unwrap())
        );
        assert!(
            !compiler_inputs.contains(subject.join("assets/tools").join(name).to_str().unwrap())
        );
    }
    for name in ["integration-worker.sh", "integration-status.sh"] {
        assert_eq!(
            fs::read_to_string(
                scratch
                    .join("integration-bootstrap-package/assets/tools/source_ci")
                    .join(name)
            )
            .unwrap(),
            format!("executor {name}\n")
        );
        assert_eq!(
            fs::read_to_string(subject.join("assets/tools/source_ci").join(name)).unwrap(),
            format!("candidate {name}\n")
        );
    }
    assert_eq!(
        fs::read(
            scratch.join("integration-bootstrap-package/integration-bootstrap/source-ci-profile")
        )
        .unwrap(),
        fs::read(executor.join("profile")).unwrap()
    );
    assert!(
        subject
            .join("target/integration-bootstrap-artifacts/integration-bootstrap.tar")
            .is_file()
    );
    assert!(
        subject
            .join("target/integration-custody-artifacts/preparation.v1")
            .is_file()
    );
    assert!(!executor.join("target").exists());
}

#[cfg(target_os = "linux")]
#[test]
fn measured_dual_root_execution_binds_subject_and_never_runs_candidate_recipe() {
    use profile::custody::{self, Context};
    // Isolate toolchain policy probes from Cargo/Bazel's test-runner overrides.
    if std::env::var("SOURCE_CI_DUAL_ROOT_CHILD").as_deref() != Ok("1") {
        let home = Temp::new();
        fs::create_dir(home.0.join("cargo-home")).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "measured_dual_root_execution_binds_subject_and_never_runs_candidate_recipe",
                "--nocapture",
            ])
            .env("SOURCE_CI_DUAL_ROOT_CHILD", "1")
            .env("HOME", &home.0)
            .env("CARGO_HOME", home.0.join("cargo-home"));
        for key in custody::declared_toolchain::ENVIRONMENT_KEYS {
            child.env_remove(key);
        }
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("CARGO_TARGET_") {
                child.env_remove(key);
            }
        }
        let output = child.output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "{output:?}"
        );
        return;
    }
    let (temp, executor, subject) = dual_root_fixture();
    git(&subject, &["init", "-q"]);
    git(&subject, &["config", "user.name", "Subject fixture"]);
    git(
        &subject,
        &["config", "user.email", "subject@example.invalid"],
    );
    git(&subject, &["config", "commit.gpgsign", "false"]);
    git(&subject, &["config", "core.hooksPath", "/dev/null"]);
    fs::write(subject.join("source.txt"), "base\n").unwrap();
    git(&subject, &["add", "."]);
    git(&subject, &["commit", "-qm", "base"]);
    let base = git(&subject, &["rev-parse", "HEAD"]);
    let candidate_recipe = subject.join("assets/tools/source_ci/commit-lint.sh");
    fs::create_dir_all(candidate_recipe.parent().unwrap()).unwrap();
    fs::write(&candidate_recipe, "#!/bin/sh\nexit 99\n").unwrap();
    git(&subject, &["add", "."]);
    git(&subject, &["commit", "-qm", "candidate source"]);
    let head = git(&subject, &["rev-parse", "HEAD"]);
    let tree = git(&subject, &["rev-parse", "HEAD^{tree}"]);
    let check = find_check("commit-lint").unwrap();
    let paths = profile::changed_paths(&subject, &base, &head).unwrap();
    let recipe = commands(check, &paths).unwrap();
    let mut context = Context {
        snapshot: head.clone(),
        validation_head: head.clone(),
        tree: tree.clone(),
        provider: "github".into(),
        run_id: "12".into(),
        run_attempt: "1".into(),
        job_id: check.job.into(),
        step_key: String::new(),
        expected_profile: custody::profile_digest().unwrap(),
        expected_invocation: custody::invocation_digest(&recipe).unwrap(),
        expected_toolchain: None,
        preparation_file: None,
        evidence_dir: temp.0.join("preparation"),
        executor: Some(executor.clone()),
    };
    assert!(custody::run(&subject, check, &paths, &base, &head, &context, true).unwrap());
    let preparation: serde_json::Value =
        serde_json::from_slice(&fs::read(context.evidence_dir.join("preparation.json")).unwrap())
            .unwrap();
    assert_eq!(preparation["schema"], custody::DUAL_ROOT_PREPARATION_SCHEMA);
    assert_eq!(preparation["execution_context"]["subject_revision"], head);
    assert_eq!(preparation["execution_context"]["subject_tree"], tree);
    context.expected_toolchain = Some(format!(
        "sha256:{}",
        custody::integration_custody::file_sha256(&context.evidence_dir.join("toolchain.json"),)
            .unwrap()
    ));
    context.preparation_file = Some(context.evidence_dir.join("preparation.json"));
    context.evidence_dir = temp.0.join("execution");
    assert!(custody::run(&subject, check, &paths, &base, &head, &context, false).unwrap());
    let execution: serde_json::Value =
        serde_json::from_slice(&fs::read(context.evidence_dir.join("execution.json")).unwrap())
            .unwrap();
    assert_eq!(execution["schema"], custody::DUAL_ROOT_EXECUTION_SCHEMA);
    assert_eq!(execution["execution_complete"], true);
    assert_eq!(execution["authenticated"], false);
    assert_eq!(execution["complete_pr_profile"], false);
    assert_eq!(
        execution["execution_context"],
        preparation["execution_context"]
    );
    assert_eq!(
        execution["final_executor_closure_digest"],
        context.expected_profile
    );
    assert_eq!(
        fs::read_to_string(&candidate_recipe).unwrap(),
        "#!/bin/sh\nexit 99\n"
    );
    context.tree = "2".repeat(40);
    context.evidence_dir = temp.0.join("wrong-subject");
    assert!(custody::run(&subject, check, &paths, &base, &head, &context, true).is_err());
    assert!(!context.evidence_dir.exists());
    context.tree = tree;
    fs::write(
        executor.root.join("assets/tools/source_ci/commit-lint.sh"),
        "changed after freeze\n",
    )
    .unwrap();
    context.evidence_dir = temp.0.join("changed-executor");
    assert!(custody::run(&subject, check, &paths, &base, &head, &context, false).is_err());
    assert!(!context.evidence_dir.exists());
}

fn source_profile_compile_recipe() -> String {
    PREPARE
        .split_once("      run: |\n")
        .unwrap()
        .1
        .split_once("    - name: Execute reviewed source check with measured custody\n")
        .unwrap()
        .0
        .lines()
        .map(|line| line.strip_prefix("        ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn provider_carrier_selects_only_immutable_opted_in_executor_and_keeps_legacy_rollout() {
    let recipe = source_profile_compile_recipe();
    assert!(recipe.contains("base=\"${SOURCE_CI_BASE:-}\""));
    assert!(recipe.contains("SOURCE_INPUT_BASE\" != \"$base"));
    assert!(recipe.contains("git_local ls-tree \"$base\" -- assets/tools/source_ci_custody.rs"));
    assert!(recipe.contains(
        "grep -Fqx 'pub const DUAL_ROOT_SCHEMA: &str = \"decapod.source-ci.dual-root.v1\";'"
    ));
    assert!(recipe.contains("worktree add --quiet --detach \"$executor_root\" \"$base\""));
    assert!(
        recipe.contains(
            "rustc --edition=2024 -O \"$executor_root/assets/tools/source_ci_profile.rs\""
        )
    );
    assert!(recipe.contains("run-ci|prepare-evidence|run-evidence)"));
    assert!(recipe.contains("--execution-context decapod.source-ci.dual-root.v1"));
    assert!(recipe.contains(
        "--executor-root \"$root\" --executor-revision \"$revision\" --executor-tree \"$tree\""
    ));
    assert!(recipe.contains("*) exec \"$executor\" \"$@\""));
    assert!(!recipe.contains("origin/master"));
    assert!(!recipe.contains("git_local fetch"));
    assert!(!recipe.contains("SOURCE_CI_EXECUTOR_ROOT="));
    let fallback = recipe
        .find("rustc --edition=2024 -O assets/tools/source_ci_profile.rs")
        .unwrap();
    let stage = recipe.find("worktree add --quiet --detach").unwrap();
    assert!(fallback < stage);
    assert!(recipe[fallback..stage].contains("exit 0\nfi"));
}

#[cfg(unix)]
struct ProfileCarrierFixture {
    _temp: Temp,
    subject: PathBuf,
    scratch: PathBuf,
    bin: PathBuf,
    base: String,
    head: String,
}
#[cfg(unix)]
impl ProfileCarrierFixture {
    fn new(supported_base: bool) -> Self {
        let temp = Temp::new();
        let subject = temp.0.join("subject");
        let scratch = temp.0.join("scratch with space");
        let bin = temp.0.join("bin");
        for path in [&subject, &scratch, &bin] {
            fs::create_dir(path).unwrap();
        }
        fs::create_dir_all(subject.join("assets/tools")).unwrap();
        let marker = "pub const DUAL_ROOT_SCHEMA: &str = \"decapod.source-ci.dual-root.v1\";\n";
        fs::write(
            subject.join("assets/tools/source_ci_custody.rs"),
            if supported_base {
                marker
            } else {
                "legacy custody\n"
            },
        )
        .unwrap();
        fs::write(
            subject.join("assets/tools/source_ci_profile.rs"),
            "reviewed executor R\n",
        )
        .unwrap();
        git(&subject, &["init", "-q"]);
        git(&subject, &["config", "user.name", "Carrier fixture"]);
        git(
            &subject,
            &["config", "user.email", "carrier@example.invalid"],
        );
        git(&subject, &["config", "commit.gpgsign", "false"]);
        git(&subject, &["config", "core.hooksPath", "/dev/null"]);
        git(&subject, &["add", "."]);
        git(&subject, &["commit", "-qm", "accepted executor"]);
        let base = git(&subject, &["rev-parse", "HEAD"]);
        // Opposite H marker proves that only the captured B object opts in.
        fs::write(
            subject.join("assets/tools/source_ci_custody.rs"),
            if supported_base {
                "candidate has no authority marker\n"
            } else {
                marker
            },
        )
        .unwrap();
        fs::write(
            subject.join("assets/tools/source_ci_profile.rs"),
            "candidate subject H\n",
        )
        .unwrap();
        git(&subject, &["add", "."]);
        git(&subject, &["commit", "-qm", "candidate subject"]);
        let head = git(&subject, &["rev-parse", "HEAD"]);
        fake_tool(
            &bin.join("mock-profile"),
            "printf '%s\\0' \"$@\" > \"$MOCK_ARGV\"; exit \"${MOCK_EXIT_CODE:-0}\"",
        );
        fake_tool(
            &bin.join("rustc"),
            r#"
set -eu
source=''
output=''
while [ "$#" -gt 0 ]; do
  case "$1" in *.rs) source="$1";; -o) shift; output="$1";; esac
  shift
done
printf '%s\n' "$source" >> "$MOCK_COMPILES"
cat "$source" >> "$MOCK_COMPILED_SOURCE"
[ "${FAIL_COMPILATION:-0}" = 0 ] || exit 19
cp "$MOCK_EXECUTABLE" "$output"
chmod +x "$output"
"#,
        );
        Self {
            _temp: temp,
            subject,
            scratch,
            bin,
            base,
            head,
        }
    }
    fn compile(&self) -> Command {
        let mut command = Command::new("bash");
        command
            .args(["-c", &source_profile_compile_recipe()])
            .current_dir(&self.subject)
            .env("SOURCE_CI_BASE", &self.base)
            .env("SOURCE_CI_HEAD", &self.head)
            .env("SOURCE_INPUT_BASE", "")
            .env("RUNNER_TEMP", &self.scratch)
            .env("MOCK_COMPILES", self.scratch.join("compiles"))
            .env("MOCK_COMPILED_SOURCE", self.scratch.join("compiled-source"))
            .env("MOCK_EXECUTABLE", self.bin.join("mock-profile"))
            .env(
                "PATH",
                format!("{}:{}", self.bin.display(), std::env::var("PATH").unwrap()),
            );
        command
    }
    fn invoke(&self, mode: &str, code: i32) -> (std::process::Output, Vec<String>) {
        let output = Command::new(self.scratch.join("source-ci-profile"))
            .args([mode, "--check", "fmt"])
            .current_dir(&self.subject)
            .env("MOCK_ARGV", self.scratch.join("argv"))
            .env("MOCK_EXIT_CODE", code.to_string())
            // Frozen explicit values win; ambient source/helper overrides have no role.
            .env("SOURCE_CI_BASE", "f".repeat(40))
            .env("SOURCE_CI_EXECUTOR_ROOT", "/not-the-executor")
            .output()
            .unwrap();
        let bytes = fs::read(self.scratch.join("argv")).unwrap();
        let args = bytes
            .split(|b| *b == 0)
            .filter(|v| !v.is_empty())
            .map(|arg| String::from_utf8(arg.to_vec()).unwrap())
            .collect();
        (output, args)
    }
}

#[cfg(unix)]
#[test]
fn provider_carrier_executes_immutable_r_and_wrapper_preserves_fixed_context_and_failure() {
    let fixture = ProfileCarrierFixture::new(true);
    let output = fixture
        .compile()
        .env("SOURCE_INPUT_BASE", &fixture.base)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(fixture.scratch.join("compiled-source")).unwrap(),
        "reviewed executor R\n"
    );
    let compiled = fs::read_to_string(fixture.scratch.join("compiles")).unwrap();
    let source = Path::new(compiled.trim());
    let executor = source.ancestors().nth(3).unwrap();
    assert!(executor.starts_with(&fixture.scratch));
    assert!(!executor.starts_with(&fixture.subject));
    assert_eq!(git(executor, &["rev-parse", "HEAD"]), fixture.base);
    let tree = git(executor, &["rev-parse", "HEAD^{tree}"]);
    assert_eq!(git(&fixture.subject, &["rev-parse", "HEAD"]), fixture.head);
    for mode in ["run-ci", "prepare-evidence", "run-evidence"] {
        let (output, args) = fixture.invoke(mode, 0);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            args,
            [
                mode,
                "--execution-context",
                profile::custody::DUAL_ROOT_SCHEMA,
                "--executor-root",
                executor.to_str().unwrap(),
                "--executor-revision",
                &fixture.base,
                "--executor-tree",
                &tree,
                "--check",
                "fmt"
            ]
        );
    }
    for mode in ["plan", "paths", "migration-changed", "docs-changed", "run"] {
        let (output, args) = fixture.invoke(mode, 0);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(args, [mode, "--check", "fmt"]);
    }
    let (output, _) = fixture.invoke("run-evidence", 23);
    assert_eq!(output.status.code(), Some(23));
    assert_eq!(
        fs::read_to_string(fixture.scratch.join("compiles"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn provider_carrier_never_falls_back_after_supported_r_compilation_failure() {
    let fixture = ProfileCarrierFixture::new(true);
    let output = fixture
        .compile()
        .env("FAIL_COMPILATION", "1")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(19), "{output:?}");
    assert_eq!(
        fs::read_to_string(fixture.scratch.join("compiled-source")).unwrap(),
        "reviewed executor R\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.scratch.join("compiles"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert!(!fixture.scratch.join("source-ci-profile").exists());
}

#[cfg(unix)]
#[test]
fn provider_carrier_legacy_rollout_manual_and_no_change_keep_original_subject_compilation() {
    for mode in ["legacy", "manual", "no-change"] {
        let fixture = ProfileCarrierFixture::new(false);
        let mut command = fixture.compile();
        match mode {
            "manual" => {
                command.env("SOURCE_CI_BASE", "");
            }
            "no-change" => {
                command.env("SOURCE_CI_BASE", &fixture.head);
            }
            _ => (),
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{mode}: {output:?}");
        assert_eq!(
            fs::read_to_string(fixture.scratch.join("compiled-source")).unwrap(),
            "candidate subject H\n"
        );
        assert_eq!(
            fs::read_to_string(fixture.scratch.join("compiles")).unwrap(),
            "assets/tools/source_ci_profile.rs\n"
        );
        let (output, args) = fixture.invoke("run-ci", 0);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(args, ["run-ci", "--check", "fmt"]);
    }
}

#[cfg(unix)]
#[test]
fn provider_carrier_rejects_mismatched_mutable_or_nested_executor_inputs_before_compilation() {
    for mode in ["mismatched", "mutable", "nested", "missing-head"] {
        let fixture = ProfileCarrierFixture::new(true);
        let mut command = fixture.compile();
        match mode {
            "mismatched" => {
                command.env("SOURCE_INPUT_BASE", &fixture.head);
            }
            "mutable" => {
                command.env("SOURCE_CI_BASE", "HEAD");
            }
            "nested" => {
                let nested = fixture.subject.join("runner");
                fs::create_dir(&nested).unwrap();
                command.env("RUNNER_TEMP", nested);
            }
            "missing-head" => {
                command.env_remove("SOURCE_CI_HEAD");
            }
            _ => unreachable!(),
        }
        let output = command.output().unwrap();
        assert!(!output.status.success(), "{mode}: {output:?}");
        assert!(!fixture.scratch.join("compiles").exists(), "{mode}");
    }
}

mod native_vega_driver {
    use crate::profile::custody::integration_custody as hashing;
    use crate::profile::native_vega::model;

    include!("../assets/tools/native_vega_driver_core.rs");
}

#[test]
fn native_vega_future_inventory_adds_four_checks_without_changing_current_obligations() {
    let paths = full_paths();
    for release_only in [false, true] {
        let current = inventory(&paths, release_only);
        let unchanged = profile::inventory_for_contract(&paths, release_only, false);
        let future = profile::inventory_for_contract(&paths, release_only, true);
        assert_eq!(format!("{current:?}"), format!("{unchanged:?}"));
        assert_eq!(future.len(), current.len() + 1);
        assert_eq!(
            format!("{:?}", &future[..current.len()]),
            format!("{current:?}")
        );
        let native = future.last().unwrap();
        assert_eq!(native.workflow, "ci.yml");
        assert_eq!(native.job, "native-vega");
        assert_eq!(native.matrix, None);
        assert_eq!(native.applicable, !release_only);
        assert!(!native.workflow_owned);
        assert_eq!(
            native.checks,
            [
                "native-vega-gpu",
                "native-vega-worker",
                "native-vega-model-setup",
                "native-vega-inference"
            ]
        );
    }
    assert!(
        CHECKS
            .iter()
            .all(|check| !profile::native_vega::is_check(check.id))
    );
    let all: BTreeSet<_> = profile::all_checks().map(|c| c.id).collect();
    assert_eq!(all.len(), CHECKS.len() + 4);
    assert!(!profile::native_vega::contract_enabled(Path::new(env!(
        "CARGO_MANIFEST_DIR"
    ))));
}

#[test]
fn native_vega_selects_only_exact_future_workflow_bytes() {
    let temp = Temp::new();
    fs::create_dir_all(temp.0.join(".github/workflows")).unwrap();
    let path = temp.0.join(".github/workflows/ci.yml");
    let fixture = include_bytes!("../assets/tools/source_ci/contracts/ci-native.yml.fixture");
    assert!(fixture.starts_with(CI.as_bytes()));
    assert!(!profile::native_vega::contract_enabled(&temp.0));
    fs::write(&path, CI).unwrap();
    assert!(!profile::native_vega::contract_enabled(&temp.0));
    fs::write(&path, fixture).unwrap();
    assert!(profile::native_vega::contract_enabled(&temp.0));
    let future = std::str::from_utf8(fixture).unwrap();
    for unmatched in [
        String::new(),
        format!("{future}\n"),
        future.replace(
            "name: Native Vega (Apple Metal)",
            "name: Native Vega (Apple Other)",
        ),
        future.replace(
            "--check native-vega-inference",
            "--check native-vega-model-setup",
        ),
        "jobs:\n  native-vega:\n    runs-on: macos-latest\n".into(),
    ] {
        assert_ne!(unmatched.as_bytes(), fixture);
        fs::write(&path, unmatched).unwrap();
        assert!(!profile::native_vega::contract_enabled(&temp.0));
        for check in profile::native_vega::CHECKS {
            assert!(profile::native_vega::preparation(&temp.0, &temp.0, check).is_err());
        }
    }
    #[cfg(unix)]
    {
        fs::remove_file(&path).unwrap();
        let target = temp.0.join("reviewed-workflow.yml");
        fs::write(&target, fixture).unwrap();
        std::os::unix::fs::symlink(target, &path).unwrap();
        assert!(!profile::native_vega::contract_enabled(&temp.0));
    }
}

#[test]
fn native_vega_gpu_recipe_requires_generic_shader_operations_without_claiming_execution() {
    // Inspect the reviewed capability recipe, not evidence that Metal ran here.
    let probe = include_str!("../assets/tools/native_metal_probe.m");
    for required in [
        "newLibraryWithSource",
        "newComputePipelineStateWithFunction",
        "dispatchThreads",
        "waitUntilCompleted",
        "MTLCommandBufferStatusCompleted",
        "actual[i] != expected[i]",
        "poison[i] = NAN",
        "alarm(60)",
        "decapod.native-metal-capability.v1",
    ] {
        assert!(
            probe.contains(required),
            "missing reviewed GPU proof operation: {required}"
        );
    }
    for check in profile::native_vega::CHECKS {
        assert_eq!(check.platform, Platform::MacosAarch64);
        assert!(check.skipped_for_release_only);
        assert!(!commands(check, &[]).unwrap().is_empty());
        assert!(!check.capabilities.contains(&"python3"));
    }
}

#[test]
fn native_vega_helper_uses_accepted_sources_and_subject_outputs() {
    let executor = Path::new("/reviewed-executor");
    let subject = Path::new("/candidate-subject");
    let recipe = commands(find_check("native-vega-gpu").unwrap(), &[]).unwrap();
    let helper = &recipe[0];
    assert!(profile::native_vega::is_driver_build(helper));
    let args = profile::custody::configured_arguments_for_subject(helper, executor, subject);
    assert!(
        args.contains(
            &executor
                .join("assets/tools/native_vega_driver/Cargo.toml")
                .into_os_string()
        )
    );
    assert!(
        args.contains(
            &subject
                .join("target/source-ci-native-vega/driver-build")
                .into_os_string()
        )
    );
    let probe = recipe.iter().find(|v| v.program == "xcrun").unwrap();
    let args = profile::custody::configured_arguments(probe, executor);
    assert!(
        args.contains(
            &executor
                .join("assets/tools/native_metal_probe.m")
                .into_os_string()
        )
    );
    let source_paths: BTreeSet<_> = profile::custody::profile_sources()
        .into_iter()
        .map(|(name, _)| profile::custody::profile_source_path(name))
        .collect();
    for (path, _) in profile::native_vega::SOURCES {
        assert!(
            source_paths.contains(Path::new(path)),
            "unbound native executor dependency: {path}"
        );
    }
    for check in profile::native_vega::CHECKS.iter().skip(1) {
        let driver = "target/source-ci-native-vega/driver-build/release/native-vega-driver";
        assert!(
            profile::native_vega::input_files(check)
                .iter()
                .any(|p| p == driver)
        );
        assert!(
            profile::native_vega::artifact_paths(check)
                .iter()
                .any(|p| p == driver)
        );
    }
    let worker = commands(find_check("native-vega-worker").unwrap(), &[]).unwrap();
    let cargo: Vec<_> = worker
        .iter()
        .filter(|step| step.program == "cargo")
        .collect();
    assert_eq!(cargo.len(), 4);
    for verb in ["build", "test"] {
        let root = cargo
            .iter()
            .find(|step| step.args.contains(&verb) && !step.args.contains(&"--manifest-path"))
            .unwrap();
        assert!(
            root.args
                .windows(2)
                .any(|pair| pair == ["--features", "vega-native"])
        );
        assert!(!root.args.contains(&"decapod-vega-infer"));
        let isolated = cargo
            .iter()
            .find(|step| step.args.contains(&verb) && step.args.contains(&"--manifest-path"))
            .unwrap();
        assert!(
            isolated
                .args
                .windows(2)
                .any(|pair| pair == ["--manifest-path", "assets/tools/vega-native/Cargo.toml"])
        );
        assert!(
            isolated
                .args
                .windows(2)
                .any(|pair| pair == ["--target-dir", "target/source-ci-native-vega/worker-build"])
        );
        assert!(
            isolated
                .args
                .windows(2)
                .any(|pair| pair == ["--bin", "decapod-vega-infer"])
        );
        assert!(isolated.args.contains(&"--locked"));
    }
}

#[test]
fn native_vega_models_are_pinned_and_never_uploaded_as_bulk_custody() {
    let files = profile::native_vega::MODEL_FILES;
    assert_eq!(files.len(), 9);
    assert_eq!(
        files.iter().map(|(name, _, _)| *name).collect::<Vec<_>>(),
        [
            "vega_config.json",
            "engine.safetensors",
            "backbone/config.json",
            "backbone/model.safetensors.index.json",
            "backbone/model.safetensors-00001-of-00001.safetensors",
            "backbone/tokenizer.json",
            "backbone/tokenizer_config.json",
            "backbone/LICENSE",
            "licenses/vega.LICENSE",
        ]
    );
    for (_, bytes, pin) in files {
        assert!(*bytes > 0);
        let digest = pin.strip_prefix("sha256:").unwrap();
        assert_eq!(digest.len(), 64);
        assert!(
            digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
    }
    for check in profile::native_vega::CHECKS {
        assert!(profile::native_vega::artifact_paths(check).iter().all(|p| {
            !p.contains(".safetensors") && !p.contains("model-cache") && !p.contains('*')
        }));
        for artifact in profile::native_vega::artifact_paths(check) {
            assert!(files.iter().all(|(path, _, _)| !artifact.ends_with(*path)));
        }
    }
    let helper = include_str!("../assets/tools/native_vega_driver_core.rs");
    assert!(helper.contains("#[serde(deny_unknown_fields)]"));
    assert!(helper.contains("--cache-directory"));
    assert!(!helper.contains("\"--refresh\""));
    assert!(
        helper.contains("\"publication_ready\":false")
            || helper.contains("\"publication_ready\": false")
    );
    let inference =
        profile::native_vega::artifact_paths(find_check("native-vega-inference").unwrap());
    assert!(
        inference
            .iter()
            .any(|path| path.ends_with("/native-proof.json"))
    );
}
