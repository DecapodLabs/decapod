# PLUGINS.md - Subsystems and Extensibility Routing (Embedded)

**Authority:** guidance (how subsystems surface through the thin waist)
**Layer:** Interfaces
**Binding:** No
**Scope:** the subsystem registry, truth labels, and the plugin-grade contract
**Non-goals:** tutorial workflows or restating authority (see SYSTEM)

This document is embedded within the Decapod binary and serves as an internal reference for how Decapod exposes subsystems to agents and how new extensible functionality can be added without creating split-brain interfaces.

"Plugin" here refers to a first-class subsystem surface that aligns with the control-plane contract, rather than a dynamic loading mechanism.

---

## 0. What "Proof" Means in Decapod

**Proof = executable check.** There is no new DSL or spec language.

A proof is any command that can fail loudly: tests, linters, formatters, policy checks, `decapod validate`, schema validators, or custom scripts. If it exits non-zero, it fails. That's it.

Decapod's role:
- Make proofs machine-invocable (agents run them deterministically)
- Record proof outcomes (append-only event logs)
- Gate promotion on proof results (no REAL without passing proof)

The canonical proof surface today is `decapod validate`. Future epochs will add configurable proof registries.

---

## 0.1 What Decapod Does NOT Do (Concurrency)

Decapod does not "solve" Git. Merge conflicts remain real.

What Decapod can do:
- **Reduce collisions by design:** work-unit partitioning, leases/claims (planned), serialized state writes via control plane (planned)
- **Isolate agent work:** branch/patch flow remains Git-native
- **Gate merges:** proof surfaces can block promotion until checks pass

Do not claim brokered serialization is implemented. It is SPEC (see §4.1).

---

## 1. Plugin Integration Guidelines

For a subsystem to be considered "plugin-grade," it generally aims to provide:

- A stable CLI command group: `decapod <group> <subsystem> ...` (or top-level for core commands)
- Stable output envelopes (prefer `--format json` as a default for agents).
- Store-awareness, indicated by options like `--store user|repo` and `--root <path>`.
- Schema/discovery capabilities: `decapod <subsystem> schema`.
- Proof hooks: the ability for `decapod validate` to detect drift for the subsystem.

If a subsystem does not yet meet these criteria, it may be considered a planned feature rather than a fully integrated plugin.

---

## 2. Routing: How Agents Interact with Subsystems

Agents are encouraged to interact with Decapod as a request router:

1. Determine the relevant store context (user vs repo).
2. Utilize the CLI surface for reads/writes.
3. Use schema to discover fields and constraints.
4. Run validation before confirming correctness.

Agents are generally advised not to open SQLite directly, especially as a broker for this functionality becomes available.

---

## 3. Subsystems (Current, REAL)

### 3.1 Task & Schedule (Operational)
- **todo** ⭐: Backlog management with audit trail. **See: `embedded/plugins/TODO.md`** (CLI: `decapod todo`)
- **cron**: Scheduled jobs and repo-local automation. (CLI: `decapod auto cron`)
- **reflex**: Event-driven automated responses. (CLI: `decapod auto reflex`)

### 3.2 Knowledge & Memory (Intelligence)
- **knowledge**: Rationale, context, and project-specific facts. (CLI: `decapod data knowledge`)
- **teammate** ⭐: User preference memory for persistent behaviors. **See: `embedded/plugins/TEAMMATE.md`** (CLI: `decapod data teammate`)
- **context**: Token budget management and MOVE-not-TRIM archival. (CLI: `decapod data context`)
- **archive**: Immutable session history indexing. (CLI: `decapod data archive`)

### 3.3 Integrity & Governance (Core)
- **health**: Claim/proof ledger and system state monitoring. (CLI: `decapod govern health`)
- **policy**: Risk classification, blast-radius zones, and approvals. (CLI: `decapod govern policy`)
- **trust**: Agent autonomy tiers based on historical proof. **DEPRECATED - now `decapod govern health autonomy`**
- **watcher**: Proactive read-only integrity checks. (CLI: `decapod govern watcher`)

### 3.4 Operational & Meta
- **broker**: The thin-waist for state mutations (serialized SQLite). (CLI: `decapod data broker`)
- **feedback**: Operator preference refinement and non-binding diffs. (CLI: `decapod govern feedback`)
- **heartbeat**: High-level system health and pending action summary. **DEPRECATED - now `decapod govern health summary`**
- **docs**: Methodology discovery and ingestion. (CLI: `decapod docs`)

