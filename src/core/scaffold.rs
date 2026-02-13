//! Project scaffolding for Decapod initialization.
//!
//! This module handles the creation of Decapod project structure, including:
//! - Root entrypoints (CLAUDE.md, GEMINI.md, AGENTS.md)
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
use crate::core::tui;
use colored::Colorize;
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
    /// Force creation of all 3 entrypoint files regardless of existing state
    pub all: bool,
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

    // Project Structure box
    tui::render_box(
        "📦 PROJECT STRUCTURE",
        "Entrypoint & config generation",
        tui::BoxStyle::Magenta,
    );
    println!();

    // Ensure .decapod/data directory exists (constitution is embedded, not scaffolded)
    fs::create_dir_all(opts.target_dir.join(data_dir_rel)).map_err(error::DecapodError::IoError)?;

    // Determine which agent files to generate
    // If --all flag is set, force generate all three regardless of existing state
    // If agent_files is empty, generate all three
    // If agent_files has entries, only generate those
    let files_to_generate = if opts.all || opts.agent_files.is_empty() {
        vec!["AGENTS.md", "CLAUDE.md", "GEMINI.md"]
    } else {
        opts.agent_files.iter().map(|s| s.as_str()).collect()
    };

    // Root entrypoints from embedded templates
    let readme_md = assets::get_template("README.md").expect("Missing template: README.md");
    let override_md = assets::get_template("OVERRIDE.md").expect("Missing template: OVERRIDE.md");

    // AGENT ENTRYPOINTS - Neural Interfaces (only generate specified files)
    if !files_to_generate.is_empty() {
        tui::print_section("▼ AGENT ENTRYPOINTS");

        let mut results: Vec<(&str, tui::ItemStatus)> = Vec::new();
        for file in files_to_generate {
            let content =
                assets::get_template(file).unwrap_or_else(|| panic!("Missing template: {}", file));
            let action = write_file(opts, file, &content)?;
            let status = match action {
                FileAction::Created => tui::ItemStatus::Created,
                FileAction::Unchanged => tui::ItemStatus::Unchanged,
                FileAction::Preserved => tui::ItemStatus::Preserved,
            };
            results.push((file, status));
        }
        tui::print_items_grid(&results, 3);
    }

    tui::print_section("▼ CONTROL PLANE CONFIGURATION");

    let mut config_results: Vec<(&str, tui::ItemStatus)> = Vec::new();

    let readme_action = write_file(opts, ".decapod/README.md", &readme_md)?;
    let readme_status = match readme_action {
        FileAction::Created => tui::ItemStatus::Created,
        FileAction::Unchanged => tui::ItemStatus::Unchanged,
        FileAction::Preserved => tui::ItemStatus::Preserved,
    };
    config_results.push((".decapod/README.md", readme_status));

    // Preserve existing OVERRIDE.md - it contains project-specific customizations
    let override_path = opts.target_dir.join(".decapod/OVERRIDE.md");
    let override_status = if override_path.exists() {
        tui::ItemStatus::Preserved
    } else {
        let action = write_file(opts, ".decapod/OVERRIDE.md", &override_md)?;
        match action {
            FileAction::Created => tui::ItemStatus::Created,
            FileAction::Unchanged => tui::ItemStatus::Unchanged,
            FileAction::Preserved => tui::ItemStatus::Preserved,
        }
    };
    config_results.push((".decapod/OVERRIDE.md", override_status));
    tui::print_items_grid(&config_results, 3);

    // SUCCESS - System Online
    tui::render_box(
        "✨ CONTROL PLANE OPERATIONAL",
        "Agent workflow infrastructure ready",
        tui::BoxStyle::Success,
    );
    println!();
    tui::print_status_line("System ready for agentic workflows", tui::ItemStatus::Pass);
    println!(
        "  {} {}",
        "▸".bright_green(),
        "Neural interfaces: AGENTS.md | CLAUDE.md | GEMINI.md".bright_cyan()
    );

    // Show backup instructions if .bak files were created
    if opts.created_backups {
        tui::render_box(
            "⚠  ACTION REQUIRED",
            "Backup files need merging",
            tui::BoxStyle::Warning,
        );
        println!();
        println!("  {} Tell your agent to:", "▸".bright_yellow());
        println!(
            "    {} Blend your {} file(s) into {}",
            "1.".bright_cyan(),
            "*.bak".bright_white().bold(),
            ".decapod/OVERRIDE.md".bright_cyan()
        );
        println!(
            "    {} Delete the old {} files",
            "2.".bright_cyan(),
            "*.bak".bright_white().bold()
        );
    }

    println!();
    println!();

    // Constitution is embedded in binary - no scaffolding needed.
    // Users customize via OVERRIDE.md (scaffolded above).
    Ok(())
}
