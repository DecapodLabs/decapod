# Storage Compaction Benchmarks & Dogfood Measurements — v0.169.1 (`#1630`)

## Scope & Measurement Boundary

- **Binary & Format Pin**: `decapod v0.169.1` release binary (`target/release/decapod`, built from `origin/master` `74ecb5908` + `#1630` contracts/triggers/benchmark harness), `storage_format_version = 2` (`sqlite.compact_lossless_storage.v002`).
- **Commands**:
  - `decapod houseboat storage-benchmark --tasks 25` (`src/decapod/core/houseboat.rs::run_local_storage_compaction_benchmark`)
  - `decapod houseboat storage-snapshot-benchmark --source-data <dir> --scope <label>` (`src/decapod/core/houseboat.rs::run_snapshot_compaction_benchmark`, using `DactylBridge::open_canonical(..., AccessMode::ReadOnly)` + `DactylBridge::backup` into an isolated scratch directory; source stores are never mutated)
- **Hosted Backend Status**: `unavailable_local_only` — live hosted Supabase tenant byte counts remain unavailable until authorized hosted execution (`#1378`/`#1369`). Local PostgreSQL 17 (`initdb`/`psql`) write-boundary and `storage_components` GC parity were independently verified on `octet_length(convert_to(col, 'UTF8'))` (`"🦀".repeat(256)` = `1,024` B accepted, `"🦀".repeat(257)` = `1,028` B rejected, `chr(0)` rejected, orphaned components GC'd).
- **Privacy & Integrity Policy**: Aggregate metrics, timings, rusage, and SHA-256 parity digests only; no task titles, operational text, credentials, or local user paths are published.

## Synthetic Bounded Growth Workload (25 Tasks / 75 Governed Operations)

With `federation` initialized cleanly (`0` stderr warnings) and repeated structured task descriptions (`> 512` B `min_component_bytes`), v2 writes `_ce_v1` compact envelopes inline and deduplicates **`75` references onto `1` `storage_components` row** (`components_stored = 1`, `component_refs_stored = 75`):

| Metric | v1 Uncompacted Twin | v2 Before Compaction | v2 After Compaction + Explicit `VACUUM` Reclaim |
| --- | ---: | ---: | ---: |
| Main DB (`decapod.db`) bytes | 1,114,112 | 946,176 | **946,176** |
| SQLite `page_count` (4,096 B pages) | — | 231 | 231 |
| `freelist_count` | — | 0 | 0 |
| Bytes / operation | 14,854 | 12,615 | **12,615** |
| Bytes / task | 44,564 | 37,847 | **37,847** |
| v1 -> v2 Savings | — | — | **167,936 B (15.07%)** |

- **Operation Latency (`exact_per_operation_monotonic_instant`)**: `p50 = 83,803 µs`, `p95 = 265,702 µs`, `p99 = 318,187 µs`, `max = 318,187 µs`.
- **Process Resource Usage (`libc::getrusage`)**: `cpu_user = 2,996,420 µs`, `cpu_sys = 2,662,732 µs`, `max_rss = 21,807,104` B (`20.80 MiB`).

## Real Dogfood Store Snapshot Compaction & Reconstruction (`dactyl`, `propodus`, `decapod`)

Source stores (`dactyl`, `propodus`, `decapod`) were snapshot-copied read-only (`source_wal_sidecar_bytes = 0`). Compaction runs `storage_compaction::compact_datastore_lossless(..., dry_run=false, reclaim_pages=false)` followed by explicit `storage_compaction::reclaim_pages_explicit` (`VACUUM`), verifying exact logical-event counts, `events_logical_sha256`, `todo_stream_parity`, and `task_journal_parity`.

| Repository Scope | Logical Events | Todo Events | Snapshot Before Bytes (Pages) | After Reclaim Bytes (Pages) | Bytes Saved vs Snapshot (% Reduction) | Events Compacted | Compact / Reclaim Duration |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `dactyl` | 76,261 | 2,269 | 46,329,856 (11,311) | 35,835,904 (8,749) | **10,493,952 (22.65%)** | 1 | 2,215 ms / 370 ms |
| `propodus` | 235,795 | 6,396 | 135,012,352 (32,962) | 111,149,056 (27,136) | **23,863,296 (17.67%)** | 5 | 6,497 ms / 1,609 ms |
| `decapod` | 587,405 | 19,248 | 355,540,992 (86,802) | 304,103,424 (74,244) | **51,437,568 (14.47%)** | 12 | 16,675 ms / 4,761 ms |
| **Combined Dogfood Total** | **899,461** | **27,913** | **536,883,200** | **451,088,384** | **85,794,816 (15.98%)** | **18** | **25,387 ms / 6,740 ms** |

### Reconstruction Cost & Lossless Parity Before vs After Reclaim

| Repository Scope | `logical_inventory_ms` (Before -> After) | `task_journal_verify_ms` (Before -> After) | `todo_events_query_ms` (Before -> After) | Lossless / Todo / Journal Parity |
| --- | ---: | ---: | ---: | --- |
| `dactyl` | 441 ms -> 451 ms | 177 ms -> 173 ms | 19 ms -> 19 ms | `true` / `true` / `true` |
| `propodus` | 1,395 ms -> 1,417 ms | 1,097 ms -> 930 ms | 38 ms -> 38 ms | `true` / `true` / `true` |
| `decapod` | 3,541 ms -> 3,569 ms | 5,420 ms -> 4,480 ms | 102 ms -> 102 ms | `true` / `true` / `true` |

Machine-readable benchmark artifacts are stored in [dogfood-compaction-v0.169.1.json](dogfood-compaction-v0.169.1.json).

## Unified Versioned Field & SQL Write-Boundary Parity (`#1630`)

- **Rust Contract Table (`src/decapod/core/storage_contracts.rs::field_contract_table`)**: Exports every active persisted `(table, column)` pair, UTF-8 byte cap, non-empty/nullable invariant, `MAX_JSON_DEPTH = 32`, collection cardinality limit (`tags_per_task = 64`, `owners_per_task = 32`, `dependencies_per_task = 128`, `evidence_refs_per_record = 128`), and `propodus_abi_v1_max_bytes` reconciliation (`decapod.logical_storage_abi.v1`), surfaced in `contract_manifest()["field_table"]`.
- **SQLite Engine Write-Boundary Triggers (`ensure_sqlite_write_boundary_triggers`)**: `todo::ensure_schema` and `events::ensure_tables` install compact per-table `BEFORE INSERT` and `BEFORE UPDATE OF <cols>` triggers (`trg_decapod_{tasks,task_verification,events}_{ins,upd}_contract`) enforcing `length(CAST(col AS BLOB))` and `instr(CAST(col AS BLOB), zeroblob(1)) > 0` while allowing reads and unrelated-column updates on historical oversized rows.
- **PostgreSQL Parity (`postgres_check_constraint_sql`)**: Emits matching `octet_length(convert_to(col, 'UTF8')) <= <max_utf8_bytes>` constraints aligned with PostgreSQL native `0x00` rejection and Propodus `logical-storage-abi-v1.json`, reconciled in [logical_contract_v1.json](logical_contract_v1.json).
