# Project Specs

Canonical path: `.decapod/generated/specs/`.
These files are the project-local contract for humans and agents.

## Snapshot
- Project: decapod
- Outcome: Daemonless control plane that drives agent convergence to user intent with proof-backed completion.
- Detected languages: rust
- Detected surfaces: cargo, cli, rpc, validation gates

## How to use this folder
- `INTENT.md`: product outcome, scope boundaries, and objective acceptance criteria.
- `ARCHITECTURE.md`: topology, runtime model, deployment shape, and ADR/risk register.
- `INTERFACES.md`: CLI/RPC/event/data contracts, timeout budgets, and failure semantics.
- `VALIDATION.md`: promotion gate design, evidence model, and bounded execution.
- `SEMANTICS.md`: state machines, invariants, replay semantics, and idempotency contracts.
- `OPERATIONS.md`: SLOs, monitoring, incident response, and capacity planning.
- `SECURITY.md`: trust boundaries, STRIDE threats, auth/authz model, and supply-chain controls.

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
- [ ] Confirm user-facing outcome and non-goals in `INTENT.md`.
- [ ] Confirm architecture topology and runtime/deployment model in `ARCHITECTURE.md`.
- [ ] Confirm all CLI/RPC interfaces and error taxonomy in `INTERFACES.md`.
- [ ] Confirm proof surfaces and blocking gates in `VALIDATION.md`.
- [ ] Confirm state transitions and invariants in `SEMANTICS.md`.
- [ ] Confirm SLO/monitoring/incident ownership in `OPERATIONS.md`.
- [ ] Confirm trust boundaries and threat mitigations in `SECURITY.md`.
- [ ] Confirm docs + architecture diagram + changelog proof gates are defined.
- [ ] Confirm tests pass locally and in CI.
- [ ] Attach evidence artifacts before promotion.

## Agent Directive
- Specs are executable governance, not placeholders. Before coding: resolve ambiguity in these docs. Before marking done: validate, update drifted sections, and attach evidence.
