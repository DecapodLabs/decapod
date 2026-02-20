use crate::core::assets;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

/// A fragment of a constitution or authority document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocFragment {
    pub kind: String,
    pub r#ref: String,
    pub title: String,
    pub excerpt: String,
    pub hash: String,
}

/// A specific rule or mandate extracted from the constitution that governs agent behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mandate {
    pub id: String,
    pub severity: String, // "non-negotiable" | "required" | "guidance"
    pub fragment: DocFragment,
    pub check_tag: String, // Link to programmatic validation gate
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bindings {
    pub ops: std::collections::HashMap<String, String>,
    pub paths: std::collections::HashMap<String, String>,
    pub tags: std::collections::HashMap<String, String>,
    pub mandates: std::collections::HashMap<String, Vec<String>>, // op -> [mandate_ids]
}

pub fn get_bindings(_repo_root: &Path) -> Bindings {
    let mut ops = std::collections::HashMap::new();
    ops.insert(
        "workspace.ensure".to_string(),
        "core/DECAPOD.md#workspaces".to_string(),
    );
    ops.insert(
        "workspace.status".to_string(),
        "core/DECAPOD.md#workspaces".to_string(),
    );
    ops.insert(
        "validate".to_string(),
        "core/DECAPOD.md#validation".to_string(),
    );

    let mut paths = std::collections::HashMap::new();
    paths.insert("rpc".to_string(), "interfaces/CONTROL_PLANE.md".to_string());

    let mut tags = std::collections::HashMap::new();
    tags.insert("security".to_string(), "specs/SECURITY.md".to_string());

    let mut mandates = std::collections::HashMap::new();
    mandates.insert(
        "agent.init".to_string(),
        vec!["mandatory-init".to_string(), "mandatory-todo".to_string()],
    );
    mandates.insert(
        "workspace.ensure".to_string(),
        vec!["isolated-worktree".to_string()],
    );
    mandates.insert(
        "any".to_string(),
        vec!["no-master".to_string(), "validate-before-done".to_string()],
    );

    Bindings {
        ops,
        paths,
        tags,
        mandates,
    }
}

/// Resolve formal mandates for a given operation.
pub fn resolve_mandates(repo_root: &Path, op: &str) -> Vec<Mandate> {
    let bindings = get_bindings(repo_root);
    let mut mandate_ids = bindings.mandates.get("any").cloned().unwrap_or_default();
    if let Some(specific) = bindings.mandates.get(op) {
        mandate_ids.extend(specific.clone());
    }

    mandate_ids
        .into_iter()
        .filter_map(|id| get_mandate_by_id(repo_root, &id))
        .collect()
}

fn get_mandate_by_id(repo_root: &Path, id: &str) -> Option<Mandate> {
    match id {
        "no-master" => Some(Mandate {
            id: id.to_string(),
            severity: "non-negotiable".to_string(),
            fragment: get_fragment(
                repo_root,
                "core/DECAPOD.md",
                Some("Workspace Rules (Non-Negotiable)"),
            )?,
            check_tag: "gate.worktree.no_master".to_string(),
        }),
        "mandatory-init" => Some(Mandate {
            id: id.to_string(),
            severity: "non-negotiable".to_string(),
            fragment: get_fragment(
                repo_root,
                "core/DECAPOD.md",
                Some("For Agents: Quick Start"),
            )?,
            check_tag: "gate.session.active".to_string(),
        }),
        "mandatory-todo" => Some(Mandate {
            id: id.to_string(),
            severity: "required".to_string(),
            fragment: get_fragment(repo_root, "core/DECAPOD.md", Some("Subsystems"))?, // We'll link to todo section
            check_tag: "gate.todo.active_task".to_string(),
        }),
        "validate-before-done" => Some(Mandate {
            id: id.to_string(),
            severity: "required".to_string(),
            fragment: get_fragment(
                repo_root,
                "core/DECAPOD.md",
                Some("Validation (must pass before claiming done)"),
            )?,
            check_tag: "gate.validation.pass".to_string(),
        }),
        "isolated-worktree" => Some(Mandate {
            id: id.to_string(),
            severity: "required".to_string(),
            fragment: get_fragment(
                repo_root,
                "core/DECAPOD.md",
                Some("Workspace Rules (Non-Negotiable)"),
            )?,
            check_tag: "gate.worktree.isolated".to_string(),
        }),
        _ => None,
    }
}

/// Extract a markdown fragment by anchor (heading).
/// If anchor is None, returns the whole file.
pub fn get_fragment(repo_root: &Path, path: &str, anchor: Option<&str>) -> Option<DocFragment> {
    let content = assets::get_merged_doc(repo_root, path)?;

    let (fragment_content, title) = if let Some(a) = anchor {
        extract_section(&content, a)?
    } else {
        let title = content
            .lines()
            .next()
            .unwrap_or("Untitled")
            .trim_start_matches("# ")
            .to_string();
        (content.clone(), title)
    };

    let mut hasher = Sha256::new();
    hasher.update(fragment_content.as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    let excerpt = fragment_content
        .lines()
        .take(10)
        .collect::<Vec<_>>()
        .join("\n");
    let excerpt = if excerpt.len() > 500 {
        format!("{}...", &excerpt[..497])
    } else {
        excerpt
    };

    Some(DocFragment {
        kind: "constitution".to_string(),
        r#ref: if let Some(a) = anchor {
            format!("{}#{}", path, a)
        } else {
            path.to_string()
        },
        title,
        excerpt,
        hash,
    })
}

fn extract_section(content: &str, anchor: &str) -> Option<(String, String)> {
    let slug = anchor.to_lowercase().replace(' ', "-");
    let mut lines = content.lines();
    let mut section_lines = Vec::new();
    let mut in_section = false;
    let mut section_title = String::new();
    let mut section_level = 0;

    while let Some(line) = lines.next() {
        if line.starts_with('#') {
            let level = line.chars().take_while(|&c| c == '#').count();
            let title = line.trim_start_matches('#').trim();
            let current_slug = title.to_lowercase().replace(' ', "-");

            if in_section {
                if level <= section_level {
                    break;
                }
            } else if current_slug == slug || title.to_lowercase() == anchor.to_lowercase() {
                in_section = true;
                section_title = title.to_string();
                section_level = level;
            }
        }

        if in_section {
            section_lines.push(line);
        }
    }

    if in_section {
        Some((section_lines.join("\n"), section_title))
    } else {
        None
    }
}
