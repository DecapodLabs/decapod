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
- Trigger types include `human`, `cron`, and `health_state`.
- Supported autonomy actions include:
  - `todo.heartbeat.autoclaim`
  - `todo.human.trigger.loop`
  - `todo.health.remediate`
- `todo.human.trigger.loop` composes:
  1. create task
  2. run worker heartbeat loop for the created task
  3. capture lesson/context updates via worker
- `todo.health.remediate` composes:
  1. evaluate all health claims against watched states (STALE, CONTRADICTED)
  2. create a remediation task per degraded claim
  3. assign to the configured agent with health-remediation tags

## Condition-Based Health Triggers
- `health_state` trigger type evaluates health claim states at run time.
- All maintenance is condition-triggered, never time-based.
- Install via: `decapod auto reflex add-health-trigger [--watch-states STALE,CONTRADICTED]`
- Run via: `decapod auto reflex run --trigger-type health_state`
- Condition evaluation: queries `govern health` for all claims, matches against `watch_states` in trigger config.
- When claims match, remediation tasks are created automatically with provenance tags.

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
