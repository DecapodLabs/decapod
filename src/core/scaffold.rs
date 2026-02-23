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
    LOCAL_PROJECT_SPECS_INTERFACES, LOCAL_PROJECT_SPECS_README, LOCAL_PROJECT_SPECS_VALIDATION,
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

fn specs_readme_template() -> String {
    r#"# Project Specs

This directory is managed by Decapod for the human+agent engineering contract of this repository.
Canonical path: `.decapod/generated/specs/`.
Decapod updates and validates these files as intent and implementation evolve.

- `INTENT.md` captures why this repository exists and what outcome it must achieve.
- `ARCHITECTURE.md` captures implementation topology, interfaces, and operational gates.
- `INTERFACES.md` captures inbound/outbound service contracts and failure behavior.
- `VALIDATION.md` captures proof surfaces, gate criteria, and required evidence artifacts.

Keep these documents current as requirements and implementation evolve.
"#
    .to_string()
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

    format!(
        r#"# Intent

## Product Outcome
- {product_outcome}

## Scope
- In scope for {product_name}:
- Out of scope:

## Constraints
- Technical:
- Operational:
- Security/compliance:

## Acceptance Criteria
- [ ] {done_criteria}
- [ ] Non-functional targets are met (latency, reliability, cost, etc.).
- [ ] Validation gates pass and artifacts are attached.

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

## Executive Summary
{summary}

## Integrated Surface
- Runtime/languages: {runtime_langs}
- Frameworks/libraries: {surfaces}
- Infrastructure/services: {product_type}
- External dependencies:

## Runtime and Deployment Matrix
- Execution environments (local/dev/stage/prod):
- Where each component runs (host/container/edge/serverless):
- Network topology and trust zones:
- Deployment/rollback strategy:
- Deployment assumptions inferred from repository: {deployment_hint}

## Implementation Strategy
- What is being built now:
- What is deferred:
- Why this cut line is chosen:

## System Topology
{diagram}

## Execution Physics
- End-to-end execution path (event to promoted output):
{flow_diagram}
- Concurrency and scheduling model:
- Queueing, backpressure, and retry semantics:
- Timeouts, cancellation, and idempotency model:
- Runtime execution note: {execution_hint}

## Service Contracts
- Inbound interfaces (API/events/CLI):
- Outbound interfaces (datastores/queues/third-party):
- Data ownership and consistency boundaries:

## Schema and Data Contracts
- Canonical entities and schema owners:
- Storage engines and data lifecycle (retention/compaction/archive):
- Migration policy (forward/backward compatibility, rollout order):
- Data validation and invariant checks:
- Schema responsibility note: {schema_hint}

## API and ABI Contracts
- API surface inventory (internal/external, versioning policy):
- Request/response compatibility contract (required/optional fields, defaults):
- Event contract compatibility rules:
- Binary/runtime ABI boundaries (plugins, FFI, wire formats, language interop):
- Deprecation window and breaking-change process:

## Multi-Agent Delivery Model
- Work partitioning strategy:
- Shared context/proof artifacts:
- Coordination and handoff rules:

## Validation Gates
- Unit/integration/e2e gates:
- Statistical/variance-aware gates (if nondeterministic surfaces exist):
- Release/promotion blockers:

## Operational Planes
- Observability contract (logs/metrics/traces and correlation IDs):
- On-call/incident response expectations:
- Capacity and scaling controls:
- Security controls (authn/authz, secret handling, audit trail):

## Failure Topology and Recovery
- Critical failure modes by component:
- Detection signals and health checks:
- Automated recovery paths and manual break-glass steps:
- Data integrity and replay strategy after faults:

## Performance Envelope
- Latency budgets (p50/p95/p99) per critical path:
- Throughput targets and saturation indicators:
- Cost envelope (compute/storage/network) and budget guardrails:
- Benchmark and load-test evidence requirements:

## Delivery Plan
- Milestone 1:
- Milestone 2:
- Milestone 3:

## Change Management
- Architectural Decision Record policy:
- Contract-change review checklist:
- Migration/release choreography:
- Post-release verification and rollback criteria:

## Risks and Mitigations
- Risk:
  Mitigation:
"#
    )
}

fn specs_interfaces_template() -> String {
    r#"# Interfaces

## Inbound Contracts
- API / RPC entrypoints:
- CLI surfaces:
- Event/webhook consumers:

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
"#
    .to_string()
}

fn specs_validation_template() -> String {
    r#"# Validation

## Proof Surfaces
- `decapod validate`
- Required test commands:
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
    .to_string()
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
    "!.decapod/generated/specs/",
    "!.decapod/generated/specs/*.md",
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
    let dockerfile_path = generated_dir.join("Dockerfile");
    if !dockerfile_path.exists() {
        let dockerfile_content = container::generated_dockerfile_for_repo(&opts.target_dir);
        fs::write(&dockerfile_path, dockerfile_content).map_err(error::DecapodError::IoError)?;
    }

    let (specs_created, specs_unchanged, specs_preserved) = if opts.generate_specs {
        let mut created = 0usize;
        let mut unchanged = 0usize;
        let mut preserved = 0usize;

        let seed = opts.specs_seed.as_ref();
        let mut specs_files: Vec<(&str, String)> = Vec::new();
        for spec in LOCAL_PROJECT_SPECS {
            let content = match spec.path {
                LOCAL_PROJECT_SPECS_README => specs_readme_template(),
                LOCAL_PROJECT_SPECS_INTENT => specs_intent_template(seed),
                LOCAL_PROJECT_SPECS_ARCHITECTURE => {
                    specs_architecture_template(opts.diagram_style, seed)
                }
                LOCAL_PROJECT_SPECS_INTERFACES => specs_interfaces_template(),
                LOCAL_PROJECT_SPECS_VALIDATION => specs_validation_template(),
                _ => continue,
            };
            specs_files.push((spec.path, content));
        }

        for (rel_path, content) in specs_files {
            match write_file(opts, rel_path, &content)? {
                FileAction::Created => created += 1,
                FileAction::Unchanged => unchanged += 1,
                FileAction::Preserved => preserved += 1,
            }
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
