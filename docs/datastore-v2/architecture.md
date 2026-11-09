# Datastore v2: contract and first increment

Status: incremental implementation and audit, not completion of phases A–E.
Accepted source inspected: Decapod `7035a854`, Dactyl `1fa52a2` / published
`dactyl-db 0.11.1`, Propodus `0bf9e42`. This document distinguishes implemented
behavior, proposed contracts and unmeasured work. `entities.json` is the source
classification companion to the effective SQLite catalog in the measured report.
It explicitly leaves column-level deletion authority unresolved.

## Authority and entity boundaries

Keep one logical contract with independently chosen SQLite/PostgreSQL layouts.
Decapod judges admissibility and completion. Dactyl binds parameters, opens a
physical route, executes transactions and reports typed results. Propodus derives
organization/repository authorization from trusted identity and enforces ordinary
integrity inside a restricted RPC transaction. Local operation has no dependency
on Auth, entitlements or a network. Git continues to own repository artifacts,
living specifications and existing file-based proof encodings.

The following logical entities need independent constraints; table count is not a
design objective:

* Operational task/custody state: stable task ID, monotonic revision, current
  assignment, lease generation/expiry, checkout binding and dependencies. Task
  revision CAS and generation fencing must participate in the state/event
  transaction. Secondary ownership cannot grant primary custody. Category
  exclusivity needs a database-enforced key if retained as exclusive authority;
  scanning JSON arrays in agent rows is not itself a unique constraint.
* Immutable evidence: stable event identity, stream-scoped sequence, schema
  version, subject attribution, actor/provenance and sufficient payload/reference
  to interpret the historical fact. Retain task revision/proof bindings and
  workunit predecessor/generation chains. Existing task checkpoints deliberately
  contain reconstructible projections; replacing these with deltas requires an
  explicit baseline/version and independently tested replay, not payload deletion.
* Operation receipts: request identity, fingerprint and persisted outcome remain
  authoritative while a caller can retry or resolve an uncertain commit. A
  transport error is `UNKNOWN`, not rollback. Never expire an unresolved receipt
  to save space. The existing `request_dedupe` is lazily created by the broker;
  its absence in an in-process initializer fixture is not evidence it is unused.
* Knowledge/configuration: advisory does not mean reconstructible. Custom
  preferences, automation rules, decisions and knowledge can be irreplaceable
  authored input. Presence timestamps and usage counters can have different
  retention from trust grants or ownership on the same row.
* Projections: health caches and LCM indexes declare their source records and
  rebuild procedure. Summary regeneration need not produce identical model
  text. No projection independently proves a governance outcome.

Namespace metadata requires key-level classification before compaction. In
particular, migration versions, task journal baselines, authoring/transfer seals
and arbitrary advisory metadata must not share a blanket cache retention rule.

## Implemented event index revision 2

`events.compact_sequence_index.v002` follows migration sequence 900 at sequence
1000. It is additive to the migration registry and targets version 0.133.1.
`EVENTS_STORAGE_VERSION=2` identifies this physical layout revision; it does not
change any event/proof encoding. Historical `EVENTS_TABLE_SCHEMA` is preserved
byte-for-byte for fixtures and migration consumers. Current initialization uses
`EVENTS_TABLE_SCHEMA_V2`.

The historical plain `(stream,seq)` index duplicates the canonical unique index.
The new migration runs under `BEGIN IMMEDIATE`, verifies the unique replacement's
table, columns, key count, nonpartial definition, collation and sort direction,
and refuses an unexpectedly redefined historical index. An older store without
the unique index receives it additively; duplicate historical sequence values
abort through the original typed storage constraint error. No normalization is
attempted. Only then does it drop
`idx_events_stream_seq`. It never updates a row, renumbers a sequence, changes
an event ID, rewrites JSON or changes a proof hash. Failed preconditions roll
back with `EVENT_INDEX_COMPACTION_UNSAFE` for unexpected definitions, or the
original storage error for execution/constraint failures. Unknown transport
outcomes are never relabeled as validation failures.

Fresh initialization creates only the unique sequence tree and the subject index.
Repeated initialization cannot recreate the plain tree. Older binaries can
recreate it; this wastes space but preserves correctness. An explicitly rerun
idempotent migration removes it again. Do not automatically clear migration
ledgers or delete old indexes by name without checking their structure.

SQLite retains freed pages for reuse. Dropping an index does **not** guarantee a
smaller file. The benchmark separately records live btree bytes, freelist pages,
file bytes after DROP and file bytes after fixture-only VACUUM. No automatic
vacuum, page-size, journal-mode or synchronous change is introduced. Real
maintenance must use the existing lifecycle-lock/backup/integrity boundary; the
benchmark is not an operator command for rewriting a live database.

The unique sequence index serves both latest-event reads and indexed `MAX(seq)`.
This increment removes one btree insertion per new event, not an application SQL
statement or an audit record. Idempotent replay, payload bytes and RPC counts are
unchanged. A transactional counter is not justified merely because the SQL says
`MAX`: measure the indexed plan before introducing another write/hot row.

## Cloud differences and compatibility gaps

