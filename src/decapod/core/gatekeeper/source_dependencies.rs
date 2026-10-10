//! Follow literal source includes without allowing missing dependencies to pass.

use crate::core::error::DecapodError;
use proc_macro2::{TokenStream, TokenTree};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;

fn unresolved(message: impl Into<String>) -> DecapodError {
    DecapodError::ValidationError(format!(
        "Gatekeeper source dependency unresolved: {}",
        message.into()
    ))
}

pub(super) struct ExpandedSources {
    pub(super) paths: Vec<PathBuf>,
    pub(super) included: BTreeSet<PathBuf>,
}

pub(super) fn expand(root: &Path, paths: &[PathBuf]) -> Result<ExpandedSources, DecapodError> {
    let canonical_root = root.canonicalize().map_err(DecapodError::IoError)?;
    let mut queue = paths.to_vec();
    let mut visited = BTreeSet::new();
    let mut result = Vec::new();
    let mut included_paths = BTreeSet::new();
    while let Some(path) = queue.pop() {
        if !visited.insert(path.clone()) {
            continue;
        }
        if visited.len() > 1024 {
            return Err(unresolved("include graph exceeds 1024 files"));
        }
        result.push(path.clone());
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let full_path = root.join(&path);
        let source = match std::fs::read_to_string(&full_path) {
            Ok(source) => source,
            // Missing explicit paths are rejected by the CLI. Deleted staged
            // files retain the caller's existing semantics at this layer.
            Err(_) => continue,
        };
        let tokens = match TokenStream::from_str(&source) {
            Ok(tokens) => tokens,
            Err(_) if source.contains("include_str!") => {
                return Err(unresolved(format!(
                    "cannot parse include in {}",
                    path.display()
                )));
            }
            Err(_) => continue,
        };
        let mut includes = Vec::new();
        collect(tokens, &mut includes)?;
        for include in includes {
            if Path::new(&include).is_absolute() {
                return Err(unresolved(format!(
                    "absolute include in {}",
                    path.display()
                )));
            }
            let included = full_path.parent().unwrap_or(root).join(&include);
            let canonical = included.canonicalize().map_err(|error| {
                unresolved(format!("{} includes {include}: {error}", path.display()))
            })?;
            let relative = canonical.strip_prefix(&canonical_root).map_err(|_| {
                unresolved(format!(
                    "{} includes a path outside the repository",
                    path.display()
                ))
            })?;
            if !canonical.is_file() {
                return Err(unresolved(format!(
                    "{} includes a non-file path",
                    path.display()
                )));
            }
            std::fs::read_to_string(&canonical).map_err(|error| {
                unresolved(format!(
                    "{} includes unreadable text {include}: {error}",
                    path.display()
                ))
            })?;
            // Record origin before deduplication: a dependency must retain
            // source coverage even when it was also explicitly requested.
            included_paths.insert(relative.to_path_buf());
            queue.push(relative.to_path_buf());
        }
    }
    result.sort();
    result.dedup();
    Ok(ExpandedSources {
        paths: result,
        included: included_paths,
    })
}

fn collect(tokens: TokenStream, includes: &mut Vec<String>) -> Result<(), DecapodError> {
    let tokens: Vec<_> = tokens.into_iter().collect();
    for (index, token) in tokens.iter().enumerate() {
        if matches!(token, TokenTree::Ident(name) if name == "include_str")
            && matches!(tokens.get(index + 1), Some(TokenTree::Punct(punctuation)) if punctuation.as_char() == '!')
        {
            let Some(TokenTree::Group(arguments)) = tokens.get(index + 2) else {
                return Err(unresolved("malformed include_str invocation"));
            };
            let literal = syn::parse2::<syn::LitStr>(arguments.stream()).map_err(|_| {
                unresolved("include_str requires a literal path for safety scanning")
            })?;
            includes.push(literal.value());
        }
        if let TokenTree::Group(group) = token {
            collect(group.stream(), includes)?;
        }
    }
    Ok(())
}

