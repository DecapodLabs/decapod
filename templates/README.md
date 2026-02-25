# .decapod - Decapod Control Plane

Decapod is a software engineering harness interfaced through AI coding agents.
You get governed execution, proof-backed delivery, and integrated project management with near-zero operator overhead.

GitHub: https://github.com/DecapodLabs/decapod

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
7. `decapod todo add "<task>" && decapod todo claim --id <task-id>`
8. `decapod workspace ensure`

## Canonical Layout

- `README.md`: operator onboarding and control-plane map.
- `OVERRIDE.md`: project-local override layer for embedded constitution.
- `data/`: canonical control-plane state (SQLite + ledgers).
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
