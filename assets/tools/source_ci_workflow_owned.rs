//! Explicit workflow-owned source recipes. Never accepts a caller command or script.
use super::*;
#[allow(dead_code)]
#[path = "release_metadata.rs"]
mod release_classifier;
pub fn release_only(repo: &Path, event: &str, base: &str, head: &str) -> bool {
    release_classifier::classify(repo, event, base, head).unwrap_or(false)
}
pub const SOURCES: &[(&str, &[u8])] = &[
    (
        "assets/tools/source_ci/commit-lint.sh",
        include_bytes!("source_ci/commit-lint.sh"),
    ),
    (
        "assets/tools/source_ci/docs-drift.sh",
        include_bytes!("source_ci/docs-drift.sh"),
    ),
    (
        "assets/tools/source_ci/governance-artifacts.sh",
        include_bytes!("source_ci/governance-artifacts.sh"),
    ),
    (
        "assets/tools/source_ci/health.sh",
        include_bytes!("source_ci/health.sh"),
    ),
    (
        "assets/tools/source_ci/integration-prepare.sh",
        include_bytes!("source_ci/integration-prepare.sh"),
    ),
    (
        "assets/tools/source_ci/integration-status.sh",
        include_bytes!("source_ci/integration-status.sh"),
    ),
    (
        "assets/tools/source_ci/integration-worker.sh",
        include_bytes!("source_ci/integration-worker.sh"),
    ),
    (
        "assets/tools/source_ci/material-specs.sh",
        include_bytes!("source_ci/material-specs.sh"),
    ),
    (
        "assets/tools/source_ci/migration-detection.sh",
        include_bytes!("source_ci/migration-detection.sh"),
    ),
    (
        "assets/tools/source_ci/private-checkout.sh",
        include_bytes!("source_ci/private-checkout.sh"),
    ),
    (
        "assets/tools/source_ci/release-metadata.sh",
        include_bytes!("source_ci/release-metadata.sh"),
    ),
    (
        "assets/tools/source_ci/standalone-validation.sh",
        include_bytes!("source_ci/standalone-validation.sh"),
    ),
    (
        "assets/tools/release_metadata.rs",
        include_bytes!("release_metadata.rs"),
    ),
    (
        "src/decapod/core/release_metadata.rs",
        include_bytes!("../../src/decapod/core/release_metadata.rs"),
    ),
    ("assets/tools/docs_sync.rs", include_bytes!("docs_sync.rs")),
];
pub fn script(check: &Check) -> Option<&'static str> {
    Some(match check.id {
        "release-metadata" => "assets/tools/source_ci/release-metadata.sh",
        "commit-lint" => "assets/tools/source_ci/commit-lint.sh",
        "governance-artifacts" => "assets/tools/source_ci/governance-artifacts.sh",
        "material-specs" => "assets/tools/source_ci/material-specs.sh",
        "migration-detection" => "assets/tools/source_ci/migration-detection.sh",
        "health" => "assets/tools/source_ci/health.sh",
        "integration-prepare" => "assets/tools/source_ci/integration-prepare.sh",
        "integration-status" => "assets/tools/source_ci/integration-status.sh",
        "standalone-validation" => "assets/tools/source_ci/standalone-validation.sh",
        "docs-drift" => "assets/tools/source_ci/docs-drift.sh",
        "integration-gatling-0" => "assets/tools/source_ci/integration-worker.sh",
        "integration-gatling-1" => "assets/tools/source_ci/integration-worker.sh",
        "integration-gatling-2" => "assets/tools/source_ci/integration-worker.sh",
        "integration-gatling-3" => "assets/tools/source_ci/integration-worker.sh",
        "integration-release-policies" => "assets/tools/source_ci/integration-worker.sh",
        "integration-contract_conformance" => "assets/tools/source_ci/integration-worker.sh",
        "integration-constitution_claims" => "assets/tools/source_ci/integration-worker.sh",
        "integration-spec_conformance" => "assets/tools/source_ci/integration-worker.sh",
        "integration-substrate_integrity" => "assets/tools/source_ci/integration-worker.sh",
        _ => return None,
    })
}
pub fn artifact_worker(check: &Check) -> bool {
    check.job == "test-integration" || check.id == "integration-status"
}
pub fn script_sources(check: &Check) -> Vec<(&'static str, &'static [u8])> {
    SOURCES
        .iter()
        .copied()
        .filter(|(path, _)| {
            Some(*path) == script(check)
                || (matches!(
                    check.id,
                    "health" | "governance-artifacts" | "standalone-validation"
                ) && path.ends_with("/private-checkout.sh"))
        })
        .collect()
}
pub fn input_files(check: &Check, repo: &Path) -> Vec<String> {
    let mut files: Vec<String> = match check.id {
        "governance-artifacts" => vec!["target/debug/decapod".into()],
        "health" => vec![
            "target/debug/decapod".into(),
            "target/debug/decapod-release-projection-drift".into(),
        ],
        "integration-status" => vec!["target/integration-custody/integration-custody".into()],
        _ if check.job == "test-integration" => {
            vec!["target/integration-download/integration-tests.tar".into()]
        }
        _ => vec![],
    };
    if check.id == "integration-status" {
        for shard in &[
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
            let path = format!("target/integration-receipts/{shard}.receipt");
            if repo.join(&path).is_file() {
                files.push(path);
            }
        }
    }
    files
}

/// Source-selected compile targets for the bounded integration dependency manifest.
pub fn integration_targets(paths: &[String]) -> Vec<&'static str> {
    integration_shards::plan(paths).targets()
}
