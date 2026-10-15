use crate::core::assets;
use crate::core::error;
use crate::core::project_specs::{self, LOCAL_PROJECT_SPECS, LOCAL_PROJECT_SPECS_MANIFEST};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub(crate) mod input;
use input::BoundedEpochInput;

pub const VALIDATION_EPOCH_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ValidationEpochMetadata {
    pub schema_version: String,
    pub epoch_id: String,
    pub evaluator_identity: String,
    pub evaluator_set_hash: String,
    pub constitution_version: String,
    pub constitution_hash: String,
    pub validation_profile: String,
    pub validation_profile_hash: String,
    pub proof_rubric: String,
    pub proof_rubric_hash: String,
    pub generated_specs_manifest_hash: String,
    pub generated_specs_fingerprint: String,
    pub material_hashes: BTreeMap<String, String>,
}

pub fn active_validation_epoch(
    project_root: &Path,
) -> Result<ValidationEpochMetadata, error::DecapodError> {
    build_validation_epoch(project_root, &mut HostEpochInput)
}

/// Bounded, no-symlink input adapter for execution continuity. Uses exactly the
/// same epoch formula as ordinary validation; an unreadable source is an error,
/// never an "unavailable" fingerprint that can compare equal across failures.
pub(crate) fn active_validation_epoch_bounded(
    project_root: &Path,
) -> Result<ValidationEpochMetadata, error::DecapodError> {
    build_validation_epoch(project_root, &mut BoundedEpochInput::new(project_root)?)
}

trait EpochInput {
    fn read_optional(&mut self, path: &Path) -> Result<Option<Vec<u8>>, error::DecapodError>;
    fn fingerprint(&mut self, root: &Path) -> Result<String, error::DecapodError>;
}
struct HostEpochInput;
impl EpochInput for HostEpochInput {
    fn read_optional(&mut self, path: &Path) -> Result<Option<Vec<u8>>, error::DecapodError> {
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(fs::read(path)?))
    }
    fn fingerprint(&mut self, root: &Path) -> Result<String, error::DecapodError> {
        Ok(project_specs::repo_signal_fingerprint(root).unwrap_or_else(|_| "unavailable".into()))
    }
}

fn build_validation_epoch(
    project_root: &Path,
    input: &mut dyn EpochInput,
) -> Result<ValidationEpochMetadata, error::DecapodError> {
    let validation_profile =
        std::env::var("DECAPOD_VALIDATION_PROFILE").unwrap_or_else(|_| "default".to_string());
    let evaluator_identity = format!("decapod-validate@{}", env!("CARGO_PKG_VERSION"));
    let proof_rubric = "decapod-validate/current-proof-v1".to_string();

    let constitution_body = assets::get_doc("core/DECAPOD").unwrap_or_default();
    let constitution_hash = sha256_hex(constitution_body.as_bytes());
    let constitution_version = extract_version(&constitution_body)
        .unwrap_or_else(|| format!("embedded-docs@{}", env!("CARGO_PKG_VERSION")));

    let generated_specs_manifest_hash = hash_specs_manifest_using(
        project_root.join(LOCAL_PROJECT_SPECS_MANIFEST).as_path(),
        input,
    )?;
    let generated_specs_fingerprint = input.fingerprint(project_root)?;

    let mut material_hashes = BTreeMap::new();
    material_hashes.insert(
        "constitution:core/DECAPOD".to_string(),
        constitution_hash.clone(),
    );
    material_hashes.insert(
        "validation_profile".to_string(),
        sha256_hex(validation_profile.as_bytes()),
    );
    material_hashes.insert(
        "proof_rubric".to_string(),
        sha256_hex(proof_rubric.as_bytes()),
    );
    material_hashes.insert(
        "generated_specs_manifest".to_string(),
        generated_specs_manifest_hash.clone(),
    );
    material_hashes.insert(
        "generated_specs_fingerprint".to_string(),
        generated_specs_fingerprint.clone(),
    );
    for spec in LOCAL_PROJECT_SPECS {
        let path = project_root.join(spec.path);
        material_hashes.insert(
            format!("generated_spec:{}", spec.path),
            hash_file_using(path.as_path(), input)?,
        );
        // Authored living-spec prose is evidence material for proof completion
        // (#1183). Fingerprint/attestation blocks are excluded so completion
        // packages bind the contract text agents must rewrite per change.
        material_hashes.insert(
            format!("living_spec_material:{}", spec.path),
            hash_living_spec_using(path.as_path(), input)?,
        );
    }

    // Bind the plan/review and research claims to the same validation epoch.
    // The receipt itself is deliberately excluded to avoid a self-reference;
    // trajectory identity is bound separately by ValidationReceipt.
    for path in [
        "AGENTS.md",
        "CLAUDE.md",
        "CODEX.md",
        "GEMINI.md",
        crate::plugins::container::MANAGED_DOCKERFILE_REL_PATH,
    ] {
        material_hashes.insert(
            format!("governance_material:{path}"),
            hash_file_using(&project_root.join(path), input)?,
        );
    }

    for section in ["plan", "claims"] {
        material_hashes.insert(
            format!("governance_section:{section}"),
            crate::core::governance_document::section_hash(project_root, section)?,
        );
    }

    material_hashes.insert(
        "governance_context".to_string(),
        crate::core::governance_document::validation_context_hash(project_root)?,
    );

    let evaluator_set_hash = hash_named_values(&[
        ("evaluator_identity", evaluator_identity.as_str()),
        ("validation_profile", validation_profile.as_str()),
        ("proof_rubric", proof_rubric.as_str()),
    ]);
    let validation_profile_hash = sha256_hex(validation_profile.as_bytes());
    let proof_rubric_hash = sha256_hex(proof_rubric.as_bytes());

    let epoch_material = serde_json::json!({
        "evaluator_identity": evaluator_identity,
        "evaluator_set_hash": evaluator_set_hash,
        "constitution_version": constitution_version,
        "constitution_hash": constitution_hash,
        "validation_profile": validation_profile,
        "validation_profile_hash": validation_profile_hash,
        "proof_rubric": proof_rubric,
        "proof_rubric_hash": proof_rubric_hash,
        "generated_specs_manifest_hash": generated_specs_manifest_hash,
        "generated_specs_fingerprint": generated_specs_fingerprint,
        "material_hashes": material_hashes,
    });
    let epoch_material_hash = sha256_hex(
        serde_json::to_string(&epoch_material)
            .unwrap_or_default()
            .as_bytes(),
    );
    let epoch_id = format!(
        "ve_{}",
        &epoch_material_hash
            .strip_prefix("sha256:")
            .unwrap_or(&epoch_material_hash)[..16]
    );

    Ok(ValidationEpochMetadata {
        schema_version: VALIDATION_EPOCH_SCHEMA_VERSION.to_string(),
        epoch_id,
        evaluator_identity,
        evaluator_set_hash,
        constitution_version,
        constitution_hash,
        validation_profile,
        validation_profile_hash,
        proof_rubric,
        proof_rubric_hash,
        generated_specs_manifest_hash,
        generated_specs_fingerprint,
        material_hashes,
    })
}

