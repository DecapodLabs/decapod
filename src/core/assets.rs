//! Embedded constitution and template assets.
//!
//! This module provides compile-time embedded access to Decapod's methodology documents.
//! All constitution files are baked into the binary via `assets/constitution.json`.

use std::path::Path;

// Include the auto-generated compressed constitution
include!(concat!(env!("OUT_DIR"), "/constitution_compressed.rs"));

/// Get an embedded document by its ID (e.g., "core/DECAPOD")
pub fn get_embedded_doc(id: &str) -> Option<String> {
    let key = id.strip_prefix("embedded/").unwrap_or(id);

    for candidate in doc_id_candidates(key) {
        if let Some(content) = get_decompressed(&candidate) {
            return Some(content);
        }
    }

    None
}

fn doc_id_candidates(id: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let normalized = id.replace('.', "/");
    for candidate in [id.to_string(), normalized] {
        push_candidate(&mut candidates, candidate.clone());
        if let Some(stripped) = candidate
            .strip_suffix(".json")
            .or_else(|| candidate.strip_suffix(".md"))
        {
            push_candidate(&mut candidates, stripped.to_string());
        } else {
            // If it doesn't have a suffix, add them as candidates
            push_candidate(&mut candidates, format!("{}.md", candidate));
            push_candidate(&mut candidates, format!("{}.json", candidate));
        }
    }
    candidates
}

fn push_candidate(candidates: &mut Vec<String>, candidate: String) {
    if !candidates.iter().any(|existing| existing == &candidate) {
        candidates.push(candidate);
    }
}

/// List all available constitution document IDs
pub fn list_docs() -> Vec<String> {
    list_ids().into_iter().map(|s| s.to_string()).collect()
}

/// Legacy function - now just forwards to get_embedded_doc
pub fn get_doc(path: &str) -> Option<String> {
    get_embedded_doc(path)
}

pub fn get_doc_metadata(id: &str) -> Option<(String, String, Vec<String>)> {
    for candidate in doc_id_candidates(id) {
        if let Some((category, title, dependencies)) = get_metadata(&candidate) {
            return Some((
                category.to_string(),
                title.to_string(),
                dependencies.into_iter().map(ToString::to_string).collect(),
            ));
        }
    }
    None
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
    let override_start = override_content
        .find("CHANGES ARE NOT PERMITTED ABOVE THIS LINE")
        .unwrap_or(0);
    let searchable_content = &override_content[override_start..];

    let candidates = doc_id_candidates(id);
    let mut best_extracted = None;

    let lines: Vec<&str> = searchable_content.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        let is_target = candidates.iter().any(|c| line == format!("### {}", c));

        if is_target {
            let mut extracted_lines = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with("### ") {
                extracted_lines.push(lines[i]);
                i += 1;
            }
            let extracted = extracted_lines.join("\n").trim().to_string();
            if !extracted.is_empty() {
                best_extracted = Some(extracted);
            }
            continue; // i is already at the next possible header or end
        }
        i += 1;
    }

    best_extracted
}

/// Get merged document (embedded base + optional project override from OVERRIDE.md)
pub fn get_merged_doc(repo_root: &Path, id: &str) -> Option<String> {
    // Get embedded base
    let embedded_content = render_embedded_doc_text(id, &get_embedded_doc(id)?);

    // Check for component-specific override in .decapod/OVERRIDE.md
    if let Some(override_content) = get_override_doc(repo_root, id) {
        return Some(merge_override_content(&embedded_content, &override_content));
    }

    Some(embedded_content)
}

