use crate::core::error;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const GENERATED_POLICY_REL_PATH: &str = ".decapod/generated/policy/context_capsule_policy.json";
pub const OVERRIDE_POLICY_REL_PATH: &str = ".decapod/policy/context_capsule_policy.json";
pub const POLICY_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapsuleRiskTierRule {
    pub allowed_scopes: Vec<String>,
    pub max_limit: usize,
    pub allow_write: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapsulePolicyContract {
    pub schema_version: String,
    pub policy_version: String,
    pub repo_revision_binding: String,
    pub default_risk_tier: String,
    pub tiers: BTreeMap<String, CapsuleRiskTierRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CapsulePolicyBinding {
    pub risk_tier: String,
    pub policy_hash: String,
    pub policy_version: String,
    pub policy_path: String,
    pub repo_revision: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedCapsulePolicy {
    pub binding: CapsulePolicyBinding,
    pub effective_limit: usize,
}

pub fn default_capsule_policy_contract() -> CapsulePolicyContract {
    let mut tiers = BTreeMap::new();
    tiers.insert(
        "low".to_string(),
        CapsuleRiskTierRule {
            allowed_scopes: vec!["interfaces".to_string()],
            max_limit: 4,
            allow_write: false,
        },
    );
    tiers.insert(
        "medium".to_string(),
        CapsuleRiskTierRule {
            allowed_scopes: vec!["core".to_string(), "interfaces".to_string()],
            max_limit: 6,
            allow_write: true,
        },
    );
    tiers.insert(
        "high".to_string(),
        CapsuleRiskTierRule {
            allowed_scopes: vec![
                "core".to_string(),
                "interfaces".to_string(),
                "plugins".to_string(),
            ],
            max_limit: 12,
            allow_write: true,
        },
    );
    tiers.insert(
        "critical".to_string(),
        CapsuleRiskTierRule {
            allowed_scopes: vec![
                "core".to_string(),
                "interfaces".to_string(),
                "plugins".to_string(),
            ],
            max_limit: 20,
            allow_write: true,
        },
    );

    CapsulePolicyContract {
        schema_version: POLICY_SCHEMA_VERSION.to_string(),
        policy_version: "jit-capsule-policy-v1".to_string(),
        repo_revision_binding: "HEAD".to_string(),
        default_risk_tier: "medium".to_string(),
        tiers,
    }
}

pub fn default_policy_json_pretty() -> Result<String, error::DecapodError> {
    serde_json::to_string_pretty(&default_capsule_policy_contract()).map_err(|e| {
        error::DecapodError::ValidationError(format!("CAPSULE_POLICY_ENCODE_FAILED: {}", e))
    })
}

pub fn ensure_generated_policy_contract(project_root: &Path) -> Result<(), error::DecapodError> {
    let path = project_root.join(GENERATED_POLICY_REL_PATH);
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    }
    let body = default_policy_json_pretty()?;
    fs::write(path, body).map_err(error::DecapodError::IoError)?;
    Ok(())
}

fn policy_path_candidates(project_root: &Path) -> Vec<PathBuf> {
    vec![
        project_root.join(OVERRIDE_POLICY_REL_PATH),
        project_root.join(GENERATED_POLICY_REL_PATH),
    ]
}

fn resolve_policy_path(project_root: &Path) -> Option<PathBuf> {
    policy_path_candidates(project_root)
        .into_iter()
        .find(|p| p.exists())
}

pub fn load_policy_contract(
    project_root: &Path,
) -> Result<(CapsulePolicyContract, PathBuf), error::DecapodError> {
    let path = resolve_policy_path(project_root).ok_or_else(|| {
        error::DecapodError::ValidationError(format!(
            "CAPSULE_POLICY_MISSING: expected {} or {}",
            OVERRIDE_POLICY_REL_PATH, GENERATED_POLICY_REL_PATH
        ))
    })?;
    let raw = fs::read_to_string(&path).map_err(error::DecapodError::IoError)?;
    let parsed: CapsulePolicyContract = serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!("CAPSULE_POLICY_INVALID: {}", e))
    })?;
    if parsed.schema_version != POLICY_SCHEMA_VERSION {
        return Err(error::DecapodError::ValidationError(format!(
            "CAPSULE_POLICY_SCHEMA_MISMATCH: actual={} expected={}",
            parsed.schema_version, POLICY_SCHEMA_VERSION
        )));
    }
    Ok((parsed, path))
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn resolve_repo_revision(
    project_root: &Path,
    binding: &str,
) -> Result<String, error::DecapodError> {
    if !binding.eq_ignore_ascii_case("HEAD") {
        return Err(error::DecapodError::ValidationError(format!(
            "CAPSULE_POLICY_UNSUPPORTED_BINDING: {}",
            binding
        )));
    }
    let output = Command::new("git")
        .current_dir(project_root)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(error::DecapodError::IoError)?;
    if !output.status.success() {
        let branch_output = Command::new("git")
            .current_dir(project_root)
            .args(["symbolic-ref", "--short", "HEAD"])
            .output()
            .map_err(error::DecapodError::IoError)?;
        if branch_output.status.success() {
            let branch = String::from_utf8_lossy(&branch_output.stdout)
                .trim()
                .to_string();
            if !branch.is_empty() {
                return Ok(format!("UNBORN:{}", branch));
            }
        }
        return Err(error::DecapodError::ValidationError(
            "CAPSULE_POLICY_REPO_REVISION_UNRESOLVED".to_string(),
        ));
    }
    let rev = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if rev.is_empty() {
        return Err(error::DecapodError::ValidationError(
            "CAPSULE_POLICY_REPO_REVISION_UNRESOLVED".to_string(),
        ));
    }
    Ok(rev)
}

