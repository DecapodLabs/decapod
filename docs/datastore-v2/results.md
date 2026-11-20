# Increment 1 measurements and reproduction

Fixture source: `tests/datastore_efficiency.rs`. Measurements were captured on
macOS arm64, SQLite 3.54.0, Dactyl 0.11.1, Cargo test profile (opt-level 1).
The measured source was based on accepted Decapod `7035a854` / 0.133.1. Rebase to
`b0af0c28` / 0.133.2 changes governance lifecycle code, not these event/schema/
migration or adapter bytes. The v0.134.0 port onto `1d865efd` also preserves those storage source bytes. Benchmark results remain the original measurements; current-candidate correctness and publication proof are separate.

Each fixture initializes actual subsystems, adds 25/250/2,500 tasks and
1K/10K/100K synthetic historical events, and retains initialization evidence.
Payload descriptions are 128 bytes in the small fixture and 1,024 bytes in the
larger fixtures. Measurements include all tables/indexes in the file, not only
event payload bytes. These are controlled fixtures, not observed production
input maxima. The catalog contains 31 exercised tables; lazy broker dedupe is
declared separately in `entities.json`. Column authority remains incomplete.

## Physical bytes

Both variants start with equally packed files. No rows are omitted to obtain
savings. Before/after hashes cover all event columns and every other exercised
table. File shrink below is after **fixture-only VACUUM**, not merely DROP.

| Seeded history | Baseline file | Compacted file | Bytes saved | Reduction |
|---:|---:|---:|---:|---:|
| 1,000 | 901,120 | 880,640 | 20,480 | 2.27% |
| 10,000 | 15,073,280 | 14,917,632 | 155,648 | 1.03% |
| 100,000 | 146,755,584 | 145,104,896 | 1,650,688 | 1.12% |

The deleted btree's DBSTAT byte count equals each packed-file difference. At
100K events this saves 16.51 bytes per seeded historical event. Payloads dominate
these fixtures, so index removal alone cannot plausibly meet the 30% target.
No PostgreSQL application-storage savings are claimed.

DROP's observed file size is larger than the initial baseline in the report
because the timing workload first inserts and removes 512 additional events.
Those free pages remain allocated. The live btree counts and final equal-row
packed comparison distinguish this from actual reclamation.

## Writes and latency

Each variant/trial runs 512 new append transactions, 512 identical retries and
512 sequence reads. Three trials alternate timing order. Both DELETE and WAL
fixtures use `synchronous=FULL`; no runtime pragmas change. A checkpoint truncates
the fixture WAL after each sample so cumulative WAL **bytes**, rather than its
maximum file length, can be counted. Checkpoint cost is excluded from latency.
The source-owned SQL path is unchanged: five statements for a standalone new
append, four for replay. One fewer sequence btree is maintained per new event.

At 100K history, 512 appends generate 12,454,664 WAL bytes before versus
10,213,384 after: **2,241,280 fewer bytes (18.00%)**. This is measured WAL-file
traffic under the declared checkpoint pattern, not total SSD write traffic or
a cloud egress reduction. DELETE fixtures report zero WAL because none exists.

| 100K fixture | Median trial p95 before/after | Median trial p99 before/after |
|---|---:|---:|
| DELETE / FULL | 0.611 / 0.550 ms | 0.857 / 0.662 ms |
| WAL / FULL | 0.592 / 0.589 ms | 0.876 / 0.870 ms |

These are medians of three trial percentiles, not pooled percentiles. All
trials, including regressions, remain in `measurements.json`. For example, one
100K DELETE trial regressed from p95 0.611 to 0.779 ms; the others improved.
The shared host and synchronous-I/O variance do not establish a universal tail
latency guarantee. Query plans consistently use the unique index for latest
events and covering indexed MAX; no scan or sorting regression was observed.

The complete 18-case measurement process took 59.38 seconds and peaked at
685,981,696 resident bytes (macOS peak memory footprint counter: 415,302,520).
It deliberately materializes complete synthetic event hashes and catalogs; this
is **not** peak memory per Decapod operation or an allocation-count measurement.
`init_ns` measures the exercised current initializer sequence, not comparative
CLI cold start. Cold OS cache, complete governance transitions and network
transport are unmeasured.

## Reproduction

Use the repository's supported Rust toolchain and Dactyl-compatible SQLite host
library. No credentials, live datastore or hosted account is needed.

```sh
cargo test --test datastore_efficiency
cargo test --lib core::migration
cargo test --lib core::events
DECAPOD_DATASTORE_REPORT=/absolute/new/output.json \
  cargo test --test datastore_efficiency datastore_before_after_evidence \
  -- --ignored --nocapture
```

The output path must not already exist. All database files are private disposable
tempdirs. The raw report includes all object sizes, per-column logical value byte
statistics and query plans. `inventory.json` extracts its sole populated catalog;
`measurements.json` retains all case/timing/digest results while aggregating
physical object arrays into table/index byte totals. Raw exploratory reports
and command logs remain outside Git; only synthetic evidence is checked in.

MacOS `/usr/bin/time -l` measures process memory around the built measurement
test binary. The sandbox initially denied its system-counter read; the final
counter measurement ran with approved access and did not touch live data.

## Separate cloud baseline

Accepted Propodus `0bf9e42` passed `TestPostgresNativeRPC` using a disposable
PostgreSQL 17 cluster and real PostgREST. A fresh official Go 1.27.3 archive
(SHA-256 `dd3908cf20959d798c1e06ebe5b72213f0505ce01373b40ce3110975a6cd99c2`)
and task-specific cache replaced incomplete temporary toolchain/cache inputs
for that invocation only. No shared toolchain or project configuration was
replaced. Fixture-created databases, roles and PostgREST processes were cleaned
by the test. This is baseline RPC evidence, not a cloud optimization or hosted
Supabase/published-Decapod-client qualification.

## Increment 2: Lossless component-deduplicated checkpoints and compact event envelopes (`#1630`)

Increment 1 established that index compaction alone saves `1.03%–2.27%` of packed SQLite bytes because repeated task checkpoint projections and duplicated event-envelope JSON keys dominate the file. Increment 2 (`#1630`) addresses payload growth directly without lossy truncation:

- **Task journal v2 (`storage_components` + `storage_component_refs`)**: Externalizes immutable strings `>= 128` UTF-8 bytes (`task.description`, `task.scope`, `verification.proof_plan`, `verification.verification_artifacts`, `verification.last_verified_notes`) with content-addressed SHA-256 + zlib compression and `ON DELETE RESTRICT` reference tracking, preserving exact `TaskProjection` reconstruction and `projection_digest` parity.
- **Compact event envelopes (`_ce_v1` / `_ce_raw`)**: Strips envelope metadata already stored in dedicated `events` columns on non-`federation` streams and reconstructs exact JSON on read; forward migration `sqlite.compact_lossless_storage.v002` (`1200`) compacts historical rows atomically under pre/post logical SHA-256 verification.
- **Measured packed SQLite savings (`docs/datastore-v2/compaction-benchmarks-v0.141.0.md`)**:
  - `1,000` checkpoints (`25` tasks): `2,383,872` B -> `1,650,688` B (**`30.76%` reduction**, `-65.96%` `events.payload` bytes).
  - `10,000` checkpoints (`250` tasks): `21,999,616` B -> `14,544,896` B (**`33.89%` reduction**, `-65.93%` `events.payload` bytes).
  - `100,000` checkpoints (`2,500` tasks): `218,513,408` B -> `143,917,056` B (**`34.14%` reduction**, `74,596,352` B saved, `-65.88%` `events.payload` bytes).
