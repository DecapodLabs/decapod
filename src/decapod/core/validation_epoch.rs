use crate::core::assets;
use crate::core::error;
use crate::core::project_specs::{self, LOCAL_PROJECT_SPECS, LOCAL_PROJECT_SPECS_MANIFEST};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

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
    let validation_profile =
        std::env::var("DECAPOD_VALIDATION_PROFILE").unwrap_or_else(|_| "default".to_string());
    let evaluator_identity = format!("decapod-validate@{}", env!("CARGO_PKG_VERSION"));
    let proof_rubric = "decapod-validate/current-proof-v1".to_string();

    let constitution_body = assets::get_doc("core/DECAPOD").unwrap_or_default();
    let constitution_hash = sha256_hex(constitution_body.as_bytes());
    let constitution_version = extract_version(&constitution_body)
        .unwrap_or_else(|| format!("embedded-docs@{}", env!("CARGO_PKG_VERSION")));

    let generated_specs_manifest_hash =
        hash_specs_manifest_material(project_root.join(LOCAL_PROJECT_SPECS_MANIFEST).as_path())?;
    let generated_specs_fingerprint = project_specs::repo_signal_fingerprint(project_root)
        .unwrap_or_else(|_| "unavailable".to_string());

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
        material_hashes.insert(
            format!("generated_spec:{}", spec.path),
            hash_file_if_exists(project_root.join(spec.path).as_path())?,
        );
    }

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

fn hash_file_if_exists(path: &Path) -> Result<String, error::DecapodError> {
    if !path.exists() {
        return Ok("absent".to_string());
    }
    let bytes = fs::read(path).map_err(error::DecapodError::IoError)?;
    Ok(sha256_hex(&bytes))
}

fn hash_specs_manifest_material(path: &Path) -> Result<String, error::DecapodError> {
    if !path.exists() {
        return Ok("absent".to_string());
    }
    let raw = fs::read_to_string(path).map_err(error::DecapodError::IoError)?;
    let mut value = serde_json::from_str::<serde_json::Value>(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "Invalid generated specs manifest for validation epoch: {e}"
        ))
    })?;
    if let Some(obj) = value.as_object_mut() {
        obj.remove("generated_at");
    }
    let canonical = serde_json::to_string(&value).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "Failed to canonicalize generated specs manifest for validation epoch: {e}"
        ))
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
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn specs_manifest_material_hash_ignores_generated_at() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("manifest.json");
        fs::write(
            &path,
            r#"{"schema_version":"1.0.0","template_version":"scaffold-v3","generated_at":"1Z","repo_signal_fingerprint":"abc","files":[]}"#,
        )
        .expect("write first manifest");
        let first = hash_specs_manifest_material(&path).expect("first hash");

        fs::write(
            &path,
            r#"{"schema_version":"1.0.0","template_version":"scaffold-v3","generated_at":"2Z","repo_signal_fingerprint":"abc","files":[]}"#,
        )
        .expect("write second manifest");
        let second = hash_specs_manifest_material(&path).expect("second hash");

        assert_eq!(
            first, second,
            "timestamp-only specs refreshes must not create new validation epochs"
        );
    }
}
