# Repository-Shaped Kernel Workload Baseline & Regression Budgets (`#1938`)

## Scope, Cross-References & Measurement Boundary

- **Binary & Format Pin**: `decapod v0.164.1`, `storage_format_version = 2` (`sqlite.compact_lossless_storage.v002`).
- **Suite & Wiring**: `tests/kernel_workload_1938.rs` (wired into `tests/core/core.rs` and `.config/build/decapod.BUILD.bzl`, fixed deterministic seed `0xDECA_90D4_1938_0001`).
- **Related Issues & Boundaries**:
  - `#1630` ([`compaction-benchmarks-v0.162.0.md`](datastore-v2/compaction-benchmarks-v0.162.0.md)): micro-level storage compaction & downgrade/rollback proofs.
  - `#1400`: validation preflight latency budget (this manifest measures `pre_build_validate::evaluate` cold/warm structural check cost separately from full CI/Buildkite validation).
  - `#1095`: bounded, privacy-preserving time-series execution telemetry (`src/decapod/core/telemetry.rs`, `src/decapod/core/observability_projections/telemetry.rs`), distinct from **Propodus `#230`** service SLOs and **Dactyl `#144`** transport budgets.
  - `#1933`: ecosystem hardening ledger.
- **Local vs Cloud Comparison**:
  - **Local (`local_sqlite`)**: measured directly against real `DbBroker`, `todo`, `events`, `storage_compaction`, `db_maintenance`, `pre_build_validate`, and `telemetry` on disposable SQLite + Git repositories.
  - **Cloud (`cloud_supabase_dactyl`)**: `unavailable_offline_no_paid_infra_see_propodus_230_dactyl_144` — offline tests never hit paid Supabase/Propodus infrastructure or fabricate hosted latency/byte numbers.

## Dataset Shapes (Fixed Seed `0xDECA_90D4_1938_0001`)

Skewed Zipf-like workload: `70%` of high-rate heartbeat/trace events target the hottest `hot_bucket_pct` of tasks (`20%` small / `15%` medium / `10%` large), plus `10` seeded legacy uncompacted event rows per tier on `events` (`stream = 'traces'`) to exercise active `_ce_v1` compaction and zero-loss payload reconstruction.

| Tier | Tasks | Agents | Worktrees | Skewed Events | Hot Subset | Proof Records | Concurrent Workers | Total Stream Events |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `small` | 12 | 2 | 2 | 120 | 20% | 2 | 2 | 262 |
| `medium` | 36 | 6 | 8 | 600 | 15% | 6 | 4 | 986 |
| `large` | 48 | 16 | 24 | 1,800 | 10% | 12 | 6 | 2,312 |

## Measured Baseline (`v0.164.1`, Debug Test Profile)

### Latency, Throughput, Retry & Pre-Build Timing

| Tier | Claim/Renew/Release `p50` | `p95` | `p99` | Throughput (ops/s) | Contended Conflict+Retry | Cold Read | Warm Read `p50` / `p99` | Replay + Journal Verify | Compaction + GC + Reclaim | Maintenance (`inspect`+`verify`+`wal`) | Pre-Build Cold / Warm |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `small` | 148.2 ms | 388.7 ms | 393.1 ms | 6.81 | 345.2 ms | 13.5 ms | 11.5 ms / 11.9 ms | 107.3 ms | 14.4 ms | 70.3 ms | 15.2 ms / 13.7 ms |
| `medium` | 215.9 ms | 496.8 ms | 613.3 ms | 7.14 | 337.6 ms | 14.2 ms | 11.2 ms / 11.7 ms | 120.4 ms | 24.4 ms | 77.2 ms | 17.0 ms / 13.8 ms |
| `large` | 280.6 ms | 713.1 ms | 1,214.5 ms | 7.41 | 340.5 ms | 14.0 ms | 11.7 ms / 12.0 ms | 132.4 ms | 38.3 ms | 86.3 ms | 17.0 ms / 13.3 ms |

### SQLite Growth, Write Amplification, Lock Waits & RSS

| Tier | DB Bytes (Before -> After Reclaim) | Pages (4 KiB) | WAL Sidecar | Tables / Indexes | Logical Payload Bytes | Bytes / Event | Write Amplification | Lock Wait `p50` / `p99` | Process RSS | Events Compacted | Lossless Digest |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `small` | 638,976 -> 638,976 | 156 -> 156 | 0 B | 19 / 39 | 50,882 | 2,438 B | 12.56x | 120.3 ms / 151.4 ms | 14,816 KiB | 10 | verified |
| `medium` | 1,458,176 -> 1,437,696 | 356 -> 351 | 0 B | 19 / 39 | 240,908 | 1,458 B | 6.05x | 147.2 ms / 256.4 ms | 17,872 KiB | 10 | verified |
| `large` | 2,334,720 -> 2,293,760 | 570 -> 560 | 0 B | 19 / 39 | 714,472 | 992 B | 3.27x | 153.2 ms / 692.9 ms | 20,816 KiB | 10 | verified |

