//! The PR matrix may move test execution, but may not drop coverage or isolation.
#![allow(dead_code)]
#[path = "../assets/tools/integration_shards.rs"]
mod integration_shards;

use integration_shards::{parse_inventory, parse_paths, partition_inventory, plan};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

fn paths(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

#[test]
fn original_changed_path_conditions_retain_every_target() {
    assert_eq!(plan(&paths(&["tests/gatling.rs"])).targets(), ["gatling"]);
    for changed in [
        "assets/tools/release_metadata.rs",
        "assets/tools/release_projection_drift.rs",
        "assets/tools/release_projection_drift_tests.rs",
        "tests/release_metadata_policy.rs",
        "tests/release_projection_drift_policy.rs",
    ] {
        assert_eq!(
            plan(&paths(&[changed])).targets(),
            ["release_metadata_policy", "release_projection_drift_policy"],
            "{changed}"
        );
    }
    for changed in [
        "Cargo.toml",
        "dist.toml",
        "dist-workspace.toml",
        "tests/release_distribution_policy.rs",
    ] {
        assert_eq!(
            plan(&paths(&[changed])).targets(),
            ["release_distribution_policy"],
            "{changed}"
        );
    }
    for changed in ["src/main.rs", "tests/fixtures/src/lib.rs"] {
        assert_eq!(
            plan(&paths(&[changed])).targets(),
            [
                "contract_conformance",
                "constitution_claims",
                "spec_conformance",
                "substrate_integrity"
            ]
        );
    }
    assert!(
        plan(&paths(&["README.md", "tests/not-gatling.rs"]))
            .targets()
            .is_empty()
    );
    assert_eq!(plan(&[]).matrix(), "{\"include\":[]}");
}

#[test]
fn runner_changes_cover_the_runner_and_workflow_retains_release_policies() {
    for changed in [
        "assets/tools/integration_shards.rs",
        "tests/integration_sharding_policy.rs",
        ".config/build/decapod.BUILD.bzl",
        "BUILD.bazel",
    ] {
        let selection = plan(&paths(&[changed]));
        let mut expected = vec!["integration_sharding_policy"];
        if changed == ".config/build/decapod.BUILD.bzl" {
            expected.push("integration_custody_policy");
        }
        if [".config/build/decapod.BUILD.bzl", "BUILD.bazel"].contains(&changed) {
            expected.push("source_ci_profile_policy");
        }
        assert_eq!(selection.targets(), expected);
        assert_eq!(selection.shards(), ["release-policies"]);
    }
    assert_eq!(
        plan(&paths(&[".github/workflows/ci.yml"])).targets(),
        [
            "release_metadata_policy",
            "release_projection_drift_policy",
            "integration_sharding_policy",
            "ci_checkout_policy",
            "integration_custody_policy",
            "ci_event_context_policy",
            "release_workflow_policy",
            "source_ci_profile_policy"
        ]
    );
}

#[test]
fn combined_selection_is_deterministic_and_each_target_is_unique() {
    let changed = paths(&[
        "src/main.rs",
        "Cargo.toml",
        "tests/gatling.rs",
        ".github/workflows/ci.yml",
    ]);
    let selection = plan(&changed);
    let mut reversed = changed.clone();
    reversed.reverse();
    assert_eq!(selection, plan(&reversed));
    let targets = selection.targets();
    assert_eq!(targets.len(), 14);
    assert_eq!(targets.len(), targets.iter().collect::<BTreeSet<_>>().len());
    assert_eq!(
        selection.shards(),
        paths(&[
            "gatling-0",
            "gatling-1",
            "gatling-2",
            "gatling-3",
            "release-policies",
            "contract_conformance",
            "constitution_claims",
            "spec_conformance",
            "substrate_integrity"
        ])
    );
}

#[test]
fn workflow_repairs_select_their_executable_regressions() {
    for changed in [
        ".github/actions/validation-checkout/action.yml",
        ".github/workflows/decapod-validate.yml",
        "tests/ci_checkout_policy.rs",
    ] {
        let mut expected = vec!["ci_checkout_policy"];
        if changed.starts_with(".github/workflows/") {
            expected.push("source_ci_profile_policy");
        }
        assert_eq!(plan(&paths(&[changed])).targets(), expected);
    }
    for changed in [
        ".github/workflows/release.yml",
        ".github/scripts/release-plan.sh",
        "tests/release_workflow_policy.rs",
    ] {
        assert_eq!(
            plan(&paths(&[changed])).targets(),
            if changed.starts_with(".github/workflows/") {
                vec!["release_workflow_policy", "source_ci_profile_policy"]
            } else {
                vec!["release_workflow_policy"]
            }
        );
    }
}

#[test]
fn preflight_context_and_docs_changes_select_real_policy_tests() {
    for changed in [
        "assets/tools/ci_event_context.rs",
        "tests/ci_event_context_policy.rs",
    ] {
        assert_eq!(
            plan(&paths(&[changed])).targets(),
            ["ci_event_context_policy"]
        );
    }
    for changed in [
        "assets/tools/docs_sync.rs",
        ".github/actions/docs-drift/action.yml",
        ".github/workflows/docs_sync.yml",
        "tests/docs_sync_checkpoint_boundary.rs",
        "tests/support/docs_sync_cases.rs",
    ] {
        assert_eq!(
            plan(&paths(&[changed])).targets(),
            if changed.starts_with(".github/workflows/") {
                vec!["docs_sync_checkpoint_boundary", "source_ci_profile_policy"]
            } else {
                vec!["docs_sync_checkpoint_boundary"]
            }
        );
    }
    assert!(
        plan(&paths(&["src/decapod/core/docs_cli.rs"]))
            .targets()
            .contains(&"docs_sync_checkpoint_boundary")
    );
}

fn workflow_job<'a>(workflow: &'a str, name: &str) -> &'a str {
    let marker = format!("\n  {name}:\n");
    let start = workflow.find(&marker).expect("job exists") + 1;
    let rest = &workflow[start..];
    let mut end = rest.lines().next().unwrap().len() + 1;
    for line in rest.lines().skip(1) {
        if line.starts_with("  ") && !line.starts_with("   ") && line.ends_with(':') {
            break;
        }
        end += line.len() + 1;
    }
    &rest[..end.min(rest.len())]
}

