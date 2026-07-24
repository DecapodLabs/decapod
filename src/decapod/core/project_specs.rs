use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use crate::core::capabilities::reconcile_capability_overlays;
use crate::core::error;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const LOCAL_PROJECT_SPECS_DIR: &str = ".decapod/managed/specs";
pub const LOCAL_PROJECT_SPECS_README: &str = ".decapod/managed/specs/README.md";
pub const LOCAL_PROJECT_SPECS_INTENT: &str = ".decapod/managed/specs/INTENT.md";
pub const LOCAL_PROJECT_SPECS_ARCHITECTURE: &str = ".decapod/managed/specs/ARCHITECTURE.md";
pub const LOCAL_PROJECT_SPECS_INTERFACES: &str = ".decapod/managed/specs/INTERFACES.md";
pub const LOCAL_PROJECT_SPECS_VALIDATION: &str = ".decapod/managed/specs/VALIDATION.md";
pub const LOCAL_PROJECT_SPECS_SEMANTICS: &str = ".decapod/managed/specs/SEMANTICS.md";
pub const LOCAL_PROJECT_SPECS_OPERATIONS: &str = ".decapod/managed/specs/OPERATIONS.md";
pub const LOCAL_PROJECT_SPECS_SECURITY: &str = ".decapod/managed/specs/SECURITY.md";
pub const LOCAL_PROJECT_SPECS_MANIFEST: &str = ".decapod/managed/specs/.manifest.json";
pub const LOCAL_PROJECT_SPECS_MANIFEST_SCHEMA: &str = "1.1.0";
pub const CAPABILITY_DEFINITION_VERSION: &str = "1.0.0";

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
        constitution_ref: "interfaces/PROJECT_SPECS#Canonical Local Project Specs Set",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_INTENT,
        role: "intent_purpose",
        constitution_ref: "specs/INTENT",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_ARCHITECTURE,
        role: "implementation_architecture",
        constitution_ref: "interfaces/ARCHITECTURE_FOUNDATIONS",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_INTERFACES,
        role: "service_contracts",
        constitution_ref: "interfaces/CONTROL_PLANE",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_VALIDATION,
        role: "proof_and_gate_plan",
        constitution_ref: "interfaces/TESTING",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_SEMANTICS,
        role: "state_machines_and_invariants",
        constitution_ref: "interfaces/PROJECT_SPECS",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_OPERATIONS,
        role: "operational_readiness",
        constitution_ref: "interfaces/PROJECT_SPECS",
    },
    LocalProjectSpec {
        path: LOCAL_PROJECT_SPECS_SECURITY,
        role: "security_posture",
        constitution_ref: "interfaces/PROJECT_SPECS",
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

pub fn first_markdown_content_line(markdown: &str) -> Option<String> {
    let mut in_fence = false;
    for line in markdown.lines() {
        let mut trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence
            || trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with('<')
            || trimmed.starts_with("![")
            || trimmed.starts_with('|')
            || trimmed == "---"
        {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("- ") {
            trimmed = rest.trim();
        } else if let Some(rest) = trimmed.strip_prefix("* ") {
            trimmed = rest.trim();
        }
        if trimmed.is_empty() || trimmed.starts_with("[ ]") || trimmed.starts_with("[x]") {
            continue;
        }
        return Some(trimmed.to_string());
    }
    None
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
        .and_then(|s| first_markdown_content_line(&s));
    ctx.architecture = read_if_exists(project_root, LOCAL_PROJECT_SPECS_ARCHITECTURE)
        .and_then(|s| first_markdown_content_line(&s));
    ctx.interfaces = read_if_exists(project_root, LOCAL_PROJECT_SPECS_INTERFACES)
        .and_then(|s| first_markdown_content_line(&s));
    ctx.validation = read_if_exists(project_root, LOCAL_PROJECT_SPECS_VALIDATION)
        .and_then(|s| first_markdown_content_line(&s));
    ctx.semantics = read_if_exists(project_root, LOCAL_PROJECT_SPECS_SEMANTICS)
        .and_then(|s| first_markdown_content_line(&s));
    ctx.operations = read_if_exists(project_root, LOCAL_PROJECT_SPECS_OPERATIONS)
        .and_then(|s| first_markdown_content_line(&s));
    ctx.security = read_if_exists(project_root, LOCAL_PROJECT_SPECS_SECURITY)
        .and_then(|s| first_markdown_content_line(&s));
    ctx.update_guidance = "Treat .decapod/managed/specs/*.md as living project contracts: when user intent, interfaces, architecture, or proof gates change, update these specs before implementation proceeds.".to_string();
    ctx
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSpecManifestEntry {
    pub path: String,
    pub template_hash: String,
    pub content_hash: String,
    #[serde(default)]
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSpecsManifest {
    pub schema_version: String,
    pub template_version: String,
    pub generated_at: String,
    pub repo_signal_fingerprint: String,
    /// Optional for manifests written before capability provenance was added.
    /// Refresh upgrades legacy manifests to the current schema deterministically.
    #[serde(default)]
    pub declared_capabilities: Vec<String>,
    #[serde(default)]
    pub capability_definition_version: String,
    /// Hash of the canonical, parsed `.decapod/config.toml` input.
    #[serde(default)]
    pub config_input_hash: String,
    /// Hash of the ordered living-spec inputs, excluding this manifest.
    #[serde(default)]
    pub spec_input_hash: String,
    /// Release identity of the Decapod binary that produced the projections.
    #[serde(default)]
    pub decapod_release: String,
    /// Generated agent entrypoints attested using the same template/content
    /// hash shape as the living spec files above.
    #[serde(default)]
    pub entrypoints: Vec<ProjectSpecManifestEntry>,
    pub files: Vec<ProjectSpecManifestEntry>,
}

pub fn hash_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn config_input_hash(project_root: &Path) -> Result<String, error::DecapodError> {
    if !project_root.join(".decapod/config.toml").exists() {
        return Ok(String::new());
    }
    let config = crate::cli::DecapodProjectConfig::load(project_root)?;
    let canonical = serde_json::to_vec(&config).map_err(|e| {
        error::DecapodError::ValidationError(format!("Failed to canonicalize config.toml: {e}"))
    })?;
    let override_content =
        fs::read_to_string(project_root.join(".decapod/OVERRIDE.md")).unwrap_or_default();
    let mut input = String::from_utf8_lossy(&canonical).into_owned();
    input.push_str("\n-- OVERRIDE.md --\n");
    input.push_str(&override_content);
    Ok(hash_text(&input))
}

pub fn spec_input_hash(project_root: &Path) -> Result<String, error::DecapodError> {
    let mut hasher = Sha256::new();
    for spec in LOCAL_PROJECT_SPECS {
        let path = project_root.join(spec.path);
        if !path.exists() {
            hasher.update(spec.path.as_bytes());
            hasher.update(b"\0absent\n");
            continue;
        }
        let body = fs::read_to_string(path).map_err(error::DecapodError::IoError)?;
        hasher.update(spec.path.as_bytes());
        hasher.update(b"\0");
        hasher.update(hash_text(&body).as_bytes());
        hasher.update(b"\n");
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn repo_signal_requires_content_hash(rel_path: &str) -> bool {
    rel_path == "AGENTS.md"
        || rel_path == "CLAUDE.md"
        || rel_path == "CODEX.md"
        || rel_path == "GEMINI.md"
        || rel_path == "Cargo.toml"
        || rel_path == "Cargo.lock"
        || rel_path == "package.json"
        || rel_path == "package-lock.json"
        || rel_path == "pyproject.toml"
        || rel_path == "requirements.txt"
        || rel_path == "go.mod"
        || rel_path == "go.sum"
        || rel_path == "Dockerfile"
        || rel_path == "docker-compose.yml"
        || rel_path == "docker-compose.yaml"
        || rel_path == "compose.yml"
        || rel_path == "compose.yaml"
        || rel_path == "README.md"
        || rel_path == "Makefile"
        || rel_path.starts_with("infra/")
        || rel_path.starts_with("deploy/")
        || rel_path.starts_with("k8s/")
        || rel_path.starts_with("src/")
        || rel_path.starts_with("tests/")
        || rel_path.starts_with(".github/workflows/")
        || rel_path.ends_with(".sql")
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
        if repo_signal_requires_content_hash(&rel) {
            let content = fs::read(&path).map_err(error::DecapodError::IoError)?;
            let content_hash = hash_text(&String::from_utf8_lossy(&content));
            hasher.update(content_hash.as_bytes());
        } else {
            hasher.update(b"path-only");
        }
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
        error::DecapodError::ValidationError(format!("Invalid project specs manifest: {e}"))
    })?;
    Ok(Some(manifest))
}

pub fn entrypoint_manifest_entries(
    project_root: &Path,
) -> Result<Vec<ProjectSpecManifestEntry>, error::DecapodError> {
    let mut entries = Vec::new();
    for surface in crate::core::entrypoint_integrity::ENTRYPOINT_FILES {
        let Some(rendered) = crate::core::entrypoint_integrity::render_entrypoint(surface) else {
            continue;
        };
        let path = project_root.join(surface);
        let content_hash = if path.is_file() {
            hash_text(&fs::read_to_string(&path).map_err(error::DecapodError::IoError)?)
        } else {
            String::new()
        };
        entries.push(ProjectSpecManifestEntry {
            path: surface.to_string(),
            template_hash: hash_text(&rendered),
            content_hash,
            fingerprint: crate::core::entrypoint_integrity::expected_fingerprint(surface)
                .unwrap_or_default()
                .to_string(),
        });
    }
    Ok(entries)
}

pub fn refresh_specs_manifest(
    project_root: &Path,
    declared_capabilities: &[String],
) -> Result<ProjectSpecsManifest, error::DecapodError> {
    let existing = read_specs_manifest(project_root)?;

    let mut manifest_entries = Vec::new();
    for spec in LOCAL_PROJECT_SPECS {
        let path = project_root.join(spec.path);
        if !path.exists() {
            continue;
        }
        let body = fs::read_to_string(&path).map_err(error::DecapodError::IoError)?;
        let content_hash = hash_text(&body);

        let template_hash = existing
            .as_ref()
            .and_then(|manifest| {
                manifest
                    .files
                    .iter()
                    .find(|f| f.path == spec.path)
                    .map(|f| f.template_hash.clone())
            })
            .unwrap_or_else(|| content_hash.clone());

        manifest_entries.push(ProjectSpecManifestEntry {
            path: spec.path.to_string(),
            template_hash,
            content_hash,
            fingerprint: String::new(),
        });
    }

    let template_version = existing
        .as_ref()
        .map(|manifest| manifest.template_version.clone())
        .unwrap_or_else(|| crate::core::scaffold::PROJECT_SPEC_TEMPLATE_VERSION.to_string());
    let canonical_capabilities =
        crate::core::capabilities::CapabilityRegistry::canonicalize_capabilities(
            declared_capabilities,
        );
    let repo_signal_fingerprint = repo_signal_fingerprint(project_root)?;
    let config_input_hash = config_input_hash(project_root)?;
    let spec_input_hash = spec_input_hash(project_root)?;
    let entrypoints = entrypoint_manifest_entries(project_root)?;
    let generated_at = existing
        .as_ref()
        .filter(|manifest| {
            manifest.schema_version == LOCAL_PROJECT_SPECS_MANIFEST_SCHEMA
                && manifest.template_version == template_version
                && manifest.repo_signal_fingerprint == repo_signal_fingerprint
                && manifest.declared_capabilities == canonical_capabilities
                && manifest.capability_definition_version == CAPABILITY_DEFINITION_VERSION
                && manifest.config_input_hash == config_input_hash
                && manifest.spec_input_hash == spec_input_hash
                && manifest.decapod_release == crate::core::entrypoint_integrity::RELEASE_VERSION
                && manifest.entrypoints == entrypoints
                && manifest.files == manifest_entries
        })
        .map(|manifest| manifest.generated_at.clone())
        .unwrap_or_else(crate::core::time::now_epoch_z);
    let manifest = ProjectSpecsManifest {
        schema_version: LOCAL_PROJECT_SPECS_MANIFEST_SCHEMA.to_string(),
        template_version,
        generated_at,
        repo_signal_fingerprint,
        declared_capabilities: canonical_capabilities,
        capability_definition_version: CAPABILITY_DEFINITION_VERSION.to_string(),
        config_input_hash,
        spec_input_hash,
        decapod_release: crate::core::entrypoint_integrity::RELEASE_VERSION.to_string(),
        entrypoints,
        files: manifest_entries,
    };

    let manifest_path = project_root.join(LOCAL_PROJECT_SPECS_MANIFEST);
    let manifest_body = serde_json::to_string_pretty(&manifest).map_err(|e| {
        error::DecapodError::ValidationError(format!("Failed to serialize specs manifest: {e}"))
    })?;
    fs::write(manifest_path, manifest_body).map_err(error::DecapodError::IoError)?;

    Ok(manifest)
}

const CODEBASE_ATTESTATION_START: &str = "<!-- decapod:codebase-attestation:start -->";
const CODEBASE_ATTESTATION_END: &str = "<!-- decapod:codebase-attestation:end -->";

fn codebase_surface_summary(project_root: &Path) -> Result<String, error::DecapodError> {
    let mut files = Vec::new();
    collect_significant_repo_paths(project_root, project_root, &mut files)?;
    let mut counts = BTreeMap::<String, usize>::new();
    for path in files {
        let rel = path
            .strip_prefix(project_root)
            .unwrap_or(path.as_path())
            .to_string_lossy();
        let surface = rel.split('/').next().unwrap_or(".").to_string();
        *counts.entry(surface).or_default() += 1;
    }
    Ok(counts
        .into_iter()
        .map(|(surface, count)| format!("`{surface}/` ({count} files)"))
        .collect::<Vec<_>>()
        .join(", "))
}

fn update_codebase_attestation(body: &str, fingerprint: &str, surfaces: &str) -> String {
    let section = format!(
        "{CODEBASE_ATTESTATION_START}\n## Codebase Attestation\n\n- Repository signal fingerprint: `{fingerprint}`\n- Significant implementation surfaces: {surfaces}\n- Refreshed from the current codebase by `decapod specs.refresh`\n{CODEBASE_ATTESTATION_END}"
    );
    if let Some(start) = body.find(CODEBASE_ATTESTATION_START) {
        let end = body[start..]
            .find(CODEBASE_ATTESTATION_END)
            .map(|offset| start + offset + CODEBASE_ATTESTATION_END.len());
        if let Some(end) = end {
            let mut updated = String::with_capacity(body.len());
            updated.push_str(&body[..start]);
            updated.push_str(&section);
            updated.push_str(&body[end..]);
            return updated;
        }
    }
    format!("{}\n\n{}\n", body.trim_end(), section)
}

/// Re-evaluate existing living specs against the current repository.
///
/// This intentionally preserves each document's authored content and updates
/// only the codebase-derived attestation block plus manifest hashes. Scaffold
/// rendering belongs exclusively to fresh `decapod init`.
pub fn refresh_specs_from_codebase(
    project_root: &Path,
    declared_capabilities: &[String],
) -> Result<ProjectSpecsManifest, error::DecapodError> {
    let fingerprint = repo_signal_fingerprint(project_root)?;
    let surfaces = codebase_surface_summary(project_root)?;
    for spec in LOCAL_PROJECT_SPECS {
        let path = project_root.join(spec.path);
        if !path.exists() {
            continue;
        }
        let body = fs::read_to_string(&path).map_err(error::DecapodError::IoError)?;
        let updated = reconcile_capability_overlays(spec.path, body.clone(), declared_capabilities);
        let updated = update_codebase_attestation(&updated, &fingerprint, &surfaces);
        if updated != body {
            fs::write(path, updated).map_err(error::DecapodError::IoError)?;
        }
    }
    refresh_specs_manifest(project_root, declared_capabilities)
}
#[cfg(test)]
#[path = "../../../tests/unit/core/project_specs_tests.rs"]
mod tests;
