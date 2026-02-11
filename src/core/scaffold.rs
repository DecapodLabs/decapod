use crate::core::assets;
use crate::core::error;
use std::fs;
use std::path::{Path, PathBuf};

pub struct ScaffoldOptions {
    pub target_dir: PathBuf,
    pub force: bool,
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
                "  would-skip: {} (exists; pass --force to overwrite)",
                dest.display()
            );
            return Ok(());
        }
        return Err(error::DecapodError::ValidationError(format!(
            "Refusing to overwrite existing path without --force: {}",
            dest.display()
        )));
    }

    if opts.dry_run {
        println!("  would-write: {}", dest.display());
        return Ok(());
    }

    ensure_parent(&dest)?;
    fs::write(&dest, content).map_err(error::DecapodError::IoError)?;
    println!("  wrote: {}", dest.display());
    Ok(())
}

pub fn scaffold_project_entrypoints(opts: &ScaffoldOptions) -> Result<(), error::DecapodError> {
    let const_docs_rel = ".decapod/constitutions";
    let data_dir_rel = ".decapod/data";

    println!(
        "Scaffolding Decapod entrypoints into {}",
        opts.target_dir.display()
    );

    // Ensure main .decapod/constitutions and .decapod/data directories exist
    fs::create_dir_all(opts.target_dir.join(const_docs_rel))
        .map_err(error::DecapodError::IoError)?;
    fs::create_dir_all(opts.target_dir.join(data_dir_rel)).map_err(error::DecapodError::IoError)?;

    // Root entrypoints from embedded templates (AGENTS.md, CLAUDE.md, GEMINI.md)
    let agents_md = assets::get_template("AGENTS.md").expect("Missing template: AGENTS.md");
    let claude_md = assets::get_template("CLAUDE.md").expect("Missing template: CLAUDE.md");
    let gemini_md = assets::get_template("GEMINI.md").expect("Missing template: GEMINI.md");
    let readme_md = assets::get_template("README.md").expect("Missing template: README.md");

    write_file(opts, "AGENTS.md", &agents_md)?;
    write_file(opts, "CLAUDE.md", &claude_md)?;
    write_file(opts, "GEMINI.md", &gemini_md)?;
    write_file(opts, ".decapod/README.md", &readme_md)?;

    // Constitutions for the current project context
    let const_templates = [
        (
            "core/CONTROL_PLANE.md",
            assets::TEMPLATES_CORE_CONTROL_PLANE,
        ),
        ("core/DECAPOD.md", assets::TEMPLATES_CORE_DECAPOD),
        ("core/PLUGINS.md", assets::TEMPLATES_CORE_PLUGINS),
        ("core/CLAIMS.md", assets::TEMPLATES_CORE_CLAIMS),
        ("core/DEMANDS.md", assets::TEMPLATES_CORE_DEMANDS),
        ("core/DEPRECATION.md", assets::TEMPLATES_CORE_DEPRECATION),
        ("core/DOC_RULES.md", assets::TEMPLATES_CORE_DOC_RULES),
        ("core/GLOSSARY.md", assets::TEMPLATES_CORE_GLOSSARY),
        ("core/KNOWLEDGE.md", assets::TEMPLATES_CORE_KNOWLEDGE),
        ("core/MEMORY.md", assets::TEMPLATES_CORE_MEMORY),
        ("core/SOUL.md", assets::TEMPLATES_CORE_SOUL),
        ("core/STORE_MODEL.md", assets::TEMPLATES_CORE_STORE_MODEL),
        ("specs/AMENDMENTS.md", assets::TEMPLATES_SPECS_AMENDMENTS),
        (
            "specs/ARCHITECTURE.md",
            assets::TEMPLATES_SPECS_ARCHITECTURE,
        ),
        ("specs/INTENT.md", assets::TEMPLATES_SPECS_INTENT),
        ("specs/SYSTEM.md", assets::TEMPLATES_SPECS_SYSTEM),
        ("plugins/DB_BROKER.md", assets::TEMPLATES_PLUGINS_DB_BROKER),
        ("plugins/MANIFEST.md", assets::TEMPLATES_PLUGINS_MANIFEST),
        ("plugins/TODO.md", assets::TEMPLATES_PLUGINS_TODO),
        ("plugins/CRON.md", assets::TEMPLATES_PLUGINS_CRON),
        ("plugins/REFLEX.md", assets::TEMPLATES_PLUGINS_REFLEX),
        ("plugins/HEALTH.md", assets::TEMPLATES_PLUGINS_HEALTH),
        ("plugins/POLICY.md", assets::TEMPLATES_PLUGINS_POLICY),
        ("plugins/WATCHER.md", assets::TEMPLATES_PLUGINS_WATCHER),
        ("plugins/KNOWLEDGE.md", assets::TEMPLATES_PLUGINS_KNOWLEDGE),
        ("plugins/ARCHIVE.md", assets::TEMPLATES_PLUGINS_ARCHIVE),
        ("plugins/FEEDBACK.md", assets::TEMPLATES_PLUGINS_FEEDBACK),
        ("plugins/TRUST.md", assets::TEMPLATES_PLUGINS_TRUST),
        ("plugins/CONTEXT.md", assets::TEMPLATES_PLUGINS_CONTEXT),
        ("plugins/HEARTBEAT.md", assets::TEMPLATES_PLUGINS_HEARTBEAT),
    ];

    for (name, content) in const_templates {
        let rel_path = format!("{}/{}", const_docs_rel, name);
        let dest = opts.target_dir.join(&rel_path);
        if dest.exists() && !opts.force {
            println!("  skipping existing constitution doc: {}", rel_path);
            continue;
        }
        write_file(opts, &rel_path, content)?;
    }
    Ok(())
}