#[test]
fn static_workflow_matrix_covers_every_selected_shard_exactly_once() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let worker = workflow_job(workflow, "test-integration");
    let matrix = worker
        .split_once("      matrix:\n        shard:\n")
        .expect("static shard matrix, compatible with workflow concurrency")
        .1
        .split_once("    steps:\n")
        .unwrap()
        .0;
    let scheduled: Vec<_> = matrix
        .lines()
        .map(|line| line.strip_prefix("          - ").expect("literal shard"))
        .collect();
    let all = plan(&paths(&[
        "tests/gatling.rs",
        "src/main.rs",
        "Cargo.toml",
        ".github/workflows/ci.yml",
    ]))
    .shards();
    assert_eq!(scheduled, all);
    assert_eq!(
        scheduled.len(),
        scheduled.iter().collect::<BTreeSet<_>>().len()
    );
    assert!(!worker.contains("fromJSON(needs.integration-prepare.outputs.matrix)"));
    assert!(workflow.contains("\nconcurrency:\n"));
    assert!(workflow.contains("  cancel-in-progress: true\n"));

    // Every combination of existing selectors must be covered, including the
    // empty plan. Static scheduling must never introduce a second partition.
    let selectors = [
        "tests/gatling.rs",
        "src/main.rs",
        "Cargo.toml",
        ".github/workflows/ci.yml",
    ];
    for mask in 0..1 << selectors.len() {
        let changed: Vec<_> = selectors
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, path)| path.to_string())
            .collect();
        for shard in plan(&changed).shards() {
            assert!(
                scheduled.contains(&shard.as_str()),
                "unscheduled shard: {shard}"
            );
        }
    }
}

