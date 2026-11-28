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

For deep behavioral changes, use `.decapod/OVERRIDE.md` (see [Config Overrides](concepts/overrides.md)). Find the generated directive section and replace the instruction inside its four-backtick source block with Markdown or any documentation style you prefer. Decapod extracts the block contents as authority while the outer file renders them as code. Duplicate or Decapod-namespaced unknown directive headings outside a body block fail closed instead of applying partial authority.

## Legacy Upgrades and Config Backups

When upgrading an existing Decapod repository after installing a newer `decapod` binary:

- **Idempotent store migration:** `decapod init`, `decapod validate`, and other governed commands one-shot-import any residual legacy event JSONL into the canonical SQLite event store (`.decapod/data/decapod.db`) and move retired files under `.decapod/data/.retired-jsonl/`. Runtime writers never append to JSONL, and legacy SQLite stores recreated by an older binary are copied forward and removed.
- **Override preservation:** Existing `.decapod/OVERRIDE.md` body bytes are preserved while sections are rendered into fenced documentation source areas with visible authoring instructions; unsafe or ambiguous authority fails closed.
- **Non-destructive config refresh:** `decapod init --refresh` preserves existing authored settings as defaults, makes no state edits during `--dry-run`, and backs up changed configuration privately under `.decapod/data/config-backups/`. Agents run `decapod init --proof` during validation for non-interactive upgrade checks without authenticating Cloud credentials or asserting readiness.
