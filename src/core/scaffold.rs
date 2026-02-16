//! Project scaffolding for Decapod initialization.
//!
//! This module handles the creation of Decapod project structure, including:
//! - Root entrypoints (AGENTS.md, CLAUDE.md, GEMINI.md, CODEX.md)
//! - Constitution directory (.decapod/constitution/)
//! - Embedded methodology documents
//!
//! # For AI Agents
//!
//! - **Scaffolding is idempotent**: Safe to run multiple times with `--force`
//! - **Dry-run mode available**: Use `--dry-run` to preview changes
//! - **Never scaffold over existing files**: Requires explicit `--force` flag
//! - **Constitution is embedded**: Templates come from binary, not external files

use crate::core::assets;
use crate::core::error;
use std::fs;
use std::path::{Path, PathBuf};

/// Scaffolding operation configuration.
///
/// Controls how project initialization templates are written to disk.
pub struct ScaffoldOptions {
    /// Target directory for scaffold output (usually project root)
    pub target_dir: PathBuf,
    /// Force overwrite of existing files
    pub force: bool,
    /// Preview mode - log actions without writing files
    pub dry_run: bool,
    /// Which agent entrypoint files to generate (empty = all)
    pub agent_files: Vec<String>,
    /// Whether .bak files were created during init
    pub created_backups: bool,
    /// Force creation of all 5 entrypoint files regardless of existing state
    pub all: bool,
}

/// Ensure a given entry exists in the project's .gitignore file.
/// Creates the file if it doesn't exist. Appends the entry if not already present.
fn ensure_gitignore_entry(target_dir: &Path, entry: &str) -> Result<(), error::DecapodError> {
    let gitignore_path = target_dir.join(".gitignore");
    let content = fs::read_to_string(&gitignore_path).unwrap_or_default();

    // Check if the entry already exists (exact line match)
    if content.lines().any(|line| line.trim() == entry) {
        return Ok(());
    }

    let mut new_content = content;
    if !new_content.is_empty() && !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    new_content.push_str(entry);
    new_content.push('\n');
    fs::write(&gitignore_path, new_content).map_err(error::DecapodError::IoError)?;
    Ok(())
}

fn ensure_parent(path: &Path) -> Result<(), error::DecapodError> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).map_err(error::DecapodError::IoError)?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub enum FileAction {
    Created,
    Unchanged,
    Preserved,
}

fn write_file(
    opts: &ScaffoldOptions,
    rel_path: &str,
    content: &str,
) -> Result<FileAction, error::DecapodError> {
    use sha2::{Digest, Sha256};

    let dest = opts.target_dir.join(rel_path);

    if dest.exists() {
        if let Ok(existing_content) = fs::read_to_string(&dest) {
            let mut template_hasher = Sha256::new();
            template_hasher.update(content.as_bytes());
            let template_hash = format!("{:x}", template_hasher.finalize());

            let mut existing_hasher = Sha256::new();
            existing_hasher.update(existing_content.as_bytes());
            let existing_hash = format!("{:x}", existing_hasher.finalize());

            if template_hash == existing_hash {
                return Ok(FileAction::Unchanged);
            }
        }

        if !opts.force {
            if opts.dry_run {
                return Ok(FileAction::Unchanged);
            }
            return Err(error::DecapodError::ValidationError(format!(
                "Refusing to overwrite existing path without --force: {}",
                dest.display()
            )));
        }
    }

    if opts.dry_run {
        return Ok(FileAction::Created);
    }

    ensure_parent(&dest)?;
    fs::write(&dest, content).map_err(error::DecapodError::IoError)?;

    Ok(FileAction::Created)
}

pub fn scaffold_project_entrypoints(opts: &ScaffoldOptions) -> Result<(), error::DecapodError> {
    let data_dir_rel = ".decapod/data";
    println!("init: generating entrypoints and control-plane config");

    // Ensure .decapod/data directory exists (constitution is embedded, not scaffolded)
    fs::create_dir_all(opts.target_dir.join(data_dir_rel)).map_err(error::DecapodError::IoError)?;

    // Ensure .decapod/data is in .gitignore (sqlite databases should not be version controlled)
    if !opts.dry_run {
        ensure_gitignore_entry(&opts.target_dir, ".decapod/data")?;
    }

    // Determine which agent files to generate
    // If --all flag is set, force generate all five regardless of existing state
    // If agent_files is empty, generate all five
    // If agent_files has entries, only generate those
    let files_to_generate = if opts.all || opts.agent_files.is_empty() {
        vec!["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"]
    } else {
        opts.agent_files.iter().map(|s| s.as_str()).collect()
    };

    // Root entrypoints from embedded templates
    let readme_md = assets::get_template("README.md").expect("Missing template: README.md");
    let override_md = assets::get_template("OVERRIDE.md").expect("Missing template: OVERRIDE.md");

    // AGENT ENTRYPOINTS - Neural Interfaces (only generate specified files)
    let mut ep_created = 0usize;
    let mut ep_unchanged = 0usize;
    let mut ep_preserved = 0usize;
    for file in files_to_generate {
        let content = assets::get_template(file).unwrap_or_else(|| panic!("Missing template: {}", file));
        match write_file(opts, file, &content)? {
            FileAction::Created => ep_created += 1,
            FileAction::Unchanged => ep_unchanged += 1,
            FileAction::Preserved => ep_preserved += 1,
        }
    }

    let mut cfg_created = 0usize;
    let mut cfg_unchanged = 0usize;
    let mut cfg_preserved = 0usize;

    match write_file(opts, ".decapod/README.md", &readme_md)? {
        FileAction::Created => cfg_created += 1,
        FileAction::Unchanged => cfg_unchanged += 1,
        FileAction::Preserved => cfg_preserved += 1,
    }

    // Preserve existing OVERRIDE.md - it contains project-specific customizations.
    let override_path = opts.target_dir.join(".decapod/OVERRIDE.md");
    if override_path.exists() {
        cfg_preserved += 1;
    } else {
        match write_file(opts, ".decapod/OVERRIDE.md", &override_md)? {
            FileAction::Created => cfg_created += 1,
            FileAction::Unchanged => cfg_unchanged += 1,
            FileAction::Preserved => cfg_preserved += 1,
        }
    }

    println!(
        "init: entrypoints created={}, unchanged={}, preserved={}",
        ep_created, ep_unchanged, ep_preserved
    );
    println!(
        "init: config created={}, unchanged={}, preserved={}",
        cfg_created, cfg_unchanged, cfg_preserved
    );
    println!("init: control plane ready");

    // Show backup instructions if .bak files were created
    if opts.created_backups {
        println!(
            "init: backup files were created (*.bak); merge required content into .decapod/OVERRIDE.md"
        );
    }

    // Constitution is embedded in binary - no scaffolding needed.
    // Users customize via OVERRIDE.md (scaffolded above).
    Ok(())
}
