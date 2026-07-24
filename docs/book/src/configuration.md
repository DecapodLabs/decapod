# Configuration

Decapod is configured via `.decapod/config.toml` (see the [Config Specification Reference](reference/config-toml.md)). This file is human-editable and should be committed to your repository.

## The `[init]` Section

Controls how `decapod init` behaves.

- `specs`: (bool) Whether to generate spec scaffolding under `.decapod/managed/specs/`.
- `diagram_style`: ("ascii" or "mermaid") The style for generated architecture diagrams.
- `entrypoints`: (list) Which agent entrypoints to create (e.g., `["AGENTS.md", "CLAUDE.md"]`).

## The `[repo]` Section

Defines project-specific policy and metadata.

- `product_name`: The name of your project.
- `product_summary`: A short description of what the project does.
- `architecture_direction`: A high-level note on the architectural style (e.g., "modular monolith").
- `product_type`: (e.g., "library", "service", "application").
- `done_criteria`: Global "done" criteria that all tasks must satisfy.
- `primary_languages`: A list of languages used in the repo.
- `container_workspaces`: (bool) Whether to enforce container isolation for all work. **Recommended for multi-agent workflows.**

## Project Overrides

For deep behavioral changes, use `.decapod/OVERRIDE.md` (see [Config Overrides](concepts/overrides.md)). This allows you to override specific directives in the embedded Decapod constitution (see [Repository Constitution](concepts/constitution.md)) without modifying the Decapod binary itself.
