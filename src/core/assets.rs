// src/assets.rs
// Contains embedded assets for Decapod, including constitution documents, templates,
// and project living document templates.

// NOTE: All include_str! paths are relative to the crate root.

// Constitution Core Docs (embedded from embedded directory)
pub const CONSTITUTION_CORE_CONTROL_PLANE: &str = include_str!("../../constitution/embedded/core/CONTROL_PLANE.md");
pub const CONSTITUTION_CORE_DECAPOD: &str = include_str!("../../constitution/embedded/core/DECAPOD.md");
pub const CONSTITUTION_CORE_PLUGINS: &str = include_str!("../../constitution/embedded/core/PLUGINS.md");
pub const CONSTITUTION_CORE_CLAIMS: &str = include_str!("../../constitution/embedded/core/CLAIMS.md");
pub const CONSTITUTION_CORE_DEMANDS: &str = include_str!("../../constitution/embedded/core/DEMANDS.md");
pub const CONSTITUTION_CORE_DEPRECATION: &str = include_str!("../../constitution/embedded/core/DEPRECATION.md");
pub const CONSTITUTION_CORE_DOC_RULES: &str = include_str!("../../constitution/embedded/core/DOC_RULES.md");
pub const CONSTITUTION_CORE_GLOSSARY: &str = include_str!("../../constitution/embedded/core/GLOSSARY.md");
pub const CONSTITUTION_CORE_KNOWLEDGE: &str = include_str!("../../constitution/embedded/core/KNOWLEDGE.md");
pub const CONSTITUTION_CORE_MEMORY: &str = include_str!("../../constitution/embedded/core/MEMORY.md");
pub const CONSTITUTION_CORE_SOUL: &str = include_str!("../../constitution/embedded/core/SOUL.md");
pub const CONSTITUTION_CORE_STORE_MODEL: &str = include_str!("../../constitution/embedded/core/STORE_MODEL.md");

// Constitution Specs Docs (embedded from embedded directory)
pub const CONSTITUTION_SPECS_AMENDMENTS: &str = include_str!("../../constitution/embedded/specs/AMENDMENTS.md");
pub const CONSTITUTION_SPECS_ARCHITECTURE: &str = include_str!("../../constitution/embedded/specs/ARCHITECTURE.md");
pub const CONSTITUTION_SPECS_INTENT: &str = include_str!("../../constitution/embedded/specs/INTENT.md");
pub const CONSTITUTION_SPECS_SYSTEM: &str = include_str!("../../constitution/embedded/specs/SYSTEM.md");

// Constitution Plugins Docs (embedded from embedded directory)
pub const CONSTITUTION_PLUGINS_DB_BROKER: &str = include_str!("../../constitution/embedded/plugins/DB_BROKER.md");
pub const CONSTITUTION_PLUGINS_MANIFEST: &str = include_str!("../../constitution/embedded/plugins/MANIFEST.md");
pub const CONSTITUTION_PLUGINS_TODO: &str = include_str!("../../constitution/embedded/plugins/TODO.md");
pub const CONSTITUTION_PLUGINS_TODO_USER: &str = include_str!("../../constitution/embedded/plugins/TODO_USER.md");
pub const CONSTITUTION_PLUGINS_CRON: &str = include_str!("../../constitution/embedded/plugins/CRON.md");
pub const CONSTITUTION_PLUGINS_REFLEX: &str = include_str!("../../constitution/embedded/plugins/REFLEX.md");
pub const CONSTITUTION_PLUGINS_HEALTH: &str = include_str!("../../constitution/embedded/plugins/HEALTH.md");
pub const CONSTITUTION_PLUGINS_POLICY: &str = include_str!("../../constitution/embedded/plugins/POLICY.md");
pub const CONSTITUTION_PLUGINS_WATCHER: &str = include_str!("../../constitution/embedded/plugins/WATCHER.md");
pub const CONSTITUTION_PLUGINS_KNOWLEDGE: &str = include_str!("../../constitution/embedded/plugins/KNOWLEDGE.md");
pub const CONSTITUTION_PLUGINS_ARCHIVE: &str = include_str!("../../constitution/embedded/plugins/ARCHIVE.md");
pub const CONSTITUTION_PLUGINS_FEEDBACK: &str = include_str!("../../constitution/embedded/plugins/FEEDBACK.md");
pub const CONSTITUTION_PLUGINS_TRUST: &str = include_str!("../../constitution/embedded/plugins/TRUST.md");
pub const CONSTITUTION_PLUGINS_CONTEXT: &str = include_str!("../../constitution/embedded/plugins/CONTEXT.md");
pub const CONSTITUTION_PLUGINS_HEARTBEAT: &str = include_str!("../../constitution/embedded/plugins/HEARTBEAT.md");

