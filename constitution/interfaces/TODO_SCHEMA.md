# TODO_SCHEMA.md - TODO Interface Contract

**Authority:** interface (machine-readable schema + invariants)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** task record fields, event types, and validation invariants
**Non-goals:** backlog prioritization guidance

---

## 1. Task Record (Required Fields)

Each task record MUST include:
- `id`
- `hash`
- `title`
- `status` (`open` | `done` | `archived`)
- `priority` (`low` | `medium` | `high`)
- `scope`
- `created_at`
- `updated_at`

---

## 2. Optional Task Fields

- `description`
- `category`
- `tags`
- `owner`
- `assigned_to`
- `assigned_at`
- `depends_on`
- `blocks`
- `due`
- `parent_task_id`
- `component`
- `ref`

---

## 3. Event Types

Canonical event types:
- `task.add`
- `task.edit`
- `task.done`
- `task.archive`
- `task.comment`
- `task.claim`
- `task.release`

Unknown event types are validation errors.

---

## 4. Invariants

1. `updated_at` MUST be >= `created_at`.
2. `status=done` SHOULD set `completed_at`.
3. `status=archived` SHOULD retain audit trail history.
4. Task IDs MUST be stable and unique.
5. Task IDs MUST use `<type4>_<16-alnum>` format (for example: `docs_a1b2c3d4e5f6g7h8`).
6. `hash` MUST equal the first 6 characters after `<type4>_` in `id`.
7. Event log replay MUST deterministically rebuild current state.

Canonical `type4` values:
`aiml`, `apis`, `appl`, `arch`, `bend`, `bugs`, `cicd`, `code`, `data`, `desn`, `devx`, `docs`, `feat`, `fend`, `lang`, `perf`, `plat`, `proj`, `refa`, `root`, `secu`, `spec`, `test`.

---

## 5. Proof Surface

Primary gate: `decapod validate`.

Expected checks:
- task/event schema conformance
- enum validity
- deterministic rebuild from event log
- audit-trail continuity

---

## Links

- `core/INTERFACES.md` - Interface contracts registry
- `plugins/TODO.md` - TODO subsystem
- `interfaces/CONTROL_PLANE.md` - Sequencing patterns
- `interfaces/STORE_MODEL.md` - Store semantics
