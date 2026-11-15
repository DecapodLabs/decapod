# Legacy TODO input and canonical journal authority

The current repair distinguishes one provably inert input from unknown SQLite
state. A zero-byte regular `todo.db` with no WAL, SHM or rollback-journal sidecar
contains no SQLite task authority. Preserve it without opening SQLite, importing
rows or removing the file. A symlink, nonregular object, any sidecar, nonzero
content or uncertain metadata is not classified as inert.

`migration::legacy_database_artifact_exists`,
`checkpoint_legacy_databases`, `migrate_reconstruct_todo_events` and
`migrate_consolidate_single_datastore` share that classification. Consolidation
captures the inert decision once and excludes that source from both import and
cleanup for the entire invocation. If another old writer subsequently populates
the placeholder, this invocation still does not import or remove it. A later
operation sees the changed non-inert input and must re-evaluate it.

Every other TODO artifact beside canonical task-journal history remains blocked
by `TASK_JOURNAL_MIGRATION_REQUIRED` before implicit source import or removal.
An initialized but apparently empty SQLite file is not equivalent to a zero-byte
regular placeholder. Unknown or old policy, metadata and sidecars are not thrown
away merely because selected task tables appear empty.

The migration regressions create a real task baseline and verify its canonical
journal before and after repeated startup reconciliation. They cover unrelated
legacy reconciliation, preserved empty bytes, corrupt/nonzero input, all three
sidecars and symlink uncertainty. These are store-boundary tests; hosted execution
and final publication readiness remain pending.

## General logical recovery remains unresolved

Decapod #1657 still requires an explicit versioned, idempotent recovery operation
for valid nonempty legacy stores beside canonical history. Equivalent historical
rows, divergent state, malformed stores and interrupted recovery need independent
classification and source-bound receipts under accepted repair authority. The
narrow placeholder repair does not implement or silently authorize that cutover.
Repository owner repair remains separately coordinated with #1590.

Use supported read-only inventory and task-journal inspection to preserve the
actual state and blocker. Do not manually delete a source, fabricate a historical
checkpoint, import unproven completion, or replay a task mutation whose broker
outcome is unknown. Restored ability to enter a workspace does not prove the
outcome of a previously interrupted command.

## Explicit versioned physical recovery

`decapod todo journal reconcile-legacy` reviews a quiesced local store without
normal startup mutations. V1 accepts only zero-byte/no-schema input or a
byte-identical complete canonical datastore copy. The canonical task journal
must verify completely within the bounded review. Nonmatching nonempty layouts,
uncertain integrity and divergent historical authority remain blocked; no task
rows are merged, imported or declared complete.

A repository maintainer must explicitly grant `todo.legacy.reconcile` using
`governance.rbac.legacy_todo_reconcile_roles` in the canonical main repository
config. Existing journal-repair and maintenance grants do not authorize it.
Review returns `review_sha256`; inspect its classification, digests and scope
before applying that exact token with an active local session:

```sh
decapod todo journal reconcile-legacy
decapod todo journal reconcile-legacy --expected-review sha256:<review-digest>
```

`--prepare-only` with the same token durably records preparation without retiring
the source. Ordinary startup remains blocked by `LEGACY_TODO_RECOVERY_PENDING`
until explicit resume. Repeating the apply command rechecks the source (or its
preserved archive), canonical identity/content, policy and journal. Completed
retries inspect the historical archive; they do not validate current task state.
A reappeared source is never covered by a previous committed operation.

The bounded `legacy-todo-recovery-v1.json` tracks one physical retirement, with
`todo.db.legacy-recovery-v1.archive` preserving original bytes and inode. The
canonical database is never mutated. An interrupted change to canonical bytes or
policy blocks resume; preserve both inputs and the receipt for investigation.
There is no supported automatic cancellation/supersession in V1. Never manually
mark preparation committed or delete sidecars to force recovery.

The operation holds cooperating storage locks, rejects symlinks and unknown hard
links, uses bounded snapshots and checks byte digests rather than timestamps.
Source snapshots are capped at 64 MiB, canonical snapshots at 512 MiB, receipt
reads at 16 KiB and each snapshot pass at 30 seconds. Canonical journal review
is capped at 10,000 task/checkpoint rows each. Evidence is a small operation
record plus one preserved original file; temporary review copies are removed.

Every result leaves publication and completion unproven. SHA-256 does not
provide authenticated independent evidence against an unrestricted same-UID
producer. Cloud authority never falls back to this local operation. General
logical reconciliation in #1657 and repository owner repair in #1590 remain
separate unresolved requirements.