// Templates for user overrides (embedded for scaffolding)
pub const TEMPLATES_CORE_CONTROL_PLANE: &str = include_str!("../../constitution/templates/core/CONTROL_PLANE.md");
pub const TEMPLATES_CORE_DECAPOD: &str = include_str!("../../constitution/templates/core/DECAPOD.md");
pub const TEMPLATES_CORE_PLUGINS: &str = include_str!("../../constitution/templates/core/PLUGINS.md");
pub const TEMPLATES_CORE_CLAIMS: &str = include_str!("../../constitution/templates/core/CLAIMS.md");
pub const TEMPLATES_CORE_DEMANDS: &str = include_str!("../../constitution/templates/core/DEMANDS.md");
pub const TEMPLATES_CORE_DEPRECATION: &str = include_str!("../../constitution/templates/core/DEPRECATION.md");
pub const TEMPLATES_CORE_DOC_RULES: &str = include_str!("../../constitution/templates/core/DOC_RULES.md");
pub const TEMPLATES_CORE_GLOSSARY: &str = include_str!("../../constitution/templates/core/GLOSSARY.md");
pub const TEMPLATES_CORE_KNOWLEDGE: &str = include_str!("../../constitution/templates/core/KNOWLEDGE.md");
pub const TEMPLATES_CORE_MEMORY: &str = include_str!("../../constitution/templates/core/MEMORY.md");
pub const TEMPLATES_CORE_SOUL: &str = include_str!("../../constitution/templates/core/SOUL.md");
pub const TEMPLATES_CORE_STORE_MODEL: &str = include_str!("../../constitution/templates/core/STORE_MODEL.md");

pub const TEMPLATES_SPECS_AMENDMENTS: &str = include_str!("../../constitution/templates/specs/AMENDMENTS.md");
pub const TEMPLATES_SPECS_ARCHITECTURE: &str = include_str!("../../constitution/templates/specs/ARCHITECTURE.md");
pub const TEMPLATES_SPECS_INTENT: &str = include_str!("../../constitution/templates/specs/INTENT.md");
pub const TEMPLATES_SPECS_SYSTEM: &str = include_str!("../../constitution/templates/specs/SYSTEM.md");

