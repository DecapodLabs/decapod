//! Embedded constitution and template assets.
//!
//! This module provides compile-time embedded access to Decapod's methodology documents.
//! All constitution files (core, specs, plugins) are baked into the binary for
//! hermetic deployment - no external files required.

use std::path::Path;

/// Macro to embed constitution documents at compile time as text.
///
/// Generates:
/// - Public constants for each embedded document
/// - `get_embedded_doc(path)` function for lookup
/// - `list_docs()` function for discovery
macro_rules! embedded_docs {
    ($($path:expr => $const_name:ident),* $(,)?) => {
        $(
            pub const $const_name: &str =
                include_str!(concat!("../../constitution/", $path));
        )*

        pub fn get_embedded_doc(path: &str) -> Option<String> {
            // Support both bare paths and legacy "embedded/" prefix
            let key = path.strip_prefix("embedded/").unwrap_or(path);
            match key {
                $( $path => Some($const_name.to_string()), )*
                _ => None,
            }
        }

        pub fn list_docs() -> Vec<String> {
            vec![ $( $path.to_string(), )* ]
        }
    };
}

embedded_docs! {
    // Core: Routers and indices
    "core/DECAPOD.md" => EMBEDDED_CORE_DECAPOD,
    "core/INTERFACES.md" => EMBEDDED_CORE_INTERFACES,
    "core/METHODOLOGY.md" => EMBEDDED_CORE_METHODOLOGY,
    "core/PLUGINS.md" => EMBEDDED_CORE_PLUGINS,
    "core/GAPS.md" => EMBEDDED_CORE_GAPS,
    "core/DEMANDS.md" => EMBEDDED_CORE_DEMANDS,
    "core/DEPRECATION.md" => EMBEDDED_CORE_DEPRECATION,

    // Specs: System contracts
    "specs/INTENT.md" => EMBEDDED_SPECS_INTENT,
    "specs/SYSTEM.md" => EMBEDDED_SPECS_SYSTEM,
    "specs/AMENDMENTS.md" => EMBEDDED_SPECS_AMENDMENTS,
    "specs/SECURITY.md" => EMBEDDED_SPECS_SECURITY,
    "specs/GIT.md" => EMBEDDED_SPECS_GIT,
    "specs/evaluations/VARIANCE_EVALS.md" => EMBEDDED_SPECS_VARIANCE_EVALS,
    "specs/evaluations/JUDGE_CONTRACT.md" => EMBEDDED_SPECS_JUDGE_CONTRACT,
    "specs/engineering/FRONTEND_BACKEND_E2E.md" => EMBEDDED_SPECS_FRONTEND_BACKEND_E2E,
    "specs/skills/SKILL_GOVERNANCE.md" => EMBEDDED_SPECS_SKILL_GOVERNANCE,

    // Interfaces: Binding contracts
    "interfaces/CLAIMS.md" => EMBEDDED_INTERFACES_CLAIMS,
    "interfaces/CONTROL_PLANE.md" => EMBEDDED_INTERFACES_CONTROL_PLANE,
    "interfaces/DOC_RULES.md" => EMBEDDED_INTERFACES_DOC_RULES,
    "interfaces/GLOSSARY.md" => EMBEDDED_INTERFACES_GLOSSARY,
    "interfaces/STORE_MODEL.md" => EMBEDDED_INTERFACES_STORE_MODEL,
    "interfaces/TESTING.md" => EMBEDDED_INTERFACES_TESTING,
    "interfaces/KNOWLEDGE_SCHEMA.md" => EMBEDDED_INTERFACES_KNOWLEDGE_SCHEMA,
    "interfaces/KNOWLEDGE_STORE.md" => EMBEDDED_INTERFACES_KNOWLEDGE_STORE,
    "interfaces/MEMORY_SCHEMA.md" => EMBEDDED_INTERFACES_MEMORY_SCHEMA,
    "interfaces/DEMANDS_SCHEMA.md" => EMBEDDED_INTERFACES_DEMANDS_SCHEMA,
    "interfaces/TODO_SCHEMA.md" => EMBEDDED_INTERFACES_TODO_SCHEMA,
    "interfaces/PLAN_GOVERNED_EXECUTION.md" => EMBEDDED_INTERFACES_PLAN_GOVERNED_EXECUTION,
    "interfaces/AGENT_CONTEXT_PACK.md" => EMBEDDED_INTERFACES_AGENT_CONTEXT_PACK,

    // Methodology: Practice guides
    "methodology/ARCHITECTURE.md" => EMBEDDED_METHODOLOGY_ARCHITECTURE,
    "methodology/SOUL.md" => EMBEDDED_METHODOLOGY_SOUL,
    "methodology/KNOWLEDGE.md" => EMBEDDED_METHODOLOGY_KNOWLEDGE,
    "methodology/MEMORY.md" => EMBEDDED_METHODOLOGY_MEMORY,
    "methodology/TESTING.md" => EMBEDDED_METHODOLOGY_TESTING,
    "methodology/CI_CD.md" => EMBEDDED_METHODOLOGY_CI_CD,

    // Architecture: Domain patterns
    "architecture/DATA.md" => EMBEDDED_ARCHITECTURE_DATA,
    "architecture/CACHING.md" => EMBEDDED_ARCHITECTURE_CACHING,
    "architecture/MEMORY.md" => EMBEDDED_ARCHITECTURE_MEMORY,
    "architecture/WEB.md" => EMBEDDED_ARCHITECTURE_WEB,
    "architecture/CLOUD.md" => EMBEDDED_ARCHITECTURE_CLOUD,
    "architecture/FRONTEND.md" => EMBEDDED_ARCHITECTURE_FRONTEND,
    "architecture/ALGORITHMS.md" => EMBEDDED_ARCHITECTURE_ALGORITHMS,
    "architecture/SECURITY.md" => EMBEDDED_ARCHITECTURE_SECURITY,
    "architecture/CONCURRENCY.md" => EMBEDDED_ARCHITECTURE_CONCURRENCY,
    "architecture/OBSERVABILITY.md" => EMBEDDED_ARCHITECTURE_OBSERVABILITY,

    // Embedded docs used by entrypoints/operators
    "docs/ARCHITECTURE_OVERVIEW.md" => EMBEDDED_DOCS_ARCHITECTURE_OVERVIEW,
    "docs/CONTROL_PLANE_API.md" => EMBEDDED_DOCS_CONTROL_PLANE_API,
    "docs/MAINTAINERS.md" => EMBEDDED_DOCS_MAINTAINERS,
    "docs/MIGRATIONS.md" => EMBEDDED_DOCS_MIGRATIONS,
    "docs/NEGLECTED_ASPECTS_LEDGER.md" => EMBEDDED_DOCS_NEGLECTED_ASPECTS_LEDGER,
    "docs/PLAYBOOK.md" => EMBEDDED_DOCS_PLAYBOOK,
    "docs/README.md" => EMBEDDED_DOCS_README,
    "docs/RELEASE_PROCESS.md" => EMBEDDED_DOCS_RELEASE_PROCESS,
    "docs/SECURITY_THREAT_MODEL.md" => EMBEDDED_DOCS_SECURITY_THREAT_MODEL,
    "docs/EVAL_TRANSLATION_MAP.md" => EMBEDDED_DOCS_EVAL_TRANSLATION_MAP,
    "docs/SKILL_TRANSLATION_MAP.md" => EMBEDDED_DOCS_SKILL_TRANSLATION_MAP,

    "plugins/ARCHIVE.md" => EMBEDDED_PLUGINS_ARCHIVE,
    "plugins/AUTOUPDATE.md" => EMBEDDED_PLUGINS_AUTOUPDATE,
    "plugins/CONTEXT.md" => EMBEDDED_PLUGINS_CONTEXT,
    "plugins/CRON.md" => EMBEDDED_PLUGINS_CRON,
    "plugins/DB_BROKER.md" => EMBEDDED_PLUGINS_DB_BROKER,
    "plugins/DECIDE.md" => EMBEDDED_PLUGINS_DECIDE,
    "plugins/EMERGENCY_PROTOCOL.md" => EMBEDDED_PLUGINS_EMERGENCY_PROTOCOL,
    "plugins/FEDERATION.md" => EMBEDDED_PLUGINS_FEDERATION,
    "plugins/FEEDBACK.md" => EMBEDDED_PLUGINS_FEEDBACK,
    "plugins/HEALTH.md" => EMBEDDED_PLUGINS_HEALTH,
    "plugins/HEARTBEAT.md" => EMBEDDED_PLUGINS_HEARTBEAT,
    "plugins/KNOWLEDGE.md" => EMBEDDED_PLUGINS_KNOWLEDGE,
    "plugins/MANIFEST.md" => EMBEDDED_PLUGINS_MANIFEST,
    "plugins/POLICY.md" => EMBEDDED_PLUGINS_POLICY,
    "plugins/REFLEX.md" => EMBEDDED_PLUGINS_REFLEX,
    "plugins/APTITUDE.md" => EMBEDDED_PLUGINS_APTITUDE,
    "plugins/TODO.md" => EMBEDDED_PLUGINS_TODO,
    "plugins/TRUST.md" => EMBEDDED_PLUGINS_TRUST,
    "plugins/VERIFY.md" => EMBEDDED_PLUGINS_VERIFY,
    "plugins/WATCHER.md" => EMBEDDED_PLUGINS_WATCHER,
}