fn render_embedded_doc_text(id: &str, raw_content: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw_content) else {
        return raw_content.to_string();
    };

    // For JSON and schema files, return only the raw content from summary/sections
    // to avoid breaking machine-readable consumers with markdown headers.
    if id.ends_with(".json") || id.ends_with(".schema") {
        if let Some(summary) = value.get("summary").and_then(|v| v.as_str())
            && !summary.is_empty()
        {
            return summary.to_string();
        }
        if let Some(sections) = value.get("sections").and_then(|v| v.as_object())
            && let Some(first_val) = sections.values().next().and_then(|v| v.as_str())
        {
            return first_val.to_string();
        }
        return raw_content.to_string();
    }

    let mut rendered = String::new();
    rendered.push_str("# ");
    rendered.push_str(id);
    rendered.push('\n');

    if let Some(summary) = value.get("summary").and_then(|summary| summary.as_str())
        && !summary.trim().is_empty()
    {
        rendered.push('\n');
        rendered.push_str(summary.trim());
        rendered.push('\n');
    }

    if let Some(sections) = value
        .get("sections")
        .and_then(|sections| sections.as_object())
    {
        for (title, section) in sections {
            rendered.push('\n');
            rendered.push_str("## ");
            rendered.push_str(title);
            rendered.push_str("\n\n");
            if let Some(text) = section.as_str() {
                rendered.push_str(text.trim());
            } else {
                rendered.push_str(&section.to_string());
            }
            rendered.push('\n');
        }
    } else {
        rendered.push('\n');
        rendered.push_str(&value.to_string());
        rendered.push('\n');
    }

    rendered
}

/// Merge embedded content with override additions
pub fn merge_override_content(embedded_content: &str, override_content: &str) -> String {
    format!(
        "{}\n\n---\n\n## Project Overrides\n\n{}",
        embedded_content.trim(),
        override_content.trim()
    )
}

fn agent_entrypoint_body() -> &'static str {
    r#"You are working in a Decapod-managed repository.
See `AGENTS.md` for the universal contract.

## Orientation & Documentation

- **Read `docs/agent/api-index.md` before using Decapod beyond help/version checks.**
- Use `docs/agent/command-contracts.md` instead of guessing command arguments.
- Treat Decapod errors as recovery instructions.
- Respect repo-local config policy and workspace boundaries.
- Do not bypass Decapod boundaries to appear productive.

## Project Context

- Read `.decapod/config.toml` before planning; it captures project name, summary, architecture, primary languages, and entrypoint preferences.
- Treat `.decapod/config.toml` as human-editable project context. You may update it when user intent or project direction changes.
- Read `.decapod/OVERRIDE.md` when present; it is the repo-local place for constitution overrides.
- Do not mutate Decapod-owned state under `.decapod/` directly; use Decapod CLI surfaces for generated specs, data, workspaces, and sessions.

## Quick Start

```bash
# Smart bootstrap: only install/init if a newer version is available
(decapod capabilities --format json | grep -q '"is_latest":true') || (cargo install decapod && decapod init --proof)
decapod docs ingest && decapod validate && decapod session acquire
decapod rpc --op agent.init
decapod workspace status
decapod todo add "<task>" && decapod todo claim --id <task-id>
decapod infer orientation --task-id <task-id>
decapod workspace ensure
cd .decapod/workspaces/<your-worktree>
decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'
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
- External task managers are allowed, but Decapod still requires its own todo layer for isolation, proof, and exclusive cross-agent ownership.
- Create and claim a Decapod todo before `decapod workspace ensure`, `decapod workspace ensure --container`, or any container run.
- request elevated permissions before Docker/container workspace commands.
- `.decapod files are accessed only via decapod CLI`.
- Read and update `.decapod/config.toml` as project context; use Decapod CLI for other `.decapod/` state.
- Read `.decapod/OVERRIDE.md` for repo-local constitution overrides when present.
- `DECAPOD_SESSION_PASSWORD` is required for session-scoped operations.
- Read canonical router: `decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'`.
- Use shared aptitude memory for human-taught preferences across sessions/providers: `decapod data memory add|get` (aliases: `decapod data aptitude`).
- Operator reference: `decapod rpc --op constitution.get --params '{"section":"docs/PLAYBOOK"}'`.
- Capability authority: `decapod capabilities --format json`.
-- Scoped context feature: `decapod rpc --op context.scope --params '{"query":"<problem>","op":"<op>"}'` or `decapod rpc --op context.scope`.

Treat `.decapod/generated/specs/*` as living documents. Adjust specs as intent and code change over time.

Stop if requirements are ambiguous or conflicting.
<!-- decapod-validator-anchors
Strict Dependency: You are strictly bound to the Decapod control plane
Strict Dependency: You are strictly bound to the Decapod governance kernel
-->
"#
}