pub fn resolve_capsule_policy(
    project_root: &Path,
    requested_scope: &str,
    requested_risk_tier: Option<&str>,
    requested_limit: usize,
    write: bool,
) -> Result<ResolvedCapsulePolicy, error::DecapodError> {
    if !matches!(requested_scope, "core" | "interfaces" | "plugins") {
        return Err(error::DecapodError::ValidationError(format!(
            "invalid scope '{}': expected one of core|interfaces|plugins",
            requested_scope
        )));
    }
    let (contract, policy_path) = load_policy_contract(project_root)?;
    let risk_tier = requested_risk_tier
        .unwrap_or(contract.default_risk_tier.as_str())
        .trim()
        .to_lowercase();
    let rule = contract.tiers.get(&risk_tier).ok_or_else(|| {
        error::DecapodError::ValidationError(format!("CAPSULE_RISK_TIER_UNKNOWN: {}", risk_tier))
    })?;
    if !rule.allowed_scopes.iter().any(|s| s == requested_scope) {
        return Err(error::DecapodError::ValidationError(format!(
            "CAPSULE_SCOPE_DENIED: scope={} risk_tier={}",
            requested_scope, risk_tier
        )));
    }
    if write && !rule.allow_write {
        return Err(error::DecapodError::ValidationError(format!(
            "CAPSULE_WRITE_DENIED: risk_tier={}",
            risk_tier
        )));
    }

    let policy_bytes = fs::read(&policy_path).map_err(error::DecapodError::IoError)?;
    let policy_hash = hash_bytes(&policy_bytes);
    let repo_revision = resolve_repo_revision(project_root, &contract.repo_revision_binding)?;
    let effective_limit = requested_limit.clamp(1, rule.max_limit.max(1));
    let rel_policy_path = policy_path
        .strip_prefix(project_root)
        .unwrap_or(policy_path.as_path())
        .to_string_lossy()
        .to_string();

    Ok(ResolvedCapsulePolicy {
        binding: CapsulePolicyBinding {
            risk_tier,
            policy_hash,
            policy_version: contract.policy_version,
            policy_path: rel_policy_path,
            repo_revision,
        },
        effective_limit,
    })
}
