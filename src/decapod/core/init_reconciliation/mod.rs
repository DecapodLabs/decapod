//! Bounded, read-only adoption inventory. This never executes workflow contents,
//! rewrites authored contracts, approves a profile pin, or produces proof evidence.
//! The complete-suite planner remains the authority for executable obligations.

use crate::core::error::DecapodError;
use crate::core::proof::ProjectProofConfig;
use crate::core::readiness_profile::{self, SourceInventory};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path};

const MAX_ENTRIES: usize = 512;
const MAX_DEPTH: usize = 8;
const MAX_FILE_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 8 * 1024 * 1024;

pub(crate) const TOOLCHAIN_ALIAS: &str = "rust-toolchain.toml";
pub(crate) const TOOLCHAIN_TARGET: &str = ".config/build/rust-toolchain.toml";

/// The sole reviewed source alias. Its literal link bytes and the independently
/// inventoried regular destination remain separate material inputs.
pub(crate) fn reviewed_source_alias(
    root: &Path,
    relative: &str,
) -> Result<Option<Vec<u8>>, DecapodError> {
    if relative != TOOLCHAIN_ALIAS {
        return Ok(None);
    }
    let alias = root.join(relative);
    let meta = match fs::symlink_metadata(&alias) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(DecapodError::IoError(error)),
    };
    if !meta.file_type().is_symlink() {
        return Ok(None);
    }
    let target = fs::read_link(&alias).map_err(DecapodError::IoError)?;
    if target != Path::new(TOOLCHAIN_TARGET) {
        return Err(invalid("unreviewed Rust toolchain alias target"));
    }
    if !metadata(root, TOOLCHAIN_TARGET)?.is_some_and(|meta| meta.is_file()) {
        return Err(invalid(
            "Rust toolchain alias requires a regular in-root destination",
        ));
    }
    Ok(Some(TOOLCHAIN_TARGET.as_bytes().to_vec()))
}

// These are discovery roots, not an inferred executable profile. Unknown source
// providers are deliberately retained so additions cannot disappear from the pin.
const SOURCE_DIRECTORIES: &[&str] = &[
    ".github/workflows",
    ".github/actions",
    ".github/scripts",
    ".buildkite",
    ".circleci",
    ".gitlab",
    ".azure-pipelines",
    ".woodpecker",
    ".cargo",
    ".config",
    "assets/tools",
    "assets/scripts",
    "assets/build",
    "scripts",
];
const SOURCE_FILES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain",
    "rust-toolchain.toml",
    "package.json",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "pyproject.toml",
    "requirements.txt",
    "uv.lock",
    "poetry.lock",
    "tox.ini",
    "go.mod",
    "go.sum",
    "Makefile",
    "justfile",
    "Justfile",
    "CMakeLists.txt",
    "BUILD.bazel",
    "MODULE.bazel",
    "MODULE.bazel.lock",
    ".bazelrc",
    ".bazelversion",
    "flake.nix",
    "flake.lock",
    "Dockerfile",
    "dist-workspace.toml",
    "dist.toml",
    ".github/release.toml",
    "release-plz.toml",
    "cliff.toml",
    ".releaserc",
    ".releaserc.json",
    "release-please-config.json",
    "release-please-manifest.json",
    ".gitlab-ci.yml",
    ".gitlab-ci.yaml",
    ".buildkite.yml",
    ".buildkite.yaml",
    "azure-pipelines.yml",
    "azure-pipelines.yaml",
    "Jenkinsfile",
    ".travis.yml",
    "bitbucket-pipelines.yml",
    "appveyor.yml",
    ".drone.yml",
    ".drone.yaml",
    ".woodpecker.yml",
    ".woodpecker.yaml",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepairTask {
    pub code: String,
    pub title: String,
    pub sources: Vec<String>,
    pub action: String,
    /// Configuration and repair belong to the acting agent.
    #[serde(default)]
    pub owner: String,
    /// Run after the task is resolved and the exact candidate is checkpointed.
    #[serde(default)]
    pub resume_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconciliationReport {
    pub schema_version: String,
    /// `proof_pending` only means discovery and the declaration match. No suite ran.
    pub status: String,
    pub provider: String,
    pub merge_ready: bool,
    pub proof_status: String,
    pub inventory_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_contract_digest: Option<String>,
    pub discovered_sources: BTreeMap<String, String>,
    pub repair_tasks: Vec<RepairTask>,
}

fn invalid(message: impl Into<String>) -> DecapodError {
    DecapodError::ValidationError(format!("PROOF_DISCOVERY_BLOCKED: {}", message.into()))
}

#[derive(Default)]
struct Discovery {
    files: BTreeMap<String, String>,
    bytes: usize,
    entries: usize,
    package_name: Option<String>,
}

/// Reject symlinks at every component rather than following repository content
/// outside the root. No shell, Git command, YAML expression or script is executed.
fn metadata(root: &Path, relative: &str) -> Result<Option<fs::Metadata>, DecapodError> {
    let path = Path::new(relative);
    if relative.is_empty()
        || relative.contains(['\\', ':'])
        || relative.chars().any(char::is_control)
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(invalid(format!("unsupported source path: {relative}")));
    }
    let mut current = root.to_path_buf();
    let mut last = None;
    for component in path.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                if relative == TOOLCHAIN_ALIAS && current == root.join(TOOLCHAIN_ALIAS) {
                    reviewed_source_alias(root, relative)?;
                    last = Some(meta);
                } else {
                    return Err(invalid(format!("symlink source or ancestor: {relative}")));
                }
            }
            Ok(meta) => last = Some(meta),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(invalid(format!("cannot inspect {relative}: {error}"))),
        }
    }
    Ok(last)
}

