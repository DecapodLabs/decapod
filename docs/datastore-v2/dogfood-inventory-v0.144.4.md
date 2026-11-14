# Native dogfood inventory, Decapod 0.144.4

At 2026-11-14 00:19:23 UTC, the official installed Decapod 0.144.4 inspected
three authorized local SQLite dogfood stores through `data database inspect`.
The compact machine-readable observations are in
[dogfood-inventory-v0.144.4.json](dogfood-inventory-v0.144.4.json).
No migration, repair, vacuum or packed-size comparison was requested.

| Repository scope | Main apparent bytes before / after | Filesystem allocated bytes | SQLite snapshot pages × 4096 | Storage metadata | Checkpoint sample |
| --- | ---: | ---: | ---: | --- | --- |
| Decapod | 156,835,840 / 156,839,936 | 167,817,216 | 38,291 × 4096 | v2, min reader/writer 0.141.0 | 800 rows: 777 v1, 23 v2 with component refs |
| Dactyl | 25,985,024 / 25,985,024 | 26,640,384 | 6,344 × 4096 | unmigrated v1 | 93 v1 rows |
| Propodus local dogfood | 59,387,904 / 59,400,192 | 59,621,376 | 14,502 × 4096 | unmigrated v1 | 173 v1 rows |

Each engine reported SQLite 3.54.0, UTF-8, DELETE journaling, zero snapshot
freelist pages, and TODO schema 18. WAL/SHM/rollback journal sidecars were absent
at both filesystem observations; their sizes remain null, not fabricated zeros.
Filesystem allocation and snapshot page bytes are different measures and must
not be added together. Before/after file growth is visible while concurrent
writers operate; it does not establish a write by the read-only inspector or a
single atomic filesystem snapshot. The database report uses one deferred Dactyl
read transaction; filesystem stats are observed separately around that read.

## Coverage and interpretation

All three overall reports say `coverage=sampled`: at most 10,000 rows per table,
64 MiB scanned, 4 MiB output and five seconds, with no long-read opt-in. The
separate checkpoint prefix fits completely within that bounded scan in each
store. Header/version observations are not checkpoint-chain, digest, body or
migration verification. The primary-key prefix sample is not a random sample of
current activity, and cannot predict ongoing storage-growth slope or a global
adoption percentage.

These are actual store measurements, rather than the earlier maintainer-rounded
file sizes or the disposable v0.141.0 benchmark. They do not measure savings.
`measured_savings_bytes` is null in all three native reports. Dactyl and Propodus
were not silently upgraded to make the measurements look equivalent. Propodus's
local dogfood file is separate from the hosted PostgreSQL service.

The JSON records the measured binary SHA-256, exact schema digests, engine and
format context, invocation budgets, native report SHA-256 and observation time.
Only aggregates are published; private full native reports remain outside Git.
The report hash is an integrity reference, not independent issuer authentication.
A later native inspection can reproduce the method, but concurrent stores may
have different contents and sizes. Exact historical reproduction requires the
corresponding authorized source snapshot, which is not published here.

## Reproduction

Run from each intended repository using the accepted installed binary:

```sh
decapod data database inspect --scope decapod --format json
decapod data database inspect --scope dactyl --format json
decapod data database inspect --scope propodus --format json
```

Each invocation reports the current repository's store; the scope flag is a
non-secret report label, not a database selector. Keep raw operational content,
credentials and machine-specific paths private. Increasing scan duration needs
the explicit supported long-read option and an independently chosen budget.

## Acceptance still requiring proof

The merged v2 representation, field limits, event codec, paged journal verifier
and guarded maintenance have executable source coverage. Historical benchmark
numbers in `compaction-benchmarks-v0.141.0.md` retain their original release and
fixture scope; this inventory does not recertify them on 0.144.4.

Decapod #1630 remains open pending complete acceptance reconciliation, including
current pinned before/after growth workloads, migration/recovery evidence and
cross-backend consumer/authorized hosted proof at the required levels. Closed
Dactyl #101 and Propodus #100 source work is a dependency to audit, not evidence
that these live local files were migrated or that all hosted workload gates ran.
No claim of complete storage-program acceptance follows from this inventory.
