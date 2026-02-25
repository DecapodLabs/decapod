# Project Specs

Canonical path: `.decapod/generated/specs/`.
These files are the project-local contract for humans and agents.

## Snapshot

- Project: decapod
- Outcome: A daemonless control plane for AI coding agents.
- Detected languages: rust
- Detected surfaces: cargo
- Version: 0.44.3

## How to use this folder

- `INTENT.md`: What success means, in-scope capabilities, and falsifiable non-goals.
- `ARCHITECTURE.md`: The topology, data flow, and component responsibilities.
- `INTERFACES.md`: CLI commands, JSON-RPC operations, and data schemas with concrete invocations.
- `VALIDATION.md`: Proof surfaces, promotion gates, and typed error codes.

## Quick Verification (Machine-Checkable)

```bash
# 1. Get capabilities
decapod capabilities --format json

# 2. Get schemas
decapod data schema --format json --deterministic

# 3. Run validation gate
decapod validate --format json
```

## Canonical `.decapod/` Layout

- `.decapod/data/`: Canonical control-plane state (SQLite + ledgers).
- `.decapod/generated/specs/`: Living project specs for humans and agents.
- `.decapod/generated/context/`: Deterministic context capsules.
- `.decapod/generated/policy/context_capsule_policy.json`: Repo-native JIT context policy contract.
- `.decapod/generated/artifacts/provenance/`: Promotion manifests and convergence checklist.
- `.decapod/generated/artifacts/inventory/`: Deterministic release inventory.
- `.decapod/generated/artifacts/diagnostics/`: Opt-in diagnostics artifacts.
- `.decapod/workspaces/`: Isolated todo-scoped git worktrees.

## Stability Guarantees

| Surface | Stability |
|---------|-----------|
| Core CLI commands | Stable |
| JSON-RPC operations | Stable |
| Validation gate | Stable |
| Schema format | Stable |
| Error codes | Stable |

## Non-Goals (Explicitly)

- No daemon mode
- No remote/cloud dependency for core function
- No agent framework / prompt management
- No cross-repo state sync
