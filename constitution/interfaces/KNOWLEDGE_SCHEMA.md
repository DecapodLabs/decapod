# KNOWLEDGE_SCHEMA.md - Knowledge Interface Contract

**Authority:** interface (machine-readable schema + validation gates)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** knowledge entry schema, lifecycle states, and validation requirements
**Non-goals:** editorial writing guidance

---

## 1. Entry Schema (Required Fields)

Each entry MUST include:
- `id`
- `title`
- `summary`
- `content`
- `tags` (array)
- `status` (`active` | `stale` | `superseded`)
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

## 5. Proof Surface

Minimum checks:
- schema conformance for persisted entries
- status value validity
- timestamp consistency
- provenance presence (`author` + creation time)

Primary gate: `decapod validate`.

---

## Links

- `core/INTERFACES.md` - Interface contracts registry
- `interfaces/STORE_MODEL.md` - Store semantics
- `methodology/KNOWLEDGE.md` - Knowledge practice
- `plugins/KNOWLEDGE.md` - Knowledge subsystem reference