#[test]
fn static_workers_preserve_selection_shared_build_and_five_minute_test_budget() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let prepare = workflow_job(workflow, "integration-prepare");
    let worker = workflow_job(workflow, "test-integration");
    let prepare_recipe = include_str!("../assets/tools/source_ci/integration-prepare.sh");
    let worker_recipe = include_str!("../assets/tools/source_ci/integration-worker.sh");
    assert!(prepare.contains("shards: ${{ steps.plan.outputs.shards }}"));
    assert!(prepare.contains("archive_sha256: ${{ steps.plan.outputs.archive_sha256 }}"));
    assert!(prepare.contains("run-ci --check integration-prepare"));
    assert!(prepare_recipe.contains("jq -c '[.include[].shard]'"));
    assert!(prepare_recipe.contains("jq '.include | length'"));
    assert_eq!(
        prepare_recipe
            .matches("\"$RUNNER_TEMP/integration-shards\" prepare")
            .count(),
        1
    );
    assert!(prepare.contains("uses: actions/upload-artifact@v4"));
    assert!(prepare.contains("path: target/integration-artifacts/integration-tests.tar"));
    assert!(
        prepare.contains("path: target/integration-bootstrap-artifacts/integration-bootstrap.tar")
    );
    assert!(worker.contains("needs: [release-metadata, integration-prepare]"));
    assert!(worker.contains("needs.integration-prepare.outputs.count != '0'"));
    assert!(worker.contains("needs.release-metadata.outputs.release_only != 'true'"));
    let selection =
        "        if: contains(fromJSON(needs.integration-prepare.outputs.shards), matrix.shard)\n";
    for step in [
        "Download measured integration bootstrap",
        "Authenticate integration executor and recipes",
        "Download shared integration tests",
        "Run selected integration shard and record measured success",
        "Upload measured integration shard receipt",
    ] {
        assert!(
            worker.contains(&format!("      - name: {step}\n{selection}")),
            "{step}"
        );
    }
    assert_eq!(worker.matches(selection).count(), 5);
    assert!(worker.contains("        timeout-minutes: 6\n"));
    assert!(worker_recipe.contains("timeout --signal=TERM --kill-after=30s 300s"));
    assert!(worker_recipe.contains("\"$RUNNER_TEMP/integration-tests/integration-shards\" run"));
    assert!(worker_recipe.contains("--shard \"$CUSTODY_SHARD\""));
    assert!(worker_recipe.contains("trap stop_shard EXIT"));
    assert!(worker.contains("CUSTODY_SHARD: ${{ matrix.shard }}"));
    assert!(worker.contains("integration-bootstrap/source-ci-profile run-ci --check"));
    assert!(worker.contains("integration-${{ matrix.shard }}"));
    assert!(!worker.contains("bazelisk"));
    assert!(!worker.contains("cargo "));
    assert!(!worker.contains("rustc "));
    assert!(!worker.contains("actions/checkout"));
    assert!(!worker_recipe.contains("cargo "));
    assert!(!worker_recipe.contains("rustc "));
    assert!(!worker_recipe.contains("bazelisk"));
}
#[test]
fn paths_support_commas_spaces_and_nul_without_silently_narrowing_selection() {
    assert_eq!(
        parse_paths(b"tests/gatling.rs\0docs/a,b c.md\0").unwrap(),
        paths(&["docs/a,b c.md", "tests/gatling.rs"])
    );
    assert_eq!(
        parse_paths(b"src/main.rs\nsrc/main.rs\n").unwrap(),
        paths(&["src/main.rs"])
    );
    for malformed in [
        b"../src/main.rs".as_slice(),
        b"/src/main.rs",
        b"src/../main.rs",
        b"src/main.rs\r\n",
        b"-option",
        b"src/main.rs\nREADME.md\0",
        &[255],
    ] {
        assert!(parse_paths(malformed).is_err(), "{malformed:?}");
    }
}