fn template_agents() -> String {
    r#"# AGENTS.md — Universal Agent Contract

This is a Decapod-managed repository. **Strict Dependency: You are strictly bound to the Decapod governance kernel.**

## Mandatory Orientation

**Before performing any Decapod operations beyond help/version checks, you MUST read the Agent API Orientation corpus:**

1. Read **[docs/agent/api-index.md](docs/agent/api-index.md)** for high-level orientation.
2. Consult **[docs/agent/command-contracts.md](docs/agent/command-contracts.md)** for specific command requirements and recovery paths.
3. Reference **[docs/agent/payload-examples.md](docs/agent/payload-examples.md)** for valid call shapes.

## Mandatory Initialization

```bash
# Smart bootstrap: only install/init if a newer version is available
(decapod capabilities --format json | grep -q '"is_latest":true') || (cargo install decapod && decapod init --proof)
decapod docs ingest && decapod validate && decapod session acquire
decapod rpc --op agent.init
decapod workspace status
decapod todo add "<task>" && decapod todo claim --id <task-id>
decapod infer orientation --task-id <task-id>
decapod workspace ensure
cd .decapod/workspaces/<your-worktree>
decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'
decapod rpc --op context.resolve
```

## Control-Plane First Loop

```bash
decapod capabilities --format json
decapod data schema --deterministic
decapod infer orientation --intent "<your-goal>" --task-id <id>
decapod govern capsule query --topic "<topic>" --scope interfaces --task-id <task-id>
decapod rpc --op context.scope --params '{"query":"<problem>","limit":8}'
```

## Golden Rules (Non-Negotiable)

1. **MUST** refine intent with the user before inference-heavy work.
2. **MUST** use `decapod infer orientation` before non-trivial implementation.
3. **MUST** stop and ask the human when Decapod emits a **Decision Gate**.
4. **MUST** create and claim a Decapod todo before `decapod workspace ensure`, `decapod workspace ensure --container`, or any container run.
5. **MUST NOT** work on main/master or modify the root repository's active branch. **MUST** use `decapod workspace ensure`.
6. **MUST** read [.decapod/config.toml](.decapod/config.toml) as user-editable project context.
7. **MUST NOT** claim done without `decapod validate` passing.
8. **MUST NOT** invent capabilities that are not exposed by the binary.
9. **MUST** stop if requirements conflict or intent is ambiguous.
10. **MUST** respect the interface abstraction boundary.
11. **MUST** maintain **Living Specs**: treat `.decapod/generated/specs/*` as dynamic documents.
12. **MUST** use the command contracts in `docs/agent/command-contracts.md` instead of guessing arguments.

## Decapod Invocation Contract

Agents act. Decapod orients. Call Decapod at decision boundaries: ambiguous requests, public impact, unclear proof, todo lifecycle, scope expansion, context loss, or multi-agent collision risk.

## Living Specs & Governance

The files under `.decapod/generated/specs/` are living contracts. Review and update [INTENT.md](.decapod/generated/specs/INTENT.md), [ARCHITECTURE.md](.decapod/generated/specs/ARCHITECTURE.md), and [INTERFACES.md](.decapod/generated/specs/INTERFACES.md) to align with evolving intent and reality.

## Epistemic Custody

Preserve the chain between intent, context, assumptions, action, and proof.
1. **Preserve Uncertainty**: Summaries must preserve risk instead of compressing it.
2. **Recursive Continuity**: Prior assumptions MUST carry forward until resolved.
3. **Evidence-Based Claims**: Claims of completion must be tied to measured evidence.
4. **Clarification Trigger**: Stop if a critical assumption cannot be proven.

## Invariants (Normative)
- **INV-DAEMONLESS**: Decapod MUST NOT leave background processes running.
- **INV-BOUNDED-VALIDATE**: `decapod validate` MUST terminate within bounded time.
- **INV-STORE-BOUNDARY**: Agents MUST NOT directly mutate `.decapod/*`.
- **INV-SESSION-AUTH**: Mutations require active session.
- **INV-PROOF-GATED**: `VERIFIED` status requires passed proof-plan gates.
- **INV-ROOT-ISOLATION**: Agents MUST NOT mutate files in the main repository checkout.

## Safety Invariants
- ✅ Router: `decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'`
- ✅ Gates: `decapod capabilities` | `decapod docs ingest` | `decapod validate`
- ✅ Claims: `decapod todo claim --id <task-id>` | ✅ Auth: `DECAPOD_SESSION_PASSWORD`
- ✅ Workspace: Docker git workspaces | ✅ Privilege: request elevated permissions before Docker/container workspace commands

## Operating Notes
- Read `.decapod/config.toml` and `.decapod/OVERRIDE.md`.
- External task managers are allowed, but Decapod todos are still mandatory for workspace isolation.
- DO NOT mutate `.decapod/` state directly; all access MUST use CLI. Access to `.decapod/` is strictly via decapod CLI.
- Stop if requirements conflict or intent is ambiguous. Respect the interface abstraction boundary.
<!-- decapod-validator-anchors
Strict Dependency: You are strictly bound to the Decapod control plane
Strict Dependency: You are strictly bound to the Decapod governance kernel
-->
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
Canonical Contract: `assets/constitution.json` section `core/DECAPOD`

## What This Directory Is

This `.decapod/` directory is the local control plane for this repository.
It keeps Decapod-owned state, generated artifacts, and isolated workspaces separate from your product source tree.

`OVERRIDE.md` and `README.md` intentionally stay at this top level.

## Quick Start

1. `decapod init --proof`
2. `decapod validate`
3. `decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'`
4. `decapod session acquire`
5. `decapod rpc --op agent.init`
6. `decapod workspace status`
7. `decapod todo add \"<task>\" && decapod todo claim --id <task-id>`
8. `decapod workspace ensure`

## Migrating Custom Agent Files

If you have existing files like `SKILLS.md`, `SOUL.md`, or `MEMORY.md` that were used for agent instructions, you can easily migrate them into the Decapod governance layer. 

After running `decapod init`, simply ask your agent to **"consolidate my [FILE.md] content into the .decapod/OVERRIDE.md substrate"**. This ensures your project-specific intent is merged into the correct constitutional sections while allowing Decapod to manage the primary entrypoints.

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
- `OVERRIDE.md`: project-local override layer for embedded constitution directives.
- `data/`: canonical control-plane state (SQLite + ledgers).
- `skills/`: imported skill cards (auto-generated, tracked for reproducibility).
- `generated/specs/`: living project specs scaffolded by `decapod init`.
- `generated/context/`: deterministic context capsule artifacts.
- `generated/artifacts/provenance/`: promotion manifests and convergence checklist.
- `generated/artifacts/inventory/`: deterministic release inventory artifacts.
- `generated/artifacts/diagnostics/`: opt-in diagnostics artifacts.
- `workspaces/`: isolated todo-scoped git worktrees for implementation.

## How It Works

Decapod uses a **JSON-based constitution** to govern agent behavior. Instead of the agent reading full Markdown documents, it uses the Decapod CLI to query specific directives.

1. **Indexing**: Decapod indexes the constitution graph when called.
2. **Selective Context**: Agents query exact sections (directives) needed for the current task, minimizing context overhead.
3. **Local Overrides**: You can override any constitution directive in [.decapod/OVERRIDE.md](OVERRIDE.md) using the specific directive ID.

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
**Binding:** Yes (overrides embedded constitution directives)

<!-- ═══════════════════════════════════════════════════════════════════════ -->
<!-- ⚠️  CHANGES ARE NOT PERMITTED ABOVE THIS LINE                           -->
<!-- ═══════════════════════════════════════════════════════════════════════ -->

Use this file to override specific constitution directives. Decapod indexes these sections
using the H3 headers below (e.g., `### core/DECAPOD`). Overrides in this file take precedence
over the embedded JSON constitution.
"#
    .to_string();

    // Group nodes by category for the template
    let mut categories: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    let mut ids = list_ids();
    ids.sort();

    for id in &ids {
        if let Some((cat, _title, _deps)) = get_metadata(id) {
            categories.entry(cat).or_default().push(id);
        }
    }

    // Manually add specs to the template since they are generated
    let specs = [
        "specs/README.md",
        "specs/INTENT.md",
        "specs/ARCHITECTURE.md",
        "specs/INTERFACES.md",
        "specs/VALIDATION.md",
        "specs/SEMANTICS.md",
        "specs/OPERATIONS.md",
        "specs/SECURITY.md",
    ];
    for spec in &specs {
        categories.entry("specs").or_default().push(spec);
    }

    let cat_order = [
        "core",
        "specs",
        "interfaces",
        "methodology",
        "architecture",
        "plugins",
        "docs",
    ];

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
