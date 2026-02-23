use crate::core::assets;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
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

/// Resolve a scoped constitution context package for a concrete problem/query.
///
/// This function is intentionally deterministic:
/// - same query + same repo state => same ordered fragment list
/// - ordering is score-desc, then ref asc
///
/// It combines:
/// - explicit control-plane bindings (op/path/tag)
/// - lexical matching across embedded/merged constitution documents
pub fn resolve_scoped_fragments(
    repo_root: &Path,
    query: Option<&str>,
    op: Option<&str>,
    touched_paths: &[String],
    intent_tags: &[String],
    limit: usize,
) -> Vec<DocFragment> {
    let bindings = get_bindings(repo_root);
    let mut path_boosts: HashMap<String, i64> = HashMap::new();
    let mut preferred_anchors: HashMap<String, String> = HashMap::new();

    if let Some(op_name) = op
        && let Some(doc_ref) = bindings.ops.get(op_name)
    {
        let (path, anchor) = split_doc_ref(doc_ref);
        *path_boosts.entry(path.to_string()).or_insert(0) += 60;
        if let Some(a) = anchor {
            preferred_anchors.insert(path.to_string(), a.to_string());
        }
    }

    for touched in touched_paths {
        for (prefix, doc_ref) in &bindings.paths {
            if touched.contains(prefix) {
                let (path, anchor) = split_doc_ref(doc_ref);
                *path_boosts.entry(path.to_string()).or_insert(0) += 25;
                if let Some(a) = anchor {
                    preferred_anchors.insert(path.to_string(), a.to_string());
                }
            }
        }
    }

    for tag in intent_tags {
        if let Some(doc_ref) = bindings.tags.get(tag) {
            let (path, anchor) = split_doc_ref(doc_ref);
            *path_boosts.entry(path.to_string()).or_insert(0) += 30;
            if let Some(a) = anchor {
                preferred_anchors.insert(path.to_string(), a.to_string());
            }
        }
    }

    let terms = query
        .map(tokenize)
        .unwrap_or_default()
        .into_iter()
        .collect::<Vec<_>>();

    let mut candidate_paths = BTreeSet::new();
    for path in path_boosts.keys() {
        candidate_paths.insert(path.clone());
    }
    if !terms.is_empty() {
        for path in assets::list_docs() {
            candidate_paths.insert(path);
        }
    }

    let mut ranked: Vec<(i64, DocFragment)> = Vec::new();
    for path in candidate_paths {
        let Some(content) = assets::get_merged_doc(repo_root, &path) else {
            continue;
        };
        let mut score = *path_boosts.get(&path).unwrap_or(&0);
        let content_lc = content.to_lowercase();

        if !terms.is_empty() {
            for term in &terms {
                score += (count_occurrences(&content_lc, term) as i64) * 3;
                if path.to_lowercase().contains(term) {
                    score += 2;
                }
            }
        }

        let fragment = if let Some(anchor) = preferred_anchors.get(&path) {
            get_fragment(repo_root, &path, Some(anchor))
        } else if !terms.is_empty() {
            get_best_fragment_for_terms(&path, &content, &terms)
        } else {
            get_fragment(repo_root, &path, None)
        };

        if let Some(f) = fragment
            && (score > 0 || !terms.is_empty() || path_boosts.contains_key(&path))
        {
            ranked.push((score, f));
        }
    }

    ranked.sort_by(|(sa, fa), (sb, fb)| sb.cmp(sa).then_with(|| fa.r#ref.cmp(&fb.r#ref)));
    ranked
        .into_iter()
        .map(|(_, f)| f)
        .take(limit.max(1))
        .collect()
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
    let lines = content.lines();
    let mut section_lines = Vec::new();
    let mut in_section = false;
    let mut section_title = String::new();
    let mut section_level = 0;

    for line in lines {
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

fn split_doc_ref(doc_ref: &str) -> (&str, Option<&str>) {
    let parts: Vec<&str> = doc_ref.split('#').collect();
    (parts[0], parts.get(1).copied())
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            if current.len() >= 3 {
                tokens.push(current.clone());
            }
            current.clear();
        }
    }
    if !current.is_empty() && current.len() >= 3 {
        tokens.push(current);
    }
    tokens.sort();
    tokens.dedup();
    tokens
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    haystack.match_indices(needle).count()
}

fn get_best_fragment_for_terms(path: &str, content: &str, terms: &[String]) -> Option<DocFragment> {
    let mut current_title: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();
    let mut sections: Vec<(String, String)> = Vec::new();

    for line in content.lines() {
        if line.starts_with('#') {
            if let Some(title) = current_title.take() {
                sections.push((title, current_lines.join("\n")));
                current_lines.clear();
            }
            let heading = line.trim_start_matches('#').trim().to_string();
            current_title = Some(heading);
        }
        if current_title.is_some() {
            current_lines.push(line.to_string());
        }
    }
    if let Some(title) = current_title.take() {
        sections.push((title, current_lines.join("\n")));
    }

    let mut best: Option<(i64, String, String)> = None;
    for (title, section) in sections {
        let lc = section.to_lowercase();
        let mut score = 0_i64;
        for term in terms {
            score += count_occurrences(&lc, term) as i64;
        }
        if score <= 0 {
            continue;
        }
        match &best {
            Some((best_score, best_title, _)) => {
                if score > *best_score || (score == *best_score && title < *best_title) {
                    best = Some((score, title, section));
                }
            }
            None => best = Some((score, title, section)),
        }
    }

    let (_, title, section) = best?;
    let mut hasher = Sha256::new();
    hasher.update(section.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let excerpt = section.lines().take(12).collect::<Vec<_>>().join("\n");
    let excerpt = if excerpt.len() > 700 {
        format!("{}...", &excerpt[..697])
    } else {
        excerpt
    };

    Some(DocFragment {
        kind: "constitution".to_string(),
        r#ref: format!("{}#{}", path, title),
        title,
        excerpt,
        hash,
    })
}
