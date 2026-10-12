//! Reviewed source-CI command profile shared by the workflow and native proof.
//!
//! Compile with `rustc --edition=2024 assets/tools/source_ci_profile.rs`.
//! This is a finite command catalog, not a YAML or shell interpreter. Workflow
//! setup, credentials, artifacts, and the bounded process supervisor remain
//! outside this module. A successful check is never an entire PR readiness proof.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

#[allow(dead_code)]
#[path = "integration_shards.rs"]
mod integration_shards;

#[allow(dead_code)]
#[path = "source_ci_custody.rs"]
pub mod custody;

#[allow(dead_code)]
#[path = "source_ci_workflow_owned.rs"]
pub mod workflow_owned;

pub const SCHEMA: &str = "decapod.source-ci.v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    LinuxX86_64,
    MacosAarch64,
}

impl Platform {
    pub fn name(self) -> &'static str {
        match self {
            Self::LinuxX86_64 => "linux-x86_64",
            Self::MacosAarch64 => "macos-aarch64",
        }
    }
    pub fn matches(self, os: &str, arch: &str) -> bool {
        match self {
            Self::LinuxX86_64 => os == "linux" && arch == "x86_64",
            Self::MacosAarch64 => os == "macos" && arch == "aarch64",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Always,
    Core,
    Plugins,
    Cli,
    Migration,
    Docs,
    Integration,
}

#[derive(Clone, Copy, Debug)]
pub struct Check {
    pub id: &'static str,
    pub workflow: &'static str,
    pub job: &'static str,
    pub matrix: Option<&'static str>,
    pub platform: Platform,
    /// Executables must exist; other requirements describe setup that must be
    /// supplied by the caller and is ultimately exercised by the actual command.
    pub capabilities: &'static [&'static str],
    pub selection: Selection,
    /// Release-only PRs still compile both musl targets and the shared binary.
    pub skipped_for_release_only: bool,
}

macro_rules! check {
    ($id:literal, $job:literal, $caps:expr) => {
        Check {
            id: $id,
            workflow: "ci.yml",
            job: $job,
            matrix: None,
            platform: Platform::LinuxX86_64,
            capabilities: $caps,
            selection: Selection::Always,
            skipped_for_release_only: true,
        }
    };
}

