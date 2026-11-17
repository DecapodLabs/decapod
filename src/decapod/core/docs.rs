use crate::core::assets;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
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

/// Selection evidence is separate from the stable fragment wire contract.
/// A source hash proves identity, never relevance, human approval or execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FragmentSelection {
    pub fragment_ref: String,
    pub node_id: String,
    pub authority: String,
    pub binding: String,
    pub status: String,
    pub source_layer: String,
    pub source_hash: String,
    pub fragment_hash: String,
    pub resolver_version: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextSelection {
    pub selected: Vec<FragmentSelection>,
    pub excluded: BTreeMap<String, String>,
    pub excluded_counts: BTreeMap<String, usize>,
    pub advisory_status: String,
    pub context_bytes: usize,
}

pub const MAX_CONTEXT_FRAGMENTS: usize = 12;
pub const MAX_CONTEXT_BYTES: usize = 8192;
pub const RESOLVER_VERSION: &str = "authority-projection-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedContext {
    pub fragments: Vec<DocFragment>,
    pub selection: ContextSelection,
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

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
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
    resolve_scoped_context(
        repo_root,
        query,
        op,
        touched_paths,
        intent_tags,
        limit,
        None,
    )
    .fragments
}

/// Filter the namespace before ranking, so unrelated high scores cannot starve
/// an applicable in-scope reference. Explicit bindings precede lexical matches.
pub fn resolve_scoped_context(
    repo_root: &Path,
    query: Option<&str>,
    op: Option<&str>,
    touched_paths: &[String],
    intent_tags: &[String],
    limit: usize,
    scope: Option<&str>,
) -> ScopedContext {
    let bindings = get_bindings(repo_root);
    let mut reasons: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut preferred_anchors = BTreeMap::new();

    if let Some(op_name) = op
        && let Some(doc_ref) = bindings.ops.get(op_name)
    {
        let (path, anchor) = split_doc_ref(doc_ref);
        reasons
            .entry(path.to_string())
            .or_default()
            .insert(format!("operation:{op_name}"));
        if let Some(a) = anchor {
            preferred_anchors.insert(path.to_string(), a.to_string());
        }
    }

    for touched in touched_paths {
        for (prefix, doc_ref) in &bindings.paths {
            if touched.contains(prefix) {
                let (path, anchor) = split_doc_ref(doc_ref);
                reasons
                    .entry(path.to_string())
                    .or_default()
                    .insert(format!("path:{prefix}"));
                if let Some(a) = anchor {
                    preferred_anchors.insert(path.to_string(), a.to_string());
                }
            }
        }
    }

    for tag in intent_tags {
        if let Some(doc_ref) = bindings.tags.get(tag) {
            let (path, anchor) = split_doc_ref(doc_ref);
            reasons
                .entry(path.to_string())
                .or_default()
                .insert(format!("tag:{tag}"));
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
    for path in reasons.keys() {
        candidate_paths.insert(path.clone());
    }
    if !terms.is_empty() {
        for path in assets::list_docs() {
            candidate_paths.insert(path);
        }
    }

    let mut selection = ContextSelection::default();
    // Invalid overrides must never be silently treated as absent. Fallible
    // CLI/RPC owners return the parser's typed error; this legacy Vec API emits
    // no authority on malformed input.
    if assets::validate_override_structure(repo_root).is_err() {
        selection
            .excluded
            .insert(".decapod/OVERRIDE.md".into(), "invalid_override".into());
        selection.advisory_status = "invalid_authority".into();
        return ScopedContext {
            fragments: vec![],
            selection,
        };
    }
    let mut ranked = Vec::new();
    for path in candidate_paths {
        if scope.is_some_and(|prefix| !path.starts_with(&format!("{prefix}/"))) {
            continue;
        }
        let Some(raw) = assets::get_embedded_doc(&path) else {
            continue;
        };
        let Ok(node) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let status = node["status"].as_str().unwrap_or("active");
        if status != "active" {
            selection
                .excluded
                .insert(path, format!("inactive:{status}"));
            continue;
        }
        let override_body = assets::get_override_doc(repo_root, &path);
        let Some(content) = override_body
            .as_ref()
            .map(|body| format!("# Project Overrides\n{body}"))
            .or_else(|| assets::get_merged_doc(repo_root, &path))
        else {
            continue;
        };
        let bound = reasons.contains_key(&path);
        let mut node_reasons = reasons.remove(&path).unwrap_or_default();
        let content_terms: BTreeSet<_> = tokenize(&content).into_iter().collect();
        let metadata_terms: BTreeSet<_> =
            tokenize(&format!("{path} {} {}", node["title"], node["terms"]))
                .into_iter()
                .collect();
        let matched: Vec<_> = terms
            .iter()
            .filter(|term| content_terms.contains(*term) || metadata_terms.contains(*term))
            .collect();
        // Presence/coverage, never raw frequency or document size.
        let score = matched
            .iter()
            .map(|term| if metadata_terms.contains(*term) { 4 } else { 1 })
            .sum::<usize>();
        if !bound && score == 0 {
            *selection
                .excluded_counts
                .entry("no_relevant_advisory_fragment".into())
                .or_default() += 1;
            continue;
        }
        if !matched.is_empty() {
            node_reasons.insert(format!(
                "unique_terms:{}",
                matched
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        let fragment = if override_body.is_some() {
            node_reasons.insert("repository_project_override".into());
            fragment_from_section(&path, "Project Overrides", &content)
        } else if let Some(anchor) = preferred_anchors.get(&path) {
            get_fragment(repo_root, &path, Some(anchor))
                .or_else(|| get_fragment(repo_root, &path, None))
        } else if !terms.is_empty() {
            get_best_fragment_for_terms(&path, &content, &terms)
                .or_else(|| get_fragment(repo_root, &path, None))
        } else {
            get_fragment(repo_root, &path, None)
        };
        if let Some(f) = fragment {
            let evidence = FragmentSelection {
                fragment_ref: f.r#ref.clone(),
                node_id: path,
                authority: if override_body.is_some() {
                    "override"
                } else {
                    node["authority"].as_str().unwrap_or("unknown")
                }
                .into(),
                binding: node["binding"].as_str().unwrap_or("advisory").into(),
                status: status.into(),
                source_layer: if override_body.is_some() {
                    "repository_project_override"
                } else {
                    "binary_embedded"
                }
                .into(),
                source_hash: format!(
                    "{:x}",
                    Sha256::digest(if let Some(body) = &override_body {
                        body.as_bytes()
                    } else {
                        raw.as_bytes()
                    })
                ),
                fragment_hash: f.hash.clone(),
                resolver_version: RESOLVER_VERSION.into(),
                reasons: node_reasons.into_iter().collect(),
            };
            ranked.push((bound, score, f, evidence));
        }
    }

    ranked.sort_by(|(ba, sa, fa, _), (bb, sb, fb, _)| {
        bb.cmp(ba)
            .then_with(|| sb.cmp(sa))
            .then_with(|| fa.r#ref.cmp(&fb.r#ref))
    });
    let mut fragments = Vec::new();
    for (_, _, fragment, evidence) in ranked {
        if fragments.len() >= limit.clamp(1, MAX_CONTEXT_FRAGMENTS)
            || selection.context_bytes + fragment.excerpt.len() > MAX_CONTEXT_BYTES
        {
            *selection
                .excluded_counts
                .entry("context_budget".into())
                .or_default() += 1;
            if selection.excluded.len() < MAX_CONTEXT_FRAGMENTS {
                selection
                    .excluded
                    .insert(evidence.node_id, "context_budget".into());
            }
            continue;
        }
        selection.context_bytes += fragment.excerpt.len();
        selection.selected.push(evidence);
        fragments.push(fragment);
    }
    selection.advisory_status = if fragments.is_empty() {
        "no_relevant_advisory_fragment"
    } else {
        "selected"
    }
    .into();
    ScopedContext {
        fragments,
        selection,
    }
}

pub fn get_bindings(_repo_root: &Path) -> Bindings {
    let mut ops = std::collections::HashMap::new();
    ops.insert(
        "workspace.ensure".to_string(),
        "docs/agent/contribution-conventions.md#Current-PR sequence".to_string(),
    );
    ops.insert(
        "workspace.status".to_string(),
        "docs/agent/contribution-conventions.md#Current-PR sequence".to_string(),
    );
    ops.insert(
        "validate".to_string(),
        "docs/agent/command-contracts.md#`decapod validate`".to_string(),
    );

    let mut paths = std::collections::HashMap::new();
    paths.insert("rpc".to_string(), "interfaces/CONTROL_PLANE".to_string());

    let mut tags = std::collections::HashMap::new();
    tags.insert("security".to_string(), "specs/SECURITY".to_string());

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
                "docs/agent/contribution-conventions.md",
                Some("Current-PR sequence"),
            )?,
            check_tag: "gate.worktree.no_master".to_string(),
        }),
        "mandatory-init" => Some(Mandate {
            id: id.to_string(),
            severity: "non-negotiable".to_string(),
            fragment: get_fragment(
                repo_root,
                "docs/agent/api-index.md",
                Some("Before Any Agent Action"),
            )?,
            check_tag: "gate.session.active".to_string(),
        }),
        "mandatory-todo" => Some(Mandate {
            id: id.to_string(),
            severity: "required".to_string(),
            fragment: get_fragment(
                repo_root,
                "docs/agent/api-index.md",
                Some("Before Any Agent Action"),
            )?,
            check_tag: "gate.todo.active_task".to_string(),
        }),
        "validate-before-done" => Some(Mandate {
            id: id.to_string(),
            severity: "required".to_string(),
            fragment: get_fragment(
                repo_root,
                "docs/agent/command-contracts.md",
                Some("`decapod validate`"),
            )?,
            check_tag: "gate.validation.pass".to_string(),
        }),
        "isolated-worktree" => Some(Mandate {
            id: id.to_string(),
            severity: "required".to_string(),
            fragment: get_fragment(
                repo_root,
                "docs/agent/contribution-conventions.md",
                Some("Current-PR sequence"),
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
        truncate_chars(&excerpt, 497)
    } else {
        excerpt
    };

    Some(DocFragment {
        kind: "constitution".to_string(),
        r#ref: if let Some(a) = anchor {
            format!("{path}#{a}")
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
    usize::from(tokenize(haystack).iter().any(|term| term == needle))
}

fn get_best_fragment_for_terms(path: &str, content: &str, terms: &[String]) -> Option<DocFragment> {
    let mut current_title: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut in_project_override = false;

    for line in content.lines() {
        if line == "## Project Overrides" {
            if let Some(title) = current_title.take() {
                sections.push((title, current_lines.join("\n")));
                current_lines.clear();
            }
            current_title = Some("Project Overrides".to_string());
            in_project_override = true;
        } else if !in_project_override && line.starts_with('#') {
            if let Some(title) = current_title.take() {
                sections.push((title, current_lines.join("\n")));
                current_lines.clear();
            }
            current_title = Some(line.trim_start_matches('#').trim().to_string());
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
    fragment_from_section(path, &title, &section)
}

fn fragment_from_section(path: &str, title: &str, section: &str) -> Option<DocFragment> {
    let mut hasher = Sha256::new();
    hasher.update(section.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let excerpt = section.lines().take(12).collect::<Vec<_>>().join("\n");
    let excerpt = if excerpt.len() > 700 {
        truncate_chars(&excerpt, 697)
    } else {
        excerpt
    };

    Some(DocFragment {
        kind: "constitution".to_string(),
        r#ref: format!("{path}#{title}"),
        title: title.to_string(),
        excerpt,
        hash,
    })
}
#[cfg(test)]
#[path = "../../../tests/unit/core/docs_tests.rs"]
mod tests;
