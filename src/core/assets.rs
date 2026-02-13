//! Embedded constitution and template assets.
//!
//! This module provides compile-time embedded access to Decapod's methodology documents.
//! All constitution files (core, specs, plugins) are baked into the binary for
//! hermetic deployment - no external files required.
//!
//! # For AI Agents
//!
//! - **Constitution is embedded in binary**: No need for external doc files
//! - **Use `decapod docs show <path>`**: Access docs via CLI, not direct file reads
//! - **Override mechanism**: Projects can override docs in `.decapod/constitution/`
//! - **Merge semantics**: Overrides append to embedded base (see `get_merged_doc`)
//! - **Templates for scaffolding**: CLAUDE.md, GEMINI.md, etc. are embedded here

use std::path::Path;

/// Macro to embed constitution documents at compile time as text.
///
/// Generates:
/// - Public constants for each embedded document
/// - `get_embedded_doc(path)` function for lookup
/// - `list_docs()` function for discovery
macro_rules! embedded_docs {
    ($($path:expr => $const_name:ident),* $(,)?) => {
        $(
            pub const $const_name: &str =
                include_str!(concat!("../../constitution/embedded/", $path));
        )*

        pub fn get_embedded_doc(path: &str) -> Option<String> {
            let key = path.strip_prefix("embedded/").unwrap_or(path);
            match key {
                $( $path => Some($const_name.to_string()), )*
                _ => None,
            }
        }

        pub fn list_docs() -> Vec<String> {
            vec![ $( format!("embedded/{}", $path), )* ]
        }
    };
}

embedded_docs! {
    "core/CLAIMS.md" => EMBEDDED_CORE_CLAIMS,
    "core/CONTROL_PLANE.md" => EMBEDDED_CORE_CONTROL_PLANE,
    "core/DECAPOD.md" => EMBEDDED_CORE_DECAPOD,
    "core/DEMANDS.md" => EMBEDDED_CORE_DEMANDS,
    "core/DEPRECATION.md" => EMBEDDED_CORE_DEPRECATION,
    "core/DOC_RULES.md" => EMBEDDED_CORE_DOC_RULES,
    "core/GLOSSARY.md" => EMBEDDED_CORE_GLOSSARY,
    "core/KNOWLEDGE.md" => EMBEDDED_CORE_KNOWLEDGE,
    "core/MEMORY.md" => EMBEDDED_CORE_MEMORY,
    "core/PLUGINS.md" => EMBEDDED_CORE_PLUGINS,
    "core/SOUL.md" => EMBEDDED_CORE_SOUL,
    "core/STORE_MODEL.md" => EMBEDDED_CORE_STORE_MODEL,
    "specs/AMENDMENTS.md" => EMBEDDED_SPECS_AMENDMENTS,
    "specs/ARCHITECTURE.md" => EMBEDDED_SPECS_ARCHITECTURE,
    "specs/INTENT.md" => EMBEDDED_SPECS_INTENT,
    "specs/SYSTEM.md" => EMBEDDED_SPECS_SYSTEM,
    "plugins/ARCHIVE.md" => EMBEDDED_PLUGINS_ARCHIVE,
    "plugins/CONTEXT.md" => EMBEDDED_PLUGINS_CONTEXT,
    "plugins/CRON.md" => EMBEDDED_PLUGINS_CRON,
    "plugins/DB_BROKER.md" => EMBEDDED_PLUGINS_DB_BROKER,
    "plugins/EMERGENCY_PROTOCOL.md" => EMBEDDED_PLUGINS_EMERGENCY_PROTOCOL,
    "plugins/FEEDBACK.md" => EMBEDDED_PLUGINS_FEEDBACK,
    "plugins/HEALTH.md" => EMBEDDED_PLUGINS_HEALTH,
    "plugins/HEARTBEAT.md" => EMBEDDED_PLUGINS_HEARTBEAT,
    "plugins/KNOWLEDGE.md" => EMBEDDED_PLUGINS_KNOWLEDGE,
    "plugins/MANIFEST.md" => EMBEDDED_PLUGINS_MANIFEST,
    "plugins/POLICY.md" => EMBEDDED_PLUGINS_POLICY,
    "plugins/REFLEX.md" => EMBEDDED_PLUGINS_REFLEX,
    "plugins/TODO.md" => EMBEDDED_PLUGINS_TODO,
    "plugins/TODO_USER.md" => EMBEDDED_PLUGINS_TODO_USER,
    "plugins/TRUST.md" => EMBEDDED_PLUGINS_TRUST,
    "plugins/WATCHER.md" => EMBEDDED_PLUGINS_WATCHER,
}

/// Legacy function - now just forwards to get_embedded_doc
pub fn get_doc(path: &str) -> Option<String> {
    get_embedded_doc(path)
}

/// Get only the override document from .decapod/OVERRIDE.md for a specific component
pub fn get_override_doc(repo_root: &Path, relative_path: &str) -> Option<String> {
    let override_path = repo_root.join(".decapod").join("OVERRIDE.md");

    if !override_path.exists() {
        return None;
    }

    let override_content = std::fs::read_to_string(&override_path).ok()?;
    extract_component_override(&override_content, relative_path)
}

/// Extract a specific component's override content from OVERRIDE.md
fn extract_component_override(override_content: &str, component_path: &str) -> Option<String> {
    // Only look after the "CHANGES ARE NOT PERMITTED ABOVE THIS LINE" marker
    let override_start = override_content.find("CHANGES ARE NOT PERMITTED ABOVE THIS LINE")?;
    let searchable_content = &override_content[override_start..];

    // Look for the section heading: ### core/DECAPOD.md (or other path)
    let section_marker = format!("\n### {}", component_path);

    let start = searchable_content.find(&section_marker)?;
    let content_start = start + section_marker.len();

    // Find the next ### heading or end of file
    let content_after = &searchable_content[content_start..];
    let end = content_after.find("\n### ")
        .map(|pos| content_start + pos)
        .unwrap_or(searchable_content.len());

    let extracted = searchable_content[content_start..end].trim();

    if extracted.is_empty() {
        None
    } else {
        Some(extracted.to_string())
    }
}

/// Get merged document (embedded base + optional project override from OVERRIDE.md)
pub fn get_merged_doc(repo_root: &Path, relative_path: &str) -> Option<String> {
    // Get embedded base
    let embedded_content = get_embedded_doc(relative_path)?;

    // Check for component-specific override in .decapod/OVERRIDE.md
    if let Some(override_content) = get_override_doc(repo_root, relative_path) {
        return Some(merge_override_content(&embedded_content, &override_content));
    }

    Some(embedded_content)
}

/// Merge embedded content with override additions
fn merge_override_content(embedded_content: &str, override_content: &str) -> String {
    format!(
        "{}\n\n---\n\n## Project Overrides\n\n{}",
        embedded_content.trim(),
        override_content.trim()
    )
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
pub const TEMPLATE_OVERRIDE: &str = include_str!("../../constitution/templates/OVERRIDE.md");

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
        "OVERRIDE.md" => Some(TEMPLATE_OVERRIDE.to_string()),
        _ => None,
    }
}
