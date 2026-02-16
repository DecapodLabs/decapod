# ARCHITECTURE.md - Architecture Practice

**Authority:** guidance (architectural practice and tradeoff discipline)
**Layer:** Guides
**Binding:** No
**Scope:** architectural thinking, tradeoff evaluation, and design workflow
**Non-goals:** test contracts, interface schemas, and binding system rules

This guide explains how to make architecture decisions in Decapod-managed repos.

---

## 1. Architecture Mission

Architecture exists to improve delivery outcomes:
- velocity
- reliability
- maintainability
- operability
- cost efficiency

If a design adds complexity without improving outcomes, reject it.

---

## 2. Practice Principles

1. Prefer boring defaults until evidence demands complexity.
2. Make tradeoffs explicit (what you gain, what you pay).
3. Keep boundaries legible (ownership, inputs, outputs, failure modes).
4. Optimize for debuggability over cleverness.
5. Design for migration, not permanence.

---

## 3. Decision Workflow

1. State the intent impact.
2. Identify constraints (scale, latency, reliability, security, cost).
3. Compare at least two viable options.
4. Record tradeoffs and selected default.
5. Define proof strategy and rollback path.

---

## 4. Domain Maps

Use `constitution/architecture/*` documents as deeper references for domain-specific concerns:
- data
- caching
- memory
- web
- cloud
- frontend
- algorithms
- security
- observability
- concurrency

---

## 5. Layer Boundaries

This file is guidance-only.

Binding test/proof contracts live in `interfaces/TESTING.md`.
Binding machine-surface contracts live in `core/INTERFACES.md` and the `interfaces/` registry.
Binding system rules live in `specs/SYSTEM.md` and `specs/INTENT.md`.

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
- `interfaces/TESTING.md` - Testing contract
- `interfaces/CONTROL_PLANE.md` - Sequencing patterns
- `interfaces/GLOSSARY.md` - Term definitions

### Practice (Methodology Layer - This Document)
- `methodology/SOUL.md` - Agent identity
- `methodology/KNOWLEDGE.md` - Knowledge curation
- `methodology/MEMORY.md` - Memory and learning

### Operations (Plugins Layer)
- `plugins/TODO.md` - Work tracking
- `plugins/VERIFY.md` - Validation subsystem

---

## Project Override Context

Project architecture emphasis:
- Organize by responsibility domains (agent loop, channels, tools, storage, orchestration).
- Keep service-specific logic at the edge; preserve a reusable core.
- Use interface contracts and state transitions to reduce hidden coupling.
- Prefer evolvable extension points over one-off feature branches in core flow.
