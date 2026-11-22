use crate::core::context_capsule::{
    DeterministicContextCapsule, context_capsules_dir, write_context_capsule,
};
use crate::core::error;
use crate::core::project_specs::{config_input_hash, repo_signal_fingerprint, spec_input_hash};
use crate::core::workunit::WorkUnitManifest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub const PORTABLE_CONTEXT_BUNDLE_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextBundleEnvironment {
    pub os: String,
    pub arch: String,
    pub decapod_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectIdentity {
    pub product_name: Option<String>,
    pub product_summary: Option<String>,
    pub architecture_direction: Option<String>,
    pub product_type: Option<String>,
    pub declared_capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortableContextBundle {
    pub schema_version: String,
    pub capsule: DeterministicContextCapsule,
    pub project_identity: ProjectIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workunit: Option<WorkUnitManifest>,
    #[serde(default)]
    pub uncertainty: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    pub source_environment: ContextBundleEnvironment,
    pub bundle_hash: String,
}

impl PortableContextBundle {
    fn canonicalized_without_hash(&self) -> CanonicalBundle {
        let mut uncertainty = self.uncertainty.clone();
        uncertainty.sort();
        uncertainty.dedup();
        let mut constraints = self.constraints.clone();
        constraints.sort();
        constraints.dedup();
        CanonicalBundle {
            schema_version: self.schema_version.clone(),
            capsule: self.capsule.clone(),
            project_identity: self.project_identity.clone(),
            workunit: self.workunit.clone(),
            uncertainty,
            constraints,
            source_environment: self.source_environment.clone(),
        }
    }

    pub fn computed_hash_hex(&self) -> Result<String, serde_json::Error> {
        let bytes = serde_json::to_vec(&self.canonicalized_without_hash())?;
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Ok(format!("{:x}", hasher.finalize()))
    }

    pub fn with_recomputed_hash(&self) -> Result<Self, serde_json::Error> {
        let mut out = self.clone();
        out.bundle_hash = out.computed_hash_hex()?;
        Ok(out)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CanonicalBundle {
    schema_version: String,
    capsule: DeterministicContextCapsule,
    project_identity: ProjectIdentity,
    workunit: Option<WorkUnitManifest>,
    uncertainty: Vec<String>,
    constraints: Vec<String>,
    source_environment: ContextBundleEnvironment,
}

fn contains_private_policy_leak(text: &str, private_bodies: &[String]) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    private_bodies.iter().any(|secret| {
        let secret_trimmed = secret.trim();
        !secret_trimmed.is_empty()
            && (trimmed.contains(secret_trimmed)
                || (trimmed.len() >= 12 && secret_trimmed.contains(trimmed)))
    })
}

pub fn build_bundle(
    project_root: &Path,
    mut capsule: DeterministicContextCapsule,
    workunit: Option<WorkUnitManifest>,
    mut uncertainty: Vec<String>,
    mut constraints: Vec<String>,
) -> Result<PortableContextBundle, error::DecapodError> {
    let config = crate::cli::DecapodProjectConfig::load(project_root)?;
    let mut declared_capabilities = config.repo.capabilities.clone();
    declared_capabilities.sort();
    declared_capabilities.dedup();

    // Issue #877: Never leak organization_private policy text into portable context bundles.
    let loaded_overlay = crate::core::org_overlay::load_organization_overlay(project_root)?;
    let private_bodies = crate::core::org_overlay::private_directive_bodies(&loaded_overlay);
    let private_ids: std::collections::BTreeSet<String> = capsule
        .authority_chain
        .as_ref()
        .map(crate::core::org_overlay::private_directive_ids)
        .unwrap_or_default();
    let redaction_marker =
        "[REDACTED: organization-private directive policy retained by hash only]";
    for snippet in &mut capsule.snippets {
        let base_id = snippet
            .source_path
            .strip_suffix(".md")
            .unwrap_or(&snippet.source_path);
        if private_ids.contains(&snippet.source_path)
            || private_ids.contains(base_id)
            || contains_private_policy_leak(&snippet.text, &private_bodies)
        {
            snippet.text = redaction_marker.to_string();
        }
    }
    for entry in &mut uncertainty {
        if contains_private_policy_leak(entry, &private_bodies) {
            *entry = redaction_marker.to_string();
        }
    }
    for entry in &mut constraints {
        if contains_private_policy_leak(entry, &private_bodies) {
            *entry = redaction_marker.to_string();
        }
    }
    if let Some(chain) = &mut capsule.authority_chain {
        for directive in &mut chain.directives {
            if directive.visibility
                == crate::core::org_overlay::DirectiveVisibility::OrganizationPrivate
            {
                directive.redacted_for_portability = true;
            }
        }
    }
    capsule = capsule.with_recomputed_hash().map_err(|e| {
        error::DecapodError::ValidationError(format!("failed to hash redacted capsule: {e}"))
    })?;

    let bundle = PortableContextBundle {
        schema_version: PORTABLE_CONTEXT_BUNDLE_SCHEMA_VERSION.to_string(),
        capsule,
        project_identity: ProjectIdentity {
            product_name: config.repo.product_name,
            product_summary: config.repo.product_summary,
            architecture_direction: config.repo.architecture_direction,
            product_type: config.repo.product_type,
            declared_capabilities,
        },
        workunit,
        uncertainty,
        constraints,
        source_environment: ContextBundleEnvironment {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            decapod_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        bundle_hash: String::new(),
    };
    let normalized = bundle.with_recomputed_hash().map_err(|e| {
        error::DecapodError::ValidationError(format!("failed to hash context bundle: {e}"))
    })?;
    validate_bundle(project_root, &normalized)?;
    Ok(normalized)
}

pub fn validate_bundle(
    project_root: &Path,
    bundle: &PortableContextBundle,
) -> Result<(), error::DecapodError> {
    if serde_json::to_vec_pretty(bundle)
        .map_err(|e| error::DecapodError::ValidationError(format!("bundle encoding: {e}")))?
        .len()
        > crate::core::docs::MAX_CONTEXT_ENVELOPE_BYTES
    {
        return Err(error::DecapodError::ValidationError(
            "CONTEXT_BUDGET_EXCEEDED: complete bundle exceeds 262144 bytes".into(),
        ));
    }
    if bundle.schema_version != PORTABLE_CONTEXT_BUNDLE_SCHEMA_VERSION {
        return Err(error::DecapodError::ValidationError(format!(
            "CONTEXT_BUNDLE_INCOMPATIBLE: schema_version {} is not supported",
            bundle.schema_version
        )));
    }
    let expected_bundle_hash = bundle.computed_hash_hex().map_err(|e| {
        error::DecapodError::ValidationError(format!("CONTEXT_BUNDLE_INVALID: {e}"))
    })?;
    if bundle.bundle_hash != expected_bundle_hash {
        return Err(error::DecapodError::ValidationError(
            "CONTEXT_BUNDLE_TAMPERED: bundle_hash does not match canonical contents".to_string(),
        ));
    }
    let expected_capsule_hash = bundle.capsule.computed_hash_hex().map_err(|e| {
        error::DecapodError::ValidationError(format!("CONTEXT_BUNDLE_INVALID: capsule hash: {e}"))
    })?;
    if bundle.capsule.capsule_hash != expected_capsule_hash {
        return Err(error::DecapodError::ValidationError(
            "CONTEXT_BUNDLE_TAMPERED: capsule_hash does not match canonical contents".to_string(),
        ));
    }
    let current_config = config_input_hash(project_root)?;
    let current_knowledge = crate::plugins::knowledge::resolve_context(
        &crate::core::store::Store {
            kind: crate::core::store::StoreKind::Repo,
            root: crate::core::store::find_governance_root(project_root).join(".decapod/data"),
        },
        project_root,
        Some(&bundle.capsule.topic),
        bundle.capsule.task_id.as_deref(),
        &[],
        None,
    )?;
    let stale_knowledge = match &bundle.capsule.knowledge {
        Some(previous) => *previous != current_knowledge,
        None => {
            !current_knowledge.records.is_empty()
                || !current_knowledge.conflicts.is_empty()
                || current_knowledge.status == "candidate_budget_exceeded"
        }
    };
    if stale_knowledge {
        return Err(error::DecapodError::ValidationError(
            "CONTEXT_BUNDLE_STALE_KNOWLEDGE: refresh the current knowledge projection".into(),
        ));
    }
    let current_plan =
        crate::governed_plan_context(project_root, bundle.capsule.task_id.as_deref())?;
    if match &bundle.capsule.governed_plan {
        Some(previous) => *previous != current_plan,
        None => current_plan.status == "present",
    } {
        return Err(error::DecapodError::ValidationError(
            "CONTEXT_BUNDLE_STALE_INTENT: refresh the current governed plan projection".into(),
        ));
    }
    let current_specs = spec_input_hash(project_root)?;
    if bundle.capsule.config_input_hash != current_config
        || bundle.capsule.spec_input_hash != current_specs
    {
        return Err(error::DecapodError::ValidationError(
            "CONTEXT_BUNDLE_STALE: config/spec inputs differ from the receiving repository"
                .to_string(),
        ));
    }
    let config = crate::cli::DecapodProjectConfig::load(project_root)?;
    let mut current_capabilities = config.repo.capabilities;
    current_capabilities.sort();
    current_capabilities.dedup();
    if bundle.project_identity.declared_capabilities != current_capabilities
        || bundle.capsule.capabilities != current_capabilities
    {
        return Err(error::DecapodError::ValidationError(
            "CONTEXT_BUNDLE_IDENTITY_MISMATCH: declared capabilities differ from canonical config"
                .to_string(),
        ));
    }
    if let Some(workunit) = &bundle.workunit
        && bundle.capsule.task_id.as_deref() != Some(workunit.task_id.as_str())
    {
        return Err(error::DecapodError::ValidationError(
            "CONTEXT_BUNDLE_IDENTITY_MISMATCH: capsule task_id and workunit task_id differ"
                .to_string(),
        ));
    }
    if bundle.capsule.repo_signal_fingerprint != repo_signal_fingerprint(project_root)? {
        return Err(error::DecapodError::ValidationError(
            "CONTEXT_BUNDLE_STALE: repository signal fingerprint differs from the receiving repository"
                .to_string(),
        ));
    }
    if let Ok(loaded_overlay) = crate::core::org_overlay::load_organization_overlay(project_root) {
        let private_bodies = crate::core::org_overlay::private_directive_bodies(&loaded_overlay);
        if !private_bodies.is_empty() {
            let texts = bundle
                .capsule
                .snippets
                .iter()
                .map(|s| s.text.as_str())
                .chain(bundle.uncertainty.iter().map(String::as_str))
                .chain(bundle.constraints.iter().map(String::as_str));
            for text in texts {
                if contains_private_policy_leak(text, &private_bodies) {
                    return Err(error::DecapodError::ValidationError(
                        "ORG_OVERLAY_PRIVATE_LEAK_REJECTED: portable bundle contains organization_private policy text"
                            .to_string(),
                    ));
                }
            }
        }
    }
    Ok(())
}

pub fn write_bundle(
    project_root: &Path,
    bundle: &PortableContextBundle,
) -> Result<PathBuf, error::DecapodError> {
    validate_bundle(project_root, bundle)?;
    let dir = context_capsules_dir(project_root).join("bundles");
    crate::core::fs_permissions::ensure_private_dir(&dir).map_err(error::DecapodError::IoError)?;
    let path = dir.join(format!("{}.json", bundle.bundle_hash));
    let bytes = serde_json::to_vec_pretty(bundle).map_err(|e| {
        error::DecapodError::ValidationError(format!("failed to serialize context bundle: {e}"))
    })?;
    crate::core::fs_permissions::write_private(&path, bytes)
        .map_err(error::DecapodError::IoError)?;
    write_context_capsule(project_root, &bundle.capsule)?;
    Ok(path)
}

pub fn read_bundle(
    project_root: &Path,
    input: &Path,
) -> Result<PortableContextBundle, error::DecapodError> {
    let path = if input.is_absolute() {
        input.to_path_buf()
    } else {
        project_root.join(input)
    };
    let raw = fs::read_to_string(&path).map_err(error::DecapodError::IoError)?;
    let bundle = serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!("CONTEXT_BUNDLE_INVALID: {e}"))
    })?;
    validate_bundle(project_root, &bundle)?;
    Ok(bundle)
}