## Reviewed Nonfunctional Regression Budgets (Debug CI Enforcement)

`tests/kernel_workload_1938.rs` enforces these upper bounds deterministically on every CI run (sized with headroom for shared debug-mode CI runners while catching quadratic scans, lock-hold blowups, or hot-path `VACUUM` regressions):

| Tier | Lifecycle `p99` | Cold Read | Warm Read `p99` | Replay | Compaction | Maintenance | Pre-Build Warm | Bytes / Event | Write Amp |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `small` | <= 2,500 ms | <= 1,000 ms | <= 500 ms | <= 2,000 ms | <= 3,000 ms | <= 3,000 ms | <= 1,000 ms | <= 16,384 B | <= 32.0x |
| `medium` | <= 3,500 ms | <= 1,500 ms | <= 750 ms | <= 4,000 ms | <= 5,000 ms | <= 4,000 ms | <= 1,500 ms | <= 8,192 B | <= 24.0x |
| `large` | <= 5,000 ms | <= 2,000 ms | <= 1,000 ms | <= 8,000 ms | <= 10,000 ms | <= 6,000 ms | <= 2,000 ms | <= 6,144 B | <= 20.0x |

## Measured Bottlenecks (No Speculative Engine Replacement)

1. **Serialized Task Write Lock & Journal Checkpointing**: Multi-worker `claim`/`renew`/`release` throughput remains ~`6.8–7.4 ops/sec` (`p50` climbs from `148 ms` at 2 workers to `281 ms` at 6 workers, `p99` climbs to `1,215 ms`) because `DbBroker` + `with_task_journal` acquires a single file `StorageLock`, updates `tasks`, appends `events`, and executes `task_journal` synchronization per operation.
2. **Indexed Subject Reads Scale Flatly**: `warm_read` `p50` stays at `11.2–11.7 ms` from `262` events (`small`) to `2,312` events (`large`) via `idx_events_stream_subject_seq`; `pre_build_validate::evaluate` stays at `13.3–13.8 ms` warm regardless of SQLite size.
3. **Zero-Loss Compaction Overhead is Bounded**: `compact_historical_events` + `gc_unreferenced_components` + explicit reclaim completes in `14–38 ms` with zero task or payload digest drift, and `bytes_per_event` amortizes cleanly (`2,438 B -> 992 B`).

## Explicit Scaling Activation Thresholds

Do **not** replace SQLite speculatively while workloads remain below these measured thresholds. Activate the next tier only when telemetry (`decapod observe telemetry` / `core::telemetry`) observes sustained threshold crossings:

1. **Stage 1 — Local SQLite Write-Batching & Journal Tuning** (stay on single local SQLite):
   - **Trigger**: `claim/renew` `p99 > 1,500 ms` across >= `8` concurrent local worktree agents, or `wal_sidecar_bytes > 16 MiB` between maintenance checkpoints.
   - **Action**: Coalesce non-blocking heartbeat renewals and schedule passive `db.wal.checkpoint` (`db_maintenance::run_maintain_wal_checkpoint`) without altering single-file SQLite authority.
2. **Stage 2 — Subject-Partitioned Archival Rotation** (stay local, bounded DB footprint):
   - **Trigger**: `main_db_bytes > 128 MiB` or `events` table > `100,000` rows per stream (`events::MAX_SUBJECT_RECORDS` / `MAX_SUBJECT_BYTES = 32 MiB`) or `replay_us > 5,000 ms`.
   - **Action**: Rotate closed historical workunit streams to immutable content-addressed archive segments with verified digest anchors while keeping active tasks in `decapod.db`.
3. **Stage 3 — Hosted Multi-Tenant Coordination (`backend = "cloud"`, Propodus `#230` / Dactyl `#144`)**:
   - **Trigger**: > `16` concurrent multi-host agents or cross-machine lease contention exceeding local `StorageLock` fairness (`lock_wait p99 > 2,000 ms`) or repository-wide RBAC/audit isolation requirements.
   - **Action**: Use `backend = "cloud"` over Dactyl + PostgreSQL 17/Propodus without SQLite fallback, tracking service SLOs in Propodus (`#230`) and transport budgets in Dactyl (`#144`).
