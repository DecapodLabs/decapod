# Migration Policy

## Rules

- Migrations are forward-only.
- Old data is preserved; destructive rewrite is prohibited.
- Migration operations MUST be explicit and deterministic.
- Migration output MUST be testable with fixtures.

## Current Toy Migration Path

Legacy TODO DB -> event ledger reconstruction is tested via fixtures:

- Input fixture: `tests/fixtures/migration/legacy_tasks.sql`
- Expected deterministic output: `tests/fixtures/migration/expected_todo_events.jsonl`
- Test: `tests/core/core.rs` migration fixture assertions

## Schema Evolution Discipline

- Additive changes are preferred.
- Breaking schema changes require major version bump and migration docs update.