Propodus's canonical native backend is `internal/backend/schema/001` through
`005`, applied by `internal/backend/migrate.go` under a checksum ledger and
transactional advisory migration lock. The older `migrations/001..010` and
`pkg/dactyl/canonical_schema.go` describe another compatibility surface; matching
that broad catalog is not native RPC capability evidence.

Native task rows have tenant keys and a `version` counter. They omit local lease
generation, intent anchor, checkout bindings, dependency/verification semantics
and much of the local task model. Native events have a task foreign key and
nonnullable subjects, so general workunit, assurance, federation and broker
streams are not equivalent. Native Auth sessions are distinct from local
decision-session rows with the same table name. Entitlements are server-only.
The historical quota-meter schema is not proof that the native boundary enforces
equivalent quotas. These capabilities require explicit versioned expansion.

The v1 SQL profile is an exact allowlist with parameter binding, forced tenant
RLS, separate API/executor roles, Auth/session verification and entitlements.
Writes are atomic state/event pairs and use a repository-scoped advisory lock;
unrelated repositories have no intentional shared sequencing lock. Results are
limited to 1,000 rows / 4 MiB and requests to 1 MiB / 32 operations. These bounds
reject oversized lists; they do not provide keyset pagination. Never add local
authority fallback when a cloud capability is missing.

The native unique `(org_id,repo_id,stream,seq)` constraint already supplies the
sequence tree. Do not apply this SQLite DROP migration to native PostgreSQL or
claim equivalent PostgreSQL savings. The historical PostgreSQL compatibility
schema has its own redundant-index candidates and needs separate measurements.
No Dactyl or Propodus implementation change is needed for this index increment.

## Bounds, compression and retention

No input domain is silently narrowed in this increment. The measured catalog
records actual types, nullability, defaults, constraints and synthetic value byte
sizes. Synthetic maximum lengths are not valid production allowances. Before
introducing title/path/ID/payload bounds, measure supported inputs and migration
fixtures, choose semantic byte/character rules, version them, and enforce typed
Rust errors plus the trusted PostgreSQL boundary. Preserve existing oversized
historical records without lossy rewrite. PostgreSQL TEXT versus VARCHAR alone
does not compact a value; use byte checks where physical limits matter.

Do not change lifecycle strings to integers, hashes to BLOBs, JSON encodings,
timestamps or canonical proof hashes without separate interoperability and
golden-vector evidence. Compression and content addressing remain experiments;
no savings are claimed. Large immutable artifacts can remain in Git or existing
artifact storage, referenced by digest, only when that reference supplies the
same recovery/proof guarantee as the replaced body.

Operational authority is retained until an explicit governed lifecycle retires
it. Evidence needed for committed claims is retained while those claims remain
asserted. Deletion requires authorized, recorded action and a declared effect
on historical proof. Rebuildable projections may have bounded retention after
their sources and rebuild contracts are proven. Quotas must reject an entire
state/event mutation atomically; neither partial writes nor unrecorded evidence
loss is an acceptable cost control.

## Dependency and delivery sequence

1. This SQLite index increment: effective catalog, reproducible event fixture,
   forward migration, preservation/rollback/64-writer tests and measured report.
   Completion requires exact-candidate repository validation before PR creation.
2. Independent aptitude seed correction: initialization currently generates new
   IDs for default prompts on every call, so `INSERT OR IGNORE` cannot suppress
   duplicates. Measure repeated initialization, preserve custom/disabled/usage
   state and eliminate future duplicate writes. Do not silently delete history.
3. Dactyl targeted catalog capability: generic `inspect_schema` counts table rows;
   Decapod `has_table`/`has_column` repeatedly calls it. Measure statement counts,
   cold invocation and populated stores, add a generic bounded catalog API in
   Dactyl, publish it, then consume that released version in Decapod. No domain
   policy belongs in Dactyl. This depends on its own compatibility PR/release.
4. Finish phase A: per-column correctness/reconstruction and all namespace/RPC
   traces, allocation/peak-memory metrics, real historical fixtures, complete
   task/knowledge/proof/mixed workloads and disposable PostgreSQL catalogs/plans.
   Existing broad migration tests supplement, not replace, those fixtures.
5. Versioned bounded cloud paging/projections and field validation in Decapod
   plus Propodus. Preserve v1 client SQL until explicit migration. Measure wire
   bytes through actual Dactyl/PostgREST, including retry and UNKNOWN behavior.
6. Structural ownership/category/dependency and event/receipt experiments only
   after the baseline selects a justified representation. Require 32–64 competing
   claimants, stale fencing, failure injection and tenant-isolation evidence.
7. Hosted Supabase proof with explicit operator authorization: deployment,
   entitlement/Auth integration, published-client compatibility and hosted load
   are separate from disposable PostgreSQL and local SQLite proof.

No hosted access was used or production credentials requested. Unexecuted profiles
remain unavailable/pending. The 30% storage and 20% cloud transport objectives
remain targets for later measured increments, not results of removing one index.

## Sources

Source paths and effective fixture data are the implementation evidence.
Physical interpretation follows SQLite's [query planner](https://sqlite.org/queryplanner.html),
[DBSTAT](https://sqlite.org/dbstat.html) and [VACUUM](https://sqlite.org/lang_vacuum.html)
documentation. Current economic assumptions are separately recorded in
`capacity.md`; they are not throughput guarantees.
