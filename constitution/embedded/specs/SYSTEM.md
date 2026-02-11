# Decapod: The Intent-Driven Engineering System

**Authority:** constitution (authority + proof doctrine)
**Layer:** Constitution
**Binding:** Yes
**Scope:** authority hierarchy, proof doctrine, and cross-doc conflict resolution
**Non-goals:** subsystem inventories or command lists (see `embedded/core/PLUGINS.md`)

This document defines the authority rules for intent-driven repos.

It is not a substitute for proof: proof surfaces can falsify claims and must gate promotion.

Machine note:
- Authority hierarchy is defined here (see §3).
- Read order is not authority.

---

## 1. Core Philosophy: Intent is the API

The fundamental principle of the Decapod system is that **Intent is the primary interface**. We do not start by writing code; we start by declaring what must be true.

-   **Intent** is the versioned, authoritative contract.
-   **Specifications** are compiled artifacts derived from intent.
-   **Code** is an implementation artifact.
-   **Proof** is the non-negotiable price of promotion.

**The Golden Rule:** No change is legitimate until it is consistent with intent, either by preserving the existing intent or by updating the intent first.

---

## 2. The Intent-First Loop (Unidirectional Flow)

All work in an intent-driven project follows a strict, unidirectional flow:

**Intent → Specification → Code → Build/Run → Proof → Promotion**

Reverse flow (e.g., changing specs to match code) is forbidden, except during a formal, explicitly declared "drift recovery" process.

---

## 3. Authority Hierarchy

When guidance from different documents conflicts, the most specific, highest-authority document in the current working directory prevails.

1.  `embedded/specs/INTENT.md` (Binding Contract)
2.  `embedded/specs/ARCHITECTURE.md` (Compiled from Intent)
3.  Proof surface (`decapod validate`, `tests/`, and optional `proof.md`)
4.  `embedded/specs/SYSTEM.md` (This document, the foundational methodology)
5.  `embedded/core/MAESTRO.md` (Router/index; not a contract, but the default entrypoint if present)
6.  `docs/templates/AGENTS.md` (Machine-facing entrypoint)
7.  `embedded/plugins/WORKFLOW.md` (Operational guidance, must not override intent)
8.  `embedded/specs/philosophy.md` (Non-binding rationale)
9.  `embedded/specs/context.md` (Non-binding history)

---

## 4. Agent Behavior & Mode Discipline

All AI agents operating within this system must adhere to the following behavioral rules.

### 4.1. Default Agent Behavior

-   **Before Acting:**
    1.  If present, start at `embedded/core/MAESTRO.md` (repo router/index).
    2.  Read `embedded/specs/INTENT.md`.
    3.  Read `embedded/specs/ARCHITECTURE.md`.
    4.  Read the proof surface (`decapod validate`, `tests/`, and optional `proof.md`).
    5.  Then, and only then, read or modify the implementation.
-   **While Acting:**
    -   If a request changes "what must be true," propose intent deltas **before** coding.
    -   Prefer minimal diffs that satisfy proof obligations.
    -   Preserve simplicity unless complexity is demanded by the intent.
-   **After Acting:**
    -   Provide a concrete proof plan with exact commands and pass criteria.
    -   State "unverified" if proof cannot be run, and describe what is needed to confirm.

### 4.2. Mode Discipline

Agents must explicitly declare their operating mode before proposing changes:

-   **Mode A:** Intent authoring/editing
-   **Mode B:** Spec compilation/update
-   **Mode C:** Implementation
-   **Mode D:** Proof harness work
-   **Mode E:** Promotion guidance

---

## 5. Structural & Proof Discipline

To prevent drift and ensure quality, all projects must adhere to strict structural and proof-related rules.

### 5.1. Structural Enforcement

-   **Promise IDs:** Intent promises MUST use stable, unique IDs (e.g., `P1`, `P2`). These IDs must be used for tracing in `ARCHITECTURE.md`, `proof.md`, and compliance tables. Never renumber existing promises.
-   **Version Headers:**
    -   `ARCHITECTURE.md` MUST include: `**Compiled from:** INTENT.md vX.Y.Z`
    -   `proof.md` MUST include: `**Intent Version:** vX.Y.Z`
-   **Authority Constraints:** `philosophy.md` and `context.md` MUST be marked "non-binding" and must not claim authority.
-   **Constraint Scoping:** Complexity constraints (e.g., line limits) MUST be explicitly scoped to "implementation files" or similar, not applied vaguely.

