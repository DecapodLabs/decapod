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
- Legacy env-token fixtures cannot bind managed DPoP/device-gateway custody without bypassing fail-closed security invariants. Live hosted verification remains `unavailable_local_only` until an authorized managed Cloud endpoint/fixture is provided (read-only checks only; zero `$0/mo` spend or customer/billing mutations, zero `BYPASSRLS`).
- All three Supabase PostgreSQL topologies (`direct :5432`, `Supavisor session pooler :5432`, and `Supavisor transaction pooler :6543`) are documented in `docs/book/src/reference/supabase-validation.md`; transaction-pooled execution enforces `PGRST_DB_PREPARED_STATEMENTS=false` and transaction-local `set_config(..., true)` GUCs (cleared at `COMMIT`, verified against PostgreSQL 17.11) alongside `local_native_managed_project_contract` / `local_native_managed_project_revocation` over PostgREST 16.5.

## 3. Container Storage Custody & Socket Relay Boundaries (`#1379`)

- **Privileged-UID (`uid=0`) read-only store rejection (`src/decapod/core/db.rs::storage_health_preflight`)**: Inside Linux containers running as `uid=0`, `CAP_DAC_OVERRIDE` lets `write_probe` open/create files inside `0o555` directories. `storage_health_preflight` now checks `meta.permissions().mode() & 0o222 == 0` under `#[cfg(unix)]` (matching `plugins::container::is_dir_writable_by_current_user`) and fails closed with `STORAGE_PREFLIGHT_FAILED` before probing (`tests/sqlite_hardening.rs::storage_preflight_fails_for_non_writable_directory` and `core::db::storage_filesystem_tests::storage_health_preflight_rejects_read_only_directory_even_under_root_uid`).
- **Process-global fault-injection concurrency (`tests/sqlite_hardening.rs`)**: `env_lock()` now uses `RwLock<()>` held exclusively (`write()`) by `sqlite_fault_injection_produces_deterministic_io_error_path` around `DECAPOD_STORAGE_FAULT_STAGE` and shared (`read()`) by the other four tests so concurrent Linux test execution never leaks `STORAGE_FAULT_INJECTED` across threads.
- **`virtiofs` vs. `UNSUPPORTED_FS_TYPES` & container SELinux recovery boundaries**: `core::db::UNSUPPORTED_FS_TYPES` (`["nfs", "nfs4", "cifs", "smbfs", "9p", "vboxsf"]`) rejects network/VM-legacy filesystems via `/proc/self/mountinfo` (`STORAGE_PREFLIGHT_UNSUPPORTED_FS`), while intentionally allowing `virtiofs`, `fuse`, `fuseblk`, and `fuse.sshfs` (`intentional_shared_filesystems_remain_outside_the_existing_denylist`; verified live inside Podman `uid=0` with `decapod todo add` / `todo list` on a `virtiofs` mount). Inside rootless SELinux Podman containers (`container_file_t`), `dactyl-db 0.21.0::recover_from_dump_reload` intentionally refuses dump/reload recovery with `recovery_metadata_unsupported` (`Capability`) rather than dropping MAC/ACL metadata.
- **Host-to-container `DECAPOD_CONTAINER_SIGNER_SOCKET` boundary**: `plugins::container::prepare_container_signer_server` (`HostContainerSignerServer::bind_with_relay`) bind-mounts the private socket parent directory onto `/run/decapod-signer`. On native Linux hosts (Podman/Docker), guest `AF_UNIX` `connect()` reaches the host listener directly; under macOS `podman machine` (`vfkit` + `virtiofs`), `AF_UNIX` sockets cannot traverse the VM hypervisor filesystem boundary natively, so macOS-to-VM host relays fail closed (`container_signer_unavailable`) rather than leaking host bearer tokens into the container environment.

## 4. `#1220` Pure-Rust / No-SQLite-Family Boundary — Explicit Irreducible Conflict

- **Tracker Conflict**: `#1220` requests a pure-Rust datastore without SQLite-family C dependencies, whereas Decapod's accepted architecture in `Cargo.toml` (`dactyl-db = { features = ["sqlite", "neon"] }`), `src/decapod/core/dactyl.rs` (`LOCAL_SQLITE_RUNTIME_REQUIRED`), and `docs/datastore-v2/architecture.md` mandates `dactyl-db` SQLite as the single authoritative local datastore (`#1118`, `#1132`) alongside Propodus/Dactyl Cloud parity.
- **Decision**: Do NOT introduce a competing embedded datastore engine. Preserve the single authoritative SQLite/Dactyl boundary, keep `cloud_parity_matrix` and `local_sqlite_path_present` isolation airtight, and report `#1220` truthfully as an irreducible architectural conflict with the accepted Dactyl contract unless maintainers explicitly supersede Dactyl.
