# CONTROL_PLANE.md - Agent<->Decapod Control Plane Patterns (Repo-Specific)

**Authority:** patterns (interoperability and sequencing; not a project contract)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** sequencing and interoperability patterns between agents and the Decapod CLI
**Non-goals:** subsystem inventories (see PLUGINS registry) or authority definitions (see SYSTEM)

This document is about *how* agents should use Decapod as a local control plane: sequencing, patterns, and interoperability rules.

It is intentionally higher-level than subsystem docs. It exists to prevent "agents poking files and DBs" from becoming the de facto interface.

General methodology lives in `specs/INTENT.md` and `methodology/ARCHITECTURE.md`.

---

## 1. The Contract: Agents Talk to Decapod, Not the Internals

The control plane exists to make multi-agent behavior converge.

Golden rules:

1. Agents must not directly manipulate shared state (databases, state files) if a Decapod command exists for it.
2. Agents must not read or write `<repo>/.decapod/*` files directly; access is only through `decapod` CLI surfaces.
3. Agents must not invent parallel CLIs or parallel state roots.
4. Agents must claim a TODO (`decapod todo claim --id <task-id>`) before substantive implementation work on that task (claim: `claim.todo.claim_before_work`).
5. If the command surface is missing, the work is to add the surface, not to bypass it.
6. Preserve control-plane opacity at the operator interface: communicate intent/actions/outcomes, not command-surface mechanics, unless diagnostics are explicitly requested.
7. Liveness must be maintained through invocation heartbeat: each Decapod command invocation should refresh agent presence.
8. Session access must be bound to agent identity plus ephemeral password (`DECAPOD_AGENT_ID` + `DECAPOD_SESSION_PASSWORD`) for command authorization (claim: `claim.session.agent_password_required`).
9. Control-plane operations MUST remain daemonless and local-first; no required always-on coordinator may become a hidden dependency.
10. No single session may hold datastore locks across user turns; lock scope must stay within a bounded command invocation.

This is how you get determinism, auditability, and eventually policy.

---

## 2. The Standard Sequence (Every Meaningful Change)

This is the default sequence when operating in a Decapod-managed repo:

1. Read the contract: constitution `specs/INTENT.md`, `methodology/ARCHITECTURE.md`, `specs/SYSTEM.md`, then local project specs `.decapod/generated/specs/INTENT.md`, `.decapod/generated/specs/ARCHITECTURE.md`, `.decapod/generated/specs/INTERFACES.md`, `.decapod/generated/specs/VALIDATION.md`.
2. Discover proof: identify the smallest proof surface that can falsify success (`decapod validate`, tests, etc.).
3. Use Decapod as the interface: read/write shared state through `decapod ...` commands.
4. Add a repo TODO for multi-step work before implementation (dogfood mode inside this repo).
5. Claim the task before implementation (`decapod todo claim --id <task-id>`).
6. Implement the change.
7. Run proof and report results.
8. Close the TODO with the explicit command `decapod todo done --id <task-id>` and record the event.

If you cannot name the proof surface, you're not ready to claim correctness.

### 2.1 Invocation Checkpoints (Required)

For every meaningful task, agents MUST call Decapod at three checkpoints:

1. **Before plan commitment**: initialize/resolve context (`decapod rpc --op agent.init`, `decapod rpc --op context.resolve`).
2. **Before mutation**: claim work and ensure canonical workspace (`decapod todo claim`, `decapod workspace ensure`).
3. **After mutation**: run proof surfaces (`decapod validate` plus required tests) before completion claims.

Skipping a checkpoint invalidates completion claims.

---

## 3. Interoperability: The Thin Waist

Decapod is a thin waist only if subsystems share the same interface qualities.

Subsystem requirements (agent-visible):

- Stable command group (`decapod <subsystem> ...`)
- Stable JSON envelope (`--format json` or equivalent)
- Store-aware behavior (`--store user|repo` plus `--root <path>` escape hatch)
- Schema/discovery surface (`decapod <subsystem> schema`)

Cross-cutting requirements:

- one place to validate repo invariants (`decapod validate`)
- one place to discover what exists (schema/discovery, doc map)
- one place to manage entrypoints to agents (link subsystem, planned)

If a subsystem cannot meet these, it is not a control-plane subsystem yet. Treat it as planned.

---

## 3.6 Invocation Heartbeat