/// Legacy function - now just forwards to get_embedded_doc
pub fn get_doc(path: &str) -> Option<String> {
    get_embedded_doc(path)
}

/// Get only the override document from .decapod/OVERRIDE.md for a specific component
pub fn get_override_doc(repo_root: &Path, relative_path: &str) -> Option<String> {
    let override_path = repo_root.join(".decapod").join("OVERRIDE.md");

    if !override_path.exists() {
        return None;
    }

    let override_content = std::fs::read_to_string(&override_path).ok()?;
    extract_component_override(&override_content, relative_path)
}

/// Extract a specific component's override content from OVERRIDE.md
fn extract_component_override(override_content: &str, component_path: &str) -> Option<String> {
    // Only look after the "CHANGES ARE NOT PERMITTED ABOVE THIS LINE" marker
    let override_start = override_content.find("CHANGES ARE NOT PERMITTED ABOVE THIS LINE")?;
    let searchable_content = &override_content[override_start..];

    // Look for the section heading: ### core/DECAPOD.md (or other path)
    let section_marker = format!("\n### {}", component_path);

    let start = searchable_content.find(&section_marker)?;
    let content_start = start + section_marker.len();

    // Find the next ### heading or end of file
    let content_after = &searchable_content[content_start..];
    let end = content_after
        .find("\n### ")
        .map(|pos| content_start + pos)
        .unwrap_or(searchable_content.len());

    let extracted = searchable_content[content_start..end].trim();

    if extracted.is_empty() {
        None
    } else {
        Some(extracted.to_string())
    }
}