#[test]
fn inventory_partition_has_no_gaps_duplicates_or_lifecycle_collision() {
    let mut names = paths(&[
        "t040_todo_lifecycle",
        "t292_decide_session_lifecycle",
        "host_fixture_suppresses_optional_claim_startup",
        "t053_todo_rebuild", // Existing ignored test is still in the inventory.
        "future::new_test_without_a_number",
    ]);
    names.extend((0..101).map(|index| format!("new_regression_{index:03}")));
    let shards = partition_inventory(&names).unwrap();
    let mut seen = BTreeMap::new();
    for name in shards.iter().flatten() {
        *seen.entry(name.clone()).or_insert(0) += 1;
    }
    assert_eq!(seen.len(), names.len());
    assert!(seen.values().all(|count| *count == 1));
    assert!(names.iter().all(|name| seen.contains_key(name)));
    assert!(shards[0].contains(&"t040_todo_lifecycle".to_string()));
    assert!(shards[1].contains(&"t292_decide_session_lifecycle".to_string()));
    let sizes: Vec<_> = shards.iter().map(Vec::len).collect();
    assert!(sizes.iter().max().unwrap() - sizes.iter().min().unwrap() <= 1);
    names.reverse();
    assert_eq!(shards, partition_inventory(&names).unwrap());
    assert!(partition_inventory(&[]).is_err());
    assert!(partition_inventory(&paths(&["duplicate", "duplicate"])).is_err());
}