pub const TEMPLATES_PLUGINS_DB_BROKER: &str = include_str!("../../constitution/templates/plugins/DB_BROKER.md");
pub const TEMPLATES_PLUGINS_MANIFEST: &str = include_str!("../../constitution/templates/plugins/MANIFEST.md");
pub const TEMPLATES_PLUGINS_TODO: &str = include_str!("../../constitution/templates/plugins/TODO.md");
pub const TEMPLATES_PLUGINS_TODO_USER: &str = include_str!("../../constitution/templates/plugins/TODO_USER.md");
pub const TEMPLATES_PLUGINS_CRON: &str = include_str!("../../constitution/templates/plugins/CRON.md");
pub const TEMPLATES_PLUGINS_REFLEX: &str = include_str!("../../constitution/templates/plugins/REFLEX.md");
pub const TEMPLATES_PLUGINS_HEALTH: &str = include_str!("../../constitution/templates/plugins/HEALTH.md");
pub const TEMPLATES_PLUGINS_POLICY: &str = include_str!("../../constitution/templates/plugins/POLICY.md");
pub const TEMPLATES_PLUGINS_WATCHER: &str = include_str!("../../constitution/templates/plugins/WATCHER.md");
pub const TEMPLATES_PLUGINS_KNOWLEDGE: &str = include_str!("../../constitution/templates/plugins/KNOWLEDGE.md");
pub const TEMPLATES_PLUGINS_ARCHIVE: &str = include_str!("../../constitution/templates/plugins/ARCHIVE.md");
pub const TEMPLATES_PLUGINS_FEEDBACK: &str = include_str!("../../constitution/templates/plugins/FEEDBACK.md");
pub const TEMPLATES_PLUGINS_TRUST: &str = include_str!("../../constitution/templates/plugins/TRUST.md");
pub const TEMPLATES_PLUGINS_CONTEXT: &str = include_str!("../../constitution/templates/plugins/CONTEXT.md");
pub const TEMPLATES_PLUGINS_HEARTBEAT: &str = include_str!("../../constitution/templates/plugins/HEARTBEAT.md");

pub const TEMPLATES_DECAPOD_README: &str = include_str!("../../constitution/templates/DECAPOD_README.md");

// Root templates (AGENTS.md, CLAUDE.md, GEMINI.md)
pub const TEMPLATE_AGENTS: &str = include_str!("../../constitution/templates/AGENTS.md");
pub const TEMPLATE_CLAUDE: &str = include_str!("../../constitution/templates/CLAUDE.md");
pub const TEMPLATE_GEMINI: &str = include_str!("../../constitution/templates/GEMINI.md");


// Functions to access embedded documents (non-template specific)
pub fn list_docs() -> Vec<String> {
    let mut docs = Vec::new();
    docs.push("embedded/core/CONTROL_PLANE.md".to_string());
    docs.push("embedded/core/DECAPOD.md".to_string());
    docs.push("embedded/core/PLUGINS.md".to_string());
    docs.push("embedded/core/CLAIMS.md".to_string());
    docs.push("embedded/core/DEMANDS.md".to_string());
    docs.push("embedded/core/DEPRECATION.md".to_string());
    docs.push("embedded/core/DOC_RULES.md".to_string());
    docs.push("embedded/core/GLOSSARY.md".to_string());
    docs.push("embedded/core/KNOWLEDGE.md".to_string());
    docs.push("embedded/core/MEMORY.md".to_string());
    docs.push("embedded/core/SOUL.md".to_string());
    docs.push("embedded/core/STORE_MODEL.md".to_string());

    docs.push("embedded/specs/AMENDMENTS.md".to_string());
    docs.push("embedded/specs/ARCHITECTURE.md".to_string());
    docs.push("embedded/specs/INTENT.md".to_string());
    docs.push("embedded/specs/SYSTEM.md".to_string());

    docs.push("embedded/plugins/DB_BROKER.md".to_string());
    docs.push("embedded/plugins/MANIFEST.md".to_string());
    docs.push("embedded/plugins/TODO.md".to_string());
    docs.push("embedded/plugins/TODO_USER.md".to_string());
    docs.push("embedded/plugins/CRON.md".to_string());
    docs.push("embedded/plugins/REFLEX.md".to_string());
    docs.push("embedded/plugins/HEALTH.md".to_string());
    docs.push("embedded/plugins/POLICY.md".to_string());
    docs.push("embedded/plugins/WATCHER.md".to_string());
    docs.push("embedded/plugins/KNOWLEDGE.md".to_string());
    docs.push("embedded/plugins/ARCHIVE.md".to_string());
    docs.push("embedded/plugins/FEEDBACK.md".to_string());
    docs.push("embedded/plugins/TRUST.md".to_string());
    docs.push("embedded/plugins/CONTEXT.md".to_string());
    docs.push("embedded/plugins/HEARTBEAT.md".to_string());

    docs
}