/// Get merged document (embedded base + optional project override from OVERRIDE.md)
pub fn get_merged_doc(repo_root: &Path, relative_path: &str) -> Option<String> {
    // Get embedded base
    let embedded_content = get_embedded_doc(relative_path)?;

    // Check for component-specific override in .decapod/OVERRIDE.md
    if let Some(override_content) = get_override_doc(repo_root, relative_path) {
        return Some(merge_override_content(&embedded_content, &override_content));
    }

    Some(embedded_content)
}

/// Merge embedded content with override additions
fn merge_override_content(embedded_content: &str, override_content: &str) -> String {
    format!(
        "{}\n\n---\n\n## Project Overrides\n\n{}",
        embedded_content.trim(),
        override_content.trim()
    )
}

fn agent_entrypoint_body() -> &'static str {
    r#"You are working in a Decapod-managed repository.
See `AGENTS.md` for the universal contract.

## Quick Start

```bash
cargo install decapod

decapod validate
decapod docs ingest
decapod session acquire
decapod rpc --op agent.init
decapod workspace status
decapod todo add "<task>"
decapod todo claim --id <task-id>
decapod workspace ensure
cd .decapod/workspaces/<your-worktree>
decapod rpc --op context.resolve
```

## Control-Plane First

```bash
decapod capabilities --format json
decapod rpc --op context.scope --params '{"query":"<problem>","limit":8}'
decapod data schema --deterministic
```

