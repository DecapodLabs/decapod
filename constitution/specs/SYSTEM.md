# Decapod: The Intent-Driven Engineering System

**Authority:** constitution (authority + proof doctrine)
**Layer:** Constitution
**Binding:** Yes
**Scope:** authority hierarchy, proof doctrine, and cross-doc conflict resolution
**Non-goals:** subsystem inventories or command lists (see `core/PLUGINS.md`)

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

### 1.1 Decapod Foundation Demands (Binding)

For Decapod-managed repositories, the following are mandatory:

1. **Daemonless + repo-native canonicality:** Promotion-relevant state MUST be derivable from repo-native artifacts, ledgers, and receipts.
2. **Deterministic infrastructure:** Reducers, replays, and gate evaluations MUST produce stable results for equivalent inputs.
3. **Explicit boundaries:** Authority (`specs/`, `interfaces/`), interface (`decapod` CLI/RPC), and storage (`--store user|repo`) boundaries MUST be explicit and must not be bypassed.
4. **Proof-gated promotion:** No promotion-relevant claim is valid without executable proof surfaces and machine-verifiable outputs.
5. **Bounded validator liveness:** `decapod validate` MUST terminate within bounded time and return typed failure on contention, not block indefinitely.

---

## 2. The Intent-First Loop (Unidirectional Flow)

All work in an intent-driven project follows a strict, unidirectional flow:

**Intent → Specification → Code → Build/Run → Proof → Promotion**

Reverse flow (e.g., changing specs to match code) is forbidden, except during a formal, explicitly declared "drift recovery" process.

---

## 3. Authority Hierarchy

When guidance from different documents conflicts, the most specific, highest-authority document in the current working directory prevails.

1.  `specs/INTENT.md` (Binding Contract)
2.  `methodology/ARCHITECTURE.md` (Compiled from Intent)
3.  Proof surface (`decapod validate`, `tests/`, and optional `proof.md`)
4.  `specs/SYSTEM.md` (This document, the foundational methodology)
5.  `core/DECAPOD.md` (Router/index; not a contract, but the default entrypoint if present)
6.  `AGENTS.md` / `CLAUDE.md` / `GEMINI.md` / `CODEX.md` (Machine-facing entrypoints)
7.  `plugins/TODO.md` (Operational guidance, must not override intent)
8.  repo-local non-binding rationale notes (if present)
9.  repo-local non-binding context/history notes (if present)

---

## 4. Agent Behavior & Mode Discipline

All AI agents operating within this system must adhere to the following behavioral rules.

### 4.1. Default Agent Behavior

-   **Before Acting:**
    1.  If present, start at `core/DECAPOD.md` (repo router/index).
    2.  Run `cargo install decapod` to ensure the latest release, then `decapod version`.
    3.  Read `specs/INTENT.md`.
    4.  Read `methodology/ARCHITECTURE.md`.
    5.  Read the proof surface (`decapod validate`, `tests/`, and optional `proof.md`).
    6.  Then, and only then, read or modify the implementation.
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
3.  The compliance numbers in `proof.md` and `specs/INTENT.md` match exactly.
4.  If the intent declares invariants, there is runtime validation code for them.
5.  **Tooling validation passes** - All declared language toolchain requirements (formatting, linting, type checking) are satisfied.
6.  Validation liveness guarantees are preserved (no unbounded hang path in proof gates).

**Violation of these rules is considered drift.** The process must stop, the proof surface must be updated, and verification must be re-run.

### 5.3. Tooling Validation Gate (First-Class Citizen)

Tooling that validates the repo's own source code and the tooling the project relies on MUST be treated as first-class citizens in proof checking.

**Requirements:**

-   **Language Toolchains:** Projects MUST declare their language toolchain requirements in `specs/INTENT.md` (e.g., `lang.rust.toolchain = "stable"`, `lang.rust.format = "cargo fmt"`, `lang.rust.lint = "cargo clippy"`).
-   **Tooling Proof Gates:** Before signing off that a change is ready for PR/merge/production, the following MUST pass:
    1.  **Formatting Gate:** Source code MUST pass the declared formatter (e.g., `cargo fmt --check`).
    2.  **Linting Gate:** Source code MUST pass the declared linter (e.g., `cargo clippy --all-targets`).
    3.  **Type Safety Gate:** For typed languages, type checking MUST pass (e.g., `cargo check`).
