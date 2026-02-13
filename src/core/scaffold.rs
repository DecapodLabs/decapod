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
    /// Which agent entrypoint files to generate (empty = all)
    pub agent_files: Vec<String>,
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
    use colored::Colorize;
    use sha2::{Digest, Sha256};

    let dest = opts.target_dir.join(rel_path);

    if dest.exists() {
        // Check if content matches (by checksum)
        if let Ok(existing_content) = fs::read_to_string(&dest) {
            let mut template_hasher = Sha256::new();
            template_hasher.update(content.as_bytes());
            let template_hash = format!("{:x}", template_hasher.finalize());

            let mut existing_hasher = Sha256::new();
            existing_hasher.update(existing_content.as_bytes());
            let existing_hash = format!("{:x}", existing_hasher.finalize());

            if template_hash == existing_hash {
                // File already matches template - skip
                println!(
                    "    {} {} {}",
                    "✓".bright_green(),
                    rel_path.bright_white(),
                    "(unchanged)".bright_black()
                );
                return Ok(());
            }
        }

        if !opts.force {
            if opts.dry_run {
                println!("    {} {}", "○".bright_black(), rel_path.bright_black());
                return Ok(());
            }
            return Err(error::DecapodError::ValidationError(format!(
                "Refusing to overwrite existing path without --force: {}",
                dest.display()
            )));
        }
    }

    if opts.dry_run {
        println!("    {} {}", "◉".bright_cyan(), rel_path.bright_white());
        return Ok(());
    }

    ensure_parent(&dest)?;
    fs::write(&dest, content).map_err(error::DecapodError::IoError)?;

    // Fancy checkmark with gradient effect
    println!("    {} {}", "●".bright_green(), rel_path.bright_white());
    Ok(())
}

pub fn scaffold_project_entrypoints(opts: &ScaffoldOptions) -> Result<(), error::DecapodError> {
    let data_dir_rel = ".decapod/data";

    // ALIEN SCAFFOLD PROTOCOL
    println!();
    println!(
        "        {}",
        "╔═══════════════════════════════════════════╗"
            .bright_magenta()
            .bold()
    );
    println!(
        "        {} {} {}",
        "║".bright_magenta().bold(),
        "📦 PROJECT STRUCTURE SYNTHESIS 📦     "
            .bright_white()
            .bold(),
        "║".bright_magenta().bold()
    );
    println!(
        "        {}",
        "╚═══════════════════════════════════════════╝"
            .bright_magenta()
            .bold()
    );
    println!();

    // Ensure .decapod/data directory exists (constitution is embedded, not scaffolded)
    fs::create_dir_all(opts.target_dir.join(data_dir_rel)).map_err(error::DecapodError::IoError)?;

    // Determine which agent files to generate
    // If agent_files is empty, generate all three
    // If agent_files has entries, only generate those
    let files_to_generate = if opts.agent_files.is_empty() {
        vec!["AGENTS.md", "CLAUDE.md", "GEMINI.md"]
    } else {
        opts.agent_files.iter().map(|s| s.as_str()).collect()
    };

    // Root entrypoints from embedded templates
    let readme_md = assets::get_template("README.md").expect("Missing template: README.md");
    let override_md = assets::get_template("OVERRIDE.md").expect("Missing template: OVERRIDE.md");

    // AGENT ENTRYPOINTS - Neural Interfaces (only generate specified files)
    if !files_to_generate.is_empty() {
        println!("          {}", "▼ AGENT ENTRYPOINTS".bright_cyan().bold());
        println!();

        for file in files_to_generate {
            let content =
                assets::get_template(file).unwrap_or_else(|| panic!("Missing template: {}", file));
            write_file(opts, file, &content)?;
        }
    }

    println!();
    println!(
        "          {}",
        "▼ CONTROL PLANE CONFIGURATION".bright_cyan().bold()
    );
    println!();
    write_file(opts, ".decapod/README.md", &readme_md)?;
    write_file(opts, ".decapod/OVERRIDE.md", &override_md)?;

    // SUCCESS - System Online
    println!();
    println!(
        "        {}",
        "╔═══════════════════════════════════════════╗"
            .bright_green()
            .bold()
    );
    println!(
        "        {} {} {}",
        "║".bright_green().bold(),
        "✨ CONTROL PLANE OPERATIONAL ✨       "
            .bright_white()
            .bold(),
        "║".bright_green().bold()
    );
    println!(
        "        {}",
        "╚═══════════════════════════════════════════╝"
            .bright_green()
            .bold()
    );
    println!();
    println!(
        "          {} System ready for agentic workflows",
        "▸".bright_green()
    );
    println!(
        "          {} Neural interfaces: {}",
        "▸".bright_green(),
        "AGENTS.md | CLAUDE.md | GEMINI.md".bright_cyan()
    );
    println!();
    println!();

    // Constitution is embedded in binary - no scaffolding needed.
    // Users customize via OVERRIDE.md (scaffolded above).
    Ok(())
}
