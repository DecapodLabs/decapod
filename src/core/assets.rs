//! Embedded constitution and template assets.
//!
//! This module provides compile-time embedded access to Decapod's methodology documents.
//! All constitution files (core, specs, plugins) are baked into the binary for
//! hermetic deployment - no external files required.

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
                include_str!(concat!("../../constitution/", $path));
        )*

        pub fn get_embedded_doc(path: &str) -> Option<String> {
            // Support both bare paths and legacy "embedded/" prefix
            let key = path.strip_prefix("embedded/").unwrap_or(path);
            match key {
                $( $path => Some($const_name.to_string()), )*
                _ => None,
            }
        }

        pub fn list_docs() -> Vec<String> {
            vec![ $( $path.to_string(), )* ]
        }
    };
}

embedded_docs! {
    // Core: Routers and indices
    "core/DECAPOD.md" => EMBEDDED_CORE_DECAPOD,
    "core/INTERFACES.md" => EMBEDDED_CORE_INTERFACES,
    "core/METHODOLOGY.md" => EMBEDDED_CORE_METHODOLOGY,
    "core/PLUGINS.md" => EMBEDDED_CORE_PLUGINS,
    "core/GAPS.md" => EMBEDDED_CORE_GAPS,
    "core/DEMANDS.md" => EMBEDDED_CORE_DEMANDS,
    "core/DEPRECATION.md" => EMBEDDED_CORE_DEPRECATION,

    // Specs: System contracts
    "specs/INTENT.md" => EMBEDDED_SPECS_INTENT,
    "specs/SYSTEM.md" => EMBEDDED_SPECS_SYSTEM,
    "specs/AMENDMENTS.md" => EMBEDDED_SPECS_AMENDMENTS,
    "specs/SECURITY.md" => EMBEDDED_SPECS_SECURITY,
    "specs/GIT.md" => EMBEDDED_SPECS_GIT,
    "specs/evaluations/VARIANCE_EVALS.md" => EMBEDDED_SPECS_VARIANCE_EVALS,
    "specs/evaluations/JUDGE_CONTRACT.md" => EMBEDDED_SPECS_JUDGE_CONTRACT,
    "specs/engineering/FRONTEND_BACKEND_E2E.md" => EMBEDDED_SPECS_FRONTEND_BACKEND_E2E,

    // Interfaces: Binding contracts
    "interfaces/CLAIMS.md" => EMBEDDED_INTERFACES_CLAIMS,
    "interfaces/CONTROL_PLANE.md" => EMBEDDED_INTERFACES_CONTROL_PLANE,
    "interfaces/DOC_RULES.md" => EMBEDDED_INTERFACES_DOC_RULES,
    "interfaces/GLOSSARY.md" => EMBEDDED_INTERFACES_GLOSSARY,
    "interfaces/STORE_MODEL.md" => EMBEDDED_INTERFACES_STORE_MODEL,
    "interfaces/TESTING.md" => EMBEDDED_INTERFACES_TESTING,
    "interfaces/KNOWLEDGE_SCHEMA.md" => EMBEDDED_INTERFACES_KNOWLEDGE_SCHEMA,
    "interfaces/KNOWLEDGE_STORE.md" => EMBEDDED_INTERFACES_KNOWLEDGE_STORE,
    "interfaces/MEMORY_SCHEMA.md" => EMBEDDED_INTERFACES_MEMORY_SCHEMA,
    "interfaces/DEMANDS_SCHEMA.md" => EMBEDDED_INTERFACES_DEMANDS_SCHEMA,
    "interfaces/TODO_SCHEMA.md" => EMBEDDED_INTERFACES_TODO_SCHEMA,
    "interfaces/PLAN_GOVERNED_EXECUTION.md" => EMBEDDED_INTERFACES_PLAN_GOVERNED_EXECUTION,
    "interfaces/AGENT_CONTEXT_PACK.md" => EMBEDDED_INTERFACES_AGENT_CONTEXT_PACK,

    // Methodology: Practice guides
    "methodology/ARCHITECTURE.md" => EMBEDDED_METHODOLOGY_ARCHITECTURE,
    "methodology/SOUL.md" => EMBEDDED_METHODOLOGY_SOUL,
    "methodology/KNOWLEDGE.md" => EMBEDDED_METHODOLOGY_KNOWLEDGE,
    "methodology/MEMORY.md" => EMBEDDED_METHODOLOGY_MEMORY,
    "methodology/TESTING.md" => EMBEDDED_METHODOLOGY_TESTING,
    "methodology/CI_CD.md" => EMBEDDED_METHODOLOGY_CI_CD,

    // Architecture: Domain patterns
    "architecture/DATA.md" => EMBEDDED_ARCHITECTURE_DATA,
    "architecture/CACHING.md" => EMBEDDED_ARCHITECTURE_CACHING,
    "architecture/MEMORY.md" => EMBEDDED_ARCHITECTURE_MEMORY,
    "architecture/WEB.md" => EMBEDDED_ARCHITECTURE_WEB,
    "architecture/CLOUD.md" => EMBEDDED_ARCHITECTURE_CLOUD,
    "architecture/FRONTEND.md" => EMBEDDED_ARCHITECTURE_FRONTEND,
    "architecture/ALGORITHMS.md" => EMBEDDED_ARCHITECTURE_ALGORITHMS,
    "architecture/SECURITY.md" => EMBEDDED_ARCHITECTURE_SECURITY,
    "architecture/CONCURRENCY.md" => EMBEDDED_ARCHITECTURE_CONCURRENCY,
    "architecture/OBSERVABILITY.md" => EMBEDDED_ARCHITECTURE_OBSERVABILITY,

    // Embedded docs used by entrypoints/operators
    "docs/ARCHITECTURE_OVERVIEW.md" => EMBEDDED_DOCS_ARCHITECTURE_OVERVIEW,
    "docs/CONTROL_PLANE_API.md" => EMBEDDED_DOCS_CONTROL_PLANE_API,
    "docs/MAINTAINERS.md" => EMBEDDED_DOCS_MAINTAINERS,
    "docs/MIGRATIONS.md" => EMBEDDED_DOCS_MIGRATIONS,
    "docs/NEGLECTED_ASPECTS_LEDGER.md" => EMBEDDED_DOCS_NEGLECTED_ASPECTS_LEDGER,
    "docs/PLAYBOOK.md" => EMBEDDED_DOCS_PLAYBOOK,
    "docs/README.md" => EMBEDDED_DOCS_README,
    "docs/RELEASE_PROCESS.md" => EMBEDDED_DOCS_RELEASE_PROCESS,
    "docs/SECURITY_THREAT_MODEL.md" => EMBEDDED_DOCS_SECURITY_THREAT_MODEL,
    "docs/EVAL_TRANSLATION_MAP.md" => EMBEDDED_DOCS_EVAL_TRANSLATION_MAP,

    "plugins/ARCHIVE.md" => EMBEDDED_PLUGINS_ARCHIVE,
    "plugins/AUTOUPDATE.md" => EMBEDDED_PLUGINS_AUTOUPDATE,
    "plugins/CONTEXT.md" => EMBEDDED_PLUGINS_CONTEXT,
    "plugins/CRON.md" => EMBEDDED_PLUGINS_CRON,
    "plugins/DB_BROKER.md" => EMBEDDED_PLUGINS_DB_BROKER,
    "plugins/DECIDE.md" => EMBEDDED_PLUGINS_DECIDE,
    "plugins/EMERGENCY_PROTOCOL.md" => EMBEDDED_PLUGINS_EMERGENCY_PROTOCOL,
    "plugins/FEDERATION.md" => EMBEDDED_PLUGINS_FEDERATION,
    "plugins/FEEDBACK.md" => EMBEDDED_PLUGINS_FEEDBACK,
    "plugins/HEALTH.md" => EMBEDDED_PLUGINS_HEALTH,
    "plugins/HEARTBEAT.md" => EMBEDDED_PLUGINS_HEARTBEAT,
    "plugins/KNOWLEDGE.md" => EMBEDDED_PLUGINS_KNOWLEDGE,
    "plugins/MANIFEST.md" => EMBEDDED_PLUGINS_MANIFEST,
    "plugins/POLICY.md" => EMBEDDED_PLUGINS_POLICY,
    "plugins/REFLEX.md" => EMBEDDED_PLUGINS_REFLEX,
    "plugins/TEAMMATE.md" => EMBEDDED_PLUGINS_TEAMMATE,
    "plugins/TODO.md" => EMBEDDED_PLUGINS_TODO,
    "plugins/TRUST.md" => EMBEDDED_PLUGINS_TRUST,
    "plugins/VERIFY.md" => EMBEDDED_PLUGINS_VERIFY,
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
    let end = content_after
        .find("\n### ")
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

// Root templates (agent entrypoints) - embedded for scaffolding
pub const TEMPLATE_AGENTS: &str = include_str!("../../templates/AGENTS.md");
pub const TEMPLATE_CLAUDE: &str = include_str!("../../templates/CLAUDE.md");
pub const TEMPLATE_GEMINI: &str = include_str!("../../templates/GEMINI.md");
pub const TEMPLATE_CODEX: &str = include_str!("../../templates/CODEX.md");

pub const TEMPLATE_README: &str = include_str!("../../templates/README.md");
pub const TEMPLATE_OVERRIDE: &str = include_str!("../../templates/OVERRIDE.md");

pub fn get_template(name: &str) -> Option<String> {
    match name {
        "AGENTS.md" => Some(TEMPLATE_AGENTS.to_string()),
        "CLAUDE.md" => Some(TEMPLATE_CLAUDE.to_string()),
        "GEMINI.md" => Some(TEMPLATE_GEMINI.to_string()),
        "CODEX.md" => Some(TEMPLATE_CODEX.to_string()),

        "README.md" => Some(TEMPLATE_README.to_string()),
        "OVERRIDE.md" => Some(TEMPLATE_OVERRIDE.to_string()),
        _ => None,
    }
}