## Operating Mode

- Use Docker git workspaces and execute in `.decapod/workspaces/*`.
- Call `decapod workspace status` at startup and before implementation work.
- request elevated permissions before Docker/container workspace commands.
- `.decapod files are accessed only via decapod CLI`.
- `DECAPOD_SESSION_PASSWORD` is required for session-scoped operations.
- Read canonical router: `decapod docs show core/DECAPOD.md`.
- Use shared aptitude memory for human-taught preferences across sessions/providers: `decapod data memory add|get` (aliases: `decapod data aptitude`).
- Operator reference: `decapod docs show docs/PLAYBOOK.md`.
- Capability authority: `decapod capabilities --format json`.
- Scoped context feature: `decapod docs search --query "<problem>" --op <op>` or `decapod rpc --op context.scope`.

Stop if requirements are ambiguous or conflicting.
"#
}

fn template_agents() -> String {
    r#"# AGENTS.md — Universal Agent Contract

This is a Decapod-managed repository. **Strict Dependency: You are strictly bound to the Decapod control plane.**

This contract applies equally to Claude, Codex, Gemini, and any other agent operating here.

## Mandatory Initialization

```bash
cargo install decapod

decapod validate
decapod docs ingest
decapod session acquire
decapod rpc --op agent.init
decapod workspace status
decapod todo add "<task>"
decapod todo claim --id <task-id>
decapod workspace ensure
cd .decapod/workspaces/<your-worktree>
decapod rpc --op context.resolve
```

## Control-Plane First Loop

```bash
# Discover what this binary actually supports in this repo
decapod capabilities --format json
decapod data schema --deterministic

# Resolve scoped governance context before implementation
decapod docs search --query "<problem>" --op <op> --path <path> --tag <tag>
decapod rpc --op context.scope --params '{"query":"<problem>","limit":8}'

# Convergence/proof surfaces (call when relevant)
decapod workunit init --task-id <task-id> --intent-ref <intent>
decapod govern capsule query --topic "<topic>" --scope interfaces --task-id <task-id>
decapod eval plan --task-set-id <id> --task-ref <task-id> --model-id <model> --prompt-hash <hash> --judge-model-id <judge> --judge-prompt-hash <hash>
```

## Golden Rules (Non-Negotiable)

1. Always refine intent with the user before inference-heavy work.
2. Never work on main/master. Use `.decapod/workspaces/*`.
3. `.decapod files are accessed only via decapod CLI`.
4. Never claim done without `decapod validate` passing.
5. Never invent capabilities that are not exposed by the binary.
6. Stop if requirements conflict, intent is ambiguous, or policy boundaries are unclear.
7. Respect the Interface abstraction boundary.

## Safety Invariants

- ✅ Router pointer: `core/DECAPOD.md`
- ✅ Validation gate: `decapod validate`
- ✅ Constitution ingestion gate: `decapod docs ingest`
- ✅ Workspace status gate: `decapod workspace status`
- ✅ Claim-before-work gate: `decapod todo claim --id <task-id>`
- ✅ Session auth gate: `DECAPOD_SESSION_PASSWORD`
- ✅ Workspace gate: Docker git workspaces
- ✅ Privilege gate: request elevated permissions before Docker/container workspace commands

## Operating Notes

- Use `decapod docs show core/DECAPOD.md` and `decapod docs show core/INTERFACES.md` for binding contracts.
- Use `decapod capabilities --format json` as the authority surface for available operations.
- Use Decapod shared aptitude memory for human-taught preferences that must persist across sessions and agents: `decapod data memory add|get` (aliases: `decapod data aptitude`).
- Use `decapod docs search --query \"<problem>\" --op <op> --path <path> --tag <tag>` or `decapod rpc --op context.scope --params '{\"query\":\"...\"}'` for scoped just-in-time constitution context.
- Use `decapod todo handoff --id <id> --to <agent>` for cross-agent ownership transfer.
- Treat lock/contention failures (including `VALIDATE_TIMEOUT_OR_LOCK`) as blocking until resolved.
"#
    .to_string()
}

