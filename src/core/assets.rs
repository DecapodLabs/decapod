//! Embedded constitution and template assets.
//!
//! This module provides compile-time embedded access to Decapod's methodology documents.
//! All constitution files are baked into the binary via `constitution.json`.

use std::path::Path;

// Include the auto-generated compressed constitution
include!(concat!(env!("OUT_DIR"), "/constitution_compressed.rs"));

/// Get an embedded document by its ID (e.g., "core/DECAPOD")
pub fn get_embedded_doc(id: &str) -> Option<String> {
    let key = id.strip_prefix("embedded/").unwrap_or(id);

    // 1. Try exact match (e.g., "core/DECAPOD")
    if let Some(content) = get_decompressed(key) {
        return Some(content);
    }

    // 2. Try normalizing dots to slashes (e.g., "core.DECAPOD" -> "core/DECAPOD")
    let normalized = key.replace('.', "/");
    if let Some(content) = get_decompressed(&normalized) {
        return Some(content);
    }

    None
}

/// List all available constitution document IDs
pub fn list_docs() -> Vec<String> {
    list_ids().into_iter().map(|s| s.to_string()).collect()
}

/// Legacy function - now just forwards to get_embedded_doc
pub fn get_doc(path: &str) -> Option<String> {
    get_embedded_doc(path)
}

/// Get only the override document from .decapod/OVERRIDE.md for a specific component
pub fn get_override_doc(repo_root: &Path, id: &str) -> Option<String> {
    let override_path = repo_root.join(".decapod").join("OVERRIDE.md");

    if !override_path.exists() {
        return None;
    }

    let override_content = std::fs::read_to_string(&override_path).ok()?;
    extract_component_override(&override_content, id)
}

/// List component override section headings from .decapod/OVERRIDE.md.
pub fn list_override_sections(repo_root: &Path) -> Vec<String> {
    let override_path = repo_root.join(".decapod").join("OVERRIDE.md");
    let Ok(override_content) = std::fs::read_to_string(&override_path) else {
        return Vec::new();
    };

    extract_override_section_names(&override_content)
}

