//! Standards resolution system
//!
//! Resolves industry defaults + project override.md into resolved standards
//! that agents can query for consistent behavior.

use crate::core::error::DecapodError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Resolved standards for a project
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResolvedStandards {
    /// Project name from override or default
    pub project_name: String,
    /// Standards by category
    pub standards: HashMap<String, StandardValue>,
    /// Override file path if present
    pub override_path: Option<PathBuf>,
    /// When these standards were resolved
    pub resolved_at: String,
}

/// A standard value (can be any JSON-compatible type)
pub type StandardValue = serde_json::Value;

/// Default standards for various categories
fn default_standards() -> HashMap<String, StandardValue> {
    let mut standards = HashMap::new();

    // Code style defaults
    standards.insert(
        "code_style".to_string(),
        serde_json::json!({
            "language": "Rust",
            "formatter": "rustfmt",
            "linter": "clippy",
            "max_line_length": 100,
            "indent": "4 spaces",
        }),
    );

    // Testing defaults
    standards.insert(
        "testing".to_string(),
        serde_json::json!({
            "framework": "built-in",
            "coverage_target": 80,
            "required_checks": ["cargo test", "cargo clippy"],
        }),
    );

    // Documentation defaults
    standards.insert(
        "documentation".to_string(),
        serde_json::json!({
            "readme_required": true,
            "changelog_required": true,
            "license_required": true,
            "inline_docs": "rustdoc",
        }),
    );

    // Security defaults
    standards.insert(
        "security".to_string(),
        serde_json::json!({
            "secret_scanning": true,
            "dependency_auditing": true,
            "no_hardcoded_secrets": true,
            "input_validation": "required",
        }),
    );

    // Git defaults
    standards.insert(
        "git".to_string(),
        serde_json::json!({
            "protected_branches": ["main", "master"],
            "require_signed_commits": false,
            "conventional_commits": false,
            "agent_workspaces": true,
        }),
    );

    // CI/CD defaults
    standards.insert(
        "cicd".to_string(),
        serde_json::json!({
            "required_checks": ["test", "lint", "build"],
            "auto_merge": false,
            "deployment_approval": true,
        }),
    );

    // Agent behavior defaults
    standards.insert(
        "agent_behavior".to_string(),
        serde_json::json!({
            "require_validation": true,
            "workspace_enforcement": true,
            "no_main_mutation": true,
            "receipts_required": true,
        }),
    );

    standards
}

/// Read override.md from project root
fn read_override_file(project_root: &Path) -> Option<HashMap<String, StandardValue>> {
    let override_path = project_root.join(".decapod").join("OVERRIDE.md");

    if !override_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&override_path).ok()?;

    // Parse simple key: value format from markdown
    // Format: ## Section, then key: value pairs
    let mut overrides = HashMap::new();
    let mut current_section: Option<String> = None;
    let mut section_content = serde_json::Map::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Section header
        if let Some(stripped) = trimmed.strip_prefix("## ") {
            // Save previous section if any
            if let Some(section) = current_section.take()
                && !section_content.is_empty()
            {
                overrides.insert(section, serde_json::Value::Object(section_content.clone()));
            }

            current_section = Some(stripped.trim().to_lowercase().replace(" ", "_"));
            section_content = serde_json::Map::new();
        }

        // Key: value pair
        if let Some(colon_pos) = trimmed.find(':') {
            let key = trimmed[..colon_pos].trim().to_string();
            let value = trimmed[colon_pos + 1..].trim().to_string();

            // Try to parse as JSON, otherwise use as string
            let parsed_value = serde_json::from_str::<serde_json::Value>(&value)
                .unwrap_or(serde_json::Value::String(value));

            section_content.insert(key, parsed_value);
        }
    }

    // Save last section
    if let Some(section) = current_section
        && !section_content.is_empty()
    {
        overrides.insert(section, serde_json::Value::Object(section_content));
    }

    Some(overrides)
}

/// Resolve standards by merging defaults with overrides
pub fn resolve_standards(project_root: &Path) -> Result<ResolvedStandards, DecapodError> {
    let mut standards = default_standards();
    let override_path = project_root.join(".decapod").join("OVERRIDE.md");
    let has_override = override_path.exists();

    // Apply overrides
    if let Some(overrides) = read_override_file(project_root) {
        for (key, value) in overrides {
            // Merge objects, replace primitives
            if let Some(existing) = standards.get(&key) {
                if let (
                    serde_json::Value::Object(existing_obj),
                    serde_json::Value::Object(override_obj),
                ) = (existing, &value)
                {
                    let mut merged = existing_obj.clone();
                    for (k, v) in override_obj {
                        merged.insert(k.clone(), v.clone());
                    }
                    standards.insert(key, serde_json::Value::Object(merged));
                } else {
                    standards.insert(key, value);
                }
            } else {
                standards.insert(key, value);
            }
        }
    }

    // Extract project name from override or use directory name
    let project_name = if has_override {
        standards
            .get("project")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    } else {
        None
    }
    .unwrap_or_else(|| {
        project_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    });

    Ok(ResolvedStandards {
        project_name,
        standards,
        override_path: if has_override {
            Some(override_path)
        } else {
            None
        },
        resolved_at: crate::core::time::now_epoch_z(),
    })
}

/// Get a specific standard value
pub fn get_standard(
    standards: &ResolvedStandards,
    category: &str,
    key: &str,
) -> Option<StandardValue> {
    standards
        .standards
        .get(category)
        .and_then(|v| v.get(key))
        .cloned()
}

/// Check if a standard is enabled (boolean)
pub fn is_standard_enabled(standards: &ResolvedStandards, category: &str, key: &str) -> bool {
    get_standard(standards, category, key)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Get protected branch patterns
pub fn get_protected_branches(standards: &ResolvedStandards) -> Vec<String> {
    standards
        .standards
        .get("git")
        .and_then(|v| v.get("protected_branches"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_else(|| vec!["main".to_string(), "master".to_string()])
}
