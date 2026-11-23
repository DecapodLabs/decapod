//! Fast local pre-build structural and completeness validation (#1047).
//!
//! Identifies missing or malformed governance artifacts, schema errors, missing
//! living specs, or missing agent entrypoints before incurring build/test/CI cost.
//!
//! Invariants:
//! - Zero mutations to the repository or `.decapod/`.
//! - Zero builds, tests, containers, or remote preflights.
//! - Reports `scope = "pre_build"`, `publication_ready = false`, `proof_ready = false`.
//!   Passing pre-build checks never establishes PR or publication readiness.

use crate::cli::DecapodProjectConfig;
use crate::core::bounded_process::BoundedCommand;
use crate::core::error::DecapodError;
use crate::core::project_specs;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

pub const PRE_BUILD_VALIDATE_SCHEMA_VERSION: &str = "decapod.pre-build-validate.v1";

const REQUIRED_SPEC_FILES: &[&str] = &[
    "README.md",
    "INTENT.md",
    "ARCHITECTURE.md",
    "INTERFACES.md",
    "SECURITY.md",
    "OPERATIONS.md",
    "VALIDATION.md",
    "SEMANTICS.md",
];

fn is_hex_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn extract_html_marker<'a>(body: &'a str, prefix: &str) -> Option<&'a str> {
    for line in body.lines().take(12) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(prefix)
            && let Some(val) = rest.strip_suffix("-->")
        {
            let trimmed_val = val.trim();
            if !trimmed_val.is_empty() {
                return Some(trimmed_val);
            }
        }
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreBuildCheck {
    pub id: String,
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreBuildValidateReport {
    pub schema_version: String,
    pub scope: String,
    pub read_only: bool,
    pub publication_ready: bool,
    pub proof_ready: bool,
    pub passed: bool,
    pub pass_count: usize,
    pub fail_count: usize,
    pub checks: Vec<PreBuildCheck>,
    pub failures: Vec<String>,
}

fn read_only_git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(args)
        .bounded_output(Duration::from_secs(3))
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn push_pass(checks: &mut Vec<PreBuildCheck>, id: &str, msg: String) {
    checks.push(PreBuildCheck {
        id: id.to_string(),
        status: "pass".to_string(),
        message: msg,
        remediation: None,
    });
}

fn push_fail(
    checks: &mut Vec<PreBuildCheck>,
    failures: &mut Vec<String>,
    id: &str,
    msg: String,
    remediation: &str,
) {
    failures.push(format!("{id}: {msg}"));
    checks.push(PreBuildCheck {
        id: id.to_string(),
        status: "fail".to_string(),
        message: msg,
        remediation: Some(remediation.to_string()),
    });
}

/// Evaluate fast read-only structural and completeness checks without running builds or tests.
pub fn evaluate(repo_root: &Path) -> PreBuildValidateReport {
    let mut checks = Vec::new();
    let mut failures = Vec::new();

    // 1. `.decapod/config.toml` presence and strict schema parse
    let config_path = repo_root.join(".decapod/config.toml");
    let parsed_config = match fs::read_to_string(&config_path) {
        Ok(raw) => match DecapodProjectConfig::parse(&raw) {
            Ok(cfg) => {
                push_pass(
                    &mut checks,
                    "config_schema",
                    ".decapod/config.toml exists and matches the strict schema".to_string(),
                );
                Some(cfg)
            }
            Err(err) => {
                push_fail(
                    &mut checks,
                    &mut failures,
                    "config_schema",
                    format!(".decapod/config.toml failed schema validation: {err}"),
                    "Run `decapod docs show config-schema --source embedded` and repair .decapod/config.toml",
                );
                None
            }
        },
        Err(_) => {
            push_fail(
                &mut checks,
                &mut failures,
                "config_schema",
                ".decapod/config.toml is missing or unreadable".to_string(),
                "Initialize Decapod (`decapod init`) or restore .decapod/config.toml",
            );
            None
        }
    };

    // 2. `.decapod/governance.json` presence, JSON validity, and required logical sections
    let gov_path = repo_root.join(".decapod/governance.json");
    match fs::read_to_string(&gov_path) {
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(doc) => {
                let mut missing = Vec::new();
                if doc.get("claims").is_none() {
                    missing.push("claims");
                }
                let sections = doc.get("sections");
                for key in ["plan", "trajectory"] {
                    if sections.and_then(|s| s.get(key)).is_none() {
                        missing.push(key);
                    }
                }
                if missing.is_empty() {
                    push_pass(
                        &mut checks,
                        "governance_document",
                        ".decapod/governance.json is valid and contains claims, plan, and trajectory sections".to_string(),
                    );
                } else {
                    push_fail(
                        &mut checks,
                        &mut failures,
                        "governance_document",
                        format!(
                            ".decapod/governance.json is missing required section(s): {}",
                            missing.join(", ")
                        ),
                        "Run `decapod govern artifacts inventory --repair` and populate plan/claims/trajectory",
                    );
                }
            }
            Err(err) => {
                push_fail(
                    &mut checks,
                    &mut failures,
                    "governance_document",
                    format!(".decapod/governance.json is not valid JSON: {err}"),
                    "Repair .decapod/governance.json through `decapod govern artifacts inventory --repair`",
                );
            }
        },
        Err(_) => {
            push_fail(
                &mut checks,
                &mut failures,
                "governance_document",
                ".decapod/governance.json is missing".to_string(),
                "Run `decapod govern artifacts inventory --repair` to initialize tracked governance state",
            );
        }
    }

    // 3. Living specs completeness under `.decapod/managed/specs/` and manifest integrity (#1079, #1134)
    let specs_required = parsed_config.as_ref().map(|c| c.init.specs).unwrap_or(true);
    if specs_required {
        let specs_dir = repo_root.join(".decapod/managed/specs");
        let mut missing_specs = Vec::new();
        for name in REQUIRED_SPEC_FILES {
            let p = specs_dir.join(name);
            let ok = fs::metadata(&p)
                .map(|m| m.is_file() && m.len() > 0)
                .unwrap_or(false);
            if !ok {
                missing_specs.push(*name);
            }
        }
        if missing_specs.is_empty() {
            push_pass(
                &mut checks,
                "living_specs_layout",
                format!(
                    "All {} required files under .decapod/managed/specs/ are present and non-empty",
                    REQUIRED_SPEC_FILES.len()
                ),
            );
        } else {
            push_fail(
                &mut checks,
                &mut failures,
                "living_specs_layout",
                format!(
                    "Missing or empty living spec file(s) under .decapod/managed/specs/: {}",
                    missing_specs.join(", ")
                ),
                "Author living specs and refresh projections via `decapod rpc --op specs.refresh`",
            );
        }

        // 3b. Specs manifest presence, schema, and SHA256 fingerprint integrity (#1079, #1134)
        match project_specs::resolve_specs_manifest_path(repo_root) {
            Some(manifest_path) => match project_specs::read_specs_manifest(repo_root) {
                Ok(Some(manifest)) => {
                    let mut manifest_issues = Vec::new();
                    if !is_hex_sha256(&manifest.repo_signal_fingerprint) {
                        manifest_issues
                            .push("invalid or missing repo_signal_fingerprint SHA-256".to_string());
                    }
                    if !manifest.spec_input_hash.is_empty()
                        && !is_hex_sha256(&manifest.spec_input_hash)
                    {
                        manifest_issues.push("invalid spec_input_hash SHA-256".to_string());
                    }
                    if !manifest.config_input_hash.is_empty()
                        && !is_hex_sha256(&manifest.config_input_hash)
                    {
                        manifest_issues.push("invalid config_input_hash SHA-256".to_string());
                    }
                    for entry in &manifest.files {
                        if !is_hex_sha256(&entry.content_hash) {
                            manifest_issues
                                .push(format!("missing/invalid content_hash for {}", entry.path));
                            continue;
                        }
                        let file_path = repo_root.join(&entry.path);
                        if let Ok(body) = fs::read_to_string(&file_path) {
                            let actual = project_specs::hash_text(&body);
                            if actual != entry.content_hash {
                                manifest_issues
                                    .push(format!("stale content_hash for {}", entry.path));
                            }
                        }
                    }
                    for entry in &manifest.entrypoints {
                        if !is_hex_sha256(&entry.fingerprint) {
                            manifest_issues.push(format!(
                                "missing/invalid entrypoint fingerprint SHA-256 for {}",
                                entry.path
                            ));
                        }
                        if !entry.content_hash.is_empty() {
                            let file_path = repo_root.join(&entry.path);
                            if let Ok(body) = fs::read_to_string(&file_path) {
                                let actual = project_specs::hash_text(&body);
                                if actual != entry.content_hash {
                                    manifest_issues.push(format!(
                                        "stale entrypoint content_hash for {}",
                                        entry.path
                                    ));
                                }
                            }
                        }
                    }
                    let rel_display = manifest_path
                        .strip_prefix(repo_root)
                        .unwrap_or(&manifest_path)
                        .to_string_lossy();
                    if manifest_issues.is_empty() {
                        push_pass(
                            &mut checks,
                            "specs_manifest",
                            format!(
                                "{rel_display} is valid and matches spec/entrypoint SHA-256 hashes"
                            ),
                        );
                    } else {
                        push_fail(
                            &mut checks,
                            &mut failures,
                            "specs_manifest",
                            format!(
                                "{rel_display} integrity failure: {}",
                                manifest_issues.join("; ")
                            ),
                            "Run `decapod rpc --op specs.refresh` and commit refreshed manifest and projections",
                        );
                    }
                }
                Ok(None) => push_fail(
                    &mut checks,
                    &mut failures,
                    "specs_manifest",
                    format!("{} is missing", project_specs::LOCAL_PROJECT_SPECS_MANIFEST),
                    "Run `decapod rpc --op specs.refresh` to generate .decapod/managed/.manifest.json",
                ),
                Err(err) => push_fail(
                    &mut checks,
                    &mut failures,
                    "specs_manifest",
                    format!("Specs manifest failed JSON validation: {err}"),
                    "Run `decapod rpc --op specs.refresh` to regenerate .decapod/managed/.manifest.json",
                ),
            },
            None => push_fail(
                &mut checks,
                &mut failures,
                "specs_manifest",
                format!(
                    "Missing {} (and legacy {})",
                    project_specs::LOCAL_PROJECT_SPECS_MANIFEST,
                    project_specs::LEGACY_PROJECT_SPECS_MANIFEST
                ),
                "Run `decapod rpc --op specs.refresh` to generate .decapod/managed/.manifest.json",
            ),
        }
    } else {
        push_pass(
            &mut checks,
            "living_specs_layout",
            "Living specs disabled in .decapod/config.toml (init.specs = false)".to_string(),
        );
    }

    // 4. Configured agent entrypoints and fingerprint SHA-256 markers (#1134)
    let entrypoints: Vec<String> = parsed_config
        .as_ref()
        .map(|c| c.init.entrypoints.clone())
        .filter(|list| !list.is_empty())
        .unwrap_or_else(|| vec!["AGENTS.md".to_string()]);
    let mut missing_ep = Vec::new();
    let mut unanchored_ep = Vec::new();
    let mut missing_fp_ep = Vec::new();
    for ep in &entrypoints {
        let p = repo_root.join(ep);
        match fs::read_to_string(&p) {
            Ok(body) => {
                if !body.contains("decapod-release:") && !body.contains("Decapod") {
                    unanchored_ep.push(ep.clone());
                }
                if body.contains("decapod-release:") {
                    match extract_html_marker(&body, "<!-- decapod-fingerprint:") {
                        Some(fp) if is_hex_sha256(fp) => {}
                        _ => missing_fp_ep.push(ep.clone()),
                    }
                }
            }
            Err(_) => missing_ep.push(ep.clone()),
        }
    }
    if missing_ep.is_empty() && unanchored_ep.is_empty() && missing_fp_ep.is_empty() {
        push_pass(
            &mut checks,
            "agent_entrypoints",
            format!(
                "All configured agent entrypoints present with Decapod release and fingerprint SHA-256 markers ({})",
                entrypoints.join(", ")
            ),
        );
    } else {
        let mut parts = Vec::new();
        if !missing_ep.is_empty() {
            parts.push(format!("missing: {}", missing_ep.join(", ")));
        }
        if !unanchored_ep.is_empty() {
            parts.push(format!(
                "missing Decapod anchor: {}",
                unanchored_ep.join(", ")
            ));
        }
        if !missing_fp_ep.is_empty() {
            parts.push(format!(
                "missing or invalid decapod-fingerprint SHA-256: {}",
                missing_fp_ep.join(", ")
            ));
        }
        push_fail(
            &mut checks,
            &mut failures,
            "agent_entrypoints",
            parts.join("; "),
            "Refresh entrypoints through Decapod initialization or validation (`decapod validate --refresh-specs`)",
        );
    }

    // 5. Material specs diff vs base branch (only when on a non-protected branch with commits ahead of base)
    let base_branch = parsed_config
        .as_ref()
        .and_then(|c| c.repo.base_branch.clone())
        .unwrap_or_else(|| "master".to_string());
    let current_branch = read_only_git(repo_root, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
    if let Some(branch) = current_branch.as_deref()
        && branch != base_branch
        && branch != "main"
        && branch != "master"
    {
        let base_ref = [
            format!("refs/remotes/origin/{base_branch}"),
            format!("refs/heads/{base_branch}"),
        ]
        .into_iter()
        .find(|r| read_only_git(repo_root, &["rev-parse", "--verify", r]).is_some());
        if let Some(base_ref) = base_ref {
            let diff_files = read_only_git(
                repo_root,
                &["diff", "--name-only", &format!("{base_ref}...HEAD")],
            )
            .unwrap_or_default();
            let impl_changed = diff_files
                .lines()
                .any(project_specs::is_implementation_path);
            if impl_changed {
                match project_specs::material_specs_change_vs_base(repo_root, &base_ref) {
                    Ok(res) if res.has_material_change => {
                        push_pass(
                            &mut checks,
                            "material_specs_diff",
                            format!("Material living spec change detected against {base_ref}"),
                        );
                    }
                    Ok(_) => {
                        push_fail(
                            &mut checks,
                            &mut failures,
                            "material_specs_diff",
                            format!(
                                "FINGERPRINT_ONLY_SPECS: implementation changed vs {base_ref} without a material authored update to .decapod/managed/specs/*.md"
                            ),
                            "Update relevant prose in .decapod/managed/specs/*.md and run `decapod rpc --op specs.refresh`",
                        );
                    }
                    Err(err) => {
                        push_fail(
                            &mut checks,
                            &mut failures,
                            "material_specs_diff",
                            format!("Unable to verify material living specs vs {base_ref}: {err}"),
                            "Ensure git base ref and living specs are intact",
                        );
                    }
                }
            }
        }
    }

    let pass_count = checks.iter().filter(|c| c.status == "pass").count();
    let fail_count = checks.iter().filter(|c| c.status != "pass").count();
    PreBuildValidateReport {
        schema_version: PRE_BUILD_VALIDATE_SCHEMA_VERSION.to_string(),
        scope: "pre_build".to_string(),
        read_only: true,
        publication_ready: false,
        proof_ready: false,
        passed: fail_count == 0,
        pass_count,
        fail_count,
        checks,
        failures,
    }
}

pub(crate) fn run(repo_root: &Path, format: &str) -> Result<(), DecapodError> {
    let report = evaluate(repo_root);
    if format == "json" {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(std::io::Error::other)?
        );
    } else {
        println!("=== DECAPOD PRE-BUILD VALIDATE (scope=pre_build, publication_ready=false) ===");
        for check in &report.checks {
            let tag = if check.status == "pass" {
                "PASS"
            } else {
                "FAIL"
            };
            println!("[{tag}] {}: {}", check.id, check.message);
            if let Some(rem) = &check.remediation {
                println!("       remediation: {rem}");
            }
        }
        println!(
            "Summary: {} passed, {} failed (pre-build structural check only; full `decapod validate` required for PR readiness)",
            report.pass_count, report.fail_count
        );
    }
    if report.passed {
        Ok(())
    } else {
        Err(DecapodError::ValidationError(format!(
            "PRE_BUILD_VALIDATE_FAILED: {} structural check(s) failed ({})",
            report.fail_count,
            report.failures.join("; ")
        )))
    }
}