fn extract_version(markdown: &str) -> Option<String> {
    markdown.lines().find_map(|line| {
        let trimmed = line.trim();
        for prefix in ["**Version:**", "Version:", "version:"] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let value = rest.trim().trim_matches('*').trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
        None
    })
}

fn hash_file_using(path: &Path, input: &mut dyn EpochInput) -> Result<String, error::DecapodError> {
    Ok(input
        .read_optional(path)?
        .map(|bytes| sha256_hex(&bytes))
        .unwrap_or_else(|| "absent".into()))
}

fn hash_living_spec_using(
    path: &Path,
    input: &mut dyn EpochInput,
) -> Result<String, error::DecapodError> {
    let Some(bytes) = input.read_optional(path)? else {
        return Ok("absent".into());
    };
    let body = String::from_utf8(bytes)
        .map_err(|_| error::DecapodError::ValidationError("Invalid UTF-8 living spec".into()))?;
    Ok(sha256_hex(
        project_specs::material_spec_body(&body).as_bytes(),
    ))
}

#[cfg(test)]
fn hash_specs_manifest_material(path: &Path) -> Result<String, error::DecapodError> {
    hash_specs_manifest_using(path, &mut HostEpochInput)
}

fn hash_specs_manifest_using(
    path: &Path,
    input: &mut dyn EpochInput,
) -> Result<String, error::DecapodError> {
    let Some(bytes) = input.read_optional(path)? else {
        return Ok("absent".into());
    };
    let mut value = serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|_| {
        error::DecapodError::ValidationError(
            "Invalid generated specs manifest for validation epoch".into(),
        )
    })?;
    if let Some(obj) = value.as_object_mut() {
        obj.remove("generated_at");
    }
    let canonical = serde_json::to_string(&value).map_err(|_| {
        error::DecapodError::ValidationError("Cannot canonicalize generated specs manifest".into())
    })?;
    Ok(sha256_hex(canonical.as_bytes()))
}

fn hash_named_values(values: &[(&str, &str)]) -> String {
    let mut hasher = Sha256::new();
    for (name, value) in values {
        hasher.update(name.as_bytes());
        hasher.update(b"\0");
        hasher.update(value.as_bytes());
        hasher.update(b"\n");
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}
#[cfg(test)]
#[path = "../../../tests/unit/core/validation_epoch_tests.rs"]
mod tests;