pub const CHECKS: &[Check] = &[
    check!("fmt", "fmt", &["cargo", "rustfmt", "rustc:1.96.1"]),
    check!(
        "clippy",
        "clippy",
        &["cargo", "cargo-clippy", "rustc:1.96.1"]
    ),
    check!(
        "dependencies-machete",
        "deps-lint",
        &["cargo", "cargo-machete", "rustc:1.96.1"]
    ),
    check!(
        "dependencies-deny",
        "deps-lint",
        &["cargo", "cargo-deny", "rustc:1.96.1"]
    ),
    Check {
        skipped_for_release_only: false,
        ..check!("setup-bazel", "setup", &["bazelisk", "python3"])
    },
    check!("rpc-suite", "test-rpc-suite", &["bazelisk"]),
    check!("chaos-replay", "chaos-replay", &["bazelisk"]),
    check!(
        "daemonless-lifecycle",
        "daemonless-lifecycle",
        &["bazelisk"]
    ),
    check!("contracts", "contracts", &["bazelisk"]),
    check!("cli-contracts", "contracts", &["bazelisk"]),
    Check {
        selection: Selection::Migration,
        ..check!("migration-tests", "migration-script-tests", &["bazelisk"])
    },
    Check {
        selection: Selection::Core,
        matrix: Some("core"),
        ..check!("selective-core", "test", &["bazelisk"])
    },
    Check {
        selection: Selection::Plugins,
        matrix: Some("plugins"),
        ..check!("selective-plugins", "test", &["bazelisk"])
    },
    Check {
        selection: Selection::Cli,
        matrix: Some("cli"),
        ..check!("selective-cli", "test", &["bazelisk"])
    },
    check!(
        "supabase-consumer",
        "supabase-consumer",
        &["cargo", "rustc:1.96.1"]
    ),
    check!(
        "workspace-lifecycle-docker",
        "workspace-lifecycle-docker",
        &[
            "cargo",
            "docker",
            "rustc:1.96.1",
            "docker-daemon",
            "image:busybox:1.37"
        ]
    ),
    Check {
        matrix: Some("x86_64-unknown-linux-musl"),
        skipped_for_release_only: false,
        ..check!(
            "musl-build-x86_64",
            "musl-publication",
            &[
                "cargo",
                "musl-gcc",
                "rustc:1.96.1",
                "rust-target:x86_64-unknown-linux-musl"
            ]
        )
    },
    Check {
        matrix: Some("x86_64-unknown-linux-musl"),
        ..check!(
            "musl-test-x86_64",
            "musl-publication",
            &[
                "cargo",
                "musl-gcc",
                "rustc:1.96.1",
                "rust-target:x86_64-unknown-linux-musl"
            ]
        )
    },
    Check {
        matrix: Some("aarch64-unknown-linux-musl"),
        skipped_for_release_only: false,
        ..check!(
            "musl-build-aarch64",
            "musl-publication",
            &[
                "cross",
                "cargo",
                "docker",
                "rustc:1.96.1",
                "docker-daemon",
                "cross-rev:74e99b2223de06166f8cdcc1af7f1308ec52baf4",
                "qemu-aarch64-in-pinned-image"
            ]
        )
    },
    Check {
        matrix: Some("aarch64-unknown-linux-musl"),
        ..check!(
            "musl-test-aarch64",
            "musl-publication",
            &[
                "cross",
                "cargo",
                "docker",
                "rustc:1.96.1",
                "docker-daemon",
                "cross-rev:74e99b2223de06166f8cdcc1af7f1308ec52baf4",
                "qemu-aarch64-in-pinned-image"
            ]
        )
    },
    Check {
        matrix: Some("ubuntu-latest"),
        ..check!(
            "golden-vectors-linux",
            "state-commit-golden-vectors",
            &["cargo", "git", "rustc:1.96.1", "submodules:recursive"]
        )
    },
    Check {
        matrix: Some("macos-latest"),
        platform: Platform::MacosAarch64,
        ..check!(
            "golden-vectors-macos",
            "state-commit-golden-vectors",
            &["cargo", "git", "rustc:1.96.1", "submodules:recursive"]
        )
    },
    Check {
        workflow: "docs.yml",
        skipped_for_release_only: false,
        ..check!("docs-build", "build", &["mdbook", "mdbook:0.4.40"])
    },
    Check {
        workflow: "constitution-sync.yml",
        skipped_for_release_only: false,
        ..check!(
            "constitution-schema",
            "validate-constitution",
            &["ajv", "node:20"]
        )
    },
    Check {
        id: "release-metadata",
        workflow: "ci.yml",
        job: "release-metadata",
        matrix: None,
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git", "rustc:1.96.1"],
        selection: Selection::Always,
        skipped_for_release_only: false,
    },
    Check {
        id: "commit-lint",
        workflow: "ci.yml",
        job: "commit-lint",
        matrix: None,
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git", "grep"],
        selection: Selection::Always,
        skipped_for_release_only: true,
    },
    Check {
        id: "governance-artifacts",
        workflow: "ci.yml",
        job: "governance-artifacts",
        matrix: None,
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git"],
        selection: Selection::Always,
        skipped_for_release_only: true,
    },
    Check {
        id: "material-specs",
        workflow: "ci.yml",
        job: "material-specs",
        matrix: None,
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git", "python3"],
        selection: Selection::Always,
        skipped_for_release_only: true,
    },
    Check {
        id: "migration-detection",
        workflow: "ci.yml",
        job: "detect-migration-script-changes",
        matrix: None,
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git"],
        selection: Selection::Always,
        skipped_for_release_only: true,
    },
    Check {
        id: "health",
        workflow: "ci.yml",
        job: "health",
        matrix: None,
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git", "sed", "head", "grep"],
        selection: Selection::Always,
        skipped_for_release_only: false,
    },
    Check {
        id: "integration-prepare",
        workflow: "ci.yml",
        job: "integration-prepare",
        matrix: None,
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git", "rustc:1.96.1", "bazelisk", "jq", "tar"],
        selection: Selection::Always,
        skipped_for_release_only: true,
    },
    Check {
        id: "integration-status",
        workflow: "ci.yml",
        job: "integration-status",
        matrix: None,
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "sha256sum"],
        selection: Selection::Always,
        skipped_for_release_only: true,
    },
    Check {
        id: "standalone-validation",
        workflow: "decapod-validate.yml",
        job: "validate",
        matrix: None,
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git", "cargo", "rustc:1.96.1", "sed", "head"],
        selection: Selection::Always,
        skipped_for_release_only: false,
    },
    Check {
        id: "docs-drift",
        workflow: "docs_sync.yml",
        job: "sync-docs",
        matrix: None,
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git", "cargo", "rustc"],
        selection: Selection::Docs,
        skipped_for_release_only: false,
    },
    Check {
        id: "integration-gatling-0",
        workflow: "ci.yml",
        job: "test-integration",
        matrix: Some("gatling-0"),
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git", "tar", "sha256sum", "timeout"],
        selection: Selection::Integration,
        skipped_for_release_only: true,
    },
    Check {
        id: "integration-gatling-1",
        workflow: "ci.yml",
        job: "test-integration",
        matrix: Some("gatling-1"),
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git", "tar", "sha256sum", "timeout"],
        selection: Selection::Integration,
        skipped_for_release_only: true,
    },
    Check {
        id: "integration-gatling-2",
        workflow: "ci.yml",
        job: "test-integration",
        matrix: Some("gatling-2"),
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git", "tar", "sha256sum", "timeout"],
        selection: Selection::Integration,
        skipped_for_release_only: true,
    },
    Check {
        id: "integration-gatling-3",
        workflow: "ci.yml",
        job: "test-integration",
        matrix: Some("gatling-3"),
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git", "tar", "sha256sum", "timeout"],
        selection: Selection::Integration,
        skipped_for_release_only: true,
    },
    Check {
        id: "integration-release-policies",
        workflow: "ci.yml",
        job: "test-integration",
        matrix: Some("release-policies"),
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git", "tar", "sha256sum", "timeout"],
        selection: Selection::Integration,
        skipped_for_release_only: true,
    },
    Check {
        id: "integration-contract_conformance",
        workflow: "ci.yml",
        job: "test-integration",
        matrix: Some("contract_conformance"),
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git", "tar", "sha256sum", "timeout"],
        selection: Selection::Integration,
        skipped_for_release_only: true,
    },
    Check {
        id: "integration-constitution_claims",
        workflow: "ci.yml",
        job: "test-integration",
        matrix: Some("constitution_claims"),
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git", "tar", "sha256sum", "timeout"],
        selection: Selection::Integration,
        skipped_for_release_only: true,
    },
    Check {
        id: "integration-spec_conformance",
        workflow: "ci.yml",
        job: "test-integration",
        matrix: Some("spec_conformance"),
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git", "tar", "sha256sum", "timeout"],
        selection: Selection::Integration,
        skipped_for_release_only: true,
    },
    Check {
        id: "integration-substrate_integrity",
        workflow: "ci.yml",
        job: "test-integration",
        matrix: Some("substrate_integrity"),
        platform: Platform::LinuxX86_64,
        capabilities: &["bash", "git", "tar", "sha256sum", "timeout"],
        selection: Selection::Integration,
        skipped_for_release_only: true,
    },
];

