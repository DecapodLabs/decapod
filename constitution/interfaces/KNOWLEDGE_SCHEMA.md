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

## 3. Invariants

1. `updated_ts` MUST be >= `created_ts`.
2. `status=superseded` SHOULD reference replacement entry in `links`.
3. Entries using normative terms (`must`, `shall`, `contract`) SHOULD link a spec/interface source.
4. Cross-store auto-seeding is prohibited.

---

## 4. Proof Surface

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
