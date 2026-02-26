use std::fs;
use std::path::Path;
use std::path::PathBuf;

use crate::core::error;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const LOCAL_PROJECT_SPECS_DIR: &str = ".decapod/generated/specs";
pub const LOCAL_PROJECT_SPECS_README: &str = ".decapod/generated/specs/README.md";
pub const LOCAL_PROJECT_SPECS_INTENT: &str = ".decapod/generated/specs/INTENT.md";
pub const LOCAL_PROJECT_SPECS_ARCHITECTURE: &str = ".decapod/generated/specs/ARCHITECTURE.md";
pub const LOCAL_PROJECT_SPECS_INTERFACES: &str = ".decapod/generated/specs/INTERFACES.md";
pub const LOCAL_PROJECT_SPECS_VALIDATION: &str = ".decapod/generated/specs/VALIDATION.md";
pub const LOCAL_PROJECT_SPECS_SEMANTICS: &str = ".decapod/generated/specs/SEMANTICS.md";
pub const LOCAL_PROJECT_SPECS_OPERATIONS: &str = ".decapod/generated/specs/OPERATIONS.md";
pub const LOCAL_PROJECT_SPECS_SECURITY: &str = ".decapod/generated/specs/SECURITY.md";
pub const LOCAL_PROJECT_SPECS_MANIFEST: &str = ".decapod/generated/specs/.manifest.json";
pub const LOCAL_PROJECT_SPECS_MANIFEST_SCHEMA: &str = "1.0.0";

#[derive(Clone, Copy, Debug)]
pub struct LocalProjectSpec {
    pub path: &'static str,
    pub role: &'static str,
    pub constitution_ref: &'static str,
}

pub const LOCAL_PROJECT_SPECS: &[LocalProjectSpec] = &[
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_README,
        role: "specs_index",
        constitution_ref: "interfaces/PROJECT_SPECS.md#Canonical Local Project Specs Set",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_INTENT,
        role: "intent_purpose",
        constitution_ref: "specs/INTENT.md",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_ARCHITECTURE,
        role: "implementation_architecture",
        constitution_ref: "interfaces/ARCHITECTURE_FOUNDATIONS.md",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_INTERFACES,
        role: "service_contracts",
        constitution_ref: "interfaces/CONTROL_PLANE.md",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_VALIDATION,
        role: "proof_and_gate_plan",
        constitution_ref: "interfaces/TESTING.md",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_SEMANTICS,
        role: "state_machines_and_invariants",
        constitution_ref: "interfaces/PROJECT_SPECS.md",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_OPERATIONS,
        role: "operational_readiness",
        constitution_ref: "interfaces/PROJECT_SPECS.md",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_SECURITY,
        role: "security_posture",
        constitution_ref: "interfaces/PROJECT_SPECS.md",
    },
];

#[derive(Debug, Clone, Default)]
pub struct LocalProjectSpecsContext {
    pub intent: Option<String>,
    pub architecture: Option<String>,
    pub interfaces: Option<String>,
    pub validation: Option<String>,
    pub semantics: Option<String>,
    pub operations: Option<String>,
    pub security: Option<String>,
    pub canonical_paths: Vec<String>,
    pub constitution_refs: Vec<String>,
    pub update_guidance: String,
}

