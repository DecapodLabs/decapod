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
| `backend` | enum | `"local"` | Storage backend for the project todo path (`"local"` or `"cloud"`). Cloud uses the binary-owned Propodus endpoint and GitHub origin identity. |

Cloud service details are intentionally not project configuration. Decapod
owns the Propodus deployment defaults in the binary, derives `repo_id` from
the GitHub `origin`, and keeps credentials machine-local.

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

## Exceptional RBAC operations

The main repository's `.decapod/config.toml` owns exceptional permissions;
candidate worktrees cannot grant them by editing their own config. Defaults
deny all exceptional operations. Example:

```toml
[governance.rbac]
journal_repair_roles = ["maintainer"]
validation_bypass_roles = [] # set to ["maintainer"] only for reviewed exceptions

[governance.rbac.actor_roles]
operator = ["maintainer"]
```

Callers must assert an exact configured actor label using
`DECAPOD_AGENT_ID=operator`. These are **cooperative local role labels**, not
cryptographic authentication; the local developer trusts their own workspace, and a process that can edit the main configuration
or assert another actor label remains inside the local trust boundary. Protect
the config with human review and file-system access controls.

In cloud mode, these local role labels never authorize Propodus. The cloud
backend authenticates the session bearer and resolves repository roles using
server-owned grants. Contributor, maintainer, owner, and expiring contractor
delegation are backend policy, not self-selected agent names. Local-first
operation does not require a cloud account.

`decapod todo journal checkpoint --accept-historical-gap TASK_A --accept-historical-gap TASK_B --reason "reviewed historical range"`
requires `todo.journal.repair` permission. The entire batch, each bounded
acceptance event and its subsequent transition checkpoints commit together.
Non-selected historical gaps and unrelated divergences still block recovery.

Plain `decapod validate` always runs required validation. Only an explicitly
granted `validate.bypass` permission allows
`decapod validate --bypass-validation --bypass-reason "reviewed exception"`.
This writes an audit event and returns `status=bypassed` with
`proof_ready=false` and `publication_ready=false`. It does not create a
passing validation receipt, satisfy `qa verify`, bypass CI, or authorize
publication. The publisher and proof verifier still require real evidence.

## Laya post-proof readiness

`[laya] mode = "disabled"` is the default; Laya inference runs only after
deterministic validation when the repo sets `mode = "required"` with the
reviewed `model_manifest_digest` and local installation. A candidate cannot
disable an accepted required policy. No Laya installation or inference is
required for disabled mode. Per-run assessment receipt identifiers are
excluded from verification replay hashes; receipt decisions, material,
configuration and governance bindings remain significant.
