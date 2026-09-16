# .decapod - Decapod Control Plane

This directory is the repository-local control plane for Decapod. It keeps
project policy, durable execution state, generated projections, proof artifacts,
and isolated workspaces separate from product source.

Project configuration lives in [`config.toml`](config.toml). Project-specific
policy overlays live in [`OVERRIDE.md`](OVERRIDE.md); keep them minimal,
explicit, and committed. Omitted override sections are valid when a project
does not need to customize those directives.

The authority hierarchy is embedded constitution, project override, then task
policy. Configuration selects repository behavior; living specs explain the
project contract; generated projections and proof artifacts report the governed
state.

For the human documentation and operating model, see the
[official Decapod docs](https://decapodlabs.github.io/decapod/). The source and
issue tracker are on [GitHub](https://github.com/DecapodLabs/decapod).

## Directory Map

- `config.toml`: repository configuration and declared capabilities.
- `OVERRIDE.md`: project-local policy overlays.
- `data/`: canonical local control-plane state, including `decapod.db`.
- `governance/`: plans, claims, trajectories, and validation receipts.
- `managed/specs/`: authored living specs plus generated attestations.
- `managed/context/`: generated context projections.
- `managed/artifacts/`: generated provenance, inventory, and diagnostics.
- `workspaces/`: isolated todo-scoped git worktrees.