pub fn find_check(id: &str) -> io::Result<&'static Check> {
    CHECKS
        .iter()
        .find(|check| check.id == id)
        .ok_or_else(|| io::Error::other(format!("unknown source-CI check: {id}")))
}

/// Preserve the previous unanchored grep semantics, including nested fixtures.
pub fn core_full(paths: &[String]) -> bool {
    const AREAS: &[&str] = &[
        "todo",
        "validate",
        "workspace",
        "workunit",
        "obligation",
        "docs",
        "capsule",
        "rpc",
        "migration",
        "gateway",
    ];
    paths.iter().any(|path| {
        AREAS
            .iter()
            .any(|area| path.contains(&format!("src/decapod/core/{area}")))
    })
}

pub fn selected(check: &Check, paths: &[String], release_only: bool) -> bool {
    if release_only && check.skipped_for_release_only {
        return false;
    }
    match check.selection {
        Selection::Always => true,
        Selection::Core => paths.iter().any(|p| p.contains("src/decapod/core/")),
        Selection::Plugins => paths.iter().any(|p| p.contains("src/decapod/plugins/")),
        Selection::Cli => paths.iter().any(|p| {
            ["src/decapod/cli", "src/decapod/lib", "src/main"]
                .iter()
                .any(|part| p.contains(part))
        }),
        Selection::Migration => migration_changed(paths),
        Selection::Docs => docs_sync_changed(paths),
        Selection::Integration => integration_shards::plan(paths)
            .shards()
            .iter()
            .any(|s| Some(s.as_str()) == check.matrix),
    }
}

pub fn migration_changed(paths: &[String]) -> bool {
    paths
        .iter()
        .any(|p| p.starts_with("src/decapod/core/sql/") && p.ends_with(".sql"))
}

pub fn docs_sync_changed(paths: &[String]) -> bool {
    paths.iter().any(|p| {
        (p.starts_with("src/") && p.ends_with(".rs"))
            || (p.starts_with("docs/") && p.ends_with(".md"))
            || p.starts_with(".github/actions/docs-drift/")
            || p.starts_with(".github/actions/ci-context/")
            || p.starts_with(".github/actions/source-ci-profile/")
            || p.starts_with("assets/tools/source_ci/")
            || [
                "assets/schemas/trajectory.schema.json",
                "assets/tools/docs_sync.rs",
                "assets/tools/ci_event_context.rs",
                "assets/tools/source_ci_profile.rs",
                ".github/workflows/docs_sync.yml",
                "tests/docs_sync_checkpoint_boundary.rs",
                "tests/support/docs_sync_cases.rs",
            ]
            .contains(&p.as_str())
    })
}

pub fn parse_paths(bytes: &[u8]) -> io::Result<Vec<String>> {
    let text = std::str::from_utf8(bytes).map_err(io::Error::other)?;
    let separator = if text.contains('\0') { '\0' } else { '\n' };
    let mut paths = BTreeSet::new();
    for path in text.split(separator).filter(|s| !s.is_empty()) {
        if path.contains(['\n', '\r', '\0'])
            || path.starts_with('-')
            || Path::new(path)
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(io::Error::other(format!("invalid changed path: {path:?}")));
        }
        paths.insert(path.to_string());
    }
    Ok(paths.into_iter().collect())
}

pub fn immutable_sha(sha: &str) -> bool {
    matches!(sha.len(), 40 | 64)
        && sha.bytes().all(|b| b.is_ascii_hexdigit())
        && sha.bytes().any(|b| b != b'0')
}

/// No fetch, mutable branch fallback, provider command, or head-parent fallback.
/// Missing immutable objects fail closed. The supervisor owns snapshot custody.
fn immutable_git(repo: &Path, args: &[&str]) -> io::Result<std::process::Output> {
    let mut command = Command::new("git");
    for (key, _) in env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .current_dir(repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .args([
            "--no-replace-objects",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
        ])
        .args(args)
        .output()
}

pub fn changed_paths(repo: &Path, base: &str, head: &str) -> io::Result<Vec<String>> {
    if !immutable_sha(base) || !immutable_sha(head) {
        return Err(io::Error::other(
            "base and head must be complete nonzero immutable commit SHAs",
        ));
    }
    let grafts = immutable_git(repo, &["rev-parse", "--git-path", "info/grafts"])?;
    if !grafts.status.success() {
        return Err(io::Error::other("Git repository unavailable"));
    }
    let grafts_path = String::from_utf8(grafts.stdout).map_err(io::Error::other)?;
    if repo.join(grafts_path.trim()).exists() {
        return Err(io::Error::other(
            "Git grafts are not immutable history evidence",
        ));
    }
    for sha in [base, head] {
        let output = immutable_git(repo, &["cat-file", "-t", sha])?;
        if !output.status.success() || output.stdout != b"commit\n" {
            return Err(io::Error::other("immutable commit unavailable"));
        }
    }
    let output = immutable_git(
        repo,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--name-only",
            "-z",
            &format!("{base}...{head}"),
            "--",
        ],
    )?;
    if !output.status.success() {
        return Err(io::Error::other("immutable changed-path diff failed"));
    }
    parse_paths(&output.stdout)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    pub program: &'static str,
    pub args: Vec<&'static str>,
    pub cwd: &'static str,
    pub env: Vec<(&'static str, Option<&'static str>)>,
}

