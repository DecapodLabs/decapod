//! Project scaffolding for Decapod initialization.
//!
//! This module handles the creation of Decapod project structure, including:
//! - Root entrypoints (AGENTS.md, CLAUDE.md, GEMINI.md, CODEX.md)
//! - Constitution directory (.decapod/constitution/)
//! - Embedded methodology documents

use crate::core::assets;
use crate::core::error;
use crate::core::project_specs::{
    LOCAL_PROJECT_SPECS, LOCAL_PROJECT_SPECS_ARCHITECTURE, LOCAL_PROJECT_SPECS_INTENT,
    LOCAL_PROJECT_SPECS_INTERFACES, LOCAL_PROJECT_SPECS_MANIFEST,
    LOCAL_PROJECT_SPECS_MANIFEST_SCHEMA, LOCAL_PROJECT_SPECS_README,
    LOCAL_PROJECT_SPECS_VALIDATION, ProjectSpecManifestEntry, ProjectSpecsManifest, hash_text,
    repo_signal_fingerprint,
};
use crate::plugins::container;
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
    /// Generate project-facing specs/ scaffolding.
    pub generate_specs: bool,
    /// Diagram style for generated architecture document.
    pub diagram_style: DiagramStyle,
    /// Intent/architecture seed captured from inferred or user-confirmed repo context.
    pub specs_seed: Option<SpecsSeed>,
}

pub struct ScaffoldSummary {
    pub entrypoints_created: usize,
    pub entrypoints_unchanged: usize,
    pub entrypoints_preserved: usize,
    pub config_created: usize,
    pub config_unchanged: usize,
    pub config_preserved: usize,
    pub specs_created: usize,
    pub specs_unchanged: usize,
    pub specs_preserved: usize,
}

#[derive(Clone, Copy, Debug)]
pub enum DiagramStyle {
    Ascii,
    Mermaid,
}

#[derive(Clone, Debug)]
pub struct SpecsSeed {
    pub product_name: Option<String>,
    pub product_summary: Option<String>,
    pub architecture_direction: Option<String>,
    pub product_type: Option<String>,
    pub primary_languages: Vec<String>,
    pub detected_surfaces: Vec<String>,
    pub done_criteria: Option<String>,
}

fn joined_or_fallback(items: &[String], fallback: &str) -> String {
    if items.is_empty() {
        fallback.to_string()
    } else {
        items.join(", ")
    }
}

fn default_test_commands(seed: Option<&SpecsSeed>) -> Vec<String> {
    let mut commands = Vec::new();
    let langs = seed.map(|s| s.primary_languages.as_slice()).unwrap_or(&[]);
    let surfaces = seed.map(|s| s.detected_surfaces.as_slice()).unwrap_or(&[]);

    if langs.iter().any(|l| l.contains("rust")) {
        commands.push("cargo test".to_string());
    }
    if surfaces.iter().any(|s| s == "npm")
        || langs
            .iter()
            .any(|l| l.contains("typescript") || l.contains("javascript"))
    {
        commands.push("npm test".to_string());
    }
    if langs.iter().any(|l| l == "python") {
        commands.push("pytest".to_string());
    }
    if langs.iter().any(|l| l == "go") {
        commands.push("go test ./...".to_string());
    }
    commands
}

