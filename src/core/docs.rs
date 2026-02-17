use crate::core::assets;
use sha2::{Digest, Sha256};
use std::path::Path;

/// A fragment of a constitution or authority document.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocFragment {
    pub kind: String,
    pub r#ref: String,
    pub title: String,
    pub excerpt: String,
    pub hash: String,
}

/// Extract a markdown fragment by anchor (heading).
/// If anchor is None, returns the whole file.
pub fn get_fragment(repo_root: &Path, path: &str, anchor: Option<&str>) -> Option<DocFragment> {
    let content = assets::get_merged_doc(repo_root, path)?;
    
    let (fragment_content, title) = if let Some(a) = anchor {
        extract_section(&content, a)?
    } else {
        let title = content.lines().next().unwrap_or("Untitled").trim_start_matches("# ").to_string();
        (content.clone(), title)
    };

    let mut hasher = Sha256::new();
    hasher.update(fragment_content.as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    let excerpt = fragment_content.lines().take(10).collect::<Vec<_>>().join("
");
    let excerpt = if excerpt.len() > 500 {
        format!("{}...", &excerpt[..497])
    } else {
        excerpt
    };

    Some(DocFragment {
        kind: "constitution".to_string(),
        r#ref: if let Some(a) = anchor { format!("{}#{}", path, a) } else { path.to_string() },
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
        Some((section_lines.join("
"), section_title))
    } else {
        None
    }
}