Decapod uses invocation heartbeat for agent presence:

- Decapod auto-clocks liveness on normal command invocation.
- Explicit `decapod todo heartbeat` remains available for forced/manual heartbeat and optional autoclaim.
- Control-plane checks must detect regressions where heartbeat decoration is removed.

This keeps liveness aligned with actual command-driven activity without requiring a daemon process.

---

## 3.5 Subsystem Truth (No Phantom Features)

Subsystem status is defined only in the subsystem registry:

- `core/PLUGINS.md` (§3.5)

Other docs must not restate subsystem lists. They must route to the registry.

---

## 4. Stores: How Multi-Agent Work Stays Sane

Decapod supports multiple stores. The store is part of the request context.

Rules:

- default store is the user store
- repo dogfooding must be explicit (`--store repo`), or narrowly auto-detected via sentinel in this repo
- the store boundary is a hard boundary: no auto-seeding from repo to user

If an agent is confused about which store it is operating on, it must stop and ask.

---

## 5. Concurrency Pattern: Request, Don’t Poke

SQLite is fast and simple until there are multiple writers and long-lived reads across multiple agents.

The desired pattern is:

Agents -> Decapod request surface -> serialized mutations + coalesced reads -> shared state

Scope discipline:

- start local-first and boring (in-process broker)
- do not build a distributed system inside a local tool
- prove value by solving two concrete problems first:
  - serialized writes
  - in-flight read de-duplication

The win is the protocol: once all access goes through one request layer, you can add tracing, priorities, idempotency keys, and audit trails without rewriting the world.

### 5.1 Ambiguity and Capability Boundaries

1. If intent is ambiguous or policy boundaries conflict, agents MUST stop and ask for clarification before irreversible implementation.
2. Agents MUST NOT claim capabilities absent from the command surface; missing capability is a gap to report, not permission to improvise hidden behavior.
3. Lock/contention failures (`VALIDATE_TIMEOUT_OR_LOCK` and related typed failures) are blocking failures until explicitly resolved or retried successfully.

---

## 6. Validate Doctrine (Proof Currency)

Agents should treat proof as the control plane's currency:

- if validation exists, run it
- if validation doesn't exist, add the smallest validation gate that prevents drift
- if something is claimed in docs, validation should be able to detect it

This is how the repo avoids "doc reality" diverging from "code reality."

Validate taxonomy (intended):

- structural: directory rules, template buckets, namespace purge
- store: blank-slate user store, repo dogfood invariants
- interfaces: schema presence, output envelopes
- provenance: audit trails (planned)
- docs: doc graph reachability, subsystem registry consistency

Severity levels:

- error: fails validation (blocks claims)
- warn: allowed but noisy
- info: telemetry

### 6.1 Locking and Liveness Contract

Validation and promotion-critical checks must preserve control-plane liveness:

1. `decapod validate` MUST terminate boundedly (success or typed failure).
2. Lock/contention failures MUST return structured, machine-readable error markers (`VALIDATE_TIMEOUT_OR_LOCK` family), never silent hangs.
3. Transactions in validation paths MUST be short-lived and scoped to a single invocation.
4. Promotion-relevant commands MUST treat typed timeout/lock failures as blocking failures by default.

Validate coverage matrix (starter; expand over time):

| Claim | Planned/Current Check |
|------|------------------------|
| docs are machine-traceable | Doc Graph Gate (reachability via `## Links`) |
| subsystems don’t drift | Plugins<->CLI Gate (registry matches `decapod --help`) |
| user store is blank-slate | Store: user blank-slate gate |
| repo backlog is reproducible | repo todo rebuild fingerprint gate |

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

### Contracts (Interfaces Layer - This Document)
- `interfaces/DOC_RULES.md` - Doc compilation rules
- `interfaces/STORE_MODEL.md` - Store semantics
- `interfaces/CLAIMS.md` - Promises ledger
- `interfaces/GLOSSARY.md` - Term definitions
- `interfaces/TESTING.md` - Testing contract

### Practice (Methodology Layer)
- `methodology/SOUL.md` - Agent identity
- `methodology/ARCHITECTURE.md` - Architecture practice

### Operations (Plugins Layer)
- `plugins/TODO.md` - Work tracking
- `plugins/MANIFEST.md` - Manifest patterns
- `plugins/VERIFY.md` - Validation subsystem