---

## 3.5 Subsystem Registry (Single Source of Truth)

Any document that needs to refer to subsystems should ideally point here instead of restating lists.

Truth labels:
- `REAL`: implemented and working now
- `STUB`: surface exists, behavior incomplete
- `SPEC`: intended interface; not implemented
- `IDEA`: exploratory; not a commitment
- `DEPRECATED`: do not use

Constraint:
- `REAL` generally requires a named proof surface (use the Proof Surface column).

| Name | Status | Truth | Owner Doc | Store | Mutability | Proof Surface | Safety Gates |
|------|--------|-------|-----------|-------|------------|--------------|--------------|
| todo | implemented | REAL | `embedded/plugins/TODO.md` | user+repo | writes | `decapod todo schema` | store isolation, deterministic rebuild (repo) |
| health | implemented | REAL | `embedded/plugins/HEALTH.md` | both | writes | `decapod govern health get` | deterministic proof hooks |
| policy | implemented | REAL | `embedded/plugins/POLICY.md` | both | writes | `decapod govern policy riskmap verify` | risk gating, trust analysis |
| cron | implemented | REAL | `embedded/plugins/CRON.md` | both | writes | `decapod auto cron schema` | brokered sqlite, audit trail |
| reflex | implemented | REAL | `embedded/plugins/REFLEX.md` | both | writes | `decapod auto reflex schema` | brokered sqlite, audit trail |
| watcher | implemented | REAL | `embedded/plugins/WATCHER.md` | both | reads | `decapod govern watcher run` | audit trail |
| knowledge | implemented | REAL | `embedded/plugins/KNOWLEDGE.md` | both | writes | `decapod data knowledge search` | provenance check |
| teammate | implemented | REAL | `embedded/plugins/TEAMMATE.md` | both | writes | `decapod data teammate schema` | user preference persistence |
| archive | implemented | REAL | `embedded/plugins/ARCHIVE.md` | both | writes | `decapod data archive verify` | hash matching |
| feedback | implemented | REAL | `embedded/plugins/FEEDBACK.md` | both | writes | `decapod govern feedback propose` | non-binding isolation |
| trust | implemented | DEPRECATED | `embedded/plugins/TRUST.md` | both | reads | `decapod govern health autonomy` | merged into health |
| context | implemented | REAL | `embedded/plugins/CONTEXT.md` | both | writes | `decapod data context audit` | budget gating |
| heartbeat | implemented | DEPRECATED | `embedded/plugins/HEARTBEAT.md` | both | reads | `decapod govern health summary` | merged into health |
| docs | implemented | REAL | `embedded/core/DECAPOD.md` | N/A | reads | `decapod docs list` | embedded assets |
| db_broker | planned | SPEC | `embedded/plugins/DB_BROKER.md` | both | both | (not yet enforced) | planned: "no sqlite opens outside broker" |

---

## 4. Subsystems (SPEC — Not Yet Implemented)

### 4.1 DB Broker (SQLite Front Door) — SPEC

**Status: SPEC.** Design exists; implementation does not. Do not claim this is working.

Goal: serialize writes, coalesce reads, and standardize auditing in multi-agent environments.

V1 scope (when implemented):

- serialize writes per database
- in-flight read de-dupe (join identical reads)
- small TTL cache for recent read results
- invalidation on writes
- audit trail for mutations
- enforce "no direct SQLite opens" invariant

---

## 5. Adding a New Subsystem (Minimal Process)

1. Create the command group in Rust: `decapod <subsystem> ...`
2. Define `schema` and JSON envelopes early.
3. Add a repo TODO for the work (dogfood mode).
4. Add validate gates so docs and code cannot drift.
5. Only then write docs that claim the subsystem exists.

---

## Links

- `embedded/plugins/TODO.md` — **TODO subsystem reference (START HERE)**
- `embedded/plugins/TEAMMATE.md` — **Teammate preference memory reference**
- `embedded/core/DECAPOD.md`
- `embedded/core/CONTROL_PLANE.md`
- `embedded/core/DOC_RULES.md`
- `embedded/core/CLAIMS.md`
- `embedded/core/DEPRECATION.md`
- `embedded/core/STORE_MODEL.md`
- `embedded/specs/ARCHITECTURE.md`
- `embedded/specs/INTENT.md`
- `embedded/specs/SYSTEM.md`
