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
}

fn ensure_parent(path: &Path) -> Result<(), error::DecapodError> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).map_err(error::DecapodError::IoError)?;
    }
    Ok(())
}

fn write_file(
    opts: &ScaffoldOptions,
    rel_path: &str,
    content: &str,
) -> Result<(), error::DecapodError> {
    let dest = opts.target_dir.join(rel_path);

    if dest.exists() && !opts.force {
        if opts.dry_run {
            println!(
                "  {} {} {}",
                "○".dimmed(),
                "would skip".dimmed(),
                rel_path.dimmed()
            );
            return Ok(());
        }
        return Err(error::DecapodError::ValidationError(format!(
            "Refusing to overwrite existing path without --force: {}",
            dest.display()
        )));
    }

    if opts.dry_run {
        println!(
            "  {} {} {}",
            "○".cyan(),
            "would create".cyan(),
            rel_path.cyan()
        );
        return Ok(());
    }

    ensure_parent(&dest)?;
    fs::write(&dest, content).map_err(error::DecapodError::IoError)?;
    println!("  {} {}", "✓".green(), rel_path.bright_white());
    Ok(())
}

pub fn scaffold_project_entrypoints(opts: &ScaffoldOptions) -> Result<(), error::DecapodError> {
    let data_dir_rel = ".decapod/data";

    // Header
    println!();
    println!("{}", "┌─────────────────────────────────────────────┐".cyan());
    println!("{}", "│  📦 Scaffolding Decapod Project Structure  │".cyan());
    println!("{}", "└─────────────────────────────────────────────┘".cyan());
    println!();

    // Ensure .decapod/data directory exists (constitution is embedded, not scaffolded)
    fs::create_dir_all(opts.target_dir.join(data_dir_rel)).map_err(error::DecapodError::IoError)?;

    // Root entrypoints from embedded templates (AGENTS.md, CLAUDE.md, GEMINI.md)
    let agents_md = assets::get_template("AGENTS.md").expect("Missing template: AGENTS.md");
    let claude_md = assets::get_template("CLAUDE.md").expect("Missing template: CLAUDE.md");
    let gemini_md = assets::get_template("GEMINI.md").expect("Missing template: GEMINI.md");
    let readme_md = assets::get_template("README.md").expect("Missing template: README.md");
    let override_md = assets::get_template("OVERRIDE.md").expect("Missing template: OVERRIDE.md");

    println!("{}", "  Agent Entrypoints:".bright_white().bold());
    write_file(opts, "AGENTS.md", &agents_md)?;
    write_file(opts, "CLAUDE.md", &claude_md)?;
    write_file(opts, "GEMINI.md", &gemini_md)?;

    println!();
    println!("{}", "  Decapod Configuration:".bright_white().bold());
    write_file(opts, ".decapod/README.md", &readme_md)?;
    write_file(opts, ".decapod/OVERRIDE.md", &override_md)?;

    // Footer
    println!();
    println!("{}", "  ────────────────────────────────────────────".dimmed());
    println!();
    println!("  {} Project initialized successfully!", "✓".green().bold());
    println!();
    println!("  {} Get started:", "→".cyan().bold());
    println!("    {} Read the methodology", "•".dimmed());
    println!("      {}", "decapod docs show core/DECAPOD.md".bright_white());
    println!();
    println!("    {} Validate your setup", "•".dimmed());
    println!("      {}", "decapod validate".bright_white());
    println!();

    // Constitution is embedded in binary - no scaffolding needed.
    // Users customize via OVERRIDE.md (scaffolded above).
    Ok(())
}
