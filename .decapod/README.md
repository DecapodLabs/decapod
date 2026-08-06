# .decapod - Decapod Control Plane

Decapod is a repo-native governance kernel for AI coding agents. It turns human intent into bounded, durable, and proof-backed agent work. Its layer is explicit: models produce intelligence, agents perform work, repositories preserve state, and Decapod governs the transition from intent to proof. Reliability is designed, not hoped for. Agents invoke it at decision, validation, recovery, and publication boundaries; it does not perform the agent's work.

GitHub: https://github.com/DecapodLabs/decapod
Canonical Contract: `assets/constitution.json` section `core/DECAPOD`

## What This Directory Is

This `.decapod/` directory is the durable execution surface for governed work in this repository. It keeps authored specifications, Decapod-owned state, generated projections and evidence, and isolated workspaces separate from product source.

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
- `managed/specs/`: agent-authored living project specs; only fresh initialization scaffolds their starting structure.
- `managed/context/`: generated deterministic context projections.
- `managed/artifacts/provenance/`: promotion manifests and convergence checklist.
- `managed/artifacts/inventory/`: deterministic release inventory artifacts.
- `managed/artifacts/diagnostics/`: opt-in diagnostics artifacts.
- `workspaces/`: isolated todo-scoped git worktrees for implementation.

## How It Works

Each Decapod process is ephemeral. The repository preserves the durable state that lets one task span many invocations, processes, models, and harnesses.

1. **Intent and Boundaries**: The agent records its interpretation and accepts a governed task scope.
2. **Execution**: The agent performs the work in an isolated workspace and maintains living specifications.
3. **Validation and Recovery**: Decapod evaluates invariants. The agent follows supported remediation and revalidates.
4. **Publication and Proof**: Publication remains blocked until required validation and evidence are satisfied.

## Why Teams Use This

- Agent-first interface with explicit governance.
- Local-first execution without daemon overhead.
- Integrated TODO, claims, context, validation, and proof in one harness.
- Cleaner repos: Decapod concerns stay in `.decapod/`.

## Override Workflow

Edit `.decapod/OVERRIDE.md` to add project-specific policy overlays without forking Decapod.
Keep overrides minimal, explicit, and committed.
