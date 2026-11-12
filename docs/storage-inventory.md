# Native storage inventory

`decapod data database inspect --scope decapod` measures the current repository's
canonical `.decapod/data/decapod.db`. Run it separately in each authorized project;
`scope` is a non-secret label, never a database path or an authorization grant.
Worktrees resolve the same canonical repository store. There is no cross-project
discovery and no automatic upload.

```sh
decapod data database inspect --scope decapod --format json
decapod data database inspect --scope dactyl --format text
decapod rpc --op data.database.inspect --params '{"scope":"propodus","options":{"rows_per_table":1000}}'
```

The JSON report uses `decapod.storage-inventory.v1`. The text form gives the main
file size, table count, coverage and a limitation when present. Missing stores
are reported without creating a database, initializing a session, running
migrations, updating projections or repairing anything. CLI, direct RPC, stdin
RPC and authenticated HTTP's child RPC dispatch inspection before normal startup.
Filesystem access remains controlled by the caller's OS identity. HTTP still
requires its existing transport authentication.

The physical boundary is Dactyl's dedicated read-only snapshot capability. One
deferred transaction observes the SQLite engine, effective schema digest, page
and freelist counts, optional `dbstat` allocation and table data. The schema digest
covers tables, indexes, triggers and views; it never emits their DDL. `dbstat`
physical cell counts are engine measurements, not a portable logical row count.
The existing
bounded lifecycle lock protects file replacement; it may create its ordinary
`.lock` sidecar. Foreground broker custody can temporarily block a maintenance
connection. Close that foreground invocation and retry; inspection never takes
over its connection. SQLite can access its normal WAL/SHM sidecars. This is a
read-only database operation, not a claim of zero filesystem activity.

Main/WAL/SHM/journal/lock apparent and allocated bytes are separate filesystem
observations before and after the transaction. They are not one atomic filesystem
snapshot. Missing or unavailable values are `null`, not zero. Database pages,
B-tree allocation, payload, unused bytes and freelist pages overlap. Do not sum
them, treat freelist space as disposable live data, or infer backup/artifact or
hosted PostgreSQL sizes from them. `dbstat` is optional; the report records its
absence without installing an extension. Virtual tables need a separately
reviewed scope and produce explicit unavailable table entries and partial status.

Every ordinary table has an exact count and a bounded sample in primary-key
order (otherwise rowid). `coverage=sampled` describes that prefix, not a random
population estimate. UTF-8 text, raw BLOB and canonical JSON scalar lengths,
null counts, sample quantiles and five largest lengths contain no field values.
Type-tagged SHA-256 comparisons identify repeated sampled values of at least
128 bytes; hashes never appear in output. At most 1024 distinct comparison
fingerprints per field are retained; overflow is an explicit lower bound.
Potential repeated bytes exclude reference, index and codec overhead and are
not measured savings.

Task checkpoints have a separate primary-key-prefix sample with the same row
ceiling and shared scan/time/VM budgets. This prevents older ordinary event IDs
from hiding the checkpoint stratum. Exact checkpoint/other event counts and
`length(CAST(payload AS BLOB))` totals cover the entire events table in the same
snapshot. Those byte totals follow the engine encoding and exclude cell/index
overhead; they partition events and must not be added to table totals. Stored
event-type strings are reduced to two fixed public labels.

The checkpoint sample additionally reports logical component lengths and
repeated bytes. Structured components use sorted canonical JSON; text uses
decoded UTF-8. Embedded envelope metadata is a sorted projection reported
separately for the event prefix and checkpoint samples. Those classes
overlap stored payload bytes and each other. They must not be added to table
totals. JSON interpretation is limited to 1 MiB per payload with serde's normal
depth bound. Oversized or invalid JSON is counted as unavailable sample
observations. Prefix metadata and targeted checkpoint observations may overlap,
so their unavailable counts are not unique-row counts. Checkpoint
version fields and LCM v1 BLOB framing are observations only: inventory does not
verify task chains, compressed bodies, checksums or migration eligibility. LCM
declared decoded sizes remain explicitly unverified. Only the whitelisted numeric
TODO schema version is emitted from metadata; raw metadata values stay private.

The default sample ceiling is 10,000 rows per table, the conservative decoded
scan budget is 64 MiB, and the pretty JSON report limit is 4 MiB. Limits allow
1..100,000 rows, 1 byte..1 GiB scanned and 4 KiB..16 MiB reported. Snapshot
duration defaults to five seconds; 6..300 seconds requires `--allow-long-read`.
Underlying Dactyl limits also apply: 100 million VM instructions, 4096 queries,
one million total rows, 8 MiB per cell/SQLite record, 128 columns and 64 KiB SQL.
Budgets cover incremental decoding and VM progress, not hard process RSS or
blocking OS I/O deadlines. Binary hashing observes at most a 512 MiB executable
outside the database snapshot budget and is not process attestation.

Budget exhaustion returns partial status with a typed limitation and a nonzero
exit. Report overflow replaces measurements with a small typed failure report;
it never silently truncates a successful report. RPC reports
`success=false,error.code=INVENTORY_INCOMPLETE` for incomplete inventory and
does not persist a receipt in the store. RPC framing now rejects requests over
32 MiB before JSON decoding; HTTP retains its separate body limit. Invalid
inventory options are rejected rather than ignored.

The three real-store reports are the measurement prerequisite for #1630. Keep
raw reports outside Git until reviewed for organization-specific schema names.
Agree numeric reduction and latency/CPU budgets from those baselines before
selecting a compact representation. This command changes no retained data,
implements no compaction or migration, and establishes no local/hosted domain
parity, growth target, reclamation result or capacity entitlement.
