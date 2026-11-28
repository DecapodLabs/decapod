# Cloud Parity, Fail-Closed Boundaries, Hosted Fixtures & `#1220` Architectural Boundary

This note reconciles the remaining Section 10 storage/parity trackers (`#1110`, `#1120`, `#1221`, `#1369`, `#1378`, `#1220`) truthfully against `v0.166`.

## 1. Non-Todo Store Cloud Parity vs Explicit Fail-Closed Boundaries (`#1110`, `#1120`, `#1221`)

- **One authoritative backend per repository**:
  - `backend = "local"` persists all subsystems into canonical `.decapod/data/decapod.db`.
  - `backend = "cloud"` delegates governed tasks, claims, releases, completions, custody binding, and proof capture exclusively to Propodus/Dactyl HTTP/Supabase contracts.
- **Explicit Fail-Closed Non-Todo Boundaries**:
  - `src/decapod/core/cloud_parity_matrix.rs` enumerates supported operations and remaining Local-only surfaces. The latter return a typed refusal without accessing SQLite in Cloud mode.

## Shared operational context

In a Cloud repository, `decapod data knowledge add` records scoped guidance with an immutable `--id`, `--title`, `--text`, and `--provenance`. Use `--kind decision` to record a decision and `--scope repo`, `--scope task:<id>`, or `--scope path:<relative-path>` to set its relevance. A correction uses its own ID and `--supersedes <current-id>`. The service records the authenticated writer and time, refuses a concurrent correction of the same predecessor, and retains the old record for `decapod data knowledge show --id <id>`. Use the original ID and arguments to reconcile an uncertain result. Context records are advisory and always retain their provenance.

`context.resolve` and context capsules use a single hosted statement to observe eligible current guidance, the requested task's version and assignment, and the todo event watermark. The response includes the observed revision and truncation status. The existing capsule and bundle checks independently bind the Git-owned plan, config and specs and require refresh when their bytes or the hosted observation change. This boundary does not provide a joint database transaction with a Git worktree or a reservation of task custody. Publication must still verify live custody and candidate-bound proof.

| Ownership | Content |
| --- | --- |
| Shared operational authority | Hosted tasks and events, custody, proof references, and the supported knowledge and decision revision profile |
| Git-owned authority | Source, project config, living specs, plan, policy, and repository-native evidence references |
| Private custody | Human session, DPoP keys, checkout bindings, and local broker data |
| Local-only or derived | Full decision trees, federation, feedback, promotion, archival, telemetry, and maintenance surfaces as listed in the parity matrix |

## Existing Local repositories

The backend selector changes which authority Decapod contacts. A lossless Local-to-Cloud migration remains unavailable. Leave the repository in Local mode when active Local work, decision history or evidence must carry forward. A supported cutover needs an operator preview, Local writer quiescence, verified actor and owner mapping, exact history and evidence reconciliation, a restartable import, and a tested rollback point before changing authority. Retain the original database and sidecars. The Cloud profile never imports them or consults them as fallback.
  - `src/decapod/lib.rs` enforces this fail-closed contract across:
    1. Top-level CLI routing (`is_cloud_file_only_command`), now honoring `predecoded_rpc_request` for `decapod rpc --stdin`.
    2. Session auto-renewal (`ensure_session_valid`), skipping `todo::cleanup_stale_agent_assignments` on `&store_root` when `is_cloud == true` so expired session renewal never creates `.decapod/data/decapod.db`.
    3. Defense-in-depth guards inside `rpc_handlers::handle_store_upsert` and `rpc_handlers::handle_store_query`.

## 2. Hosted Supabase End-to-End Proof Boundary (`#1369`, `#1378`)

- `tests/supabase_service.rs` remains gated behind `#[ignore]` + `DECAPOD_SUPABASE_HOSTED_PROOF=1` and fail-closed `require_managed_cli_fixture()` (`MANAGED_CLI_FIXTURE_UNAVAILABLE`).
- Legacy env-token fixtures cannot bind managed DPoP/device-gateway custody without bypassing fail-closed security invariants. Live hosted verification remains `unavailable_local_only` until an authorized managed Cloud endpoint/fixture is provided.

## 3. `#1220` Pure-Rust / No-SQLite-Family Boundary — Explicit Irreducible Conflict

- **Tracker Conflict**: `#1220` requests a pure-Rust datastore without SQLite-family C dependencies, whereas Decapod's accepted architecture in `Cargo.toml` (`dactyl-db = { features = ["sqlite", "neon"] }`), `src/decapod/core/dactyl.rs` (`LOCAL_SQLITE_RUNTIME_REQUIRED`), and `docs/datastore-v2/architecture.md` mandates `dactyl-db` SQLite as the single authoritative local datastore (`#1118`, `#1132`) alongside Propodus/Dactyl Cloud parity.
- **Decision**: Do NOT introduce a competing embedded datastore engine. Preserve the single authoritative SQLite/Dactyl boundary, keep `cloud_parity_matrix` and `local_sqlite_path_present` isolation airtight, and report `#1220` truthfully as an irreducible architectural conflict with the accepted Dactyl contract unless maintainers explicitly supersede Dactyl.
