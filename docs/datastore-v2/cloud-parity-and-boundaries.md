# Cloud Parity, Fail-Closed Boundaries, Hosted Fixtures & `#1220` Architectural Boundary

This note reconciles the remaining Section 10 storage/parity trackers (`#1110`, `#1120`, `#1221`, `#1369`, `#1378`, `#1220`) truthfully against `v0.166`.

## 1. Non-Todo Store Cloud Parity vs Explicit Fail-Closed Boundaries (`#1110`, `#1120`, `#1221`)

- **One authoritative backend per repository**:
  - `backend = "local"` persists all subsystems into canonical `.decapod/data/decapod.db`.
  - `backend = "cloud"` delegates governed tasks, claims, releases, completions, custody binding, and proof capture exclusively to Propodus/Dactyl HTTP/Supabase contracts.
- **Explicit Fail-Closed Non-Todo Boundaries**:
  - `src/decapod/core/cloud_parity_matrix.rs` explicitly enumerates all 17 non-todo / local-only store surfaces (`knowledge.store`, `federation.store`, `decide.store`, `health.store`, `aptitude.store`, `archive.store`, `feedback.store`, `cron.store`, `reflex.store`, `lcm.store`, `primitives.store`, `map.store`, `policy.store`, `data.broker`, `data.compact`, `store.upsert`, `store.query`) with `never_falls_back_to_sqlite = true`.
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


The Cloud `todo claim` route accepts ordinary exclusive assignment. Shared
mode, an explicit lease duration, and an explicit intent anchor each return an
unsupported error before datastore dispatch. An accepted claim commits task
assignment and its ordered event. Hosted renewal, heartbeat and authorized
handoff require a separate durable service contract.