### 5.2. Proof Discipline (Non-Negotiable)

**An agent or user must NEVER claim a change is "compliant", "verified", or "ready to promote" UNTIL ALL of the following are true:**

1.  The `proof.md` file is not a template (contains no "TODO" or "Not yet" markers).
2.  The automated proof harness (`decapod validate`, if it exists) runs and exits with code 0.
3.  The compliance numbers in `proof.md` and `embedded/specs/INTENT.md` match exactly.
4.  If the intent declares invariants, there is runtime validation code for them.

**Violation of these rules is considered drift.** The process must stop, the proof surface must be updated, and verification must be re-run.

---

## 6. Project & Capability Definitions

This system defines clear classifications for projects and a composable system for defining a project's technical capabilities.

### 6.1. Project Classes

Every repository must be classified as one of the following:

1.  **Intent-Driven:** `embedded/specs/INTENT.md` is the versioned, authoritative contract. Promotion is gated by proof.
2.  **Spec-Driven:** Specifications exist, but are not treated as a binding contract.
3.  **Prototype/Spike:** For exploration. Assumptions and exit criteria must be recorded.

### 6.2. The Capability System

To standardize architectural choices, projects can declare **Capabilities**—named, versioned, composable modules for features like language toolchains, runtimes, or data storage.

-   **Declaration:** Capabilities are declared in `embedded/specs/INTENT.md` in a dedicated section (e.g., `lang.rust`, `runtime.container`, `data.postgres`).
-   **Anatomy:** Each capability defines its dependencies, conflicts, generated artifacts, and proof obligations.
-   **No Implicit Defaults:** Agents MUST NOT introduce new capabilities (like Docker or a database) without them being explicitly declared in the intent first.

---

## 7. Workshop Overlay (Methodology as a Curriculum)

This system is designed to be teachable. The "Workshop Overlay" turns the intent-driven methodology into a curriculum that agents can run.

### 7.1. Workshop Roles

-   **Instructor Mode:** Reveal structure, ask "why," but do not provide full solutions.
-   **Participant Mode:** Optimize for learning-by-doing, with hints and proof-first iteration.
-   **Evaluator Mode:** Run proofs, verify traceability, and grade based on objective rubrics.

### 7.2. Workshop Invariants

-   The unidirectional flow (`intent` → `spec` → `code` → `proof`) is always preserved.
-   Traceability is required for all artifacts.
-   Proof is the grade.

---

## 8. Core Subsystems

Subsystems exist as interface surfaces (`decapod <subsystem> ...`), but subsystem truth is not defined here.

Canonical subsystem registry (single source of truth):
- `embedded/core/PLUGINS.md` (§3.5)

---

## 9. Extensions (Planned)

Decapod will support extensions, but this repository currently ships a single Rust CLI binary with built-in subsystems.

Planned direction (not implemented yet):
- A first-class `decapod schema` discovery surface.
- A stable extension mechanism with explicit versioning and validation.

Until this is implemented, do not document script-based plugin systems or external dispatch paths.

---

## 10. See Also

-   `embedded/core/SOUL.md`: Defines the agent's core identity and prime directives.
-   `embedded/core/MEMORY.md`: Outlines principles and mechanisms for agent's persistent memory.
-   `embedded/core/KNOWLEDGE.md`: Defines principles for managing project-specific knowledge.

For domain-specific guidance, keep it repo-local under `docs/` and reference it from your project `AGENTS.md`.

For operational workflow and TODO governance, see `embedded/plugins/WORKFLOW.md`.

## Links

- `embedded/core/CONTROL_PLANE.md`
- `embedded/core/MAESTRO.md`
- `embedded/core/DOC_RULES.md`
- `embedded/core/KNOWLEDGE.md`
- `embedded/core/MEMORY.md`
- `embedded/core/SOUL.md`
- `embedded/plugins/WORKFLOW.md`
- `embedded/core/PLUGINS.md`
- `embedded/specs/ARCHITECTURE.md`
- `embedded/specs/INTENT.md`
- `embedded/specs/SYSTEM.md`
- `embedded/specs/context.md`
- `embedded/specs/philosophy.md`
- `docs/templates/AGENTS.md`
- `docs/templates/CLAUDE.md`
- `docs/templates/GEMINI.md`
- `docs/templates/DEMANDS.md`
