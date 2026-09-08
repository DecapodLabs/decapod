//! Stable path handling for persisted governance artifacts.
//!
//! Absolute paths are useful while an operation is running, but they are
//! machine-specific and may disclose a user's home directory when copied into
//! a repository. This module keeps paths absolute at the operational boundary
//! and normalizes them only when they cross into durable governance output.

use std::fs;
use std::path::{Component, Path, PathBuf};

/// Normalize an absolute path for durable governance output.
///
/// Paths inside `base_root` become stable forward-slash relative paths.
/// Paths outside the project are represented by a non-reversible generic
/// token. Relative values are treated as already portable and are preserved.
pub fn normalize_persisted_path(base_root: &Path, raw: &str) -> String {
    if !is_absolute_like(raw) {
        return raw.to_string();
    }

    let candidate = resolve_with_missing_components(Path::new(raw));
    let base = resolve_with_missing_components(base_root);

    if let Ok(relative) = candidate.strip_prefix(&base)
        && !relative.as_os_str().is_empty()
    {
        return portable_string(relative);
    }

    "<external-path>".to_string()
}

/// Redact absolute path tokens embedded in validation prose.
///
/// Validation messages are assembled by many independent gates, so changing
/// every formatter would be brittle. This narrow scanner preserves ordinary
/// prose while replacing Unix absolute-path tokens at the final artifact
/// boundary. URLs are intentionally left untouched.
pub fn redact_text(base_root: &Path, text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        let is_path_start = chars[index] == '/'
            && (index == 0 || !is_path_boundary_predecessor(chars[index - 1]))
            && !(index > 0 && chars[index - 1] == ':');
        if !is_path_start {
            output.push(chars[index]);
            index += 1;
            continue;
        }

        let start = index;
        while index < chars.len() && !is_path_delimiter(chars[index]) {
            index += 1;
        }
        let mut end = index;
        while end > start && is_trailing_path_punctuation(chars[end - 1]) {
            end -= 1;
        }
        let token: String = chars[start..end].iter().collect();
        output.push_str(&normalize_persisted_path(base_root, &token));
        for character in &chars[end..index] {
            output.push(*character);
        }
    }

    output
}

fn is_absolute_like(raw: &str) -> bool {
    raw.starts_with('/')
        || raw
            .as_bytes()
            .get(1)
            .is_some_and(|_| raw.as_bytes()[1] == b':')
}

fn is_path_boundary_predecessor(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '/')
}

fn is_path_delimiter(character: char) -> bool {
    character.is_whitespace() || matches!(character, ',' | ';' | ')' | ']' | '}' | '"' | '\'')
}

fn is_trailing_path_punctuation(character: char) -> bool {
    matches!(character, '.' | ':' | '!' | '?')
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn resolve_with_missing_components(path: &Path) -> PathBuf {
    let mut missing = Vec::new();
    let mut existing = path;
    while !existing.exists() {
        let Some(name) = existing.file_name() else {
            break;
        };
        missing.push(name.to_os_string());
        let Some(parent) = existing.parent() else {
            break;
        };
        existing = parent;
    }

    let mut resolved = fs::canonicalize(existing).unwrap_or_else(|_| lexical_normalize(existing));
    for component in missing.iter().rev() {
        resolved.push(component);
    }
    lexical_normalize(&resolved)
}

fn portable_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
#[path = "../../../tests/unit/core/path_policy_tests.rs"]
mod tests;
