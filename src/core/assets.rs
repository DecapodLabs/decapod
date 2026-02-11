// src/assets.rs
// Contains embedded assets for Decapod, including constitution documents, templates,
// and project living document templates.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Embedded constitution blob type (matches build.rs output)
pub type ConstitutionBlob = std::collections::HashMap<String, String>;

// Embed pre-compiled constitution blob from build.rs output
pub const CONSTITUTION_BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/constitution.blob"));

/// Lazy-loaded constitution blob (deserializes on first access)
static CONSTITUTION_CACHE: std::sync::OnceLock<ConstitutionBlob> = std::sync::OnceLock::new();

/// Get the embedded constitution blob (deserializes on first call)
pub fn get_constitution_blob() -> &'static ConstitutionBlob {
    CONSTITUTION_CACHE.get_or_init(|| {
        bincode::deserialize::<ConstitutionBlob>(CONSTITUTION_BLOB)
            .expect("Invalid constitution blob - build may be corrupted")
    })
}

/// Fast access to embedded document from blob
pub fn get_embedded_doc(path: &str) -> Option<String> {
    let blob = get_constitution_blob();

    // Handle both "embedded/..." and "..." formats
    let key = if path.starts_with("embedded/") {
        path.strip_prefix("embedded/").unwrap_or(path).to_string()
    } else {
        path.to_string()
    };

    blob.get(&key).cloned()
}

/// List all embedded document paths (from compiled blob)
pub fn list_docs() -> Vec<String> {
    let blob = get_constitution_blob();
    blob.keys().map(|k| format!("embedded/{}", k)).collect()
}

/// Legacy function - now just forwards to blob-based access
pub fn get_doc(path: &str) -> Option<String> {
    get_embedded_doc(path)
}

/// Compute checksum of .decapod/constitution directory for change detection
pub fn compute_override_checksum(repo_root: &std::path::Path) -> Result<String, std::io::Error> {
    let constitution_dir = repo_root.join(".decapod").join("constitution");
    let mut hasher = DefaultHasher::new();

    if !constitution_dir.exists() {
        return Ok("empty".to_string());
    }

    // Hash all .md files recursively
    fn hash_dir(dir: &std::path::Path, hasher: &mut DefaultHasher) -> Result<(), std::io::Error> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                hash_dir(&path, hasher)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
                let content = std::fs::read_to_string(&path)?;
                path.to_string_lossy().hash(hasher);
                content.hash(hasher);
            }
        }
        Ok(())
    }

    hash_dir(&constitution_dir, &mut hasher)?;
    Ok(format!("{:x}", hasher.finish()))
}

/// Compiled override blob containing merged constitution content
#[derive(serde::Serialize, serde::Deserialize)]
pub struct OverrideBlob {
    pub documents: std::collections::HashMap<String, String>,
}

/// Load override blob from cache or recompile if .decapod/constitution changed
/// This splices override content on top of embedded constitution blob
pub fn load_override_blob(
    repo_root: &std::path::Path,
) -> Result<OverrideBlob, Box<dyn std::error::Error + Send + Sync>> {
    let current_checksum = compute_override_checksum(repo_root)?;
    let blob_path = repo_root.join(".decapod").join("constitution.blob");
    let checksum_path = repo_root.join(".decapod").join("constitution.checksum");

    // Check cache validity
    if let Ok(stored_checksum) = std::fs::read_to_string(&checksum_path) {
        if stored_checksum == current_checksum && blob_path.exists() {
            // Cache hit - load existing override blob
            if let Ok(blob_data) = std::fs::read(&blob_path) {
                if let Ok(blob) = bincode::deserialize::<OverrideBlob>(&blob_data) {
                    return Ok(blob);
                }
            }
        }
    }

    // Cache miss - recompile override layer only (splices on embedded blob)
    let embedded_blob = get_constitution_blob();
    let mut spliced_documents = std::collections::HashMap::new();

    for (relative_path, embedded_content) in embedded_blob {
        let override_path = repo_root
            .join(".decapod")
            .join("constitution")
            .join(relative_path);

        let final_content = if override_path.exists() {
            if let Ok(override_content) = std::fs::read_to_string(&override_path) {
                merge_override_content(embedded_content, &override_content)
            } else {
                embedded_content.clone()
            }
        } else {
            embedded_content.clone()
        };

        spliced_documents.insert(relative_path.clone(), final_content);
    }

    let override_blob = OverrideBlob {
        documents: spliced_documents,
    };

    // Cache override blob and checksum
    std::fs::create_dir_all(blob_path.parent().unwrap())?;
    std::fs::write(&blob_path, bincode::serialize(&override_blob)?)?;
    std::fs::write(&checksum_path, current_checksum)?;

    Ok(override_blob)
}

