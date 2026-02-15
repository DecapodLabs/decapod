# PLUGINS.md - Subsystem Registry

**Authority:** interface (subsystem truth registry)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** canonical list of subsystem surfaces, status, truth labels, and deprecation routing
**Non-goals:** tutorial workflows and architecture doctrine

This is the single source of truth for Decapod subsystem status.

---

## 1. Truth Labels

- `REAL`: implemented and supported
- `STUB`: interface exists, behavior incomplete
- `SPEC`: designed contract, not implemented
- `IDEA`: exploratory only
- `DEPRECATED`: superseded; do not target for new work

`REAL` entries must name an executable proof surface.

---

## 2. Subsystem Registry

| Name | CLI Surface | Status | Truth | Owner Doc | Proof Surface |
|------|-------------|--------|-------|-----------|---------------|
| todo | `decapod todo` | implemented | REAL | `plugins/TODO.md` | `decapod todo schema` |
| docs | `decapod docs` | implemented | REAL | `core/DECAPOD.md` | `decapod docs list` |
| validate | `decapod validate` | implemented | REAL | `plugins/VERIFY.md` | `decapod validate` |
| health | `decapod govern health` | implemented | REAL | `plugins/HEALTH.md` | `decapod govern health get` |
| policy | `decapod govern policy` | implemented | REAL | `plugins/POLICY.md` | `decapod govern policy riskmap verify` |
| watcher | `decapod govern watcher` | implemented | REAL | `plugins/WATCHER.md` | `decapod govern watcher run` |
| feedback | `decapod govern feedback` | implemented | REAL | `plugins/FEEDBACK.md` | `decapod govern feedback propose` |
| knowledge | `decapod data knowledge` | implemented | REAL | `plugins/KNOWLEDGE.md` | `decapod data knowledge search` |
| teammate | `decapod data teammate` | implemented | REAL | `plugins/TEAMMATE.md` | `decapod data teammate schema` |
| context | `decapod data context` | implemented | REAL | `plugins/CONTEXT.md` | `decapod data context audit` |
| archive | `decapod data archive` | implemented | REAL | `plugins/ARCHIVE.md` | `decapod data archive verify` |
| cron | `decapod auto cron` | implemented | REAL | `plugins/CRON.md` | `decapod auto cron schema` |
| reflex | `decapod auto reflex` | implemented | REAL | `plugins/REFLEX.md` | `decapod auto reflex schema` |
| db_broker | `decapod data broker` | planned | SPEC | `plugins/DB_BROKER.md` | not yet enforced |
| heartbeat | `decapod heartbeat` | removed | DEPRECATED | `plugins/HEARTBEAT.md` | replacement: `decapod govern health summary` |
| trust | `decapod trust` | removed | DEPRECATED | `plugins/TRUST.md` | replacement: `decapod govern health autonomy` |

---

## 3. Deprecation Routing

- `heartbeat` is replaced by `govern health summary`.
- `trust` is replaced by `govern health autonomy`.

Documentation should point to replacement surfaces, not deprecated command groups.

---

## 4. Registry Discipline

1. If a subsystem is not listed here, it is not canonical.
2. Other docs may reference subsystems but should not define competing lists.
3. Status changes must update this registry and corresponding owner docs together.

---

## Links

- `core/DECAPOD.md` - Router and navigation charter
- `core/INTERFACES.md` - Interface contracts registry
- `interfaces/CONTROL_PLANE.md` - Sequencing patterns
- `plugins/TODO.md` - TODO subsystem
- `plugins/VERIFY.md` - Validation subsystem
- `plugins/DB_BROKER.md` - Broker SPEC
