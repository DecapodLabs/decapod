# MEMORY_SCHEMA.md - Memory Interface Contract

**Authority:** interface (machine-readable schema + validation gates)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** memory entry schema, lifecycle policy, and retrieval-event tracking
**Non-goals:** narrative guidance on writing style

---

## 1. Entry Schema (Required Fields)

Each memory entry MUST include:
- `id`
- `type` (`task_residue` | `decision_residue` | `heuristic` | `fingerprint` | `external_pointer`)
- `title`
- `summary`
- `tags` (array)
- `links` (array)
- `confidence` (`high` | `medium` | `low`)
- `ttl_policy` (`ephemeral` | `decay` | `persistent`)
- `created_ts`
- `updated_ts`
- `source`

---

## 2. Optional Fields

- `rel_todos`
- `rel_knowledge`
- `rel_specs`
- `rel_proof`
- `expires_ts`

---

## 3. Retrieval Event Schema (Required)

When retrieval events are recorded, each event MUST include:
- `event_id`
- `ts`
- `store` (`user` | `repo`)
- `actor`
- `query`
- `returned_ids`
- `used_ids`
- `outcome` (`helped` | `neutral` | `hurt` | `unknown`)

---

## 4. Invariants

1. `updated_ts` MUST be >= `created_ts`.
2. `ttl_policy=ephemeral` entries SHOULD have expiry handling.
3. `outcome=hurt` retrievals SHOULD create a remediation TODO.
4. Cross-store auto-seeding is prohibited.

---

## 5. Proof Surface

Minimum checks:
- schema conformance for entries and retrieval events
- enum validity
- timestamp consistency
- required metadata presence

Primary gate: `decapod validate`.

---

## Links

- `core/INTERFACES.md` - Interface contracts registry
- `interfaces/STORE_MODEL.md` - Store semantics
- `methodology/MEMORY.md` - Memory practice
- `plugins/CONTEXT.md` - Context subsystem