pub fn get_doc(path: &str) -> Option<String> {
    match path {
        "embedded/core/CONTROL_PLANE.md" => Some(CONSTITUTION_CORE_CONTROL_PLANE.to_string()),
        "embedded/core/DECAPOD.md" => Some(CONSTITUTION_CORE_DECAPOD.to_string()),
        "embedded/core/PLUGINS.md" => Some(CONSTITUTION_CORE_PLUGINS.to_string()),
        "embedded/core/CLAIMS.md" => Some(CONSTITUTION_CORE_CLAIMS.to_string()),
        "embedded/core/DEMANDS.md" => Some(CONSTITUTION_CORE_DEMANDS.to_string()),
        "embedded/core/DEPRECATION.md" => Some(CONSTITUTION_CORE_DEPRECATION.to_string()),
        "embedded/core/DOC_RULES.md" => Some(CONSTITUTION_CORE_DOC_RULES.to_string()),
        "embedded/core/GLOSSARY.md" => Some(CONSTITUTION_CORE_GLOSSARY.to_string()),
        "embedded/core/KNOWLEDGE.md" => Some(CONSTITUTION_CORE_KNOWLEDGE.to_string()),
        "embedded/core/MEMORY.md" => Some(CONSTITUTION_CORE_MEMORY.to_string()),
        "embedded/core/SOUL.md" => Some(CONSTITUTION_CORE_SOUL.to_string()),
        "embedded/core/STORE_MODEL.md" => Some(CONSTITUTION_CORE_STORE_MODEL.to_string()),

        "embedded/specs/AMENDMENTS.md" => Some(CONSTITUTION_SPECS_AMENDMENTS.to_string()),
        "embedded/specs/ARCHITECTURE.md" => Some(CONSTITUTION_SPECS_ARCHITECTURE.to_string()),
        "embedded/specs/INTENT.md" => Some(CONSTITUTION_SPECS_INTENT.to_string()),
        "embedded/specs/SYSTEM.md" => Some(CONSTITUTION_SPECS_SYSTEM.to_string()),

        "embedded/plugins/DB_BROKER.md" => Some(CONSTITUTION_PLUGINS_DB_BROKER.to_string()),
        "embedded/plugins/MANIFEST.md" => Some(CONSTITUTION_PLUGINS_MANIFEST.to_string()),
        "embedded/plugins/TODO.md" => Some(CONSTITUTION_PLUGINS_TODO.to_string()),
        "embedded/plugins/TODO_USER.md" => Some(CONSTITUTION_PLUGINS_TODO_USER.to_string()),
        "embedded/plugins/CRON.md" => Some(CONSTITUTION_PLUGINS_CRON.to_string()),
        "embedded/plugins/REFLEX.md" => Some(CONSTITUTION_PLUGINS_REFLEX.to_string()),
        "embedded/plugins/HEALTH.md" => Some(CONSTITUTION_PLUGINS_HEALTH.to_string()),
        "embedded/plugins/POLICY.md" => Some(CONSTITUTION_PLUGINS_POLICY.to_string()),
        "embedded/plugins/WATCHER.md" => Some(CONSTITUTION_PLUGINS_WATCHER.to_string()),
        "embedded/plugins/KNOWLEDGE.md" => Some(CONSTITUTION_PLUGINS_KNOWLEDGE.to_string()),
        "embedded/plugins/ARCHIVE.md" => Some(CONSTITUTION_PLUGINS_ARCHIVE.to_string()),
        "embedded/plugins/FEEDBACK.md" => Some(CONSTITUTION_PLUGINS_FEEDBACK.to_string()),
        "embedded/plugins/TRUST.md" => Some(CONSTITUTION_PLUGINS_TRUST.to_string()),
        "embedded/plugins/CONTEXT.md" => Some(CONSTITUTION_PLUGINS_CONTEXT.to_string()),
        "embedded/plugins/HEARTBEAT.md" => Some(CONSTITUTION_PLUGINS_HEARTBEAT.to_string()),

        _ => None,
    }
}

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
