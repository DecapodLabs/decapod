# ARCHITECTURE.md - Architecture Practice (General)

**Authority:** binding (general architecture practice; not project-specific)
**Layer:** Guides
**Binding:** No
**Scope:** architecture practice guidance for intent-driven systems
**Non-goals:** authoritative requirements (defer to INTENT/SYSTEM) or subsystem registries

This file defines how to *do architecture* in an intent-driven codebase: how architectural truth is compiled, recorded, reviewed, and kept in sync with running systems.

It is intentionally not a diagram dump and not a project-specific inventory. It is a process and quality contract.

If this file conflicts with `embedded/specs/INTENT.md`, intent wins.

---

## 1. What Architecture Is (In This System)

Architecture is the smallest set of durable decisions that:

- constrain how the system can evolve without breaking promises
- define interfaces and boundaries (what talks to what, and how)
- define state and invariants (what must never become inconsistent)
- define operational reality (how it runs, fails, is observed, and is recovered)

Architecture is a compiled artifact: it must not invent requirements that are not in intent.

---

## 2. Required Outputs of Architecture Work

An architecture change is not "done" until the repo contains:

- updated interface contracts (schemas, CLI envelopes, protocol docs, etc.)
- an updated system map (diagram or adjacency list) that matches reality
- a decision record for irreversible choices (ADRs or equivalent)
- proof updates (tests/validate hooks) that make drift detectable

If you cannot create these outputs, you do not understand the change well enough to ship it safely.

---

## 3. The Architecture Update Protocol (Agent Loop)

When asked to change the system:

1. Re-state the intent impact (which promises/invariants change).
2. Update the system map first (boundaries, flows, state).
3. Identify irreversible decisions and record them (ADR).
4. Define the proof surface that will fail if the architecture is wrong.
5. Only then implement.

If implementation is already present and docs drifted, enter explicit drift recovery (see INTENT.md).

---

## 4. How to Document Architecture (Minimal, Not Performative)

Prefer a consistent set of sections. Keep each section short; include only load-bearing facts.

Minimum section set for a concrete architecture doc in a real repo:

- **System Boundary:** what is in scope vs out of scope.
- **Components:** major components and their responsibilities.
- **Interfaces:** inputs/outputs per component (schemas, commands, events).
- **State Model:** authoritative state, schema versioning, migrations.
- **Concurrency Model:** writers/readers, queues, serialization points.
- **Failure Modes:** what breaks, how it degrades, recovery steps.
- **Observability:** logs/metrics/traces and where to look first.
- **Security & Compliance:** secrets, access control, audit trail expectations.
- **Proof Surface:** what to run, what it proves, and the pass criteria.

You can add sections, but you cannot omit these without replacing them with something that carries the same truth.

---

## 5. Decision Records (ADRs)

Any decision that is hard to reverse must be recorded as a decision record:

- what choice was made
- why it was made (constraints and trade-offs)
- what alternatives were considered
- what breaks if we change it later
- what proof validates the decision in practice

The ADR index must be easy to find and kept current.

---

## 6. System Maps (Diagrams With Teeth)

System maps are not decoration. They are drift detectors.

Rules:

- diagrams must be updated in the same change as the code
- diagrams must be executable enough to falsify (names match real code paths and interfaces)
- if a diagram cannot be checked against reality, it is a story, not architecture

Mermaid is acceptable because it is diffable and repo-native.

---

## 7. Test Requirements (Non-Negotiable)

**Every code change must have a corresponding test.** This is not optional.

Rules:
- New functionality requires new tests that exercise the happy path and at least one failure path.
- Bug fixes require a regression test that would have caught the bug.
- Refactors must not reduce test coverage.
- Tests must be runnable via `cargo test` (or equivalent for non-Rust).
- Tests must be deterministic (no flaky tests in CI).

What counts as a test:
- Unit tests for pure logic
- Integration tests for subsystem boundaries
- `decapod validate` gates for methodology invariants
- Schema validation for data contracts

What does NOT count:
- Manual testing ("I tried it and it works")
- Comments saying "TODO: add tests"
- Tests that are skipped or ignored

**Claim:** `claim.test.mandatory` — No code merges without corresponding tests.

---

## 8. Architecture Quality Signals

Evidence that architecture is healthy:

- interfaces have schemas and stable envelopes
- state has explicit ownership and migrations
- concurrency model has an explicit serialization point (or explicit proof of safety)
- failure modes are named and have recovery steps
- there is a single proof entrypoint that catches drift early
- **all code paths have test coverage**

---

## 9. Changelog

- v0.0.2: Added §7 Test Requirements (mandatory tests for all code changes).
- v0.0.1: A general architecture practice contract (how to design, document, decide, and prove), aligning with the original intent-driven methodology docs.

## Links

- `embedded/specs/INTENT.md`
- `embedded/specs/SYSTEM.md`
- `embedded/core/SOUL.md`