/// Merge embedded content with override additions
fn merge_override_content(embedded_content: &str, override_content: &str) -> String {
    let overrides_section = if let Some(start) = override_content.find("## Overrides") {
        &override_content[start..]
    } else {
        override_content
    };

    let embedded_main = if let Some(start) = embedded_content.find("---") {
        embedded_content[start..].trim()
    } else {
        embedded_content.trim()
    };

    format!("{}\n\n{}", embedded_main, overrides_section)
}

/// Fast access to merged document from override blob
pub fn get_merged_doc(blob: &OverrideBlob, relative_path: &str) -> Option<String> {
    blob.documents.get(relative_path).cloned()
}

// Templates for user overrides (embedded for scaffolding)
pub const TEMPLATES_CORE_CONTROL_PLANE: &str =
    include_str!("../../constitution/templates/core/CONTROL_PLANE.md");
pub const TEMPLATES_CORE_DECAPOD: &str =
    include_str!("../../constitution/templates/core/DECAPOD.md");
pub const TEMPLATES_CORE_PLUGINS: &str =
    include_str!("../../constitution/templates/core/PLUGINS.md");
pub const TEMPLATES_CORE_CLAIMS: &str = include_str!("../../constitution/templates/core/CLAIMS.md");
pub const TEMPLATES_CORE_DEMANDS: &str =
    include_str!("../../constitution/templates/core/DEMANDS.md");
pub const TEMPLATES_CORE_DEPRECATION: &str =
    include_str!("../../constitution/templates/core/DEPRECATION.md");
pub const TEMPLATES_CORE_DOC_RULES: &str =
    include_str!("../../constitution/templates/core/DOC_RULES.md");
pub const TEMPLATES_CORE_GLOSSARY: &str =
    include_str!("../../constitution/templates/core/GLOSSARY.md");
pub const TEMPLATES_CORE_KNOWLEDGE: &str =
    include_str!("../../constitution/templates/core/KNOWLEDGE.md");
pub const TEMPLATES_CORE_MEMORY: &str = include_str!("../../constitution/templates/core/MEMORY.md");
pub const TEMPLATES_CORE_SOUL: &str = include_str!("../../constitution/templates/core/SOUL.md");
pub const TEMPLATES_CORE_STORE_MODEL: &str =
    include_str!("../../constitution/templates/core/STORE_MODEL.md");

pub const TEMPLATES_SPECS_AMENDMENTS: &str =
    include_str!("../../constitution/templates/specs/AMENDMENTS.md");
pub const TEMPLATES_SPECS_ARCHITECTURE: &str =
    include_str!("../../constitution/templates/specs/ARCHITECTURE.md");
pub const TEMPLATES_SPECS_INTENT: &str =
    include_str!("../../constitution/templates/specs/INTENT.md");
pub const TEMPLATES_SPECS_SYSTEM: &str =
    include_str!("../../constitution/templates/specs/SYSTEM.md");

pub const TEMPLATES_PLUGINS_DB_BROKER: &str =
    include_str!("../../constitution/templates/plugins/DB_BROKER.md");
pub const TEMPLATES_PLUGINS_MANIFEST: &str =
    include_str!("../../constitution/templates/plugins/MANIFEST.md");
pub const TEMPLATES_PLUGINS_TODO: &str =
    include_str!("../../constitution/templates/plugins/TODO.md");
pub const TEMPLATES_PLUGINS_TODO_USER: &str =
    include_str!("../../constitution/templates/plugins/TODO_USER.md");
pub const TEMPLATES_PLUGINS_CRON: &str =
    include_str!("../../constitution/templates/plugins/CRON.md");
pub const TEMPLATES_PLUGINS_REFLEX: &str =
    include_str!("../../constitution/templates/plugins/REFLEX.md");
pub const TEMPLATES_PLUGINS_HEALTH: &str =
    include_str!("../../constitution/templates/plugins/HEALTH.md");
pub const TEMPLATES_PLUGINS_POLICY: &str =
    include_str!("../../constitution/templates/plugins/POLICY.md");
pub const TEMPLATES_PLUGINS_WATCHER: &str =
    include_str!("../../constitution/templates/plugins/WATCHER.md");
pub const TEMPLATES_PLUGINS_KNOWLEDGE: &str =
    include_str!("../../constitution/templates/plugins/KNOWLEDGE.md");
pub const TEMPLATES_PLUGINS_ARCHIVE: &str =
    include_str!("../../constitution/templates/plugins/ARCHIVE.md");
pub const TEMPLATES_PLUGINS_FEEDBACK: &str =
    include_str!("../../constitution/templates/plugins/FEEDBACK.md");
pub const TEMPLATES_PLUGINS_TRUST: &str =
    include_str!("../../constitution/templates/plugins/TRUST.md");
