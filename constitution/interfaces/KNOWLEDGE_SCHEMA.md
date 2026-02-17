# KNOWLEDGE_SCHEMA.md - Knowledge Interface Contract

**Authority:** interface (machine-readable schema + validation gates)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** knowledge entry schema, lifecycle states, merge/supersede semantics, and validation requirements
**Non-goals:** editorial writing guidance or hidden automatic cross-store updates

---

## 1. Entry Schema (Required Fields)

Each entry MUST include:
- `id`
- `title`
- `summary`
- `content`
- `tags` (array)
- `status` (`active` | `stale` | `superseded` | `deprecated`)
- `created_ts`
- `updated_ts`
- `author`

---

## 2. Optional Fields

- `links` (files/URLs/PRs)
- `rel_todos`
- `rel_specs`
- `rel_components`
- `confidence` (`high` | `medium` | `low`)
- `expires_ts`
- `merge_key` (stable de-duplication identity)
- `supersedes_id` (entry id replaced by this row)
- `ttl_policy` (`ephemeral` | `decay` | `persistent`)

---

## 3. Storage Contract

Knowledge entries are persisted in `knowledge.db` table `knowledge` with store-scoped fields:
- `id` (TEXT, primary key)
- `title` (TEXT, required)
- `content` (TEXT, required)
- `provenance` (TEXT, required)
- `claim_id` (TEXT, optional)
- `tags` (TEXT, optional serialized list)
- `created_at` (TEXT, required)
- `updated_at` (TEXT, optional)
- `dir_path` (TEXT, required)
- `scope` (TEXT, required)
- `status` (TEXT, required; default `active`)
- `merge_key` (TEXT, optional)
- `supersedes_id` (TEXT, optional)
- `ttl_policy` (TEXT, required; default `persistent`)
- `expires_ts` (TEXT, optional)

Persistence requirements:
1. All writes MUST go through the control plane/brokered interface.
2. Direct manual writes to control-plane state databases are prohibited.
3. `dir_path` and `scope` MUST identify the write context.

---

## 4. Invariants

1. `updated_ts` MUST be >= `created_ts`.
2. `status=superseded` SHOULD reference replacement entry in `links`.
3. Entries using normative terms (`must`, `shall`, `contract`) SHOULD link a spec/interface source.
4. Cross-store auto-seeding is prohibited.

---

## 4. Merge and Supersede Invariants

1. For a given `(merge_key, scope)`, there MUST be at most one `active` entry.
2. Conflict policy MUST be explicit (`merge` | `supersede` | `reject`); no silent default rewrites.
3. `supersede` transitions MUST mark prior entry `superseded` and preserve lineage via `supersedes_id`.
4. `merge` updates MUST preserve entry identity and advance `updated_ts`.

---

## 5. Decay Invariants

1. `ttl_policy=persistent` entries are never auto-decayed.
2. `ttl_policy=ephemeral|decay` entries may transition to `stale` via deterministic decay job.
3. Decay jobs MUST emit append-only audit events with policy + as-of timestamp.

---

## 6. Proof Surface

Minimum checks:
- schema conformance for persisted entries
- status value validity
- timestamp consistency
- provenance presence (`author` + creation time)
- duplicate-active merge_key detection
- supersede lineage integrity
- decay event audit shape checks

Primary gate: `decapod validate`.

---

## Links

- `core/INTERFACES.md` - Interface contracts registry
- `interfaces/STORE_MODEL.md` - Store semantics
- `methodology/KNOWLEDGE.md` - Knowledge practice
- `plugins/KNOWLEDGE.md` - Knowledge subsystem reference
