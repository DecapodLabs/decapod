# Storage Compaction Benchmarks & Recovery Proof — v0.162.0 (`#1630`)

## Scope & Measurement Boundary

- **Binary & Format Pin**: `decapod v0.162.0`, `storage_format_version = 2` (`sqlite.compact_lossless_storage.v002`).
- **Command**: `decapod houseboat storage-benchmark --tasks 25` (`src/decapod/core/houseboat.rs::run_local_storage_compaction_benchmark`).
- **Backend Scope**: `local_sqlite_benchmark` (disposable local SQLite store exercising `todo::add_task`, `todo::claim_task`, `todo::release_task_as_actor`, `storage_compaction::compact_historical_events`, `storage_compaction::gc_unreferenced_components`, and `storage_compaction::reclaim_pages_explicit`).
- **Hosted Backend Status**: `unavailable_local_only` — hosted PostgreSQL / Propodus live-tenant byte counts and deployed migrations are NOT claimed by this local benchmark and remain open until authorized hosted execution.
- **Historical Fixture Distinction**: Earlier numbers in `docs/datastore-v2/compaction-benchmarks-v0.141.0.md` remain labeled historical synthetic SQLite baselines and are not extrapolated onto live hosted stores.

## Measured Local SQLite Results (25 Tasks / 75 Governed Operations)

| Metric | Before Compaction | After Compaction + GC | After Explicit Reclaim |
| --- | ---: | ---: | ---: |
| Main DB (`decapod.db`) bytes | 630,784 | 630,784 | 630,784 |
| WAL sidecar bytes | 0 | 0 | 0 |
| SHM sidecar bytes | 0 | 0 | 0 |
| SQLite `page_size` | 4,096 | 4,096 | 4,096 |
| SQLite `page_count` | 154 | 154 | 154 |
| SQLite `freelist_count` | 0 | 0 | 0 |
| Live payload+index bytes | 630,784 | 630,784 | 630,784 |
| Reclaimable bytes | 0 | 0 | 0 |
| Bytes / operation | 8,410 | 8,410 | 8,410 |

- **Operation Latency (`exact_per_operation_monotonic_instant`)**:
  - `p50`: `68,390 µs` (`68.4 ms`)
  - `p95`: `106,927 µs` (`106.9 ms`)
  - `max`: `134,226 µs` (`134.2 ms`)
- **Compaction / GC Activity**: `events_compacted = 0`, `unreferenced_components_collected = 0` because `v0.162.0` writes compact v2 event envelopes directly on fresh stores with zero hot-path `VACUUM`.
- **Explicit Unknown Fields**:
  - `hosted_postgresql_live_bytes`: `unavailable_local_only`
  - `process_rss_delta_bytes`: `not_sampled_in_inprocess_harness`

## Interrupted Migration, Backup/Restore, Downgrade & GC Proof

Verified deterministically in `tests/houseboat_engine.rs::houseboat_1630_storage_compaction_benchmark_and_recovery_invariants`:

1. **Backup/Restore Preservation**: Copying `decapod.db` and verifying schema/row integrity preserves exact task count and identities.
2. **Old Reader & Writer Downgrade Refusal**: `storage_compaction::check_storage_format_compatibility_for_version(&conn, "0.100.0", read_only)` fails closed with `STORAGE_FORMAT_INCOMPATIBLE` for both `read_only = true` and `read_only = false` after stamping v2 format metadata.
3. **Interrupted Migration Rollback & Idempotent Retry**: Rolling back an in-flight `BEGIN IMMEDIATE` transaction preserves exact task counts and allows idempotent component GC without losing reachable history.
4. **No Hot-Path `VACUUM`**: Page reclamation remains explicit-only via `reclaim_pages_explicit`.