fn read_if_exists(project_root: &Path, rel_path: &str) -> Option<String> {
    let path = project_root.join(rel_path);
    if !path.exists() {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn first_meaningful_line(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('-'))
        .map(|s| s.to_string())
}

pub fn local_project_specs_context(project_root: &Path) -> LocalProjectSpecsContext {
    let mut ctx = LocalProjectSpecsContext::default();
    for spec in LOCAL_PROJECT_SPECS {
        ctx.canonical_paths.push(spec.path.to_string());
        ctx.constitution_refs
            .push(spec.constitution_ref.to_string());
    }
    ctx.constitution_refs.sort();
    ctx.constitution_refs.dedup();

    ctx.intent = read_if_exists(project_root, LOCAL_PROJECT_SPECS_INTENT)
        .and_then(|s| first_meaningful_line(&s));
    ctx.architecture = read_if_exists(project_root, LOCAL_PROJECT_SPECS_ARCHITECTURE)
        .and_then(|s| first_meaningful_line(&s));
    ctx.interfaces = read_if_exists(project_root, LOCAL_PROJECT_SPECS_INTERFACES)
        .and_then(|s| first_meaningful_line(&s));
    ctx.validation = read_if_exists(project_root, LOCAL_PROJECT_SPECS_VALIDATION)
        .and_then(|s| first_meaningful_line(&s));
    ctx.semantics = read_if_exists(project_root, LOCAL_PROJECT_SPECS_SEMANTICS)
        .and_then(|s| first_meaningful_line(&s));
    ctx.operations = read_if_exists(project_root, LOCAL_PROJECT_SPECS_OPERATIONS)
        .and_then(|s| first_meaningful_line(&s));
    ctx.security = read_if_exists(project_root, LOCAL_PROJECT_SPECS_SECURITY)
        .and_then(|s| first_meaningful_line(&s));
    ctx.update_guidance = "Treat .decapod/generated/specs/*.md as living project contracts: when user intent, interfaces, architecture, or proof gates change, update these specs before implementation proceeds.".to_string();
    ctx
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSpecManifestEntry {
    pub path: String,
    pub template_hash: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSpecsManifest {
    pub schema_version: String,
    pub template_version: String,
    pub generated_at: String,
    pub repo_signal_fingerprint: String,
    pub files: Vec<ProjectSpecManifestEntry>,
}

pub fn hash_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn collect_significant_repo_paths(
    root: &Path,
    dir: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<(), error::DecapodError> {
    if !dir.is_dir() {
        return Ok(());
    }
    let name = dir.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if matches!(
        name,
        ".git" | ".decapod" | "target" | "node_modules" | ".venv"
    ) {
        return Ok(());
    }

    for entry in fs::read_dir(dir).map_err(error::DecapodError::IoError)? {
        let entry = entry.map_err(error::DecapodError::IoError)?;
        let path = entry.path();
        if path.is_dir() {
            collect_significant_repo_paths(root, &path, out)?;
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let rel = match path.strip_prefix(root) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let rel_str = rel.to_string_lossy();
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let top_level_signal = matches!(
            file_name,
            "Cargo.toml"
                | "Cargo.lock"
                | "package.json"
                | "package-lock.json"
                | "pyproject.toml"
                | "requirements.txt"
                | "go.mod"
                | "go.sum"
                | "Dockerfile"
                | "docker-compose.yml"
                | "docker-compose.yaml"
                | "compose.yml"
                | "compose.yaml"
                | "README.md"
                | "Makefile"
        );
        let path_signal = rel_str.starts_with(".github/workflows/")
            || rel_str.starts_with("src/")
            || rel_str.starts_with("app/")
            || rel_str.starts_with("api/")
            || rel_str.starts_with("backend/")
            || rel_str.starts_with("frontend/")
            || rel_str.starts_with("web/")
            || rel_str.starts_with("services/")
            || rel_str.starts_with("infra/")
            || rel_str.starts_with("deploy/")
            || rel_str.starts_with("k8s/")
            || rel_str.ends_with(".sql");
        if top_level_signal || path_signal {
            out.push(path);
        }
    }
    Ok(())
}

pub fn repo_signal_fingerprint(project_root: &Path) -> Result<String, error::DecapodError> {
    let mut files = Vec::new();
    collect_significant_repo_paths(project_root, project_root, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for path in files {
        let rel = path
            .strip_prefix(project_root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .to_string();
        hasher.update(rel.as_bytes());
        hasher.update(b"\0");
        let content = fs::read(&path).map_err(error::DecapodError::IoError)?;
        let content_hash = hash_text(&String::from_utf8_lossy(&content));
        hasher.update(content_hash.as_bytes());
        hasher.update(b"\n");
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn read_specs_manifest(
    project_root: &Path,
) -> Result<Option<ProjectSpecsManifest>, error::DecapodError> {
    let path = project_root.join(LOCAL_PROJECT_SPECS_MANIFEST);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).map_err(error::DecapodError::IoError)?;
    let manifest: ProjectSpecsManifest = serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!("Invalid project specs manifest: {}", e))
    })?;
    Ok(Some(manifest))
}
