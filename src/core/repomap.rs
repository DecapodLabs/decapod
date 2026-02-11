use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RepoMap {
    pub manifests: BTreeMap<String, String>, // file_name -> type
    pub entry_points: Vec<String>,
    pub build_hints: Vec<String>,
    pub skill_hints: Vec<String>,
    pub doc_graph: Option<DocGraph>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DocGraph {
    pub nodes: Vec<String>,
    pub edges: Vec<(String, String)>,
    pub mermaid: String,
}

pub fn generate_map(root: &Path) -> RepoMap {
    let mut manifests = BTreeMap::new();
    let mut entry_points = Vec::new();
    let mut build_hints = Vec::new();
    let mut skill_hints = Vec::new();

    // Check for common manifests
    let manifest_types = [
        ("Cargo.toml", "rust"),
        ("package.json", "node"),
        ("requirements.txt", "python"),
        ("go.mod", "go"),
        ("Makefile", "make"),
        ("docker-compose.yml", "docker"),
    ];

    for (file, mtype) in manifest_types {
        if root.join(file).exists() {
            manifests.insert(file.to_string(), mtype.to_string());
        }
    }

    // Identify entry points
    let entry_candidates = [
        "src/main.rs",
        "src/index.ts",
        "src/index.js",
        "main.py",
        "app.py",
        "cmd/main.go",
    ];

    for entry in entry_candidates {
        if root.join(entry).exists() {
            entry_points.push(entry.to_string());
        }
    }
    entry_points.sort();

    // Build hints (purely derived from manifests)
    if manifests.contains_key("Cargo.toml") {
        build_hints.push("cargo build".to_string());
        skill_hints.push("rust".to_string());
    }
    if manifests.contains_key("package.json") {
        build_hints.push("npm install".to_string());
        skill_hints.push("node".to_string());
    }
    if manifests.contains_key("Makefile") {
        build_hints.push("make".to_string());
    }
    build_hints.sort();
    skill_hints.sort();

    RepoMap {
        manifests,
        entry_points,
        build_hints,
        skill_hints,
        doc_graph: Some(generate_doc_graph(root)),
    }
}

pub fn generate_doc_graph(root: &Path) -> DocGraph {
    let mut nodes = HashSet::new();
    let mut edges = Vec::new();
    let mut md_files = Vec::new();

    collect_md_files(root, root, &mut md_files);
    md_files.sort();

    let existing: HashSet<String> = md_files.iter().cloned().collect();

    let link_re = Regex::new(r"\[[^\]]*\]\(([^)]+\.md)(?:#[^)]+)?\)").unwrap();
    let path_re = Regex::new(r"(?P<path>(?:[A-Za-z0-9_./-]+)\.md)").unwrap();

    for src_rel in &md_files {
        let full_path = root.join(src_rel);
        let content = fs::read_to_string(&full_path).unwrap_or_default();
        let mut refs = HashSet::new();

        for cap in link_re.captures_iter(&content) {
            refs.insert(cap[1].to_string());
        }
        for cap in path_re.captures_iter(&content) {
            refs.insert(cap["path"].to_string());
        }

        for r in refs {
            if r.contains("://") || !r.ends_with(".md") {
                continue;
            }
            let direct = if r.starts_with("./") { r[2..].to_string() } else { r };
            
            // Resolve relative to src file
            let src_parent = Path::new(src_rel).parent().unwrap_or(Path::new(""));
            let candidate = src_parent.join(&direct);
            
            // Normalize path (very basic)
            let mut normalized = Vec::new();
            for component in candidate.components() {
                match component {
                    std::path::Component::ParentDir => { normalized.pop(); }
                    std::path::Component::Normal(c) => { normalized.push(c); }
                    _ => {}
                }
            }
            let dst_rel = normalized.iter().map(|c| c.to_string_lossy()).collect::<Vec<_>>().join("/");

            if existing.contains(&dst_rel) && &dst_rel != src_rel {
                nodes.insert(src_rel.clone());
                nodes.insert(dst_rel.clone());
                edges.push((src_rel.clone(), dst_rel.clone()));
            }
        }
    }

    let mut sorted_nodes: Vec<String> = nodes.into_iter().collect();
    sorted_nodes.sort();
    edges.sort();
    edges.dedup();

    let mut mermaid = String::from("graph TD\n");
    for n in &sorted_nodes {
        let nid = n.replace(|c: char| !c.is_alphanumeric(), "_");
        mermaid.push_str(&format!("  {}[\"{}\"]\n", nid, n));
    }
    for (src, dst) in &edges {
        let aid = src.replace(|c: char| !c.is_alphanumeric(), "_");
        let bid = dst.replace(|c: char| !c.is_alphanumeric(), "_");
        mermaid.push_str(&format!("  {} --> {}\n", aid, bid));
    }

    DocGraph {
        nodes: sorted_nodes,
        edges,
        mermaid,
    }
}

fn collect_md_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if name == ".git" || name == "target" || name == ".decapod" {
                    continue;
                }
                collect_md_files(root, &path, out);
            } else if path.is_file() && path.extension().map_or(false, |e| e == "md") {
                if let Ok(rel) = path.strip_prefix(root) {
                    let rel_str = rel.to_string_lossy().to_string();
                    if rel_str != "docs/DOC_MAP.md" {
                        out.push(rel_str);
                    }
                }
            }
        }
    }
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "repomap",
        "version": "0.1.0",
        "description": "Deterministic repository mapping and doc graph",
        "commands": [
            { "name": "map", "description": "Output repository summary including doc graph" }
        ],
        "storage": []
    })
}