fn template_named_agent(file_stem: &str) -> String {
    format!(
        "# {}.md - Agent Entrypoint\n\n{}",
        file_stem,
        agent_entrypoint_body()
    )
}

fn template_readme() -> String {
    r#"# .decapod - Decapod Project Metadata 🦀✨

Welcome to the control-plane directory for this repo.

## Quick Start

1. **Initialize**: Run `decapod init` to set up your project
2. **Configure overrides**: Edit `.decapod/OVERRIDE.md` to customize behavior
3. **Read docs**: Use `decapod docs show <path>` to read constitution docs

## Summary

The `.decapod/OVERRIDES.md` file is your project-local override layer for Decapod's embedded constitution.

The embedded constitution (shipped with Decapod) is read-only baseline policy.
`.decapod/OVERRIDE.md` is where you add project-specific behavior without forking Decapod.

Keep overrides in the correct section, minimal and explicit.

## How to Use Overrides

The embedded constitution (read-only, shipped with Decapod) provides the base methodology. The `.decapod/OVERRIDE.md` file lets you customize behavior without forking Decapod.

**To add an override:**

1. Find the component section in `OVERRIDE.md` (Core, Specs, Interfaces, Methodology, Architecture, or Plugins)
2. Scroll to the specific component you want to override (e.g., `### plugins/TODO.md`)
3. Write your override content under that heading
4. Use markdown formatting for your overrides
5. Commit this file to version control

**Example override:**

```markdown
### plugins/TODO.md

## Priority Levels (Project Override)

For this project, we use a 5-level priority system:
- **critical**: Production down, blocking release
- **high**: Sprint commitment, must complete this iteration
- **medium**: Backlog, next sprint candidate
- **low**: Nice-to-have, future consideration
- **idea**: Exploration, needs refinement before actionable
```

## Available Override Sections

- **Core**: DECAPOD.md, INTERFACES.md, METHODOLOGY.md, PLUGINS.md, GAPS.md, DEMANDS.md, DEPRECATION.md
- **Specs**: INTENT.md, SYSTEM.md, AMENDMENTS.md, SECURITY.md, GIT.md
- **Interfaces**: CLAIMS.md, CONTROL_PLANE.md, DOC_RULES.md, GLOSSARY.md, STORE_MODEL.md
- **Methodology**: ARCHITECTURE.md, SOUL.md, KNOWLEDGE.md, MEMORY.md
- **Architecture**: DATA.md, CACHING.md, MEMORY.md, WEB.md, CLOUD.md, FRONTEND.md, ALGORITHMS.md, SECURITY.md, OBSERVABILITY.md, CONCURRENCY.md
- **Plugins**: TODO.md, MANIFEST.md, EMERGENCY_PROTOCOL.md, DB_BROKER.md, CRON.md, REFLEX.md, HEALTH.md, POLICY.md, WATCHER.md, KNOWLEDGE.md, ARCHIVE.md, FEDERATION.md, FEEDBACK.md, TRUST.md, CONTEXT.md, HEARTBEAT.md, APTITUDE.md, VERIFY.md, DECIDE.md, AUTOUPDATE.md

## Canonical Layout

- `OVERRIDE.md`: Project-local override layer for embedded constitution.
- `data/`: Canonical control-plane state (SQLite + ledgers). Access through Decapod commands.
- `generated/specs/`: Living project specs scaffolded by `decapod init`.
- `generated/context/`: Deterministic context capsule artifacts.
- `generated/artifacts/provenance/`: Promotion manifests + convergence checklist.
- `generated/artifacts/inventory/`: Release inventory artifacts.
- `generated/artifacts/diagnostics/`: Opt-in diagnostics artifacts.
- `workspaces/`: Isolated todo-scoped git worktrees for implementation.

## Policy

- Treat `.decapod/` as the single home for Decapod-managed project state.
- Keep top-level project directories focused on product/source code, not control-plane artifacts.
- Access `.decapod` files via Decapod commands unless explicitly documented otherwise.
"#
    .to_string()
}

