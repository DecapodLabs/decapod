# ARCHITECTURE.md - Architecture Practice

**Authority:** guidance (architectural practice and tradeoff discipline)
**Layer:** Guides
**Binding:** No
**Scope:** architectural thinking, tradeoff evaluation, and design workflow
**Non-goals:** test contracts, interface schemas, and binding system rules

This guide explains how to make architecture decisions in Decapod-managed repos.

---

## 1. The Oracle's Verdict: Architecture as Risk Management

*Architecture is about the important stuff. Whatever that is. The decisions that are hard to change.*

### 1.1 The CTO's Strategic View
- **The "Boring" Mandate:** Innovation tokens should be spent on the product's core value proposition, not the underlying architecture. Use boring, proven technologies (Postgres, boring monoliths, simple queues) until you hit a physical limit.
- **Conway's Law is a Feature:** Do not fight Conway's Law. Design your system architecture to match your desired organizational structure. If you want independent, fast-moving teams, you must build independent, decoupled services.

### 1.2 The SVP's Operational View
- **Operability First:** If an architecture is beautiful on a whiteboard but cannot be debugged at 3 AM by a sleep-deprived engineer, it is a failed architecture. Observability and operability must be designed in from Day 1.
- **The End of "Big Bang" Rewrites:** If an architectural change cannot be done incrementally while the system is running, the change is too risky. Strangle monoliths, dual-write to new databases, but never "stop the world."

### 1.3 The Architect's Structural View
- **Boundaries Over Patterns:** Microservices vs Monolith is the wrong debate. The only thing that matters is clear domain boundaries. A well-modularized monolith is infinitely better than a distributed ball of mud.
- **Design for Deletion:** A good architecture allows you to easily delete code and deprecate services. If removing a feature requires touching 12 different services, your boundaries are wrong.

### 1.4 The Principal's Execution View
- **Documentation is the Architecture:** An architecture that is not documented does not exist. Use ADRs (Architecture Decision Records) to capture the *context* and *why* of a decision. The code only tells you *what* was built.
- **The "YAGNI" Principle:** You Aren't Gonna Need It. Do not build abstractions, generic interfaces, or scaling mechanisms for problems you do not have today. Premature abstraction is the root of all legacy code.

---

## 2. Architecture Mission

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

## 6. Internalized Context Artifact Sequence

```text
Agent
  |
  | decapod internalize create --source doc.md --model base-model --profile noop
  v
Decapod CLI
  |
  | hashes source + resolves profile + writes manifest/adapter
  v
.decapod/generated/artifacts/internalizations/<artifact_id>
  |
  | decapod internalize attach --id <artifact_id> --session <session_id> --tool <tool> --lease-seconds 1800
  v
Session-scoped mount lease
  |
  | inference payload references artifact_id only while lease is active
  v
Inference caller
  |
  | decapod internalize detach --id <artifact_id> --session <session_id>
  v
Lease revoked
```

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