impl Discovery {
    fn visit(&mut self, root: &Path, relative: &str, depth: usize) -> Result<(), DecapodError> {
        if self.files.contains_key(relative) {
            return Ok(());
        }
        let Some(meta) = metadata(root, relative)? else {
            return Ok(());
        };
        self.entries += 1;
        if self.entries > MAX_ENTRIES || depth > MAX_DEPTH {
            return Err(invalid(
                "source inventory exceeds the bounded entry/depth limit; review and add an explicit supported discovery adapter",
            ));
        }
        if meta.file_type().is_symlink() {
            let link = reviewed_source_alias(root, relative)?
                .ok_or_else(|| invalid("unreviewed source alias"))?;
            self.bytes += link.len();
            if self.bytes > MAX_TOTAL_BYTES {
                return Err(invalid("source inventory exceeds the bounded byte limit"));
            }
            self.files.insert(
                relative.into(),
                format!("sha256:{:x}", Sha256::digest(&link)),
            );
            self.visit(root, TOOLCHAIN_TARGET, depth + 1)?;
            return Ok(());
        }
        if meta.is_dir() {
            let mut children = BTreeSet::new();
            for child in fs::read_dir(root.join(relative)).map_err(DecapodError::IoError)? {
                let child = child.map_err(DecapodError::IoError)?;
                let name = child
                    .file_name()
                    .into_string()
                    .map_err(|_| invalid("non-UTF-8 source filename"))?;
                children.insert(format!("{relative}/{name}"));
                if self.entries + children.len() > MAX_ENTRIES {
                    return Err(invalid("source inventory exceeds the bounded entry limit"));
                }
            }
            for child in children {
                self.visit(root, &child, depth + 1)?;
            }
            return Ok(());
        }
        if !meta.is_file() {
            return Err(invalid(format!("source is not a regular file: {relative}")));
        }
        if meta.len() > MAX_FILE_BYTES as u64 {
            return Err(invalid(format!(
                "source exceeds the per-file byte limit: {relative}"
            )));
        }
        let mut bytes = Vec::new();
        fs::File::open(root.join(relative))
            .map_err(DecapodError::IoError)?
            .take((MAX_FILE_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(DecapodError::IoError)?;
        self.bytes += bytes.len();
        if bytes.len() > MAX_FILE_BYTES || self.bytes > MAX_TOTAL_BYTES {
            return Err(invalid("source inventory exceeds the bounded byte limit"));
        }
        self.files.insert(
            relative.to_string(),
            format!("sha256:{:x}", Sha256::digest(&bytes)),
        );
        if relative == "Cargo.toml" {
            self.package_name = std::str::from_utf8(&bytes)
                .ok()
                .and_then(|text| toml::from_str::<toml::Value>(text).ok())
                .and_then(|value| {
                    value
                        .get("package")?
                        .get("name")?
                        .as_str()
                        .map(str::to_owned)
                });
        }
        Ok(())
    }
}

fn discover(root: &Path) -> Result<Discovery, DecapodError> {
    let mut discovery = Discovery::default();
    for path in SOURCE_DIRECTORIES
        .iter()
        .chain(SOURCE_FILES)
        .chain(readiness_profile::catalog_source_paths())
    {
        if *path == ".decapod/config.toml" {
            return Err(invalid(
                "profile declarations cannot hash their own config file",
            ));
        }
        discovery.visit(root, path, 0)?;
    }
    Ok(discovery)
}

/// Shared fresh inventory for init, first-run reconciliation and full-proof planning.
/// Discovery errors are incomplete coverage. An empty inventory is discovery
/// only: reconciliation and complete-suite planning must not treat it as proof.
pub fn discover_source_inventory(root: &Path) -> Result<SourceInventory, DecapodError> {
    Ok(SourceInventory {
        files: discover(root)?.files,
    })
}

pub(crate) fn is_workflow(path: &str) -> bool {
    path.starts_with(".github/workflows/")
        || path.starts_with(".buildkite/")
        || path.starts_with(".circleci/")
        || path.starts_with(".gitlab/")
        || path.starts_with(".azure-pipelines/")
        || path.starts_with(".woodpecker/")
        || [
            ".gitlab-ci.yml",
            ".gitlab-ci.yaml",
            ".buildkite.yml",
            ".buildkite.yaml",
            "azure-pipelines.yml",
            "azure-pipelines.yaml",
            "Jenkinsfile",
            ".travis.yml",
            "bitbucket-pipelines.yml",
            "appveyor.yml",
            ".drone.yml",
            ".drone.yaml",
            ".woodpecker.yml",
            ".woodpecker.yaml",
        ]
        .contains(&path)
}

impl ReconciliationReport {
    fn task(&mut self, code: &str, title: &str, sources: Vec<String>, action: impl Into<String>) {
        self.repair_tasks.push(RepairTask {
            code: code.into(),
            title: title.into(),
            sources,
            action: action.into(),
            owner: "agent".into(),
            resume_command: "decapod validate".into(),
        });
    }
}

/// Inspect current files each time. The authored declaration is only compared,
/// never changed; the report itself cannot authorize publication or suite reuse.
pub fn inspect(
    root: &Path,
    proof: &ProjectProofConfig,
    intended_product: Option<&str>,
) -> ReconciliationReport {
    let mut report = ReconciliationReport {
        schema_version: "1.0.0".into(),
        status: "needs_reconciliation".into(),
        provider: if proof.validation_buildkite_preflights {
            "buildkite_workbranch"
        } else {
            "native"
        }
        .into(),
        merge_ready: false,
        proof_status: "not_run".into(),
        inventory_digest: None,
        command_contract_digest: proof.repository_contract.as_ref().and_then(|contract| {
            crate::core::repository_profile::command_contract_digest(&proof.commands, contract).ok()
        }),
        discovered_sources: BTreeMap::new(),
        repair_tasks: Vec::new(),
    };
    let discovery = match discover(root) {
        Ok(discovery) => discovery,
        Err(error) => {
            report.task(
                "DISCOVERY_BLOCKED",
                "Complete bounded source discovery",
                vec![],
                error.to_string(),
            );
            return report;
        }
    };
    let inventory = SourceInventory {
        files: discovery.files.clone(),
    };
    report.discovered_sources = discovery.files;
    match inventory.digest() {
        Ok(digest) => report.inventory_digest = Some(digest),
        Err(error) => {
            report.task(
                "DISCOVERY_BLOCKED",
                "Compute the source inventory digest",
                vec![],
                format!("Repair inventory serialization: {error:?}"),
            );
            return report;
        }
    }
    if inventory.files.is_empty() {
        report.task("NO_BUILD_OR_CI_CONTRACT", "Declare the project's build and proof contract", vec![],
            "Agent: interpret the human intent and inspect the project, then configure its actual build, test and CI tools and supported complete required profile. Validation executes their configured prerequisites and proof; an empty command list is not successful proof. Preserve separately scoped release requirements and stop for unresolved intent or decisions.");
    }
    match proof.required_profile.as_ref() {
        None => report.task("MISSING_REQUIRED_PROFILE", "Select and review a complete required profile", vec![".decapod/config.toml".into()],
            "Agent: reconcile existing build, test, CI and intended proof coverage, preserving separately scoped release requirements, then declare proof.required_profile.id and reviewed_sources_digest. Review complete coverage before updating a digest; checkpoint and run decapod validate to execute configured proof."),
        Some(profile) if profile.id == crate::core::repository_profile::REQUIRED_PROFILE => {
            if proof.validation_buildkite_preflights {
                report.task("REPOSITORY_PROFILE_PROVIDER_UNSUPPORTED", "Select the repository-local proof route", vec![".decapod/config.toml".into()],
                    "The repository profile proves required local commands only; it cannot select Buildkite or attest hosted workflow execution.");
            }
            match proof.repository_contract.as_ref() {
                None => report.task("REPOSITORY_CONTRACT_REQUIRED", "Review the local command execution and source coverage contract", vec![".decapod/config.toml".into()],
                    "Declare proof.repository_contract with explicit execution tools, required command coverage for every discovered workflow, and a reviewed_commands_digest. This is local proof, not evidence that hosted CI or release packaging ran."),
                Some(contract) => {
                    if let Err(error) = crate::core::repository_profile::validate_contract(profile, &inventory, &proof.commands, contract) {
                        report.task("REPOSITORY_CONTRACT_INVALID", "Reconcile the reviewed local proof contract", vec![".decapod/config.toml".into()], error.to_string());
                    }
                }
            }
        }
        Some(profile) if profile.id != readiness_profile::REQUIRED_PROFILE => report.task("UNKNOWN_REQUIRED_PROFILE", "Implement or select a supported required profile", vec![".decapod/config.toml".into()],
            format!("The profile {:?} has no supported complete-suite adapter. Preserve the declared intent and add the missing adapter rather than replacing it with an empty profile.", profile.id)),
        Some(profile) => {
            if Some(&profile.reviewed_sources_digest) != report.inventory_digest.as_ref() {
                report.task("STALE_SOURCE_INVENTORY", "Reconcile changed proof sources", inventory.files.keys().cloned().collect(),
                    "Review added, removed and changed workflows, local actions, helpers, build and release contracts. Update the required-profile adapter and its explicitly reviewed source digest only after coverage is reconciled.");
            }
            let mismatched: Vec<_> = readiness_profile::reviewed_workflow_digests().iter()
                .filter(|(path, _)| inventory.files.get(*path).is_some_and(|actual| !readiness_profile::reviewed_workflow_matches(path, actual)))
                .map(|(path, _)| (*path).to_string()).collect();
            if !mismatched.is_empty() {
                report.task("MISMATCHED_CI_CONTRACT", "Reconcile workflows with the compiled required profile", mismatched,
                    "The authored workflow differs from the finite adapter's reviewed definition. Preserve it and review the jobs, conditions and matrices; update the adapter and its reviewed definition only after full coverage is established. Re-pinning the inventory alone is insufficient.");
            }
            let missing: Vec<_> = readiness_profile::catalog_source_paths().iter()
                .filter(|path| !inventory.files.contains_key(**path)).map(|path| (*path).to_string()).collect();
            if !missing.is_empty() {
                report.task("MISSING_PROFILE_SOURCE", "Restore or reconcile required profile sources", missing,
                    "The selected profile requires these source contracts. Restore them or implement a reviewed profile matching the authored repository; do not create placeholder successful checks.");
            }
        }
    }
    let unknown: Vec<_> = inventory
        .files
        .keys()
        .filter(|path| {
            is_workflow(path) && !readiness_profile::catalog_source_paths().contains(&path.as_str())
        })
        .cloned()
        .collect();
    if !unknown.is_empty()
        && proof
            .required_profile
            .as_ref()
            .is_none_or(|profile| profile.id != crate::core::repository_profile::REQUIRED_PROFILE)
    {
        report.task("UNKNOWN_CI_CONTRACT", "Adopt unsupported CI contracts into required proof", unknown,
            "Preserve these authored workflows. Review their jobs, conditions, matrices and helpers, then add a faithful supported profile adapter or an explicit proven exclusion. Discovery does not execute YAML or assume these checks passed.");
    }
    if let (Some(intended), Some(package)) = (intended_product, discovery.package_name.as_deref())
        && intended != package
    {
        report.task("INTENT_BUILD_MISMATCH", "Reconcile product intent with the build manifest", vec![".decapod/config.toml".into(), "Cargo.toml".into()],
            format!("The authored product name {intended:?} differs from Cargo package {package:?}. Confirm the intended relationship and update only the contract that is wrong; init preserves the authored name."));
    }
    if report.repair_tasks.is_empty() {
        report.status = "proof_pending".into();
    }
    report.task("REQUIRED_PROOF_NOT_RUN", "Run the complete required proof suite", vec![],
        "Agent: once source and the required checkpoint are ready, run decapod validate. Validation owns configured build prerequisites, verification, tests and artifact-bound evidence through the selected provider; do not build separately first. Repair reported source or build/test/CI configuration failures and revalidate. Native and Buildkite still require the complete applicable source-PR checks and candidate-build obligations; missing adapters fail closed. Stop for unresolved human intent, decision gates, missing authorization or unavailable access. Release-only packaging remains separately scoped; validation success is not authority to distribute or deploy.");
    report
}

#[cfg(test)]
mod tests;