#[test]
fn malformed_or_empty_libtest_output_fails_closed() {
    assert_eq!(
        parse_inventory("z: test\na::test: test\n").unwrap(),
        paths(&["a::test", "z"])
    );
    for invalid in [
        "",
        "\n",
        "a: test\na: test\n",
        ": test\n",
        "a: benchmark\n",
        "surprise\n",
        "--skip: test\n",
    ] {
        assert!(parse_inventory(invalid).is_err(), "{invalid:?}");
    }
}

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "decapod-integration-shards-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn artifact(&self) -> PathBuf {
        let artifact = self.0.join("relocated artifact with spaces");
        fs::create_dir(&artifact).unwrap();
        artifact
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[cfg(unix)]
fn script(path: &Path, contents: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, format!("#!/bin/sh\nset -eu\n{contents}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
fn fake_supervisor(artifact: &Path) {
    script(
        &artifact.join("build_lifecycle"),
        &format!(
            "printf 'supervisor\\n' >> {log:?}\n[ \"$1\" = '--' ]\nshift\nexec \"$@\"",
            log = artifact.join("calls.log")
        ),
    );
}

#[cfg(unix)]
fn fake_target(artifact: &Path, target: &str, list: &str, code: i32) {
    fs::create_dir_all(artifact.join(format!("{target}.runfiles/_main"))).unwrap();
    script(
        &artifact.join(target),
        &format!(
            "if [ \"${{1-}}\" = '--list' ]; then\n  [ \"$#\" = 3 ] && [ \"$2\" = '--format' ] && [ \"$3\" = 'terse' ]\n  printf '%s' '{list}'\n  exit 0\nfi\n{{\nprintf 'target={target}\\n'\nprintf 'cwd=%s\\n' \"$PWD\"\nprintf 'runfiles=%s\\n' \"$RUNFILES_DIR\"\nprintf 'manifest=%s\\n' \"${{RUNFILES_MANIFEST_FILE-unset}}\"\nprintf 'args:'\nprintf '<%s>' \"$@\"\nprintf '\\n'\n}} >> {log:?}\nexit {code}",
            log = artifact.join("calls.log")
        ),
    );
}

#[cfg(unix)]
#[test]
fn real_libtest_executable_inventory_is_read_through_the_supervisor() {
    let fixture = Fixture::new();
    let artifact = fixture.artifact();
    fake_supervisor(&artifact);
    fs::create_dir_all(artifact.join("gatling.runfiles/_main")).unwrap();
    fs::copy(std::env::current_exe().unwrap(), artifact.join("gatling")).unwrap();
    let names = integration_shards::inventory(&artifact).unwrap();
    assert!(
        names.contains(
            &"real_libtest_executable_inventory_is_read_through_the_supervisor".to_string()
        )
    );
    assert!(names.contains(
        &"inventory_partition_has_no_gaps_duplicates_or_lifecycle_collision".to_string()
    ));
    assert_eq!(
        fs::read_to_string(artifact.join("calls.log")).unwrap(),
        "supervisor\n"
    );
}

#[cfg(unix)]
#[test]
fn failed_inventory_preserves_exit_126_and_captured_diagnostics_without_retry() {
    let fixture = Fixture::new();
    let artifact = fixture.artifact();
    fake_supervisor(&artifact);
    fs::create_dir_all(artifact.join("gatling.runfiles/_main")).unwrap();
    script(
        &artifact.join("gatling"),
        "printf 'fixture stdout detail\\n'\nprintf 'fixture exec failure\\n' >&2\nexit 126",
    );
    let error = integration_shards::inventory(&artifact).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    let detail = error.to_string();
    assert!(detail.contains("supervised Gatling inventory failed: exit status: 126"));
    assert!(detail.contains("stdout=\"fixture stdout detail\\n\""));
    assert!(detail.contains("stderr=\"fixture exec failure\\n\""));
    assert_eq!(
        fs::read_to_string(artifact.join("calls.log")).unwrap(),
        "supervisor\n"
    );
}

#[cfg(unix)]
#[test]
fn failed_inventory_bounds_both_diagnostic_stream_excerpts() {
    let fixture = Fixture::new();
    let artifact = fixture.artifact();
    fake_supervisor(&artifact);
    fs::create_dir_all(artifact.join("gatling.runfiles/_main")).unwrap();
    let stdout = format!("stdout-start{}stdout-tail", "x".repeat(8192));
    let stderr = format!("stderr-start{}stderr-tail", "y".repeat(8192));
    script(
        &artifact.join("gatling"),
        &format!("printf '%s' '{stdout}'\nprintf '%s' '{stderr}' >&2\nexit 126"),
    );
    let detail = integration_shards::inventory(&artifact)
        .unwrap_err()
        .to_string();
    assert!(detail.contains("exit status: 126"));
    assert!(detail.contains("stdout-start"));
    assert!(detail.contains("stderr-start"));
    assert!(!detail.contains("stdout-tail"));
    assert!(!detail.contains("stderr-tail"));
    assert_eq!(detail.matches("(truncated;").count(), 2);
    assert!(detail.len() < 3 * 4096);
}

#[cfg(unix)]
#[test]
fn relocated_gatling_shards_route_exact_filters_and_preserve_runfiles() {
    let fixture = Fixture::new();
    let artifact = fixture.artifact();
    let names = paths(&[
        "future::new_case",
        "host_fixture_suppresses_optional_claim_startup",
        "t040_todo_lifecycle",
        "t053_todo_rebuild",
        "t292_decide_session_lifecycle",
        "test_with_a_similar_prefix",
        "test_with_a_similar_prefix_suffix",
        "z_last",
    ]);
    let listing = names
        .iter()
        .map(|name| format!("{name}: test\n"))
        .collect::<String>();
    fake_supervisor(&artifact);
    fake_target(&artifact, "gatling", &listing, 0);
    fs::write(artifact.join("changed-paths.txt"), "tests/gatling.rs\n").unwrap();
    let inventory = integration_shards::inventory(&artifact).unwrap();
    fs::write(artifact.join("gatling-inventory.txt"), inventory.join("\n")).unwrap();
    let command = integration_shards::test_command(&artifact, "gatling").unwrap();
    let env: BTreeMap<_, _> = command.get_envs().collect();
    for name in [
        "RUNFILES_MANIFEST_FILE",
        "TEST_TOTAL_SHARDS",
        "TEST_SHARD_INDEX",
        "TEST_SHARD_STATUS_FILE",
    ] {
        assert_eq!(env.get(std::ffi::OsStr::new(name)), Some(&None));
    }
    for index in 0..4 {
        integration_shards::run_shard(&artifact, &format!("gatling-{index}")).unwrap();
    }
    let log = fs::read_to_string(artifact.join("calls.log")).unwrap();
    assert_eq!(log.lines().filter(|line| *line == "supervisor").count(), 9);
    assert_eq!(log.matches("target=gatling\n").count(), 4);
    assert_eq!(log.matches("manifest=unset\n").count(), 4);
    assert_eq!(
        log.matches(&format!(
            "cwd={}\n",
            artifact.join("gatling.runfiles/_main").display()
        ))
        .count(),
        4
    );
    assert_eq!(
        log.matches(&format!(
            "runfiles={}\n",
            artifact.join("gatling.runfiles").display()
        ))
        .count(),
        4
    );
    let filters: Vec<_> = log
        .lines()
        .filter_map(|line| line.strip_prefix("args:<--test-threads=4><--exact>"))
        .collect();
    assert_eq!(filters.len(), 4);
    let mut seen = BTreeMap::new();
    for filters in filters {
        for name in filters.trim_matches(['<', '>']).split("><") {
            *seen.entry(name.to_string()).or_insert(0) += 1;
        }
    }
    assert_eq!(seen.len(), names.len());
    assert!(names.iter().all(|name| seen.get(name) == Some(&1)));
    assert!(!log.contains("--include-ignored"));
    assert!(!log.contains("--skip"));
}

#[cfg(unix)]
#[test]
fn inactive_empty_changed_or_stale_inventory_cannot_run_the_entire_suite() {
    let fixture = Fixture::new();
    let artifact = fixture.artifact();
    fake_supervisor(&artifact);
    fake_target(&artifact, "gatling", "only_one: test\n", 0);
    fs::write(artifact.join("changed-paths.txt"), "tests/gatling.rs\n").unwrap();
    fs::write(artifact.join("gatling-inventory.txt"), "only_one").unwrap();
    assert!(integration_shards::run_shard(&artifact, "gatling-0").is_err());
    assert!(integration_shards::run_shard(&artifact, "gatling-4").is_err());
    assert!(integration_shards::run_shard(&artifact, "release-policies").is_err());
    fs::write(artifact.join("gatling-inventory.txt"), "stale_test").unwrap();
    assert!(integration_shards::run_shard(&artifact, "gatling-2").is_err());
    let log = fs::read_to_string(artifact.join("calls.log")).unwrap();
    assert!(!log.contains("target="));
}

#[cfg(unix)]
#[test]
fn policy_shard_runs_every_selected_binary_and_propagates_failures() {
    let fixture = Fixture::new();
    let artifact = fixture.artifact();
    fake_supervisor(&artifact);
    fs::write(
        artifact.join("changed-paths.txt"),
        ".github/workflows/ci.yml\nCargo.toml\n",
    )
    .unwrap();
    let targets = plan(&paths(&[".github/workflows/ci.yml", "Cargo.toml"])).targets();
    for (index, target) in targets.iter().enumerate() {
        fake_target(&artifact, target, "", if index == 0 { 7 } else { 0 });
    }
    assert!(integration_shards::run_shard(&artifact, "release-policies").is_err());
    let log = fs::read_to_string(artifact.join("calls.log")).unwrap();
    for target in &targets {
        assert_eq!(log.matches(&format!("target={target}\n")).count(), 1);
    }
    assert_eq!(
        log.matches("args:<--test-threads=4>\n").count(),
        targets.len()
    );
}

#[cfg(unix)]
#[test]
fn conformance_shards_keep_the_original_serial_test_scheduler() {
    let fixture = Fixture::new();
    let artifact = fixture.artifact();
    fake_supervisor(&artifact);
    fs::write(artifact.join("changed-paths.txt"), "src/main.rs\n").unwrap();
    for target in plan(&paths(&["src/main.rs"])).targets() {
        fake_target(&artifact, target, "", 0);
        integration_shards::run_shard(&artifact, target).unwrap();
    }
    let log = fs::read_to_string(artifact.join("calls.log")).unwrap();
    assert_eq!(log.matches("args:<--test-threads=1>\n").count(), 4);
    assert_eq!(log.matches("supervisor\n").count(), 4);
}

#[cfg(unix)]
#[test]
fn packaged_runfiles_are_dereferenced_relocatable_and_deduplicated() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    let fixture = Fixture::new();
    let source = fixture.0.join("source");
    fs::create_dir(&source).unwrap();
    fs::create_dir(source.join("tree")).unwrap();
    fs::write(source.join("binary"), "executable").unwrap();
    fs::set_permissions(source.join("binary"), fs::Permissions::from_mode(0o755)).unwrap();
    symlink(source.join("binary"), source.join("tree/one")).unwrap();
    symlink(source.join("binary"), source.join("tree/two")).unwrap();
    let destination = fixture.0.join("bundle");
    integration_shards::copy_tree(
        &source.join("tree"),
        &destination,
        &[fs::canonicalize(&source).unwrap()],
        &mut BTreeMap::new(),
        &mut BTreeSet::new(),
    )
    .unwrap();
    let one = fs::symlink_metadata(destination.join("one")).unwrap();
    let two = fs::symlink_metadata(destination.join("two")).unwrap();
    assert!(one.is_file() && two.is_file());
    assert_eq!(one.ino(), two.ino());
    assert_ne!(
        one.ino(),
        fs::metadata(source.join("binary")).unwrap().ino()
    );
    assert_ne!(one.permissions().mode() & 0o111, 0);
    fs::remove_dir_all(&source).unwrap();
    fs::rename(&destination, fixture.0.join("moved")).unwrap();
    assert_eq!(
        fs::read_to_string(fixture.0.join("moved/one")).unwrap(),
        "executable"
    );
}

#[cfg(unix)]
#[test]
fn runfiles_symlink_escape_and_cycles_fail_closed() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let source = fixture.0.join("source");
    fs::create_dir(&source).unwrap();
    fs::write(fixture.0.join("outside"), "unapproved").unwrap();
    symlink(fixture.0.join("outside"), source.join("escape")).unwrap();
    let roots = [fs::canonicalize(&source).unwrap()];
    assert!(
        integration_shards::copy_tree(
            &source.join("escape"),
            &fixture.0.join("copy"),
            &roots,
            &mut BTreeMap::new(),
            &mut BTreeSet::new()
        )
        .is_err()
    );
    fs::remove_file(source.join("escape")).unwrap();
    symlink(&source, source.join("cycle")).unwrap();
    assert!(
        integration_shards::copy_tree(
            &source,
            &fixture.0.join("cycle-copy"),
            &roots,
            &mut BTreeMap::new(),
            &mut BTreeSet::new()
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn supervisor_receives_disposable_home_contract_and_owns_cleanup() {
    let fixture = Fixture::new();
    let artifact = fixture.artifact();
    fs::create_dir_all(artifact.join("gatling.runfiles/_main")).unwrap();
    let record = artifact.join("scratch-record");
    // Exercise the existing supervisor CLI/environment contract with a fixture
    // supervisor; build_lifecycle's own suite verifies its implementation.
    script(
        &artifact.join("build_lifecycle"),
        &format!(
            "[ \"$1\" = '--' ]\nshift\n[ \"$HOME\" = \"$TEST_TMPDIR\" ]\nscratch=$(mktemp -d \"$TEST_TMPDIR/shard-supervisor.XXXXXX\")\nprintf '%s' \"$scratch\" > {record:?}\nexport HOME=\"$scratch\" TEST_TMPDIR=\"$scratch\" TMPDIR=\"$scratch\"\n\"$@\"\nrm -r \"$scratch\""
        ),
    );
    script(
        &artifact.join("gatling"),
        "[ \"$HOME\" = \"$TEST_TMPDIR\" ]\n[ \"$HOME\" = \"$TMPDIR\" ]\nprintf fixture > \"$HOME/owned-output\"",
    );
    let mut command = integration_shards::test_command(&artifact, "gatling").unwrap();
    let env: BTreeMap<_, _> = command.get_envs().collect();
    let parent = std::env::temp_dir();
    assert_eq!(
        env.get(std::ffi::OsStr::new("HOME")),
        Some(&Some(parent.as_os_str()))
    );
    assert_eq!(
        env.get(std::ffi::OsStr::new("TEST_TMPDIR")),
        Some(&Some(parent.as_os_str()))
    );
    assert!(command.status().unwrap().success());
    let scratch = PathBuf::from(fs::read_to_string(record).unwrap());
    assert_ne!(scratch, parent);
    assert!(!scratch.exists());
    assert!(parent.exists());
}
