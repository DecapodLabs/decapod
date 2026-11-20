# Datastore v2 Compact Lossless Storage Benchmarks (`#1630`)

Reproducible fixture source: `tests/datastore_efficiency.rs` (`datastore_v2_compaction_benchmarks` and `v2_checkpoint_and_event_compaction_reduces_bytes_losslessly`).
Measurements captured on macOS arm64 using disposable SQLite stores through Dactyl (`cargo test --test datastore_efficiency datastore_v2_compaction_benchmarks -- --ignored --nocapture`). No live operator or repository `.decapod/data/decapod.db` store was mutated to generate these benchmarks.

## What Changed (`#1630`)

1. **Versioned Logical Field, Collection, JSON, and Authority-Convergence Contracts (`src/decapod/core/storage_contracts.rs`, `docs/datastore-v2/logical_contract_v1.json`)**:
   - All string bounds are defined in **UTF-8 bytes** (`str::len()`), rejecting embedded NUL (`0x00`) bytes and JSON nesting depth `> 32` fail-closed (`STORAGE_CONTRACT_VIOLATION`) without silent truncation.
   - Historical rows exceeding current write bounds remain losslessly readable and updatable on unrelated columns.
   - Duplicated relational/inline authority (`tasks.tags` <-> `task_tags` and `tasks.depends_on` <-> `task_dependencies`) is normalized on write and audited via `audit_task_authority_convergence`.
2. **Component-Deduplicated Task Journal v2 (`src/decapod/core/task_journal.rs`, `src/decapod/core/task_transfer.rs`)**:
   - Task checkpoints (`schema_version = 2`, `MIN_SCHEMA_VERSION = 1`) externalize eligible immutable text components (`>= 128` UTF-8 bytes across `task.description`, `task.scope`, `verification.proof_plan`, `verification.verification_artifacts`, and `verification.last_verified_notes`) into content-addressed `storage_components` (`codec_version = 1` zlib when smaller, `0` raw UTF-8 otherwise) + `storage_component_refs` (`WITHOUT ROWID`, `ON DELETE RESTRICT` FK) inside the same SQLite transaction.
   - `TaskProjection` reconstruction and `projection_digest` remain byte-for-byte identical to v1; standalone `task_transfer` bundles embed referenced components (`Bundle.components`) for self-contained cross-host import.
3. **Compact Event Envelope Codec (`src/decapod/core/events.rs`)**:
   - Non-`federation` event streams strip envelope keys duplicated in SQLite columns (`event_id`, `ts`, `event_type`, `actor`, `subject_kind`, `subject_id`) via a `_ce_v1` bitmask (with `_ce_raw` lossless escape if a caller object contains literal `_ce_v1`/`_ce_raw` keys) and reconstruct exact JSON values on read (`reconstruct_stored_payload`).
4. **Snapshot-Consistent Keyset-Paged Verification & Reference-Aware GC (`src/decapod/core/task_journal.rs`, `src/decapod/core/storage_compaction.rs`)**:
   - `verify_with_budget` replaces global `10,000`-task / `100,000`-record / `32 MiB` replay ceilings with bounded keyset-paged verification (`VerifyBudget`, `VerifyCursor`, streaming `Sha256` `current_digest`, and `TASK_JOURNAL_SNAPSHOT_CHANGED` detection across cursors).
   - `gc_unreferenced_components` reclaims only components unreachable from both `storage_component_refs` and embedded event payloads; physical page reclamation (`reclaim_pages_explicit` / `VACUUM`) is explicit-only and never runs implicitly on hot CLI paths.
5. **Checksummed Forward Migration `1200` & Downgrade Guard (`src/decapod/core/migration.rs`, `src/decapod/core/storage_compaction.rs`)**:
   - `sqlite.compact_lossless_storage.v002` (`sequence: 1200`) compacts historical non-`federation` event envelopes, deduplicates identical `agent_prompts` rows (summing `usage_count`), verifies pre/post logical event count and SHA-256 parity inside an atomic transaction, records `storage_migration_manifests`, and stamps `meta` (`storage.format_version = 2`, `min_reader_version`, `min_writer_version`) enforced fail-closed via `STORAGE_FORMAT_INCOMPATIBLE`.

## Scale Matrix (Packed SQLite File & Payload Bytes)

Each scale fixture initializes the canonical store and compares identical logical histories (`40` checkpoints per task, `~1,115`-byte authored description + `~315`-byte path scope) between v1 (inline `schema_version = 1` checkpoints + verbose event envelopes) and v2 (`schema_version = 2` deduplicated `storage_components` + `_ce_v1` compact envelopes) after equal explicit `VACUUM` page packing:

| History Scale | Tasks | v1 Packed SQLite File | v2 Packed SQLite File | File Bytes Saved | Packed File Reduction | v1 `events.payload` Bytes | v2 `events.payload` Bytes | v2 `storage_components` Bytes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Fresh initialized store | 0 | — | 233,472 B (57 pages) | — | — | — | — | 0 B |
| 1,000 checkpoints | 25 | 2,383,872 B | 1,650,688 B | 733,184 B | **30.76%** | 1,917,079 B | 652,479 B (-65.96%) | 3,847 B |
| 10,000 checkpoints | 250 | 21,999,616 B | 14,544,896 B | 7,454,720 B | **33.89%** | 19,195,579 B | 6,539,979 B (-65.93%) | 36,093 B |
| 100,000 checkpoints | 2,500 | 218,513,408 B | 143,917,056 B | 74,596,352 B | **34.14%** | 192,250,579 B | 65,594,979 B (-65.88%) | 362,221 B |

## Append Latency Across History Sizes (`v2`, 128 Samples Each)

| History Scale | Append `p50` | Append `p95` | Append `p99` |
|---|---:|---:|---:|
| 1,000 checkpoints | 0.474 ms | 0.683 ms | 0.759 ms |
| 10,000 checkpoints | 0.517 ms | 1.031 ms | 1.526 ms |
| 100,000 checkpoints | 0.446 ms | 0.839 ms | 1.784 ms |

## Multi-Agent Concurrent Append + Idempotent Replay Bursts

Concurrent threads synchronized behind a barrier execute 20 new `events::append_on_conn` writes + 20 immediate idempotent replays per agent against one shared SQLite store (`busy_timeout = 10000 ms`):

| Concurrent Agents | Committed Events (+ Replays) | Combined `p50` | Combined `p95` | Combined `p99` | Resulting SQLite File |
|---:|---:|---:|---:|---:|---:|
| 3 agents | 60 (+ 60) | 0.412 ms | 0.687 ms | 22.762 ms | 212,992 B |
| 10 agents | 200 (+ 200) | 0.445 ms | 1.669 ms | 149.944 ms | 233,472 B |
| 15 agents | 300 (+ 300) | 0.468 ms | 2.100 ms | 565.831 ms | 266,240 B |

## Scope & Explicit Non-Claims

- These benchmarks measure controlled disposable local SQLite fixtures through Dactyl; they do not claim hosted PostgreSQL/Supabase network latency or cold OS page-cache performance (hosted PostgreSQL compaction is tracked separately in `DecapodLabs/propodus#100`).
- Physical SQLite file reduction requires explicit `reclaim_pages_explicit` (`VACUUM`); ordinary mutations and `gc_unreferenced_components` release pages to SQLite's internal freelist for reuse without blocking hot CLI operations.

## Reproduction

```sh
cargo test --test datastore_contract
cargo test --test datastore_relational_compaction
cargo test --test datastore_efficiency
cargo test --lib core::todo::task_journal
cargo test --test datastore_efficiency datastore_v2_compaction_benchmarks -- --ignored --nocapture
```