/// Absolute `::std` names use the extern prelude in Rust 2018 and later.
/// Require the nearest owning Cargo manifest and an actual declared crate root;
/// an arbitrary .rs file beside Cargo.toml is not sufficient evidence.
pub(super) fn modern_crate_root(root: &Path, path: &Path) -> bool {
    if manifest_standard_library_ambiguous(root, path) {
        return false;
    }
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    let Ok(source) = root.join(path).canonicalize() else {
        return false;
    };
    let Some(mut directory) = source.parent() else {
        return false;
    };
    while directory.starts_with(&root) {
        let manifest_path = directory.join("Cargo.toml");
        if manifest_path.is_file() {
            let Some(manifest) = std::fs::read_to_string(&manifest_path)
                .ok()
                .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
            else {
                return false;
            };
            let Some(package) = manifest.get("package") else {
                return false;
            };
            // Visible aliases can replace even an absolute extern-prelude
            // name. This is a fail-closed manifest check, not authentication
            // of the compiler, sysroot, registry, or environment configuration.
            let mut ancestor = Some(directory);
            while let Some(parent) = ancestor.filter(|parent| parent.starts_with(&root)) {
                let ancestor_manifest = parent.join("Cargo.toml");
                if ancestor_manifest.is_file() {
                    let Some(value) = std::fs::read_to_string(&ancestor_manifest)
                        .ok()
                        .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
                    else {
                        return false;
                    };
                    if shadows_standard_library(&value) {
                        return false;
                    }
                }
                ancestor = parent.parent();
            }
            let edition = package.get("edition");
            let inherited;
            let edition = if edition
                .and_then(|edition| edition.get("workspace"))
                .and_then(toml::Value::as_bool)
                == Some(true)
            {
                let mut candidate = Some(directory);
                let mut value = None;
                while let Some(parent) = candidate.filter(|parent| parent.starts_with(&root)) {
                    if let Some(edition) = std::fs::read_to_string(parent.join("Cargo.toml"))
                        .ok()
                        .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
                        .and_then(|manifest| {
                            manifest
                                .get("workspace")?
                                .get("package")?
                                .get("edition")?
                                .as_str()
                                .map(str::to_owned)
                        })
                    {
                        value = Some(edition);
                        break;
                    }
                    candidate = parent.parent();
                }
                inherited = value;
                inherited.as_deref()
            } else {
                edition.and_then(toml::Value::as_str)
            };
            if !matches!(edition, Some("2018" | "2021" | "2024")) {
                return false;
            }
            let library = manifest
                .get("lib")
                .and_then(|lib| lib.get("path"))
                .and_then(toml::Value::as_str)
                .unwrap_or("src/lib.rs");
            if directory.join(library).canonicalize().ok().as_ref() == Some(&source) {
                return true;
            }
            return manifest
                .get("bin")
                .and_then(toml::Value::as_array)
                .is_some_and(|bins| {
                    bins.iter().any(|bin| {
                        bin.get("path")
                            .and_then(toml::Value::as_str)
                            .and_then(|path| directory.join(path).canonicalize().ok())
                            .as_ref()
                            == Some(&source)
                    })
                });
        }
        let Some(parent) = directory.parent() else {
            break;
        };
        directory = parent;
    }
    false
}

/// Missing manifests retain the original source-only evidence model. A
/// manifest that is present but unreadable or visibly substitutes std cannot
/// lend that trust to any contextual password exemption.
pub(super) fn manifest_standard_library_ambiguous(root: &Path, path: &Path) -> bool {
    let Ok(root) = root.canonicalize() else {
        return true;
    };
    let Ok(source) = root.join(path).canonicalize() else {
        return true;
    };
    if !source.starts_with(&root) {
        return true;
    }
    let Some(mut directory) = source.parent() else {
        return true;
    };
    while directory.starts_with(&root) {
        let manifest = directory.join("Cargo.toml");
        let present = match std::fs::symlink_metadata(&manifest) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => return true,
        };
        if present {
            let Ok(canonical) = manifest.canonicalize() else {
                return true;
            };
            if !canonical.starts_with(&root) {
                return true;
            }
            let Some(value) = std::fs::read_to_string(&manifest)
                .ok()
                .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
            else {
                return true;
            };
            if shadows_standard_library(&value) {
                return true;
            }
        }
        let Some(parent) = directory.parent() else {
            break;
        };
        directory = parent;
    }
    false
}

fn shadows_standard_library(manifest: &toml::Value) -> bool {
    if ["package", "lib"].iter().any(|section| {
        manifest
            .get(section)
            .and_then(|value| value.get("name"))
            .and_then(toml::Value::as_str)
            == Some("std")
    }) {
        return true;
    }
    fn nested(value: &toml::Value, parent: &str) -> bool {
        let Some(table) = value.as_table() else {
            return false;
        };
        table.iter().any(|(key, value)| {
            (matches!(
                parent,
                "dependencies" | "dev-dependencies" | "build-dependencies"
            ) && key == "std")
                || (parent == "replace" && (key == "std" || key.starts_with("std:")))
                || (parent == "patch"
                    && value.as_table().is_some_and(|table| {
                        table.contains_key("std")
                            || table.values().any(|dependency| {
                                dependency.get("package").and_then(toml::Value::as_str)
                                    == Some("std")
                            })
                    }))
                || nested(value, key)
        })
    }
    nested(manifest, "")
}
