//! Select, package once, and run the PR integration tests without rebuilding.
//!
//! Compile with `rustc --edition=2024 assets/tools/integration_shards.rs`.
//! The changed-paths file comes from an immutable Git diff (newline or NUL
//! separated). A staged artifact is self-contained: no Bazel output symlinks or
//! stale runfiles manifests are used by the test-only workers.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus};

pub const GATLING_SHARDS: usize = 4;
const CONFORMANCE: [&str; 4] = [
    "contract_conformance",
    "constitution_claims",
    "spec_conformance",
    "substrate_integrity",
];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub gatling: bool,
    pub policies: Vec<&'static str>,
    pub conformance: bool,
}

pub fn parse_paths(bytes: &[u8]) -> io::Result<Vec<String>> {
    let text = std::str::from_utf8(bytes).map_err(io::Error::other)?;
    let separator = if text.contains('\0') { '\0' } else { '\n' };
    let mut paths = BTreeSet::new();
    for path in text.split(separator).filter(|path| !path.is_empty()) {
        if path.contains(['\n', '\r', '\0'])
            || Path::new(path)
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
            || path.starts_with('-')
        {
            return Err(io::Error::other(format!("invalid changed path: {path:?}")));
        }
        paths.insert(path.to_string());
    }
    Ok(paths.into_iter().collect())
}

pub fn plan(paths: &[String]) -> Plan {
    let expanded;
    let paths = if paths.iter().any(|path| {
        path.starts_with("assets/tools/source_ci/")
            || [
                "assets/tools/source_ci_workflow_owned.rs",
                "assets/tools/source_ci_custody.rs",
                "assets/tools/source_ci_toolchain.rs",
            ]
            .contains(&path.as_str())
    }) {
        expanded = paths
            .iter()
            .cloned()
            .chain(std::iter::once(".github/workflows/ci.yml".to_string()))
            .collect::<Vec<_>>();
        &expanded
    } else {
        paths
    };
    let has = |names: &[&str]| paths.iter().any(|path| names.contains(&path.as_str()));
    let mut result = Plan {
        gatling: has(&["tests/gatling.rs"]),
        // Keep the previous `grep 'src/'` selection, including nested source
        // paths. Selection must never narrow existing coverage accidentally.
        conformance: paths.iter().any(|path| path.contains("src/")),
        ..Plan::default()
    };
    if has(&[
        "assets/tools/release_metadata.rs",
        "assets/tools/release_projection_drift.rs",
        "assets/tools/release_projection_drift_tests.rs",
        "tests/release_metadata_policy.rs",
        "tests/release_projection_drift_policy.rs",
        ".github/workflows/ci.yml",
    ]) {
        result
            .policies
            .extend(["release_metadata_policy", "release_projection_drift_policy"]);
    }
    if has(&[
        "Cargo.toml",
        "dist.toml",
        "dist-workspace.toml",
        "tests/release_distribution_policy.rs",
    ]) {
        result.policies.push("release_distribution_policy");
    }
    if has(&[
        "assets/tools/integration_shards.rs",
        "tests/integration_sharding_policy.rs",
        ".github/workflows/ci.yml",
        ".config/build/decapod.BUILD.bzl",
        "BUILD.bazel",
    ]) {
        result.policies.push("integration_sharding_policy");
    }
    if has(&[
        ".github/actions/validation-checkout/action.yml",
        ".github/workflows/ci.yml",
        ".github/workflows/decapod-validate.yml",
        "tests/ci_checkout_policy.rs",
    ]) {
        result.policies.push("ci_checkout_policy");
    }
    if has(&[
        "assets/tools/integration_custody.rs",
        "tests/integration_custody_policy.rs",
        ".github/workflows/ci.yml",
        ".config/build/decapod.BUILD.bzl",
    ]) {
        result.policies.push("integration_custody_policy");
    }
    if has(&[
        "assets/tools/ci_event_context.rs",
        "tests/ci_event_context_policy.rs",
        ".github/workflows/ci.yml",
    ]) {
        result.policies.push("ci_event_context_policy");
    }
    if has(&[
        "assets/tools/docs_sync.rs",
        "tests/docs_sync_checkpoint_boundary.rs",
        "tests/support/docs_sync_cases.rs",
        ".github/actions/docs-drift/action.yml",
        ".github/workflows/docs_sync.yml",
        "src/decapod/core/docs_cli.rs",
    ]) {
        result.policies.push("docs_sync_checkpoint_boundary");
    }
    if has(&[
        ".github/workflows/release.yml",
        ".github/workflows/ci.yml",
        ".github/scripts/release-plan.sh",
        "tests/release_workflow_policy.rs",
    ]) {
        result.policies.push("release_workflow_policy");
    }
    if paths
        .iter()
        .any(|path| path.starts_with(".github/workflows/") && path.ends_with(".yml"))
        || has(&[
            "assets/tools/source_ci_profile.rs",
            ".github/actions/source-ci-profile/action.yml",
            "tests/source_ci_profile_policy.rs",
            ".config/build/decapod.BUILD.bzl",
            "BUILD.bazel",
        ])
    {
        result.policies.push("source_ci_profile_policy");
    }
    result
}

