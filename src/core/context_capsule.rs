use crate::core::{assets, docs, error};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContextCapsuleSource {
    pub path: String,
    pub section: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContextCapsuleSnippet {
    pub source_path: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeterministicContextCapsule {
    pub topic: String,
    pub scope: String,
    pub task_id: Option<String>,
    pub workunit_id: Option<String>,
    pub sources: Vec<ContextCapsuleSource>,
    pub snippets: Vec<ContextCapsuleSnippet>,
    pub capsule_hash: String,
}

impl DeterministicContextCapsule {
    fn canonicalized_without_hash(&self) -> CanonicalCapsule {
        let mut sources = self.sources.clone();
        sources.sort();
        sources.dedup();

        let mut snippets = self.snippets.clone();
        snippets.sort();
        snippets.dedup();

        CanonicalCapsule {
            topic: self.topic.clone(),
            scope: self.scope.clone(),
            task_id: self.task_id.clone(),
            workunit_id: self.workunit_id.clone(),
            sources,
            snippets,
        }
    }

    pub fn canonical_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.canonicalized_without_hash())
    }

    pub fn computed_hash_hex(&self) -> Result<String, serde_json::Error> {
        let bytes = self.canonical_json_bytes()?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        Ok(format!("{:x}", hasher.finalize()))
    }

    pub fn with_recomputed_hash(&self) -> Result<Self, serde_json::Error> {
        let mut out = self.clone();
        out.capsule_hash = out.computed_hash_hex()?;
        Ok(out)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CanonicalCapsule {
    topic: String,
    scope: String,
    task_id: Option<String>,
    workunit_id: Option<String>,
    sources: Vec<ContextCapsuleSource>,
    snippets: Vec<ContextCapsuleSnippet>,
}

pub fn query_embedded_capsule(
    repo_root: &Path,
    topic: &str,
    scope: &str,
    task_id: Option<&str>,
    workunit_id: Option<&str>,
    limit: usize,
) -> Result<DeterministicContextCapsule, error::DecapodError> {
    validate_scope(scope)?;
    if topic.trim().is_empty() {
        return Err(error::DecapodError::ValidationError(
            "topic cannot be empty".to_string(),
        ));
    }
    let max = limit.max(1);
    let scope_prefix = format!("{}/", scope);

    let mut fragments = docs::resolve_scoped_fragments(
        repo_root,
        Some(topic),
        None,
        &[],
        &[],
        max.saturating_mul(3),
    )
    .into_iter()
    .filter(|f| f.r#ref.starts_with(&scope_prefix))
    .collect::<Vec<_>>();

    if fragments.is_empty() {
        let mut paths = assets::list_docs()
            .into_iter()
            .filter(|p| p.starts_with(&scope_prefix))
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths.into_iter().take(max) {
            if let Some(fragment) = docs::get_fragment(repo_root, &path, None) {
                fragments.push(fragment);
            }
        }
    }

    fragments.truncate(max);

    let mut sources = Vec::new();
    let mut snippets = Vec::new();
    for fragment in fragments {
        let source_path = fragment
            .r#ref
            .split('#')
            .next()
            .unwrap_or(fragment.r#ref.as_str())
            .to_string();
        sources.push(ContextCapsuleSource {
            path: source_path.clone(),
            section: fragment.title.clone(),
        });
        snippets.push(ContextCapsuleSnippet {
            source_path,
            text: fragment.excerpt.trim().to_string(),
        });
    }

    let capsule = DeterministicContextCapsule {
        topic: topic.to_string(),
        scope: scope.to_string(),
        task_id: task_id.map(str::to_string),
        workunit_id: workunit_id.map(str::to_string),
        sources,
        snippets,
        capsule_hash: String::new(),
    };

    capsule.with_recomputed_hash().map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "failed to canonicalize context capsule: {}",
            e
        ))
    })
}

fn validate_scope(scope: &str) -> Result<(), error::DecapodError> {
    match scope {
        "core" | "interfaces" | "plugins" => Ok(()),
        _ => Err(error::DecapodError::ValidationError(format!(
            "invalid scope '{}': expected one of core|interfaces|plugins",
            scope
        ))),
    }
}

pub fn context_capsules_dir(project_root: &Path) -> PathBuf {
    project_root
        .join(".decapod")
        .join("generated")
        .join("context")
}

pub fn context_capsule_path(project_root: &Path, capsule: &DeterministicContextCapsule) -> PathBuf {
    let file_stem = if let Some(workunit_id) = capsule.workunit_id.as_ref() {
        workunit_id.clone()
    } else if let Some(task_id) = capsule.task_id.as_ref() {
        task_id.clone()
    } else {
        let input = format!("{}::{}", capsule.scope, capsule.topic);
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        let digest = format!("{:x}", hasher.finalize());
        format!("{}-{}", capsule.scope, &digest[..12])
    };
    context_capsules_dir(project_root).join(format!("{file_stem}.json"))
}

pub fn write_context_capsule(
    project_root: &Path,
    capsule: &DeterministicContextCapsule,
) -> Result<PathBuf, error::DecapodError> {
    let normalized = capsule.with_recomputed_hash().map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "failed to canonicalize context capsule: {}",
            e
        ))
    })?;
    let path = context_capsule_path(project_root, &normalized);
    let parent = path.parent().ok_or_else(|| {
        error::DecapodError::ValidationError("invalid context capsule parent path".to_string())
    })?;
    fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    let bytes = serde_json::to_vec_pretty(&normalized).map_err(|e| {
        error::DecapodError::ValidationError(format!("failed to serialize context capsule: {}", e))
    })?;
    fs::write(&path, bytes).map_err(error::DecapodError::IoError)?;
    Ok(path)
}
