# .decapod - Decapod Control Plane

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
3. `decapod constitution get core/DECAPOD`
4. `decapod session acquire`
5. `decapod rpc --op agent.init`
6. `decapod workspace status`
7. `decapod todo add \"<task>\" && decapod todo claim --id <task-id>`
8. `decapod workspace ensure`

## Migrating Custom Agent Files

If you have existing files like `SOUL.md` or `MEMORY.md` that were used for agent instructions, you can migrate them into the Decapod governance layer.

After running `decapod init`, simply ask your agent to **"consolidate my [FILE.md] content into the .decapod/OVERRIDE.md substrate"**. This ensures your project-specific intent is merged into the correct constitutional sections while allowing Decapod to manage the primary entrypoints.

## Aptitude Memory

Decapod aptitude remains for preferences and behavior recall:

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
- `managed/specs/`: living project specs scaffolded by `decapod init`.
- `managed/context/`: deterministic context capsule artifacts.
- `managed/artifacts/provenance/`: promotion manifests and convergence checklist.
- `managed/artifacts/inventory/`: deterministic release inventory artifacts.
- `managed/artifacts/diagnostics/`: opt-in diagnostics artifacts.
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
