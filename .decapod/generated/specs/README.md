# Project Specs

Canonical path: `.decapod/generated/specs/`.
These files are the project-local contract for humans and agents.

## Snapshot
- Project: decapod
- Outcome: A daemonless control plane for AI coding agents.
- Detected languages: rust
- Detected surfaces: cargo

## How to use this folder
- `INTENT.md`: what success means and what is explicitly out of scope.
- `ARCHITECTURE.md`: the current implementation shape and planned evolution.
- `INTERFACES.md`: API/CLI/events/storage contracts and failure behavior.
- `VALIDATION.md`: required proof commands and promotion gates.

## Canonical `.decapod/` Layout
- `.decapod/data/`: canonical control-plane state (SQLite + ledgers).
- `.decapod/generated/specs/`: living project specs for humans and agents.
- `.decapod/generated/context/`: deterministic context capsules.
- `.decapod/generated/policy/context_capsule_policy.json`: repo-native JIT context policy contract.
- `.decapod/generated/artifacts/provenance/`: promotion manifests and convergence checklist.
- `.decapod/generated/artifacts/inventory/`: deterministic release inventory.
- `.decapod/generated/artifacts/diagnostics/`: opt-in diagnostics artifacts.
- `.decapod/workspaces/`: isolated todo-scoped git worktrees.

## Day-0 Onboarding Checklist
- [ ] Replace all placeholder bullets in each spec file.
- [ ] Confirm primary user outcome and acceptance criteria in `INTENT.md`.
- [ ] Document real interfaces and data boundaries in `INTERFACES.md`.
- [ ] Run and record validation commands listed in `VALIDATION.md`.