-   **Tooling as Dependencies:** Tooling versions MUST be treated as dependencies. Changes to tooling versions require the same proof discipline as code changes.
-   **CI/CD Parity:** Local `decapod validate` MUST enforce the same toolchain gates as CI/CD pipelines.

**Rationale:** Tooling drift is code drift. A project that passes tests but fails formatting or linting is not "ready." This gate ensures tooling hygiene is enforced at the same priority level as functional correctness.

---

## 6. Project & Capability Definitions

This system defines clear classifications for projects and a composable system for defining a project's technical capabilities.

### 6.1. Project Classes

Every repository must be classified as one of the following:

1.  **Intent-Driven:** `specs/INTENT.md` is the versioned, authoritative contract. Promotion is gated by proof.
2.  **Spec-Driven:** Specifications exist, but are not treated as a binding contract.
3.  **Prototype/Spike:** For exploration. Assumptions and exit criteria must be recorded.

### 6.2. The Capability System

To standardize architectural choices, projects can declare **Capabilities**—named, versioned, composable modules for features like language toolchains, runtimes, or data storage.

-   **Declaration:** Capabilities are declared in `specs/INTENT.md` in a dedicated section (e.g., `lang.rust`, `runtime.container`, `data.postgres`).
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
- `core/PLUGINS.md` (§3.5)

---

## 9. Extensions (Planned)

Decapod will support extensions, but this repository currently ships a single Rust CLI binary with built-in subsystems.

Planned direction (not implemented yet):
- A first-class `decapod schema` discovery surface.
- A stable extension mechanism with explicit versioning and validation.

Until this is implemented, do not document script-based plugin systems or external dispatch paths.

---

## 10. See Also

-   `methodology/SOUL.md`: Defines the agent's core identity and prime directives.
-   `methodology/MEMORY.md`: Outlines principles and mechanisms for agent's memory.
-   `methodology/KNOWLEDGE.md`: Defines principles for managing project-specific knowledge.

For domain-specific guidance, keep it repo-local under `docs/` and reference it from your project `AGENTS.md`.

For operational workflow and TODO governance, see `plugins/TODO.md`.

## Links

### Core Router
- `core/DECAPOD.md` - **Router and navigation charter (START HERE)**

### Authority (Constitution Layer)
- `specs/INTENT.md` - **Methodology contract (READ FIRST)**
- `specs/SECURITY.md` - Security contract
- `specs/GIT.md` - Git etiquette contract
- `specs/AMENDMENTS.md` - Change control

### Registry (Core Indices)
- `core/PLUGINS.md` - Subsystem registry
- `core/INTERFACES.md` - Interface contracts index
- `core/METHODOLOGY.md` - Methodology guides index
- `core/DEPRECATION.md` - Deprecation contract

### Contracts (Interfaces Layer)
- `interfaces/CONTROL_PLANE.md` - Sequencing patterns
- `interfaces/DOC_RULES.md` - Doc compilation rules
- `interfaces/STORE_MODEL.md` - Store semantics
- `interfaces/CLAIMS.md` - Promises ledger
- `interfaces/GLOSSARY.md` - Term definitions

### Practice (Methodology Layer)
- `methodology/SOUL.md` - Agent identity
- `methodology/ARCHITECTURE.md` - Architecture practice
- `methodology/KNOWLEDGE.md` - Knowledge management
- `methodology/MEMORY.md` - Memory and learning

### Operations (Plugins Layer)
- `plugins/TODO.md` - Work tracking
- `plugins/VERIFY.md` - Validation subsystem
- `plugins/MANIFEST.md` - Canonical vs derived vs state

---

## Project Override Context

Project system emphasis:
- Keep configuration explicit and environment-driven, with safe defaults.
- Separate provider choices (LLM, storage, embeddings, channels) behind stable abstractions.
- Support concurrent execution with guardrails for resource limits and recovery.
- Maintain operational toggles for automation features so risky behavior can be disabled quickly.
