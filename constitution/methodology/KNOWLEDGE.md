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

### Core Router
- `core/DECAPOD.md` - **Router and navigation charter (START HERE)**

### Authority (Constitution Layer)
- `specs/INTENT.md` - **Methodology contract (READ FIRST)**
- `specs/SYSTEM.md` - System definition and authority doctrine

### Registry (Core Indices)
- `core/PLUGINS.md` - Subsystem registry
- `core/INTERFACES.md` - Interface contracts index
- `core/METHODOLOGY.md` - Methodology guides index

### Contracts (Interfaces Layer)
- `interfaces/KNOWLEDGE_SCHEMA.md` - Binding knowledge schema
- `interfaces/CONTROL_PLANE.md` - Sequencing patterns

### Practice (Methodology Layer - This Document)
- `methodology/SOUL.md` - Agent identity
- `methodology/ARCHITECTURE.md` - Architecture practice
- `methodology/MEMORY.md` - Memory and learning

### Operations (Plugins Layer)
- `plugins/TODO.md` - Work tracking
- `plugins/KNOWLEDGE.md` - Knowledge subsystem

---

## Project Override Context

Project knowledge emphasis:
- Capture patterns that generalize across incidents, not only one-off fixes.
- Promote architectural learnings into shared contracts and docs.
- Track provenance so claims and decisions can be audited.
- Keep knowledge actionable: each entry should inform a concrete next decision.