fn specs_readme_template(seed: Option<&SpecsSeed>) -> String {
    let product = seed
        .and_then(|s| s.product_name.as_deref())
        .unwrap_or("this repository");
    let summary = seed
        .and_then(|s| s.product_summary.as_deref())
        .unwrap_or("Define the intended user-visible outcome.");
    let languages = joined_or_fallback(
        seed.map(|s| s.primary_languages.as_slice()).unwrap_or(&[]),
        "not detected yet",
    );
    let surfaces = joined_or_fallback(
        seed.map(|s| s.detected_surfaces.as_slice()).unwrap_or(&[]),
        "not detected yet",
    );

    format!(
        r#"# Project Specs

Canonical path: `.decapod/generated/specs/`.
These files are the project-local contract for humans and agents.

## Snapshot
- Project: {product}
- Outcome: {summary}
- Detected languages: {languages}
- Detected surfaces: {surfaces}

## How to use this folder
- `INTENT.md`: what success means and what is explicitly out of scope.
- `ARCHITECTURE.md`: the current implementation shape and planned evolution.
- `INTERFACES.md`: API/CLI/events/storage contracts and failure behavior.
- `VALIDATION.md`: required proof commands and promotion gates.

## Canonical `.decapod/` Layout
- `.decapod/data/`: canonical control-plane state (SQLite + ledgers).
- `.decapod/generated/specs/`: living project specs for humans and agents.
- `.decapod/generated/context/`: deterministic context capsules.
- `.decapod/generated/artifacts/provenance/`: promotion manifests and convergence checklist.
- `.decapod/generated/artifacts/inventory/`: deterministic release inventory.
- `.decapod/generated/artifacts/diagnostics/`: opt-in diagnostics artifacts.
- `.decapod/workspaces/`: isolated todo-scoped git worktrees.

## Day-0 Onboarding Checklist
- [ ] Replace all placeholder bullets in each spec file.
- [ ] Confirm primary user outcome and acceptance criteria in `INTENT.md`.
- [ ] Document real interfaces and data boundaries in `INTERFACES.md`.
- [ ] Run and record validation commands listed in `VALIDATION.md`.
"#
    )
}

fn specs_intent_template(seed: Option<&SpecsSeed>) -> String {
    let product_outcome = seed
        .and_then(|s| s.product_summary.as_deref())
        .unwrap_or("Define the user-visible outcome in one paragraph.");
    let done_criteria = seed
        .and_then(|s| s.done_criteria.as_deref())
        .unwrap_or("Functional behavior is demonstrably correct.");
    let product_name = seed
        .and_then(|s| s.product_name.as_deref())
        .unwrap_or("this repository");
    let product_type = seed
        .and_then(|s| s.product_type.as_deref())
        .unwrap_or("not classified yet");
    let languages = joined_or_fallback(
        seed.map(|s| s.primary_languages.as_slice()).unwrap_or(&[]),
        "not detected yet",
    );
    let surfaces = joined_or_fallback(
        seed.map(|s| s.detected_surfaces.as_slice()).unwrap_or(&[]),
        "not detected yet",
    );

    format!(
        r#"# Intent

## Product Outcome
- {product_outcome}

## Inferred Baseline
- Repository: {product_name}
- Product type: {product_type}
- Primary languages: {languages}
- Detected surfaces: {surfaces}

## Scope
- In scope for {product_name}:
- Out of scope:

## Constraints
- Technical:
- Operational:
- Security/compliance:

## Acceptance Criteria (must be objectively testable)
- [ ] {done_criteria}
- [ ] Non-functional targets are met (latency, reliability, cost, etc.).
- [ ] Validation gates pass and artifacts are attached.

## First Implementation Slice
- [ ] Define the smallest user-visible workflow to ship first.
- [ ] Define what data/contracts are required for that workflow.
- [ ] Define what is intentionally postponed until v2.

## Open Questions
- List unresolved decisions that block implementation confidence.
"#
    )
}

