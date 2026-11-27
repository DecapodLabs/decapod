# Storage Compaction Benchmarks & Dogfood Measurements — v0.166 (`#1630`)

## Scope & Measurement Boundary

- **Binary & Format Pin**: `decapod v0.166.1` release binary (`--release`), `storage_format_version = 2` (`sqlite.compact_lossless_storage.v002`).
- **Commands**:
  - `decapod houseboat storage-benchmark --tasks 25` (`run_local_storage_compaction_benchmark`)
  - `decapod houseboat storage-snapshot-benchmark --source-data <dir> --scope <label>` (`run_snapshot_compaction_benchmark`, using `DactylBridge::open_canonical(..., AccessMode::ReadOnly)` + `DactylBridge::backup` into an isolated scratch directory; source stores are never mutated)
- **Hosted Backend Status**: `unavailable_local_only` — hosted PostgreSQL / Propodus live-tenant byte counts remain unavailable until authorized hosted execution (`DECAPOD_SUPABASE_HOSTED_PROOF=1`).
- **Privacy & Integrity Policy**: Aggregate metrics and SHA-256 parity digests only; no task titles, operational text, credentials, or local user paths are published.

## Synthetic Bounded Growth Workload (25 Tasks / 75 Governed Operations)

| Metric | Before Compaction | After Compaction + GC | After Explicit `VACUUM` Reclaim |
| --- | ---: | ---: | ---: |
| Main DB (`decapod.db`) bytes | 630,784 | 630,784 | 630,784 |
| SQLite `page_count` (4,096 B pages) | 154 | 154 | 154 |
| `freelist_count` | 0 | 0 | 0 |
| Bytes / operation | 8,410 | 8,410 | 8,410 |

- **Operation Latency (`exact_per_operation_monotonic_instant`)**: `p50 = 98,045 µs`, `p95 = 180,489 µs`, `max = 219,604 µs`.
- **Compaction / GC Activity**: `events_compacted = 0`, `unreferenced_components_collected = 0` (`v0.166` writes `_ce_v1` compact envelopes directly at write admission).

## Real Dogfood Store Snapshot Compaction & Reconstruction (`dactyl`, `propodus`, `decapod`)

Source stores were snapshot-copied read-only (`source_wal_sidecar_bytes = 0` across all three stores). Compaction runs `storage_compaction::compact_datastore_lossless(..., dry_run=false, reclaim_pages=false)` followed by explicit `storage_compaction::reclaim_pages_explicit` (`VACUUM`), then verifies exact logical-event count, `events_logical_sha256`, `todo_stream_parity`, and `task_journal_parity`.

| Repository Scope | Logical Events | Todo Events | Before Bytes (Pages) | After Reclaim Bytes (Pages) | Bytes Saved (% Reduction) | Events Compacted | Compact / Reclaim Duration |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `dactyl` | 69,054 | 2,052 | 42,614,784 (10,404) | 32,165,888 (7,853) | **10,448,896 (24.52%)** | 1 | 2,391 ms / 209 ms |
| `propodus` | 229,932 | 6,228 | 131,825,664 (32,184) | 108,011,520 (26,370) | **23,814,144 (18.07%)** | 5 | 6,417 ms / 943 ms |
| `decapod` | 549,462 | 17,996 | 333,950,976 (81,531) | 282,701,824 (69,019) | **51,249,152 (15.35%)** | 12 | 18,138 ms / 2,021 ms |
| **Combined Dogfood Total** | **848,448** | **26,276** | **508,391,424** | **422,879,232** | **85,512,192 (16.82%)** | **18** | **26,946 ms / 3,173 ms** |

### Reconstruction Cost & Lossless Parity Before vs After Reclaim

| Repository Scope | `logical_inventory_ms` (Before -> After) | `task_journal_verify_ms` (Before -> After) | `todo_events_query_ms` (Before -> After) | Lossless / Todo / Journal Parity |
| --- | ---: | ---: | ---: | --- |
| `dactyl` | 483 ms -> 571 ms | 157 ms -> 159 ms | 16 ms -> 26 ms | `true` / `true` / `true` |
| `propodus` | 1,601 ms -> 1,295 ms | 1,129 ms -> 877 ms | 39 ms -> 40 ms | `true` / `true` / `true` |
| `decapod` | 3,439 ms -> 3,572 ms | 5,036 ms -> 4,496 ms | 92 ms -> 92 ms | `true` / `true` / `true` |

Machine-readable benchmark artifacts are stored in [dogfood-compaction-v0.166.json](dogfood-compaction-v0.166.json).

## Bottleneck Fix Discovered During Real Dogfood Benchmarking

While running `storage-snapshot-benchmark` on the 549K-row `decapod` snapshot, `storage_compaction.rs` (`compact_historical_events`, `collect_event_embedded_component_keys`, and `logical_events_inventory`) previously paged `events` via `WHERE (seq > ?1 OR (seq = ?1 AND event_id > ?2)) ORDER BY seq ASC, event_id ASC LIMIT 256` without a `(seq, event_id)` index — causing ~2,146 full table scans + temp B-tree sorts (`O(N^2)`). Switching those three loops to `WHERE rowid > ?1 ORDER BY rowid ASC LIMIT 1024` (with deterministic `(seq, event_id)` sort over compact 32-byte row digests in `logical_events_inventory`) keeps each query well under `dactyl-db`'s per-query row quota and reduces 549K-row compaction + verification from stalling past 180s down to `18.1s` compaction and `3.6s` full logical verification.

## Write-Admission Bounds Parity

All persisted stores (`todo`, `events`, `verify`, `knowledge`, `federation`, `health`, `decide`, `cron`, `reflex`, `feedback`, `archive`, `aptitude`, `lcm`) enforce `core::storage_contracts` UTF-8 byte limits, NUL-byte rejection, JSON nesting depth (`MAX_JSON_DEPTH = 32`), and collection cardinality bounds (`tags_per_task = 64`, `owners_per_task = 32`, `dependencies_per_task = 128`, `evidence_refs_per_record = 128`), reconciled against [logical_contract_v1.json](logical_contract_v1.json).
