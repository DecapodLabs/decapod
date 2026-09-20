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

## The `[decision]` Section

The optional decision provider supplies structured observations to Decapod's
assurance result. It does not make policy decisions or replace governance.

| Key | Type | Default | Description |
|---|---|---|---|
| `provider` | enum | `"none"` | `"none"` keeps operation local; `"jev"` enables the bounded Jev trajectory-satisfaction observation. |

When Jev is enabled, Decapod reads the API credential from the machine
environment variable `TYPESAFE_API_KEY`, falling back to the machine-local
`~/.local/share/decapod/secrets.json` file (or the `XDG_DATA_HOME` equivalent).
Running `decapod init --decision-provider jev` persists an explicitly supplied
environment key to that file with restrictive permissions. The file is separate
from the Propodus `session_token.json`; credentials are never written to this
project file. Jev uses TypeSafe's documented
[System One API](https://docs.typesafe.ai/api). A missing credential, unavailable
service, timeout, or malformed response yields `no_observation`; it never
clears an interlock or satisfies a proof gate.

For an initialized project, `decapod init --refresh` reopens the interactive
initialization questionnaire with the current configuration as each default.
Pressing Enter preserves a setting; selecting another option can enable or
disable it, including the decision provider. The refresh path preserves the
existing repository setup and does not require `--force`.

With an active trajectory run, every Jev attempt is retained in the
schema-versioned `.decapod/governance/jev.json` ledger. A new trajectory run
resets that working-tree ledger, while committed prior ledgers remain
recoverable through Git history. The ledger is advisory evidence and is not
included as a governance authority or completion proof.

The live semantic corpus is separate from normal validation and must be enabled
explicitly with `DECAPOD_RUN_JEV_EVAL=1`; it emits machine-readable observations
for comparison and does not interpret them as policy. See the repository README
for the exact command and corpus scope.

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
