# REFLEX.md - REFLEX Subsystem (Embedded)

**Authority:** subsystem (REAL)
**Layer:** Operational
**Binding:** No

This document defines the reflex subsystem.

## CLI Surface
- `decapod auto reflex ...`
- `decapod auto reflex run ...`
- `decapod auto reflex add-heartbeat-loop ...`
- `decapod auto reflex add-human-trigger-loop ...`

## Heartbeat Integration

REFLEX supports command-driven autonomy loops where the trigger is human and the action is heartbeat pull:

- Trigger type: `human`
- Action type: `todo.heartbeat.autoclaim`
- Typical action config: `{"agent":"<agent-id>","max_claims":<n>}`
- Action type: `todo.human.trigger.loop`
- Typical action config: `{"agent":"<agent-id>","task_title":"...","priority":"high|medium|low","max_tasks":1}`

This composes with invocation heartbeat to keep agent presence and claim behavior aligned.