pub const TEMPLATES_PLUGINS_CONTEXT: &str =
    include_str!("../../constitution/templates/plugins/CONTEXT.md");
pub const TEMPLATES_PLUGINS_HEARTBEAT: &str =
    include_str!("../../constitution/templates/plugins/HEARTBEAT.md");

pub const TEMPLATES_DECAPOD_README: &str =
    include_str!("../../constitution/templates/DECAPOD_README.md");

// Root templates (AGENTS.md, CLAUDE.md, GEMINI.md)
pub const TEMPLATE_AGENTS: &str = include_str!("../../constitution/templates/AGENTS.md");
pub const TEMPLATE_CLAUDE: &str = include_str!("../../constitution/templates/CLAUDE.md");
pub const TEMPLATE_GEMINI: &str = include_str!("../../constitution/templates/GEMINI.md");

pub fn get_template(name: &str) -> Option<String> {
    match name {
        "AGENTS.md" => Some(TEMPLATE_AGENTS.to_string()),
        "CLAUDE.md" => Some(TEMPLATE_CLAUDE.to_string()),
        "GEMINI.md" => Some(TEMPLATE_GEMINI.to_string()),
        "core/CONTROL_PLANE.md" => Some(TEMPLATES_CORE_CONTROL_PLANE.to_string()),
        "core/DECAPOD.md" => Some(TEMPLATES_CORE_DECAPOD.to_string()),
        "core/PLUGINS.md" => Some(TEMPLATES_CORE_PLUGINS.to_string()),
        "core/CLAIMS.md" => Some(TEMPLATES_CORE_CLAIMS.to_string()),
        "core/DEMANDS.md" => Some(TEMPLATES_CORE_DEMANDS.to_string()),
        "core/DEPRECATION.md" => Some(TEMPLATES_CORE_DEPRECATION.to_string()),
        "core/DOC_RULES.md" => Some(TEMPLATES_CORE_DOC_RULES.to_string()),
        "core/GLOSSARY.md" => Some(TEMPLATES_CORE_GLOSSARY.to_string()),
        "core/KNOWLEDGE.md" => Some(TEMPLATES_CORE_KNOWLEDGE.to_string()),
        "core/MEMORY.md" => Some(TEMPLATES_CORE_MEMORY.to_string()),
        "core/SOUL.md" => Some(TEMPLATES_CORE_SOUL.to_string()),
        "core/STORE_MODEL.md" => Some(TEMPLATES_CORE_STORE_MODEL.to_string()),
        "specs/AMENDMENTS.md" => Some(TEMPLATES_SPECS_AMENDMENTS.to_string()),
        "specs/ARCHITECTURE.md" => Some(TEMPLATES_SPECS_ARCHITECTURE.to_string()),
        "specs/INTENT.md" => Some(TEMPLATES_SPECS_INTENT.to_string()),
        "specs/SYSTEM.md" => Some(TEMPLATES_SPECS_SYSTEM.to_string()),
        "plugins/DB_BROKER.md" => Some(TEMPLATES_PLUGINS_DB_BROKER.to_string()),
        "plugins/MANIFEST.md" => Some(TEMPLATES_PLUGINS_MANIFEST.to_string()),
        "plugins/TODO.md" => Some(TEMPLATES_PLUGINS_TODO.to_string()),
        "plugins/TODO_USER.md" => Some(TEMPLATES_PLUGINS_TODO_USER.to_string()),
        "plugins/CRON.md" => Some(TEMPLATES_PLUGINS_CRON.to_string()),
        "plugins/REFLEX.md" => Some(TEMPLATES_PLUGINS_REFLEX.to_string()),
        "plugins/HEALTH.md" => Some(TEMPLATES_PLUGINS_HEALTH.to_string()),
        "plugins/POLICY.md" => Some(TEMPLATES_PLUGINS_POLICY.to_string()),
        "plugins/WATCHER.md" => Some(TEMPLATES_PLUGINS_WATCHER.to_string()),
        "plugins/KNOWLEDGE.md" => Some(TEMPLATES_PLUGINS_KNOWLEDGE.to_string()),
        "plugins/ARCHIVE.md" => Some(TEMPLATES_PLUGINS_ARCHIVE.to_string()),
        "plugins/FEEDBACK.md" => Some(TEMPLATES_PLUGINS_FEEDBACK.to_string()),
        "plugins/TRUST.md" => Some(TEMPLATES_PLUGINS_TRUST.to_string()),
        "plugins/CONTEXT.md" => Some(TEMPLATES_PLUGINS_CONTEXT.to_string()),
        "plugins/HEARTBEAT.md" => Some(TEMPLATES_PLUGINS_HEARTBEAT.to_string()),
        "README.md" => Some(TEMPLATES_DECAPOD_README.to_string()),
        _ => None,
    }
}