impl Plan {
    pub fn shards(&self) -> Vec<String> {
        let mut shards = Vec::new();
        if self.gatling {
            shards.extend((0..GATLING_SHARDS).map(|index| format!("gatling-{index}")));
        }
        if !self.policies.is_empty() {
            shards.push("release-policies".to_string());
        }
        if self.conformance {
            shards.extend(CONFORMANCE.map(str::to_string));
        }
        shards
    }

    pub fn targets(&self) -> Vec<&'static str> {
        let mut targets = Vec::new();
        if self.gatling {
            targets.push("gatling");
        }
        targets.extend(self.policies.iter().copied());
        if self.conformance {
            targets.extend(CONFORMANCE);
        }
        targets
    }

    pub fn matrix(&self) -> String {
        // Shard identifiers are fixed program constants, never untrusted paths.
        format!(
            "{{\"include\":[{}]}}",
            self.shards()
                .iter()
                .map(|shard| format!("{{\"shard\":\"{shard}\"}}"))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

/// `--list --format terse` includes ignored tests. Keep those names in the
/// partition, but do not change libtest's original ignored-test execution policy.
pub fn parse_inventory(text: &str) -> io::Result<Vec<String>> {
    let mut names = BTreeSet::new();
    for line in text.lines().filter(|line| !line.is_empty()) {
        let name = line
            .strip_suffix(": test")
            .ok_or_else(|| io::Error::other(format!("unexpected libtest inventory: {line:?}")))?;
        if name.is_empty() || name.starts_with('-') || !names.insert(name.to_string()) {
            return Err(io::Error::other(format!(
                "invalid or duplicate test: {name:?}"
            )));
        }
    }
    if names.is_empty() {
        return Err(io::Error::other("empty Gatling test inventory"));
    }
    Ok(names.into_iter().collect())
}

pub fn partition_inventory(names: &[String]) -> io::Result<Vec<Vec<String>>> {
    if names.is_empty() || names.iter().collect::<BTreeSet<_>>().len() != names.len() {
        return Err(io::Error::other("empty or duplicate Gatling inventory"));
    }
    let mut sorted = names.to_vec();
    sorted.sort();
    let mut shards = vec![Vec::new(); GATLING_SHARDS];
    let mut next = 2;
    for name in sorted {
        // The two long lifecycle tests must not share a worker. Other tests,
        // including unnumbered helper regressions and future additions, all
        // participate in the same exhaustive deterministic partition.
        let shard = match name.as_str() {
            "t040_todo_lifecycle" => 0,
            "t292_decide_session_lifecycle" => 1,
            _ => {
                let shard = next % GATLING_SHARDS;
                next += 1;
                shard
            }
        };
        shards[shard].push(name);
    }
    Ok(shards)
}

fn successful(status: ExitStatus, operation: &str) -> io::Result<()> {
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("{operation} failed: {status}")))
    }
}

/// Dereference only files inside the checked-out workspace or Bazel output base.
/// Copy each resolved file once, then hard-link duplicates *inside* the artifact.
/// Never link the artifact to mutable build outputs, or retain external symlinks.
pub fn copy_tree(
    source: &Path,
    destination: &Path,
    roots: &[PathBuf],
    copied: &mut BTreeMap<PathBuf, PathBuf>,
    ancestors: &mut BTreeSet<PathBuf>,
) -> io::Result<()> {
    let resolved = fs::canonicalize(source)?;
    if !roots.iter().any(|root| resolved.starts_with(root)) {
        return Err(io::Error::other(format!(
            "artifact source escapes approved workspace/Bazel roots: {}",
            source.display()
        )));
    }
    let metadata = fs::metadata(&resolved)?;
    if metadata.is_file() {
        if let Some(existing) = copied.get(&resolved) {
            fs::hard_link(existing, destination)?;
        } else {
            fs::copy(&resolved, destination)?;
            copied.insert(resolved, destination.to_path_buf());
        }
    } else if metadata.is_dir() {
        if !ancestors.insert(resolved.clone()) {
            return Err(io::Error::other("cyclic runfiles directory"));
        }
        fs::create_dir(destination)?;
        for entry in fs::read_dir(&resolved)? {
            let entry = entry?;
            copy_tree(
                &entry.path(),
                &destination.join(entry.file_name()),
                roots,
                copied,
                ancestors,
            )?;
        }
        ancestors.remove(&resolved);
    } else {
        return Err(io::Error::other("unsupported runfiles entry type"));
    }
    Ok(())
}

