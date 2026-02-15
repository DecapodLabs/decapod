# KNOWLEDGE.md - Knowledge Management Practice

**Authority:** guidance (how to curate and use knowledge)
**Layer:** Guides
**Binding:** No
**Scope:** capture discipline, curation workflow, and lifecycle hygiene
**Non-goals:** schema contracts and CLI interface definitions

This guide covers practical knowledge curation.

---

## 1. Purpose

Use knowledge entries to preserve context that improves future execution:
- rationale behind decisions
- reusable investigations
- runbooks and external references

---

## 2. Curation Rules

1. Prefer concise summaries with links to evidence.
2. Tag entries for discoverability.
3. Mark stale or superseded entries quickly.
4. Link actionable items to TODO IDs.

---

## 3. Boundaries

Knowledge is context, not contract.

- Contracts belong in `specs/` and `interfaces/`.
- Executable evidence belongs in proofs/tests.
- Knowledge should explain, not redefine authoritative behavior.

---

## 4. Lifecycle

1. Capture: record new learnings from non-trivial work.
2. Curate: tighten wording and link related artifacts.
3. Consolidate: merge duplicates and promote durable patterns.
4. Retire: mark stale/superseded entries.

---

## 5. Contract Routing

Binding entry schema and validation requirements are defined in `interfaces/KNOWLEDGE_SCHEMA.md`.

---

## Links

- `core/DECAPOD.md` - Router and navigation charter
- `core/INTERFACES.md` - Interface contracts registry
- `interfaces/KNOWLEDGE_SCHEMA.md` - Binding knowledge schema
- `methodology/MEMORY.md` - Memory practice
- `plugins/TODO.md` - Work tracking
