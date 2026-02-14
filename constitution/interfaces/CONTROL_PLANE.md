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
2. Agents must not invent parallel CLIs or parallel state roots.
3. If the command surface is missing, the work is to add the surface, not to bypass it.

This is how you get determinism, auditability, and eventually policy.

---

## 2. The Standard Sequence (Every Meaningful Change)

This is the default sequence when operating in a Decapod-managed repo:

1. Read the contract: `specs/INTENT.md`, `methodology/ARCHITECTURE.md`, then `specs/SYSTEM.md`.
2. Discover proof: identify the smallest proof surface that can falsify success (`decapod validate`, tests, etc.).
3. Use Decapod as the interface: read/write shared state through `decapod ...` commands.
4. Add a repo TODO for multi-step work before implementation (dogfood mode inside this repo).
5. Implement the change.
6. Run proof and report results.
7. Close the TODO and record the event.

If you cannot name the proof surface, you're not ready to claim correctness.

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

Validate coverage matrix (starter; expand over time):

| Claim | Planned/Current Check |
|------|------------------------|
| docs are machine-traceable | Doc Graph Gate (reachability via `## Links`) |
| subsystems don’t drift | Plugins<->CLI Gate (registry matches `decapod --help`) |
| user store is blank-slate | Store: user blank-slate gate |
| repo backlog is reproducible | repo todo rebuild fingerprint gate |

---

## Links

- `core/DECAPOD.md` - Router and navigation charter
- `core/INTERFACES.md` - Interface contracts index
- `interfaces/DOC_RULES.md` - Doc compilation rules
- `interfaces/STORE_MODEL.md` - Store semantics
- `interfaces/CLAIMS.md` - Promises ledger
- `core/PLUGINS.md` - Subsystem registry
- `plugins/MANIFEST.md` - Manifest patterns
- `plugins/TODO.md` - Work tracking
- `methodology/SOUL.md` - Agent identity
- `methodology/ARCHITECTURE.md` - Architecture practice
- `specs/INTENT.md` - Intent contract
- `specs/SYSTEM.md` - System definition
