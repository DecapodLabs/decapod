# REFLEX.md - REFLEX Subsystem (Embedded)

**Authority:** subsystem (REAL)
**Layer:** Operational
**Binding:** No

REFLEX defines trigger->action automations that execute when agents invoke Decapod commands.

## CLI Surface
- `decapod auto reflex add ...`
- `decapod auto reflex update --id <id> ...`
- `decapod auto reflex get --id <id>`
- `decapod auto reflex list ...`
- `decapod auto reflex run [--limit <n>] [--trigger <type>] [--scope <scope>]`
- `decapod auto reflex delete --id <id>`
- `decapod auto reflex add-heartbeat-loop --name <n> --agent <id> [--max-claims <n>]`
- `decapod auto reflex add-human-trigger-loop --name <n> --agent <id> --task-title <title> ...`
- `decapod data schema --subsystem reflex`

## Trigger and Action Contracts
- Trigger types include `human` and `cron`.
- Supported autonomy actions include:
  - `todo.heartbeat.autoclaim`
  - `todo.human.trigger.loop`
- `todo.human.trigger.loop` composes:
  1. create task
  2. run worker heartbeat loop for the created task
  3. capture lesson/context updates via worker

## Heartbeat Contract
- Invocation heartbeat is automatic at top-level command dispatch.
- Explicit `todo heartbeat` remains available and is excluded from duplicate auto clock-in.
- Reflex actions rely on this liveness model; Decapod is not a resident process.

## Proof Surfaces
- Storage: `<store-root>/reflex.db`
- Audit: `<store-root>/broker.events.jsonl` with `reflex.*` and downstream action ops
- Validation gates:
  - Heartbeat Invocation Gate
  - Control Plane Contract Gate
