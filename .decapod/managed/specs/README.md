# Project Specs

Canonical path: `.decapod/managed/specs/`.
These files are the project-local contract for humans and agents.

## Snapshot
- Project: this repository
- Outcome: Define the intended user-visible outcome.
- Detected languages: not detected yet
- Detected surfaces: not detected yet

## How to use this folder
- [INTENT.md](./INTENT.md): what success means and what is explicitly out of scope.
- [ARCHITECTURE.md](./ARCHITECTURE.md): topology, runtime model, data boundaries, and ADR trail.
- [INTERFACES.md](./INTERFACES.md): API/CLI/events/storage contracts and failure behavior.
- [VALIDATION.md](./VALIDATION.md): proof commands, quality gates, and evidence artifacts.
- [SEMANTICS.md](./SEMANTICS.md): state machines, invariants, replay rules, and idempotency.
- [OPERATIONS.md](./OPERATIONS.md): SLOs, monitoring, incident response, and rollout strategy.
- [SECURITY.md](./SECURITY.md): threat model, trust boundaries, auth/authz, and supply-chain posture.

## Canonical `.decapod/` Layout
- `.decapod/data/`: canonical control-plane state (SQLite + ledgers).
- `.decapod/managed/Dockerfile.decapod`: Decapod's project-specific execution image; Decapod runs inside it and may add project build dependencies such as Go, Python, or system packages. Glibc is the default; `--image-profile alpine` selects the GHCR `-alpine`-tagged musl image.
- `.decapod/managed/specs/`: **Living project specs** for humans and agents.
- `Dockerfile` at the project root remains the product application's container image and is the artifact users package and deploy.
- `.decapod/managed/context/`: ignored, current-run deterministic context capsules.
- `.decapod/managed/policy/`: ignored, current-run JIT context policy material; use `.decapod/policy/` for a durable override.
- `.decapod/managed/artifacts/`: ignored, current-run provenance/custody/inventory/diagnostic outputs.
- `.decapod/governance/validation.json`: tracked per-commit validation receipt, overwritten after successful validation.
- `.decapod/governance/trajectory.json`: the single tracked run cookie; Git history preserves prior merged cookies.
- `.decapod/managed/artifacts/inventory/`: deterministic release inventory.
- `.decapod/managed/artifacts/diagnostics/`: opt-in diagnostics artifacts.
- `.decapod/workspaces/`: isolated todo-scoped git worktrees.

## Day-0 Onboarding Checklist
- [ ] Replace all placeholders in all 8 spec files.
- [ ] Confirm primary user outcome and acceptance criteria in [INTENT.md](./INTENT.md).
- [ ] Confirm topology and runtime model in [ARCHITECTURE.md](./ARCHITECTURE.md).
- [ ] Document all inbound/outbound contracts in [INTERFACES.md](./INTERFACES.md).
- [ ] Define validation gates and CI proof surfaces in [VALIDATION.md](./VALIDATION.md).
- [ ] Define state machines and invariants in [SEMANTICS.md](./SEMANTICS.md).
- [ ] Define SLOs, alerting, and incident process in [OPERATIONS.md](./OPERATIONS.md).
- [ ] Define threat model and auth/authz decisions in [SECURITY.md](./SECURITY.md).
- [ ] Ensure architecture diagram, docs, changelog, and tests are mapped to promotion gates.
- [ ] Run all validation/test commands and attach evidence artifacts.

<!-- decapod:codebase-attestation:start -->
## Codebase Attestation

- Repository signal fingerprint: `eed2faeffece64615195bcb9fce2755326263fdeaf96aa65a87fa143a3ec02da`
- Significant implementation surfaces: `.github/` (8 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `docs/` (1 files), `src/` (101 files), `tests/` (4 files)
- Refreshed from the current codebase by `decapod specs.refresh`
<!-- decapod:codebase-attestation:end -->