fn bazel_path(key: &str) -> io::Result<PathBuf> {
    let output = Command::new("bazelisk").args(["info", key]).output()?;
    successful(output.status, "bazelisk info")?;
    let value = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    let path = PathBuf::from(value.trim());
    fs::canonicalize(path)
}

pub fn prepare(paths_file: &Path, output: &Path) -> io::Result<()> {
    let bytes = fs::read(paths_file)?;
    let selection = plan(&parse_paths(&bytes)?);
    if selection.targets().is_empty() {
        return Err(io::Error::other("no integration targets selected"));
    }
    if output.exists() {
        return Err(io::Error::other(
            "artifact directory must not already exist",
        ));
    }
    let labels: Vec<_> = selection
        .targets()
        .iter()
        .map(|target| format!("//:{target}"))
        .chain(std::iter::once("//:build_lifecycle".to_string()))
        .collect();
    // All compilation happens once here, outside the test-only matrix steps.
    successful(
        Command::new("bazelisk")
            .arg("build")
            .args(&labels)
            .status()?,
        "shared integration build",
    )?;
    let bazel_bin = bazel_path("bazel-bin")?;
    let roots = vec![
        fs::canonicalize(env::current_dir()?)?,
        bazel_path("output_base")?,
    ];
    fs::create_dir_all(output)?;
    let output = fs::canonicalize(output)?;
    let mut copied = BTreeMap::new();
    let mut ancestors = BTreeSet::new();
    for target in selection.targets() {
        copy_tree(
            &bazel_bin.join(target),
            &output.join(target),
            &roots,
            &mut copied,
            &mut ancestors,
        )?;
        copy_tree(
            &bazel_bin.join(format!("{target}.runfiles")),
            &output.join(format!("{target}.runfiles")),
            &roots,
            &mut copied,
            &mut ancestors,
        )?;
    }
    copy_tree(
        &bazel_bin.join("build_lifecycle"),
        &output.join("build_lifecycle"),
        &roots,
        &mut copied,
        &mut ancestors,
    )?;
    fs::copy(env::current_exe()?, output.join("integration-shards"))?;
    fs::write(output.join("changed-paths.txt"), bytes)?;
    fs::write(output.join("matrix.json"), selection.matrix())?;
    // Verify the packaged executable, not a source approximation, before upload.
    if selection.gatling {
        let inventory = inventory(&output)?;
        let partition = partition_inventory(&inventory)?;
        if partition.iter().any(Vec::is_empty) {
            return Err(io::Error::other("empty Gatling shard"));
        }
        fs::write(output.join("gatling-inventory.txt"), inventory.join("\n"))?;
    }
    Ok(())
}

fn contained(root: &Path, path: &Path) -> io::Result<PathBuf> {
    let resolved = fs::canonicalize(path)?;
    if !resolved.starts_with(root) {
        return Err(io::Error::other("artifact path escapes its directory"));
    }
    Ok(resolved)
}

pub fn test_command(artifact: &Path, target: &str) -> io::Result<Command> {
    let root = fs::canonicalize(artifact)?;
    let executable = contained(&root, &root.join(target))?;
    let supervisor = contained(&root, &root.join("build_lifecycle"))?;
    let runfiles = contained(&root, &root.join(format!("{target}.runfiles")))?;
    let cwd = contained(&root, &runfiles.join("_main"))?;
    let mut command = Command::new(supervisor);
    command
        .arg("--")
        .arg(executable)
        .current_dir(cwd)
        .env("RUNFILES_DIR", &runfiles)
        .env("TEST_SRCDIR", &runfiles)
        .env("TEST_WORKSPACE", "_main")
        // Match Bazel's disposable HOME contract. The supervisor replaces both
        // equal parent values with its invocation-owned scratch, then cleans it.
        // No files are created or removed in this shared parent by the runner.
        .env("HOME", env::temp_dir())
        .env("TEST_TMPDIR", env::temp_dir())
        .env_remove("RUNFILES_MANIFEST_FILE")
        // Bazel's sharding variables must not apply a second, hidden partition.
        .env_remove("TEST_TOTAL_SHARDS")
        .env_remove("TEST_SHARD_INDEX")
        .env_remove("TEST_SHARD_STATUS_FILE");
    Ok(command)
}

