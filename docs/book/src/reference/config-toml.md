# Configuration Reference

Decapod project policy is defined in `.decapod/config.toml`. This file should be committed to source control and is the primary mechanism for humans to communicate global rules to agents.

## The `[init]` Section

Governs the behavior of the `decapod init` command.

| Key | Type | Default | Description |
|---|---|---|---|
| `specs` | bool | `true` | If true, scaffolds living documentation under `.decapod/managed/specs/`. |
| `diagram_style` | enum | `"ascii"` | Preferred style for generated architecture diagrams (`"ascii"` or `"mermaid"`). |
| `entrypoints` | list | `[...]` | The agent entrypoint files to maintain (e.g., `AGENTS.md`, `CLAUDE.md`). |

## The `[repo]` Section

Defines the operational policy and metadata for the repository.

| Key | Type | Default | Description |
|---|---|---|---|
| `product_name` | string | `None` | The canonical name of the software product. |
| `product_summary` | string | `None` | A high-level description of the product's purpose. |
| `architecture_direction` | string | `None` | The intended architectural style (e.g., "monolithic", "event-driven"). |
| `product_type` | string | `None` | Categorization (e.g., "cli", "library", "service"). |
| `done_criteria` | string | `None` | The global definition of "done" that all work must satisfy. |
| `primary_languages` | list | `[]` | The primary programming languages used in the repository. |
| `detected_surfaces` | list | `[]` | Entrypoints and interfaces detected in the repo (e.g., "cargo", "npm"). |
| `external_tracker` | bool | `false` | Whether Decapod should expect and validate external issue references. |
| `container_workspaces` | bool | `true` | If true, Decapod will strongly encourage/enforce Docker isolation for worktrees. |

## The `[cloud]` Section

Cloud configuration is an explicit, experimental opt-in and contains only
non-secret routing data. `api_url`, `project_id`, and `repo_id` are configurable
for development, previews, and future repositories; bearer credentials belong
to `decapod cloud login`, `DECAPOD_ACCESS_TOKEN`, or another machine-local
credential boundary, never this file.

| Key | Type | Default | Description |
|---|---|---|---|
| `enabled` | bool | `false` | Records cloud intent; does not switch local SQLite automatically. |
| `experimental` | bool | `true` | Required while the hosted adapter is being rolled out. |
| `provider` | string | `"vercel"` | Non-secret provider identifier. |
| `api_url` | string | `"https://decapod-cloud.vercel.app"` | Configurable Propodus API base URL. |
| `project_id` | string | `""` | Non-secret deployment/project correlation value. |
| `repo_id` | string | `""` | Canonical repository identifier authorized by the deployment. |

## Schema Versioning

Decapod uses a `schema_version` key at the root to ensure forward and backward compatibility as the governance kernel evolves.

```toml
schema_version = "1.0.0"

[init]
specs = true
diagram_style = "mermaid"

[repo]
product_name = "decapod"
container_workspaces = true
done_criteria = "Validate passes and all unit tests are green."
```
