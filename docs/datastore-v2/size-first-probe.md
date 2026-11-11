# Size-first physical layout experiment

Status: **synthetic design evidence only**. This is not an authorized storage
format change, a cloud schema migration, or a production latency guarantee.

The single-migration cutover is gated on byte-accurate evidence and the current
Decapod / Propodus logical schema contract. The first five-index cleanup in
PR #1605 is insufficient to meet the size-first objective.

## Reproduction

Run `python3 assets/tools/storage_size_probe.py --rows 24000 --reads 500`.
All SQLite databases are disposable private temporary files; this command
opens no live project database. It constructs equivalent logical events in five
physical forms and compares decoded payloads to the original **byte-for-byte**.
The fixture uses a synthetic mixture of todo, broker, traces, and workunit
streams with intentionally repeated proof-shaped metadata. No historic user
data or live secrets enter the process.

The probe uses 4096-byte pages, a packed disposable fixture (VACUUM),
SQLite DBSTAT per-btree sizes and repeated warm-cache indexed reads. It disables
synchronous flushes for fixture construction **only**; no write-latency or durability
claim is made. Compression is ordinary zlib level 6, not a proposed wire codec.
The modeled dictionary replaces a few known strings with integral IDs.

## Baseline observation (24,000 synthetic events)

Captured with SQLite 3.46.1 on a Linux Python environment:

| Physical layout | Packed bytes | Bytes / event | Reduction | Read/decode 64 rows (illustrative) |
| --- | ---: | ---: | ---: | ---: |
| Rowid plus event ID and sequence indexes | 13,025,280 | 542.7 | baseline | 0.078 ms |
| Stream+sequence WITHOUT ROWID | 12,943,360 | 539.3 | 0.6% | 0.054 ms |
| Dictionary for stream/actor/event kind | 12,435,456 | 518.1 | 4.5% | 0.069 ms |
| zlib-compressed payload | 9,658,368 | 402.4 | 25.9% | 0.320 ms |
| Dictionary + zlib | 8,929,280 | 372.1 | 31.4% | 0.335 ms |

These read timings come from a predecessor exploratory fixture rather than
a formal controlled performance experiment. The committed probe now reports
p50 and p95 for each layout on the executing host, but cannot eliminate OS
cache, process variance or workload-selection bias. This benchmark deliberately
exaggerates repeating string values, and may overstate real-world compression.
It does establish that rowid removal alone is unlikely to justify a high-risk
journal migration.

## Engineering decision

**Do not ship any codec or table rewrite on these results alone.**
First classify every current event/proof reader and prove that the physical
encoding can return byte-exact logical payloads through local SQLite and
Propodus Supabase. Existing direct SQL expects event payloads as TEXT; writing
zlib BLOBs into that column would break those readers. Compression requires
a versioned codec seam, reversible data migration, old-client compatibility,
independent verification, and whole-catalog regression proof. PostgreSQL may
already compress suitable large values using TOAST; measure its actual table
and index bytes rather than projecting SQLite ratios.

For workload-dependent optimizations use real authorized historical payload
distributions, multiple fixture sizes and bounded p50/p95/p99 across local
and hosted paths. Measure total disk bytes (including primary and secondary
indexes), write amplification, WAL traffic, memory, and decoding cost; every
claimed gain must be reproducible. Never compress away immutable evidence,
replace unknown outcomes, or trade tenant isolation for space.
