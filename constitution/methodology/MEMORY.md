# MEMORY.md - Agent Memory Practice

**Authority:** guidance (memory hygiene and usage)
**Layer:** Guides
**Binding:** No
**Scope:** how to create, retrieve, and prune memory effectively
**Non-goals:** schema enforcement and machine interface contracts

This guide describes how to keep memory useful without turning it into a second spec.

---

## 1. Purpose

Memory exists to reduce repeated effort and improve decision quality across sessions.

---

## 2. Practical Rules

1. Store pointers and short residue, not essays.
2. Prefer links to TODO/knowledge/proof artifacts.
3. Keep confidence explicit when uncertain.
4. Prune low-value entries regularly.

---

## 3. Retrieval Discipline

1. Retrieve only what is relevant to the active task.
2. Treat low-confidence memory as a hypothesis.
3. Verify before promoting conclusions.

---

## 4. Lifecycle

1. Create from completed work and incidents.
2. Reuse during similar tasks.
3. Consolidate recurring patterns.
4. Expire stale entries according to policy.

---

## 5. Contract Routing

Binding schema, validation rules, and retrieval-event requirements are defined in `interfaces/MEMORY_SCHEMA.md`.

---

## Links

- `core/DECAPOD.md` - Router and navigation charter
- `core/INTERFACES.md` - Interface contracts registry
- `interfaces/MEMORY_SCHEMA.md` - Binding memory schema
- `methodology/KNOWLEDGE.md` - Knowledge practice
- `plugins/TODO.md` - Work tracking
