# .decapod - Decapod Control Plane

Decapod is the daemonless, local-first governance kernel behind AI coding agents. Agents call it on demand to turn intent into context, then context into explicit specifications before inference, enforce boundaries, and deliver proof-backed completion across concurrent multi-agent work.

GitHub: https://github.com/DecapodLabs/decapod
Canonical Contract: `assets/constitution.json` section `core/DECAPOD`

## What This Directory Is

This `.decapod/` directory is the local control plane for this repository.
It keeps Decapod-owned state, generated artifacts, and isolated workspaces separate from your product source tree.

`OVERRIDE.md` and `README.md` intentionally stay at this top level.

## Quick Start

1. `decapod init`
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