impl Invocation {
    fn new(program: &'static str, args: &[&'static str]) -> Self {
        Self {
            program,
            args: args.to_vec(),
            cwd: ".",
            env: Vec::new(),
        }
    }
    fn at(mut self, cwd: &'static str) -> Self {
        self.cwd = cwd;
        self
    }
    fn with(mut self, key: &'static str, value: Option<&'static str>) -> Self {
        self.env.push((key, value));
        self
    }
}

fn bazel(args: &[&'static str]) -> Invocation {
    Invocation::new("bazelisk", args)
}
fn cargo(args: &[&'static str]) -> Invocation {
    Invocation::new("cargo", args)
}

const ARM_IMAGE: &str = "ghcr.io/cross-rs/aarch64-unknown-linux-musl@sha256:ca0f4a798b756bba34c9cd2f7ceafc1d665daa6c70349a95e2eb06fb687a1ea6";

pub fn commands(check: &Check, paths: &[String]) -> io::Result<Vec<Invocation>> {
    // Conditional checks with no matching source produce no successful command.
    if !selected(check, paths, false) && !workflow_owned::artifact_worker(check) {
        return Ok(Vec::new());
    }
    let mut result = if let Some(script) = workflow_owned::script(check) {
        vec![Invocation::new("bash", &[script])]
    } else {
        match check.id {
        "fmt" => vec![cargo(&["fmt", "--all", "--", "--check"])],
        "clippy" => vec![cargo(&["clippy", "--all-targets", "--all-features", "--", "-D", "warnings"]).with("CLIPPY_CONF_DIR", Some(".config"))],
        "dependencies-machete" => vec![cargo(&["machete"])],
        "dependencies-deny" => vec![cargo(&["deny", "--config", ".config/deny.toml", "check"])],
        "setup-bazel" => vec![bazel(&["test", "//:cargo_inputs_policy"]), bazel(&["build", "//:decapod", "//:release_projection_drift"])],
        "rpc-suite" => vec![bazel(&["test", "//:agent_rpc_suite", "--test_arg=--test-threads=1"])],
        "chaos-replay" => vec![bazel(&["test", "//:chaos_replay", "--test_arg=--test-threads=1"])],
        "daemonless-lifecycle" => vec![bazel(&["test", "//:daemonless_lifecycle", "--test_arg=--test-threads=1"])],
        "contracts" => vec![bazel(&["test", "//:contract_conformance", "--test_arg=--test-threads=1"]), bazel(&["test", "//:constitution_claims", "--test_arg=--test-threads=1"])],
        "cli-contracts" => vec![bazel(&["test", "//:cli_contract_enforcement", "--test_arg=--test-threads=1"])],
        "migration-tests" => vec![bazel(&["test", "//:core_tests", "--test_arg=migration_", "--test_arg=--ignored", "--test_arg=--test-threads=1"])],
        "selective-core" if core_full(paths) => vec![
            bazel(&["test", "//:group_broker", "--test_arg=committed_claim_preparation_does_not_hold_broker_or_repeat_on_retry", "--test_arg=--exact", "--test_arg=--nocapture", "--test_output=all"]),
            bazel(&["test", "//:group_broker", "--test_arg=broker_no_sqlite_busy_surfaced_under_concurrent_mutators", "--test_arg=--exact", "--test_arg=--nocapture", "--test_output=all"]),
            bazel(&["test", "//:todo_enforcement", "//:validate_termination", "//:workspace_interlock", "//:spec_workspace_ownership", "//:workspace_prune_regression", "//:hermes_governance_substrate", "//:group_broker", "//:group_broker_strict", "//:workunit_cli", "//:context_capsule_cli", "--test_arg=--test-threads=1"]),
        ],
        "selective-core" => vec![bazel(&["test", "//:entrypoint_correctness", "--test_arg=--test-threads=1"])],
        "selective-plugins" => vec![bazel(&["test", "//:plugins_todo_tests", "//:plugins_policy_tests", "//:plugins_health_tests", "//:plugins_aptitude_tests", "//:plugins_internalize_tests", "//:plugins_federation_tests", "//:plugins_decide_tests", "//:plugins_obligation_tests", "--test_arg=--test-threads=1"])],
        "selective-cli" => vec![bazel(&["test", "//:cli_contract_enforcement", "//:entrypoint_correctness", "//:cli_contracts", "--test_arg=--test-threads=1"])],
        "supabase-consumer" => vec![
            cargo(&["test", "--locked", "--features", "supabase-cloud", "--lib", "core::fs_permissions::tests::recovery_fixture_selection_rejects_incompatible_candidates", "--", "--exact", "--nocapture"]),
            cargo(&["test", "--locked", "--features", "supabase-cloud", "--lib", "--test", "cloud_supabase_boundary", "--test", "cloud_dactyl_boundary", "--test", "release_dependency_policy", "--test", "cloud_cli_boundary", "--test", "cloud_command_path", "--test", "cloud_todo_resume_wire", "--test", "propodus_contract", "--test", "supabase_service"]),
        ],
        "workspace-lifecycle-docker" => vec![
            Invocation::new("docker", &["pull", "busybox:1.37"]),
            cargo(&["test", "--locked", "--lib", "core::workspace_lifecycle::tests::docker_parent_interrupt_recovers_owned_container", "--", "--ignored", "--exact", "--nocapture"])
                .with("DECAPOD_RUN_DOCKER_LIFECYCLE_TEST", Some("1")).with("DECAPOD_TEST_DOCKER_IMAGE", Some("busybox:1.37")),
        ],
        "musl-build-x86_64" => vec![cargo(&["build", "--locked", "--release", "--target", "x86_64-unknown-linux-musl", "--bin", "decapod"])],
        "musl-test-x86_64" => vec![cargo(&["test", "--locked", "--release", "--target", "x86_64-unknown-linux-musl", "--lib", "no_replace_publication", "--", "--nocapture"])],
        "musl-build-aarch64" => vec![Invocation::new("cross", &["build", "--locked", "--release", "--target", "aarch64-unknown-linux-musl", "--bin", "decapod"])],
        "musl-test-aarch64" => vec![Invocation::new("cross", &["test", "--locked", "--release", "--target", "aarch64-unknown-linux-musl", "--lib", "no_replace_publication", "--", "--nocapture"])],
        "golden-vectors-linux" | "golden-vectors-macos" => vec![
            cargo(&["build", "--release"]).at("tests/golden_vectors").with("RUSTFLAGS", None),
            cargo(&["test", "--release"]).at("tests/golden_vectors").with("RUSTFLAGS", None),
            Invocation::new("git", &["init"]).at("tests/fixtures/state_commit_repo"),
            Invocation::new("git", &["config", "user.email", "test@test.com"]).at("tests/fixtures/state_commit_repo"),
            Invocation::new("git", &["config", "user.name", "Test"]).at("tests/fixtures/state_commit_repo"),
            // target.txt was added only to make the symlink packageable. It is
            // unchanged fixture background, not part of the immutable v1 delta.
            Invocation::new("git", &["add", "initial.txt", "target.txt"]).at("tests/fixtures/state_commit_repo"),
            Invocation::new("git", &["commit", "-m", "base commit"]).at("tests/fixtures/state_commit_repo"),
            Invocation::new("git", &["add", "."]).at("tests/fixtures/state_commit_repo"),
            Invocation::new("git", &["commit", "-m", "fixture commit"]).at("tests/fixtures/state_commit_repo"),
            Invocation::new("./target/release/golden_vectors", &["--check"]).at("tests/golden_vectors"),
        ],
        "docs-build" => vec![Invocation::new("mdbook", &["build", "docs/book"])],
        "constitution-schema" => vec![Invocation::new("ajv", &["validate", "-s", "assets/schemas/constitution.schema.json", "-d", "assets/constitution.json", "--strict=false"])],
        _ => return Err(io::Error::other("check lacks an implementation")),
    }
    };
    for command in &mut result {
        if check.workflow == "ci.yml" {
            command.env.push(("RUST_BACKTRACE", Some("1")));
        }
        if check.id == "supabase-consumer" {
            command.env.extend([
                ("GIT_CONFIG", None),
                ("GIT_CONFIG_PARAMETERS", None),
                ("GIT_CONFIG_GLOBAL", Some("/dev/null")),
                ("GIT_CONFIG_NOSYSTEM", Some("1")),
                ("GIT_CONFIG_COUNT", Some("0")),
            ]);
        }
        if check.id.starts_with("musl-") {
            command.env.extend([
                (
                    "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER",
                    Some("musl-gcc"),
                ),
                ("CROSS_CONTAINER_ENGINE", Some("docker")),
                (
                    "CROSS_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_IMAGE",
                    Some(ARM_IMAGE),
                ),
            ]);
        }
        if check.id.starts_with("golden-vectors-") {
            command.env.push((
                "DECAPOD_STATE_COMMIT_CI_JOB",
                Some("state-commit-golden-vectors"),
            ));
        }
    }
    Ok(result)
}

fn executable(program: &str) -> bool {
    env::split_paths(&env::var_os("PATH").unwrap_or_default()).any(|dir| {
        let path = dir.join(program);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            path.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        }
        #[cfg(not(unix))]
        {
            path.is_file()
        }
    })
}

fn preflight(check: &Check) -> io::Result<()> {
    if !check.platform.matches(env::consts::OS, env::consts::ARCH) {
        return Err(io::Error::other(format!(
            "required platform {}, actual {}-{}",
            check.platform.name(),
            env::consts::OS,
            env::consts::ARCH
        )));
    }
    for capability in check.capabilities {
        if !capability.contains(':')
            && !["docker-daemon", "qemu-aarch64-in-pinned-image"].contains(capability)
            && !executable(capability)
        {
            return Err(io::Error::other(format!(
                "missing required tool: {capability}"
            )));
        }
    }
    if check.capabilities.contains(&"rustc:1.96.1") {
        let version = Command::new("rustc").arg("--version").output()?;
        if !version.status.success()
            || !String::from_utf8_lossy(&version.stdout).starts_with("rustc 1.96.1 ")
        {
            return Err(io::Error::other(
                "source-CI Cargo checks require Rust 1.96.1",
            ));
        }
    }
    for (requirement, program, argument, expected) in [
        ("mdbook:0.4.40", "mdbook", "--version", "mdbook v0.4.40"),
        ("node:20", "node", "--version", "v20."),
        (
            "cross-rev:74e99b2223de06166f8cdcc1af7f1308ec52baf4",
            "cross",
            "--version",
            "74e99b2",
        ),
    ] {
        if check.capabilities.contains(&requirement) {
            let version = Command::new(program).arg(argument).output()?;
            if !version.status.success()
                || !String::from_utf8_lossy(&version.stdout).contains(expected)
            {
                return Err(io::Error::other(format!(
                    "cannot verify required capability: {requirement}"
                )));
            }
        }
    }
    Ok(())
}

pub fn requires_private_umask(check: &Check) -> bool {
    matches!(check.id, "supabase-consumer" | "workspace-lifecycle-docker")
}

// Restrict fixture permissions exactly like the existing shell steps. Applied
// only in this standalone short-lived process; native supervision stays outside.
#[cfg(unix)]
pub fn private_umask() {
    unsafe extern "C" {
        fn umask(mask: u32) -> u32;
    }
    unsafe {
        umask(0o077);
    }
}
#[cfg(not(unix))]
pub fn private_umask() {}

fn golden_preflight(repo: &Path) -> io::Result<()> {
    for path in [
        "tests/golden/state_commit/v1/scope_record.cbor",
        "tests/golden/state_commit/v1/scope_record.cbor.hex",
        "tests/golden/state_commit/v1/scope_record_hash.txt",
        "tests/golden/state_commit/v1/state_commit_root.txt",
    ] {
        if !repo.join(path).is_file() {
            return Err(io::Error::other(format!("missing golden vector: {path}")));
        }
    }
    Ok(())
}

fn golden_output_check(bytes: &[u8]) -> io::Result<()> {
    let output = String::from_utf8_lossy(bytes);
    for key in ["scope_record_hash:", "state_commit_root:"] {
        // Preserve output diagnostics as well as the generator's strict byte
        // comparison. A successful command alone must not omit either hash.
        if !output
            .lines()
            .any(|line| line.contains(key) && line.split_whitespace().nth(1).is_some())
        {
            return Err(io::Error::other(format!("golden generator omitted {key}")));
        }
    }
    Ok(())
}

fn golden_postflight(repo: &Path) -> io::Result<()> {
    let manifest = fs::read_to_string(repo.join("tests/golden_vectors/Cargo.toml"))?;
    if ["reqwest", "hyper", "ureq", "http"]
        .iter()
        .any(|s| manifest.contains(s))
    {
        return Err(io::Error::other("golden_vectors has network dependencies"));
    }
    Ok(())
}

pub fn json_string(s: &str) -> String {
    let mut result = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c < ' ' => result.push_str(&format!("\\u{:04x}", c as u32)),
            c => result.push(c),
        }
    }
    result.push('"');
    result
}
fn json_strings(items: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    format!(
        "[{}]",
        items
            .into_iter()
            .map(|s| json_string(s.as_ref()))
            .collect::<Vec<_>>()
            .join(",")
    )
}

