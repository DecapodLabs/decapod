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
    assert_eq!(full.len(), 39);
    assert_eq!(full.iter().filter(|j| j.workflow == "ci.yml").count(), 35);
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
