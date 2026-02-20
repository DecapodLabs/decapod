# MEMORY_SCHEMA.md - Memory Interface Contract

**Authority:** interface (machine-readable schema + validation gates)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** memory entry schema, lifecycle policy, retrieval-event tracking, and temporal retrieval constraints
**Non-goals:** hosted memory services, always-on daemon requirements, or hidden capture defaults

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
- `as_of_ts` (query-time cutoff for deterministic temporal replay)
- `recency_score` (derived ranking signal, not source-of-truth)

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
- `source` (`invocation` | `manual_feedback`)

Retrieval feedback semantics:
1. Feedback logging is explicit (`retrieval-log`/equivalent command); Decapod does not claim every retrieval is automatically scored.
2. Each feedback submission MUST persist exactly one append-only event.

---

## 4. Storage Contract

Memory entries are stored in store-scoped data surfaces and MUST remain broker-mediated.

Current canonical surfaces:
- `repo` and `user` scoped stores as defined in `interfaces/STORE_MODEL.md`
- retrieval events recorded with actor, query, and outcome metadata

Storage requirements:
1. Writes MUST be scoped (`repo` or `user`) and attributable (`actor`).
2. Retrieval events MUST be append-only audit records once persisted.
3. Cross-store auto-seeding is prohibited.
4. Direct manual writes to store databases/logs are prohibited.
5. Capture may be automatic only after explicit enablement per store; capture MUST remain auditable and user-visible.

---

## 5. Invariants

1. `updated_ts` MUST be >= `created_ts`.
2. `ttl_policy=ephemeral` entries SHOULD have expiry handling.
3. `outcome=hurt` retrievals SHOULD create a remediation TODO.
4. Cross-store auto-seeding is prohibited.
5. Secret-bearing values MUST be redacted or pointerized before persistence.
6. `ttl_policy` enum is strict: `ephemeral` | `decay` | `persistent`.

---

## 6. Temporal Retrieval Invariants

1. `as_of_ts` filtering MUST exclude entries with `created_ts > as_of_ts`.
2. Recency windows (e.g., `window_days`) MUST be deterministic relative to `as_of_ts`.
3. Ranking mode `recency_decay` MUST be derivable from timestamps and declared policy; it must not mutate source entries.

---

## 7. Decay and Prune Event Invariants

When decay/prune runs are recorded, each event MUST include:
- `event_id`
- `ts`
- `policy`
- `as_of`
- `dry_run`
- `stale_ids` (array)

Requirements:
1. Decay must be deterministic for identical `(policy, as_of, store)` inputs.
2. Decay events are append-only and auditable.
3. Decay status transitions MUST be reversible only through explicit follow-up events (no silent deletion).

---

## 8. Proof Surface

Minimum checks:
- schema conformance for entries and retrieval events
- enum validity
- timestamp consistency
- required metadata presence
- as-of exclusion checks for temporal retrieval
- decay event shape checks
- secret-pattern/pointerization checks for persisted memory artifacts

Primary gate: `decapod validate`.

---

## Links

- `core/INTERFACES.md` - Interface contracts registry
- `interfaces/STORE_MODEL.md` - Store semantics
- `methodology/MEMORY.md` - Memory practice
- `plugins/CONTEXT.md` - Context subsystem