pub fn invocation_json(command: &Invocation) -> String {
    format!(
        "{{\"program\":{},\"args\":{},\"cwd\":{},\"env\":{{{}}}}}",
        json_string(command.program),
        json_strings(&command.args),
        json_string(command.cwd),
        command
            .env
            .iter()
            .map(|(key, value)| format!(
                "{}:{}",
                json_string(key),
                value.map_or_else(|| "null".into(), json_string)
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
}

pub fn run_check(
    repo: &Path,
    check: &Check,
    paths: &[String],
    base: &str,
    head: &str,
) -> io::Result<bool> {
    let start = Instant::now();
    let mut results = Vec::new();
    let mut status = "passed";
    let mut failure = None;
    let recipe = commands(check, paths)?;
    if recipe.is_empty() {
        status = "not_applicable";
    }
    let execute = || -> io::Result<()> {
        preflight(check)?;
        if requires_private_umask(check) {
            private_umask();
        }
        if check.id.starts_with("golden-vectors-") {
            golden_preflight(repo)?;
        }
        Ok(())
    };
    if !recipe.is_empty()
        && let Err(error) = execute()
    {
        status = "unavailable";
        failure = Some(error.to_string());
    }
    if status == "passed" {
        for invocation in &recipe {
            let stage_start = Instant::now();
            let mut command = Command::new(invocation.program);
            command
                .args(&invocation.args)
                .current_dir(repo.join(invocation.cwd));
            for (key, value) in &invocation.env {
                if let Some(value) = value {
                    command.env(key, value);
                } else {
                    command.env_remove(key);
                }
            }
            let golden = invocation.program == "./target/release/golden_vectors";
            let outcome = if golden {
                command.output().and_then(|out| {
                    use std::io::Write;
                    io::stderr().write_all(&out.stdout)?;
                    io::stderr().write_all(&out.stderr)?;
                    if out.status.success() {
                        let mut combined = out.stdout;
                        combined.extend(out.stderr);
                        golden_output_check(&combined)?;
                    }
                    Ok(out.status)
                })
            } else {
                command
                    .stdout(Stdio::from(io::stderr()))
                    .stderr(Stdio::inherit())
                    .status()
            };
            match outcome {
                Ok(exit) => {
                    results.push(format!(
                        "{{\"invocation\":{},\"exit_code\":{},\"success\":{},\"elapsed_ms\":{}}}",
                        invocation_json(invocation),
                        exit.code().map_or_else(|| "null".into(), |c| c.to_string()),
                        exit.success(),
                        stage_start.elapsed().as_millis()
                    ));
                    if !exit.success() {
                        status = "failed";
                        break;
                    }
                }
                Err(error) => {
                    status = "failed";
                    failure = Some(error.to_string());
                    break;
                }
            }
        }
        if status == "passed"
            && check.id.starts_with("golden-vectors-")
            && let Err(error) = golden_postflight(repo)
        {
            status = "failed";
            failure = Some(error.to_string());
        }
    }
    println!(
        "{{\"schema\":{},\"check\":{},\"status\":{},\"base\":{},\"head\":{},\"platform\":{},\"results\":[{}],\"error\":{},\"elapsed_ms\":{}}}",
        json_string(SCHEMA),
        json_string(check.id),
        json_string(status),
        json_string(base),
        json_string(head),
        json_string(check.platform.name()),
        results.join(","),
        failure
            .as_deref()
            .map_or_else(|| "null".into(), json_string),
        start.elapsed().as_millis()
    );
    Ok(status == "passed" || status == "not_applicable")
}

/// The finite source catalog now implements each PR job obligation. Provider
/// setup/authentication and whole-PR aggregation remain independently required.
pub const WORKFLOW_OWNED_OBLIGATIONS: &[(&str, &str, &str)] = &[];

/// Exact PR job-instance inventory. A missing capability never removes an
/// obligation. Excluded workflows are named in tests, not silently ignored.
#[derive(Debug, Clone)]
pub struct JobInstance {
    pub workflow: &'static str,
    pub job: &'static str,
    pub matrix: Option<&'static str>,
    pub applicable: bool,
    pub checks: Vec<&'static str>,
    pub workflow_owned: bool,
}

pub fn inventory(paths: &[String], release_only: bool) -> Vec<JobInstance> {
    let mut result = Vec::new();
    let integrations = integration_shards::plan(paths).shards();
    let mut add = |workflow: &'static str,
                   job: &'static str,
                   matrix: Option<&'static str>,
                   applicable: bool| {
        let checks: Vec<_> = CHECKS
            .iter()
            .filter(|c| c.workflow == workflow && c.job == job && c.matrix == matrix)
            .map(|c| c.id)
            .collect();
        let workflow_owned = checks.is_empty();
        result.push(JobInstance {
            workflow,
            job,
            matrix,
            applicable,
            checks,
            workflow_owned,
        });
    };
    for job in [
        "release-metadata",
        "commit-lint",
        "governance-artifacts",
        "material-specs",
        "detect-migration-script-changes",
        "supabase-consumer",
        "workspace-lifecycle-docker",
        "fmt",
        "clippy",
        "deps-lint",
        "health",
        "setup",
        "integration-prepare",
        "integration-status",
        "test-rpc-suite",
        "chaos-replay",
        "daemonless-lifecycle",
        "contracts",
        "migration-script-tests",
    ] {
        let applicable = match job {
            "release-metadata" | "health" | "setup" => true,
            "migration-script-tests" => !release_only && migration_changed(paths),
            _ => !release_only,
        };
        add("ci.yml", job, None, applicable);
    }
    for target in ["x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"] {
        add("ci.yml", "musl-publication", Some(target), true);
    }
    for os in ["ubuntu-latest", "macos-latest"] {
        add(
            "ci.yml",
            "state-commit-golden-vectors",
            Some(os),
            !release_only,
        );
    }
    for target in ["core", "plugins", "cli"] {
        add("ci.yml", "test", Some(target), !release_only);
    }
    for shard in [
        "gatling-0",
        "gatling-1",
        "gatling-2",
        "gatling-3",
        "release-policies",
        "contract_conformance",
        "constitution_claims",
        "spec_conformance",
        "substrate_integrity",
    ] {
        add(
            "ci.yml",
            "test-integration",
            Some(shard),
            !release_only && integrations.iter().any(|s| s == shard),
        );
    }
    add("constitution-sync.yml", "validate-constitution", None, true);
    add("decapod-validate.yml", "validate", None, true);
    add("docs.yml", "build", None, true);
    add("docs_sync.yml", "sync-docs", None, docs_sync_changed(paths));
    result
}

fn main_result() -> io::Result<bool> {
    let mut args = env::args().skip(1);
    let mode = args.next().ok_or_else(|| {
        io::Error::other("expected plan, paths, migration-changed, docs-changed, or run")
    })?;
    let mut repo = PathBuf::from(".");
    let mut base = None;
    let mut head = None;
    let mut id = None;
    let mut custody_flags = std::collections::BTreeMap::new();
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| io::Error::other(format!("missing {flag} value")))?;
        match flag.as_str() {
            "--repo" => repo = value.into(),
            "--base" if base.is_none() => base = Some(value),
            "--head" if head.is_none() => head = Some(value),
            "--check" if id.is_none() => id = Some(value),
            "--snapshot"
            | "--validation-head"
            | "--tree"
            | "--provider"
            | "--run-id"
            | "--run-attempt"
            | "--job-id"
            | "--step-key"
            | "--expected-profile-sha256"
            | "--expected-invocation-sha256"
            | "--expected-toolchain-sha256"
            | "--preparation-file"
            | "--evidence-dir"
                if ["run-evidence", "prepare-evidence"].contains(&mode.as_str())
                    && !custody_flags.contains_key(&flag) =>
            {
                custody_flags.insert(flag, value);
            }
            _ => {
                return Err(io::Error::other(format!(
                    "unknown or duplicate argument: {flag}"
                )));
            }
        }
    }
    if ![
        "plan",
        "paths",
        "migration-changed",
        "docs-changed",
        "run",
        "run-evidence",
        "run-ci",
        "prepare-evidence",
    ]
    .contains(&mode.as_str())
    {
        return Err(io::Error::other("unknown mode"));
    }
    let selected_check =
        if ["run", "run-evidence", "prepare-evidence", "run-ci"].contains(&mode.as_str()) {
            Some(find_check(
                id.as_deref()
                    .ok_or_else(|| io::Error::other("run requires --check"))?,
            )?)
        } else {
            None
        };
    if !["run", "run-evidence", "prepare-evidence", "run-ci"].contains(&mode.as_str())
        && id.is_some()
    {
        return Err(io::Error::other("--check is only valid for run"));
    }
    if mode == "run-ci" {
        return custody::run_ci(
            &repo,
            selected_check.unwrap(),
            base.as_deref(),
            head.as_deref(),
        );
    }
    // Unconditional commands need no diff. Conditional execution and all plans
    // always require both immutable endpoints; never a mutable-ref fallback.
    let need_paths =
        mode != "run" || selected_check.is_some_and(|c| c.selection != Selection::Always);
    let paths = if need_paths || base.is_some() || head.is_some() {
        changed_paths(
            &repo,
            base.as_deref()
                .ok_or_else(|| io::Error::other("missing --base"))?,
            head.as_deref()
                .ok_or_else(|| io::Error::other("missing --head"))?,
        )?
    } else {
        Vec::new()
    };
    match mode.as_str() {
        "paths" => {
            use std::io::Write;
            for path in paths {
                io::stdout().write_all(path.as_bytes())?;
                io::stdout().write_all(&[0])?;
            }
            Ok(true)
        }
        "migration-changed" => {
            println!("changed={}", migration_changed(&paths));
            Ok(true)
        }
        "docs-changed" => {
            println!("changed={}", docs_sync_changed(&paths));
            Ok(true)
        }
        "plan" => {
            let mut entries = Vec::new();
            for check in CHECKS {
                entries.push(format!("{{\"id\":{},\"workflow\":{},\"job\":{},\"matrix\":{},\"platform\":{},\"capabilities\":{},\"applicable\":{},\"skipped_for_release_only\":{},\"commands\":[{}],\"invocation_digest\":{}}}", json_string(check.id), json_string(check.workflow), json_string(check.job), check.matrix.map_or_else(|| "null".into(), json_string), json_string(check.platform.name()), json_strings(check.capabilities.iter().copied()), selected(check, &paths, false), check.skipped_for_release_only, commands(check, &paths)?.iter().map(invocation_json).collect::<Vec<_>>().join(","), json_string(&custody::invocation_digest(&commands(check, &paths)?)?)));
            }
            println!(
                "{{\"schema\":{},\"base\":{},\"head\":{},\"complete_pr_profile\":false,\"profile_digest\":{},\"checks\":[{}],\"inventory\":[{}],\"workflow_owned_obligations\":[{}]}}",
                json_string(SCHEMA),
                json_string(base.as_deref().unwrap()),
                json_string(head.as_deref().unwrap()),
                json_string(&custody::profile_digest()?),
                entries.join(","),
                inventory(&paths, false).iter().map(|job| format!("{{\"workflow\":{},\"job\":{},\"matrix\":{},\"applicable\":{},\"checks\":{},\"workflow_owned\":{}}}", json_string(job.workflow), json_string(job.job), job.matrix.map_or_else(|| "null".into(), json_string), job.applicable, json_strings(job.checks.iter().copied()), job.workflow_owned)).collect::<Vec<_>>().join(","),
                WORKFLOW_OWNED_OBLIGATIONS
                    .iter()
                    .map(|(workflow, job, purpose)| format!(
                        "{{\"workflow\":{},\"job\":{},\"purpose\":{}}}",
                        json_string(workflow),
                        json_string(job),
                        json_string(purpose)
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            Ok(true)
        }
        "run-evidence" | "prepare-evidence" => custody::run(
            &repo,
            selected_check.unwrap(),
            &paths,
            base.as_deref()
                .ok_or_else(|| io::Error::other("missing --base"))?,
            head.as_deref()
                .ok_or_else(|| io::Error::other("missing --head"))?,
            &custody::Context::from_flags(&custody_flags)?,
            mode == "prepare-evidence",
        ),
        "run" => run_check(
            &repo,
            selected_check.unwrap(),
            &paths,
            base.as_deref().unwrap_or(""),
            head.as_deref().unwrap_or(""),
        ),
        _ => unreachable!(),
    }
}

fn main() {
    match main_result() {
        Ok(true) => (),
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("source-ci-profile: {error}");
            std::process::exit(2);
        }
    }
}