fn inventory_output_excerpt(bytes: &[u8]) -> String {
    const LIMIT: usize = 4096;
    let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(LIMIT)]);
    if bytes.len() > LIMIT {
        format!("{preview:?} (truncated; {} bytes total)", bytes.len())
    } else {
        format!("{preview:?}")
    }
}

pub fn inventory(artifact: &Path) -> io::Result<Vec<String>> {
    let output = test_command(artifact, "gatling")?
        .args(["--list", "--format", "terse"])
        .output()?;
    if !output.status.success() {
        // Keep the real failure and bounded, escaped stream excerpts. In
        // particular, a shell's exit 126 alone loses its exec error detail.
        return Err(io::Error::other(format!(
            "supervised Gatling inventory failed: {}; stdout={}; stderr={}",
            output.status,
            inventory_output_excerpt(&output.stdout),
            inventory_output_excerpt(&output.stderr),
        )));
    }
    parse_inventory(&String::from_utf8(output.stdout).map_err(io::Error::other)?)
}

pub fn run_shard(artifact: &Path, shard: &str) -> io::Result<()> {
    let selection = plan(&parse_paths(&fs::read(
        artifact.join("changed-paths.txt"),
    )?)?);
    if !selection.shards().iter().any(|active| active == shard) {
        return Err(io::Error::other(format!(
            "inactive or unknown shard: {shard}"
        )));
    }
    if let Some(index) = shard.strip_prefix("gatling-") {
        let index: usize = index.parse().map_err(io::Error::other)?;
        let names = inventory(artifact)?;
        let prepared = fs::read_to_string(artifact.join("gatling-inventory.txt"))?;
        if prepared != names.join("\n") {
            return Err(io::Error::other(
                "Gatling inventory changed after preparation",
            ));
        }
        let partition = partition_inventory(&names)?;
        let filters = &partition[index];
        if filters.is_empty() {
            return Err(io::Error::other("empty Gatling shard"));
        }
        eprintln!(
            "{shard}: {} tests in complete inventory {}",
            filters.len(),
            names.len()
        );
        successful(
            test_command(artifact, "gatling")?
                .args(["--test-threads=4", "--exact"])
                .args(filters)
                .status()?,
            shard,
        )?;
    } else {
        let targets = if shard == "release-policies" {
            selection.policies
        } else {
            vec![
                CONFORMANCE
                    .iter()
                    .copied()
                    .find(|target| *target == shard)
                    .ok_or_else(|| io::Error::other("unknown conformance target"))?,
            ]
        };
        let threads = if shard == "release-policies" {
            "--test-threads=4"
        } else {
            "--test-threads=1"
        };
        // Run every selected binary even if an earlier one fails, preserving
        // independent failure evidence in this shared policy worker.
        let mut failures = Vec::new();
        for target in targets {
            if let Err(error) = successful(
                test_command(artifact, target)?.arg(threads).status()?,
                target,
            ) {
                eprintln!("{error}");
                failures.push(target);
            }
        }
        if !failures.is_empty() {
            return Err(io::Error::other(format!(
                "failed targets: {}",
                failures.join(", ")
            )));
        }
    }
    Ok(())
}

fn argument(args: &[OsString], name: &str) -> io::Result<PathBuf> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| PathBuf::from(&pair[1]))
        .ok_or_else(|| io::Error::other(format!("missing {name}")))
}

fn main() {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let result = (|| -> io::Result<()> {
        match args.first().and_then(|arg| arg.to_str()) {
            Some("plan") if args.len() == 3 => {
                println!(
                    "{}",
                    plan(&parse_paths(&fs::read(argument(&args, "--paths")?)?)?).matrix()
                );
                Ok(())
            }
            Some("prepare") if args.len() == 5 => {
                prepare(&argument(&args, "--paths")?, &argument(&args, "--output")?)
            }
            Some("run") if args.len() == 5 => run_shard(
                &argument(&args, "--artifact")?,
                argument(&args, "--shard")?
                    .to_str()
                    .ok_or_else(|| io::Error::other("non-UTF-8 shard"))?,
            ),
            _ => Err(io::Error::other(
                "usage: integration-shards plan --paths FILE | prepare --paths FILE --output DIR | run --artifact DIR --shard ID",
            )),
        }
    })();
    if let Err(error) = result {
        eprintln!("integration shards: {error}");
        std::process::exit(1);
    }
}
