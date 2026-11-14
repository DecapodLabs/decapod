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

## Recovery not yet provided

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
