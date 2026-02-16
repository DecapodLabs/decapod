# CRON.md - CRON Subsystem (Embedded)

**Authority:** subsystem (REAL)
**Layer:** Operational
**Binding:** No

CRON manages scheduled automation records. It is a planning surface, not a background daemon.
Execution still occurs when an agent invokes Decapod.

## CLI Surface
- `decapod auto cron add --name <n> --schedule "<cron>" --command "<cmd>"`
- `decapod auto cron list [--status <s>] [--scope <scope>] [--tags <csv>]`
- `decapod auto cron get --id <id>`
- `decapod auto cron update --id <id> ...`
- `decapod auto cron delete --id <id>`
- `decapod auto cron suggest [--limit <n>]`
- `decapod data schema --subsystem cron`

## Contracts
- All writes are brokered and audited (`broker.events.jsonl`).
- Timestamps are epoch-seconds + `Z` for deterministic replay.
- `suggest` emits deterministic schedule recommendations from open TODO tasks.
- CRON entries are metadata and intent; they do not bypass policy/trust gates.

## Proof Surfaces
- Storage: `<store-root>/cron.db`
- Audit: `<store-root>/broker.events.jsonl` with `cron.*` ops
- Validation gates:
  - Control Plane Contract Gate
  - Schema Determinism Gate
  - Tooling Validation Gate