fn specs_architecture_template(style: DiagramStyle, seed: Option<&SpecsSeed>) -> String {
    let summary = seed
        .and_then(|s| s.architecture_direction.as_deref())
        .unwrap_or(
            "Describe the architecture in 5-8 dense sentences focused on deployment reality, system boundaries, and operational risks.",
        );
    let runtime_langs = seed
        .map(|s| s.primary_languages.join(", "))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "to be confirmed".to_string());
    let surfaces = seed
        .map(|s| s.detected_surfaces.join(", "))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "to be confirmed".to_string());
    let product_type = seed
        .and_then(|s| s.product_type.as_deref())
        .unwrap_or("to be confirmed");
    let deployment_hint = if surfaces.contains("frontend") && surfaces.contains("backend") {
        "Frontend runs in user-facing edge/runtime environments; backend runs in service/container environments with explicit contract boundaries."
    } else if surfaces.contains("frontend") {
        "Primary runtime is client/edge-facing; deployment must include CDN/edge and API dependency policy."
    } else if surfaces.contains("backend") {
        "Primary runtime is service/container process space; deployment must include network, persistence, and rollout topology."
    } else {
        "Runtime topology must be explicitly defined before promotion."
    };
    let execution_hint = if runtime_langs.contains("rust") {
        "Process model should document async runtime, worker model, synchronization strategy, and blocking boundaries."
    } else {
        "Process model should document concurrency strategy, scheduling model, and isolation boundaries."
    };
    let schema_hint = if surfaces.contains("backend") {
        "Document authoritative schema objects, ownership boundaries, migration policy, and backward-compatibility rules."
    } else {
        "Document data models, state ownership, and compatibility policy for persisted/shared artifacts."
    };

    let diagram = match style {
        DiagramStyle::Ascii => {
            r#"```text
Human Intent
    |
    v
Agent Swarm(s)  <---->  Decapod Control Plane  <---->  Repo + Services
                             |      |      |
                             |      |      +-- Validation Gates
                             |      +--------- Provenance + Artifacts
                             +---------------- Work Unit / Context Governance
```"#
        }
        DiagramStyle::Mermaid => {
            r#"```mermaid
flowchart LR
  H[Human Intent] --> A[Agent Swarm(s)]
  A <--> D[Decapod Control Plane]
  D <--> R[Repo + Services]
  D --> G[Validation Gates]
  D --> P[Provenance + Artifacts]
  D --> W[Work Unit and Context Governance]
```"#
        }
    };
    let flow_diagram = match style {
        DiagramStyle::Ascii => {
            r#"```text
Input/Event --> Contract Parse --> Planning/Dispatch --> Execution --> Verification --> Promotion Gate
      |              |                  |                  |               |                 |
      +--------------+------------------+------------------+---------------+-----------------+
                                Trace + Metrics + Artifacts (durable evidence)
```"#
        }
        DiagramStyle::Mermaid => {
            r#"```mermaid
flowchart LR
  I[Input or Event] --> C[Contract Parse]
  C --> P[Planning or Dispatch]
  P --> E[Execution]
  E --> V[Verification]
  V --> G[Promotion Gate]
  I -.-> T[Trace + Metrics + Artifacts]
  C -.-> T
  P -.-> T
  E -.-> T
  V -.-> T
```"#
        }
    };

    format!(
        r#"# Architecture

## Direction
{summary}

## Current Facts
- Runtime/languages: {runtime_langs}
- Detected surfaces/framework hints: {surfaces}
- Product type: {product_type}

## Topology
{diagram}

## Execution Path
{flow_diagram}
- Deployment assumptions: {deployment_hint}
- Concurrency/runtime note: {execution_hint}

## Data and Contracts
- Inbound contracts (CLI/API/events):
- Outbound dependencies (datastores/queues/external APIs):
- Data ownership boundaries:
- Schema responsibility note: {schema_hint}

## Delivery Plan (first 3 slices)
- Slice 1 (ship first):
- Slice 2:
- Slice 3:

## Risks and Mitigations
- Risk:
  Mitigation:
"#
    )
}

fn specs_interfaces_template(seed: Option<&SpecsSeed>) -> String {
    let surfaces = joined_or_fallback(
        seed.map(|s| s.detected_surfaces.as_slice()).unwrap_or(&[]),
        "not detected yet",
    );
    let product_type = seed
        .and_then(|s| s.product_type.as_deref())
        .unwrap_or("not classified yet");
    format!(
        r#"# Interfaces

## Inbound Contracts
- API / RPC entrypoints:
- CLI surfaces:
- Event/webhook consumers:
- Repository-detected surfaces: {surfaces}

## Outbound Dependencies
- Datastores:
- External APIs/services:
- Queues/brokers:

## Data Ownership
- Source-of-truth tables/collections:
- Cross-boundary read models:
- Consistency expectations:

## Failure Semantics
- Retry/backoff policy:
- Timeout/circuit behavior:
- Degradation behavior:

## Interface Notes
- Product type hint: {product_type}
- Enumerate explicit error codes for each mutating interface.
"#
    )
}

fn specs_validation_template(seed: Option<&SpecsSeed>) -> String {
    let commands = default_test_commands(seed);
    let test_commands = if commands.is_empty() {
        "- Add repository-specific test command(s) here.".to_string()
    } else {
        commands
            .into_iter()
            .map(|c| format!("- `{}`", c))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        r#"# Validation

## Proof Surfaces
- `decapod validate`
- Required test commands:
{test_commands}
- Required integration/e2e commands:

## Promotion Gates
- Blocking gates:
- Warning-only gates:
- Kill switches:

## Evidence Artifacts
- Manifest paths:
- Required hashes/checksums:
- Trace/log attachments:

## Regression Guardrails
- Baseline references:
- Statistical thresholds (if non-deterministic):
- Rollback criteria:
"#
    )
}

/// Canonical .gitignore rules managed by `decapod init`.
///
/// These rules are appended (if missing) to the user's root `.gitignore`.
/// Keep this as the source of truth so new allowlists/denylists evolve through code review.
pub const DECAPOD_GITIGNORE_RULES: &[&str] = &[
    ".decapod/data",
    ".decapod/data/*",
    ".decapod/.stfolder",
    ".decapod/workspaces",
    ".decapod/generated/*",
    "!.decapod/data/",
    "!.decapod/data/knowledge.promotions.jsonl",
    "!.decapod/generated/Dockerfile",
    "!.decapod/generated/context/",
    "!.decapod/generated/context/*.json",
    "!.decapod/generated/artifacts/",
    "!.decapod/generated/artifacts/provenance/",
    "!.decapod/generated/artifacts/provenance/*.json",
    "!.decapod/generated/specs/",
    "!.decapod/generated/specs/*.md",
    "!.decapod/generated/specs/.manifest.json",
];

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

pub fn scaffold_project_entrypoints(
    opts: &ScaffoldOptions,
) -> Result<ScaffoldSummary, error::DecapodError> {
    let data_dir_rel = ".decapod/data";

    // Ensure .decapod/data directory exists (constitution is embedded, not scaffolded)
    fs::create_dir_all(opts.target_dir.join(data_dir_rel)).map_err(error::DecapodError::IoError)?;

    // Ensure Decapod-managed ignore/allowlist rules are present in the user's .gitignore.
    if !opts.dry_run {
        for rule in DECAPOD_GITIGNORE_RULES {
            ensure_gitignore_entry(&opts.target_dir, rule)?;
        }
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
        let content =
            assets::get_template(file).unwrap_or_else(|| panic!("Missing template: {}", file));
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

    // Blend legacy agent files if they existed before init
    if !opts.dry_run {
        blend_legacy_entrypoints(&opts.target_dir)?;
    }

    // Generate .decapod/generated/Dockerfile from Rust-owned template component.
    let generated_dir = opts.target_dir.join(".decapod/generated");
    fs::create_dir_all(&generated_dir).map_err(error::DecapodError::IoError)?;
    fs::create_dir_all(generated_dir.join("context")).map_err(error::DecapodError::IoError)?;
    fs::create_dir_all(generated_dir.join("artifacts").join("provenance"))
        .map_err(error::DecapodError::IoError)?;
    fs::create_dir_all(generated_dir.join("artifacts").join("inventory"))
        .map_err(error::DecapodError::IoError)?;
    fs::create_dir_all(
        generated_dir
            .join("artifacts")
            .join("diagnostics")
            .join("validate"),
    )
    .map_err(error::DecapodError::IoError)?;
    let dockerfile_path = generated_dir.join("Dockerfile");
    if !dockerfile_path.exists() {
        let dockerfile_content = container::generated_dockerfile_for_repo(&opts.target_dir);
        fs::write(&dockerfile_path, dockerfile_content).map_err(error::DecapodError::IoError)?;
    }

    let (specs_created, specs_unchanged, specs_preserved) = if opts.generate_specs {
        let mut created = 0usize;
        let mut unchanged = 0usize;
        let mut preserved = 0usize;
        let mut manifest_entries: Vec<ProjectSpecManifestEntry> = Vec::new();

        let seed = opts.specs_seed.as_ref();
        let mut specs_files: Vec<(&str, String)> = Vec::new();
        for spec in LOCAL_PROJECT_SPECS {
            let content = match spec.path {
                LOCAL_PROJECT_SPECS_README => specs_readme_template(seed),
                LOCAL_PROJECT_SPECS_INTENT => specs_intent_template(seed),
                LOCAL_PROJECT_SPECS_ARCHITECTURE => {
                    specs_architecture_template(opts.diagram_style, seed)
                }
                LOCAL_PROJECT_SPECS_INTERFACES => specs_interfaces_template(seed),
                LOCAL_PROJECT_SPECS_VALIDATION => specs_validation_template(seed),
                _ => continue,
            };
            specs_files.push((spec.path, content));
        }

        for (rel_path, content) in specs_files {
            let template_hash = hash_text(&content);
            match write_file(opts, rel_path, &content)? {
                FileAction::Created => created += 1,
                FileAction::Unchanged => unchanged += 1,
                FileAction::Preserved => preserved += 1,
            }
            manifest_entries.push(ProjectSpecManifestEntry {
                path: rel_path.to_string(),
                template_hash: template_hash.clone(),
                content_hash: template_hash,
            });
        }

        if !opts.dry_run {
            let manifest = ProjectSpecsManifest {
                schema_version: LOCAL_PROJECT_SPECS_MANIFEST_SCHEMA.to_string(),
                template_version: "scaffold-v1".to_string(),
                generated_at: crate::core::time::now_epoch_z(),
                repo_signal_fingerprint: repo_signal_fingerprint(&opts.target_dir)?,
                files: manifest_entries,
            };
            let manifest_path = opts.target_dir.join(LOCAL_PROJECT_SPECS_MANIFEST);
            ensure_parent(&manifest_path)?;
            let manifest_body = serde_json::to_string_pretty(&manifest).map_err(|e| {
                error::DecapodError::ValidationError(format!(
                    "Failed to serialize specs manifest: {}",
                    e
                ))
            })?;
            fs::write(manifest_path, manifest_body).map_err(error::DecapodError::IoError)?;
        }
        (created, unchanged, preserved)
    } else {
        (0usize, 0usize, 0usize)
    };

    Ok(ScaffoldSummary {
        entrypoints_created: ep_created,
        entrypoints_unchanged: ep_unchanged,
        entrypoints_preserved: ep_preserved,
        config_created: cfg_created,
        config_unchanged: cfg_unchanged,
        config_preserved: cfg_preserved,
        specs_created,
        specs_unchanged,
        specs_preserved,
    })
}

/// Automatically blends content from non-Decapod AGENT.md/CLAUDE.md/GEMINI.md backups
/// into .decapod/OVERRIDE.md and deletes the backups.
pub fn blend_legacy_entrypoints(target_dir: &Path) -> Result<(), error::DecapodError> {
    let override_path = target_dir.join(".decapod/OVERRIDE.md");
    let mut overrides_added = false;
    let mut content_to_add = String::new();

    for file in ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"] {
        let bak_path = target_dir.join(format!("{}.bak", file));
        if bak_path.exists() {
            if let Ok(bak_content) = fs::read_to_string(&bak_path) {
                // Only add if not empty
                let trimmed = bak_content.trim();
                if !trimmed.is_empty() {
                    content_to_add.push_str(&format!(
                        "\n\n### Blended from Legacy {} Entrypoint\n\n{}\n",
                        file.replace(".md", ""),
                        trimmed
                    ));
                    overrides_added = true;
                }
            }
            // Delete backup file after blending (or if empty)
            let _ = fs::remove_file(&bak_path);
        }
    }

    if overrides_added && override_path.exists() {
        let mut existing = fs::read_to_string(&override_path).unwrap_or_default();
        existing.push_str(&content_to_add);
        fs::write(&override_path, existing).map_err(error::DecapodError::IoError)?;
    }

    Ok(())
}
