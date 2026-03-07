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

The following principles apply at every level of the architecture stack:

- **Spend innovation tokens on the product, not the infrastructure:** Architecture should be as boring as possible below the product differentiation layer. Postgres, simple monoliths, boring queues — use proven technology until you hit a physical constraint. Complexity introduced at the infrastructure layer must be paid for by every engineer who joins after you.
- **Conway's Law is descriptive, not prescriptive — but it is enforced:** Your system architecture will mirror your team communication structure. Design the architecture you want, then organize the team to match it. Fighting Conway's Law is losing. Deliberate alignment with it produces clean, independently deployable boundaries.
- **An architecture that cannot be debugged at 3am is a failed architecture:** Elegance on a whiteboard is not engineering. Observability, operational runbooks, and debuggable failure modes are architectural requirements, not afterthoughts. If a component cannot be reasoned about under pressure, it is not ready for production.
- **Incremental migration is the only safe migration:** Any architectural change that cannot be done while the system remains online is too large. Strangle patterns, dual-write strategies, and feature flags exist to eliminate "big bang" cutover risk. If your change requires a maintenance window, revisit the approach.
- **Domain boundaries matter more than service topology:** The monolith vs microservices debate is a distraction. A well-modularized monolith with clear domain ownership is superior to a distributed system with tangled cross-service data access. Draw the boundaries correctly, then decide whether to deploy them separately.
- **Architecture must be designed for deletion:** If removing a feature requires coordinating a dozen services, the boundaries are wrong. Good architecture allows components to be removed cleanly, which is the truest test of how well they were isolated.
- **Undocumented architecture does not exist:** An architectural decision that lives only in someone's head has a half-life. Capture the context (what the constraints were, what alternatives were rejected, and why) in ADRs. The code tells you what was built; only the documentation tells you why.
- **YAGNI applies to architecture too:** Do not build generic interfaces, extension mechanisms, or multi-tenant scaffolding for problems you do not have. Premature architectural abstraction is how systems accumulate layers of indirection that no one understands.

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