fn template_override() -> String {
    r#"# OVERRIDE.md - Project-Specific Decapod Overrides

> **IMPORTANT:** For detailed usage instructions and examples, see [README.md](README.md).

**Canonical:** OVERRIDE.md
**Authority:** override
**Layer:** Project
**Binding:** Yes (overrides embedded constitution)

<!-- ═══════════════════════════════════════════════════════════════════════ -->
<!-- ⚠️  CHANGES ARE NOT PERMITTED ABOVE THIS LINE                           -->
<!-- ═══════════════════════════════════════════════════════════════════════ -->

## Core Overrides (Routers and Indices)

### core/DECAPOD.md

### core/INTERFACES.md

### core/METHODOLOGY.md

### core/PLUGINS.md

### core/GAPS.md

### core/DEMANDS.md

### core/DEPRECATION.md

---

## Specs Overrides (System Contracts)

### specs/INTENT.md

### specs/SYSTEM.md

### specs/AMENDMENTS.md

### specs/SECURITY.md

### specs/GIT.md

---

## Interfaces Overrides (Binding Contracts)

### interfaces/CLAIMS.md

### interfaces/CONTROL_PLANE.md

### interfaces/DOC_RULES.md

### interfaces/GLOSSARY.md

### interfaces/STORE_MODEL.md

---

## Methodology Overrides (Practice Guides)

### methodology/ARCHITECTURE.md

### methodology/SOUL.md

### methodology/KNOWLEDGE.md

### methodology/MEMORY.md

---

## Architecture Overrides (Domain Patterns)

### architecture/DATA.md

### architecture/CACHING.md

### architecture/MEMORY.md

### architecture/WEB.md

### architecture/CLOUD.md

### architecture/FRONTEND.md

### architecture/ALGORITHMS.md

### architecture/SECURITY.md

### architecture/OBSERVABILITY.md

### architecture/CONCURRENCY.md

---

## Plugins Overrides (Operational Subsystems)

### plugins/TODO.md

### plugins/MANIFEST.md

### plugins/EMERGENCY_PROTOCOL.md

### plugins/DB_BROKER.md

### plugins/CRON.md

### plugins/REFLEX.md

### plugins/HEALTH.md

### plugins/POLICY.md

### plugins/WATCHER.md

### plugins/KNOWLEDGE.md

### plugins/ARCHIVE.md

### plugins/FEDERATION.md

### plugins/FEEDBACK.md

### plugins/TRUST.md

### plugins/CONTEXT.md

### plugins/HEARTBEAT.md

### plugins/APTITUDE.md

### plugins/VERIFY.md

### plugins/DECIDE.md

### plugins/AUTOUPDATE.md
"#
    .to_string()
}

pub fn get_template(name: &str) -> Option<String> {
    match name {
        "AGENTS.md" => Some(template_agents()),
        "CLAUDE.md" => Some(template_named_agent("CLAUDE")),
        "GEMINI.md" => Some(template_named_agent("GEMINI")),
        "CODEX.md" => Some(template_named_agent("CODEX")),
        "README.md" => Some(template_readme()),
        "OVERRIDE.md" => Some(template_override()),
        _ => None,
    }
}