fn extract_override_section_names(override_content: &str) -> Vec<String> {
    let Some(override_start) = override_content.find("CHANGES ARE NOT PERMITTED ABOVE THIS LINE")
    else {
        return Vec::new();
    };
    let searchable_content = &override_content[override_start..];

    searchable_content
        .lines()
        .filter_map(|line| line.strip_prefix("### "))
        .map(str::trim)
        .filter(|section| !section.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// Extract a specific component's override content from OVERRIDE.md
fn extract_component_override(override_content: &str, id: &str) -> Option<String> {
    // Only look after the "CHANGES ARE NOT PERMITTED ABOVE THIS LINE" marker
    let override_start = override_content.find("CHANGES ARE NOT PERMITTED ABOVE THIS LINE")?;
    let searchable_content = &override_content[override_start..];

    // Look for the section heading: ### core.DECAPOD
    let section_marker = format!("### {}", id);

    let start = searchable_content.find(&section_marker)?;

    // Ensure it's a real header (either at start of searchable_content or after a newline)
    if start > 0 && searchable_content.as_bytes()[start - 1] != b'\n' {
        // This is a partial match inside a line, not a header.
        return None;
    }

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
pub fn get_merged_doc(repo_root: &Path, id: &str) -> Option<String> {
    // Get embedded base
    let embedded_content = get_embedded_doc(id)?;

    // Check for component-specific override in .decapod/OVERRIDE.md
    if let Some(override_content) = get_override_doc(repo_root, id) {
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

## Project Context

- Read `.decapod/config.toml` before planning; it captures project name, summary, architecture, primary languages, and entrypoint preferences.
- Treat `.decapod/config.toml` as human-editable project context. You may update it when user intent or project direction changes.
- Read `.decapod/OVERRIDE.md` when present; it is the repo-local place for constitution overrides.
- Do not mutate Decapod-owned state under `.decapod/` directly; use Decapod CLI surfaces for generated specs, data, workspaces, and sessions.

## Quick Start

```bash
cargo install decapod
decapod validate && decapod docs ingest && decapod session acquire
decapod rpc --op agent.init
decapod workspace status
decapod todo add "<task>" && decapod todo claim --id <task-id>
decapod infer orientation --task-id <task-id>
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
- Read and update `.decapod/config.toml` as project context; use Decapod CLI for other `.decapod/` state.
- Read `.decapod/OVERRIDE.md` for repo-local constitution overrides when present.
- `DECAPOD_SESSION_PASSWORD` is required for session-scoped operations.
- Read canonical router: `decapod docs show core/DECAPOD`.
- Use shared aptitude memory for human-taught preferences across sessions/providers: `decapod data memory add|get` (aliases: `decapod data aptitude`).
- Operator reference: `decapod docs show docs/PLAYBOOK`.
- Capability authority: `decapod capabilities --format json`.
- Scoped context feature: `decapod docs search --query "<problem>" --op <op>` or `decapod rpc --op context.scope`.

Stop if requirements are ambiguous or conflicting.
"#
}

fn template_agents() -> String {
    r#"# AGENTS.md — Universal Agent Contract

This is a Decapod-managed repository. **Strict Dependency: You are strictly bound to the Decapod control plane.**

This contract applies equally to Claude, Codex, Gemini, Cursor, Kilo, and any other agent operating here.

## Mandatory Initialization

```bash
cargo install decapod
decapod validate && decapod docs ingest && decapod session acquire
decapod rpc --op agent.init
decapod workspace status
decapod todo add "<task>" && decapod todo claim --id <task-id>
decapod infer orientation --task-id <task-id>
decapod workspace ensure
cd .decapod/workspaces/<your-worktree>
decapod rpc --op context.resolve
```

## Control-Plane First Loop

```bash
# Discover what this binary actually supports in this repo
decapod capabilities --format json
decapod data schema --deterministic

# Resolve precise orientation before implementation
decapod infer orientation --intent "<your-goal>" --task-id <id>
decapod govern capsule query --topic "<topic>" --scope interfaces --task-id <task-id>
decapod rpc --op context.scope --params '{"query":"<problem>","limit":8}'
```

## Golden Rules (Non-Negotiable)

1. **MUST** refine intent with the user before inference-heavy work.
2. **MUST** use `decapod infer orientation` before non-trivial implementation.
3. **MUST** stop and ask the human when Decapod emits a **Decision Gate**.
4. **MUST NOT** work on main/master or modify the root repository's active branch. **MUST** use `decapod workspace ensure` and `cd .decapod/workspaces/<worktree>`.
5. **MUST** read [.decapod/config.toml](.decapod/config.toml) as user-editable project context and may update it when user intent changes.
6. **MUST NOT** claim done without `decapod validate` passing.
7. **MUST NOT** invent capabilities that are not exposed by the binary.
8. **MUST** stop if requirements conflict, intent is ambiguous, or policy boundaries are unclear.
9. **MUST** respect the Interface abstraction boundary.

## Orientation & Precision (Doctrine)

Before starting expensive exploration, broad refactors, or multi-path architecture work:
- Call `decapod infer orientation`.
- Treat the returned `orientation_packet` as the authoritative starting context.
- If the packet contains `decision_gates`, present the options to the human and wait for a choice.
- Do not bypass decision gates; they are placed where ambiguity would otherwise cause expensive waste.
- Use the `allowed_scope` and `proof_required` fields to bound your work.

## Invariants (Normative)

These invariants are directly enforced by tests. Violations will cause CI failure.

- **INV-DAEMONLESS**: Decapod MUST NOT leave background processes running. (enforced by `tests/daemonless_lifecycle.rs`)
- **INV-BOUNDED-VALIDATE**: `decapod validate` MUST terminate within bounded time. (enforced by `tests/validate_termination.rs`)
- **INV-STORE-BOUNDARY**: Agents MUST NOT directly mutate `.decapod/*`; all access MUST use CLI. (enforced by validation gates)
- **INV-SESSION-AUTH**: Mutations require active session with valid credentials. (enforced by session commands)
- **INV-PROOF-GATED**: Workunit status `VERIFIED` MUST have passed proof-plan gates. (enforced by `tests/workunit_publish_gate.rs`)
- **INV-ROOT-ISOLATION**: Agents MUST NOT check out branches or mutate files in the main repository checkout. All work must happen in isolated `.decapod/workspaces/*` worktrees to avoid disrupting the human user's environment. (enforced by workspace validation)

## Safety Invariants
- ✅ Router pointer: `core/DECAPOD` | ✅ Validation gate: `decapod validate`
- ✅ Constitution ingestion gate: `decapod docs ingest`
- ✅ Workspace status gate: `decapod workspace status`
- ✅ Claim-before-work gate: `decapod todo claim --id <task-id>`
- ✅ Session auth gate: `DECAPOD_SESSION_PASSWORD`
- ✅ Workspace gate: Docker git workspaces
- ✅ Privilege gate: request elevated permissions before Docker/container workspace commands

## Universal Agent Operating Contract

Decapod governs AI coding agents to ensure convergence on human intent and proof-backed completion.

- **Doctrine:** Establish intent, shape context, bound mutation, and define proof BEFORE implementation.
- **Rules:** Avoid opportunistic rewrites; preserve behavior; stop at subsystem boundaries; run strong verification.
- **Workflow:** Claim task -> Orient -> Ensure Workspace -> Work in Worktree -> Validate -> Publish.
- **Hierarchy:** Constitution and project intent outrank agent-local execution.

Call Decapod before editing. Let Decapod validate after editing.

## Operating Notes

- Read `.decapod/config.toml` (human-editable) for project context and architecture direction.
- Read `.decapod/OVERRIDE.md` for repo-local constitution overrides.
- DO NOT mutate `.decapod/` state directly; use Decapod CLI for specs, data, workspaces, and sessions. Access to `.decapod/` is strictly via decapod CLI.
- Use `decapod docs show core/DECAPOD` for binding contracts.
- Use `decapod capabilities --format json` to discover available operations.
- Stop if requirements conflict, intent is ambiguous, or policy boundaries are unclear.
- Respect the Interface abstraction boundary.
- Treat lock/contention failures as blocking until resolved.
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
    r#"# .decapod - Decapod Control Plane

Decapod is the daemonless, local-first governance kernel behind AI coding agents. Agents call it on demand to turn intent into context, then context into explicit specifications before inference, enforce boundaries, and deliver proof-backed completion across concurrent multi-agent work.

GitHub: https://github.com/DecapodLabs/decapod
Canonical Contract: [core/DECAPOD](core/DECAPOD)

## What This Directory Is

This `.decapod/` directory is the local control plane for this repository.
It keeps Decapod-owned state, generated artifacts, and isolated workspaces separate from your product source tree.

`OVERRIDE.md` and `README.md` intentionally stay at this top level.

## Quick Start

1. `decapod init`
2. `decapod validate`
3. `decapod docs ingest`
4. `decapod session acquire`
5. `decapod rpc --op agent.init`
6. `decapod workspace status`
7. `decapod todo add \"<task>\" && decapod todo claim --id <task-id>`
8. `decapod workspace ensure`

## Skills - Your Personal Optimization Layer

**Skills are how you shape agent behavior.** Import skills to train agents how to interact with your codebase, your conventions, and your preferences.

### Why Skills Matter

- **Controls**: Add security reviews, code quality checks, or custom validation
- **Optimization**: Encode your team's conventions, patterns, and best practices
- **Context**: Give agents project-specific knowledge that persists across sessions

### Quick Skills Workflow

```bash
# Import a skill from a SKILL.md file
decapod data aptitude skill import --path path/to/your/SKILL.md

# List available skills
decapod data aptitude skill list

# Resolve skills for a specific task
decapod data aptitude skill resolve --query "how to write tests"

# Query aptitude memory for learned preferences
decapod data aptitude prompt --query "git"
```

### Creating Your Own Skills

Skills are just Markdown files with YAML frontmatter:

```yaml
---
name: my-security-review
description: Custom security checks for our codebase
allowed-tools: Bash
---

# Security Review Skill

## Triggers
- "check security"
- "review for vulnerabilities"

## Workflow
1. Run `semgrep --config=auto .`
2. Check for hardcoded secrets
3. Validate dependency vulnerabilities
4. Report findings
```

Place SKILL.md files in `metadata/skills/` and import them:

```bash
decapod data aptitude skill import --path metadata.skills.my-security-review.SKILL.md
```

### Aptitude Memory

Decapod learns from interactions. Use aptitude to record preferences:

```bash
# Record a preference
decapod data aptitude add --category git --key branch_prefix --value "feature/" --confidence 90

# Get contextual prompts
decapod data aptitude prompt --query "commit"

# Record an observation
decapod data aptitude observe --category code_style --content "Team prefers async/await over tokio::spawn"
```

## Canonical Layout

- `README.md`: operator onboarding and control-plane map.
- `OVERRIDE.md`: project-local override layer for embedded constitution.
- `data/`: canonical control-plane state (SQLite + ledgers).
- `skills/`: imported skill cards (auto-generated, tracked for reproducibility).
- `generated/specs/`: living project specs scaffolded by `decapod init`.
- `generated/context/`: deterministic context capsule artifacts.
- `generated/artifacts/provenance/`: promotion manifests and convergence checklist.
- `generated/artifacts/inventory/`: deterministic release inventory artifacts.
- `generated/artifacts/diagnostics/`: opt-in diagnostics artifacts.
- `workspaces/`: isolated todo-scoped git worktrees for implementation.

## Why Teams Use This

- Agent-first interface with explicit governance.
- Local-first execution without daemon overhead.
- Integrated TODO, claims, context, validation, and proof in one harness.
- Cleaner repos: Decapod concerns stay in `.decapod/`.

## Override Workflow

Edit `.decapod/OVERRIDE.md` to add project-specific policy overlays without forking Decapod.
Keep overrides minimal, explicit, and committed.
"#
    .to_string()
}

fn template_override() -> String {
    let mut s = r#"# OVERRIDE.md - Project-Specific Decapod Overrides

> **IMPORTANT:** For detailed usage instructions and examples, see [README.md](README.md).

**Canonical:** OVERRIDE.md
**Authority:** override
**Layer:** Project
**Binding:** Yes (overrides embedded constitution)

<!-- ═══════════════════════════════════════════════════════════════════════ -->
<!-- ⚠️  CHANGES ARE NOT PERMITTED ABOVE THIS LINE                           -->
<!-- ═══════════════════════════════════════════════════════════════════════ -->
"#.to_string();

    // Group nodes by category for the template
    let mut categories: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut ids = list_ids();
    ids.sort();

    for id in ids {
        if let Some((cat, _title, _deps)) = get_metadata(id) {
            categories.entry(cat).or_default().push(id);
        }
    }

    let cat_order = ["core", "specs", "interfaces", "methodology", "architecture", "plugins", "docs"];
    
    for cat in cat_order {
        if let Some(nodes) = categories.get(cat) {
            s.push_str(&format!("\n## {} Overrides\n", cat.to_uppercase()));
            for id in nodes {
                s.push_str(&format!("\n### {}\n", id));
            }
            s.push_str("\n---\n");
        }
    }

    s
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
