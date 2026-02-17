# DECAPOD.md - How Decapod Works (Top-Level Index)

**Authority:** routing (this file routes; it does not override contracts)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** bootloader/router for the doc stack and CLI thin waist
**Non-goals:** restating contracts; defining authority; listing subsystems (route instead)

⚠️ **CRITICAL: THIS IS YOUR OPERATING MANUAL** ⚠️

Decapod is a Project OS for Machines. You are an agent operating within a shared, deterministic environment. Humans steer; you execute.

**FAILURE TO FOLLOW THIS METHODOLOGY WILL RESULT IN UNVERIFIED, UNSAFE WORK.**

---

## 0. Navigation Charter

This document helps agents navigate the Decapod documentation. Its primary roles are to:

- Route to canonical documents.
- Prioritize reading order.
- Indicate which document is canonical for a topic.

**THIS IS NOT OPTIONAL GUIDANCE.**

This document does NOT aim to:

- Define specific behavioral rules, invariants, or interfaces (that lives in binding docs).
- Restate existing contracts or requirements.
- Override other constitutional documents through indexing.

If a specific requirement appears here, it should ideally be a reference to the authoritative document rather than a new definition.

This is a registered claim: (claim: claim.doc.decapod_is_router_only).

Change control for key documents is outlined in: `specs/AMENDMENTS.md`.

---

## 1. What Decapod Is (Core Function)

Decapod is a local-first control plane for coding agents. It helps facilitate multi-agent work by providing:

- Predictable sequencing through shared state.
- Interoperability via a consistent CLI interface.
- Auditability through events, proofs, and invariants.
- A streamlined approach to agent collaboration.
- Agent liveness through invocation heartbeat (every Decapod invocation auto-clocks presence).

⚠️ **ABSOLUTE REQUIREMENT:** Agents MUST interact with Decapod through its commands, not by directly manipulating internal state if a command exists for it.

**Bypassing the CLI = unverified, unsafe work.**

---

## 1.1. Mandatory Session Start Protocol

⚠️ **ABSOLUTE REQUIREMENT:** Every agent session MUST begin with this sequence:

```bash
cargo install decapod              # 1. Install/update to latest release
decapod version                     # 2. Check installed version
decapod --help                      # 3. Verify available commands
decapod data schema --subsystem command_registry --deterministic >/dev/null  # 4. Refresh CLI command index
decapod docs show core/DECAPOD.md   # 5. Refresh constitution
export DECAPOD_AGENT_ID=<agent-id>  # 6. Bind this session to one agent identity
decapod session acquire             # 7. Acquire per-agent token+password credentials
export DECAPOD_SESSION_PASSWORD=<password-from-acquire>  # 8. Bind ephemeral password
decapod validate                    # 9. Check system state
decapod todo list                   # 10. See active work
```

**Why this is non-negotiable:**

1. **Version drift**: The decapod binary evolves. Commands change, new features ship, old syntax becomes invalid. Always install the latest release first via `cargo install decapod`.
2. **Constitution updates**: The embedded documentation is updated independently of your knowledge cutoff.
3. **Command availability**: What worked yesterday may have moved (e.g., `decapod heartbeat` → `decapod govern health summary`).
4. **State verification**: System health and validation gates must be checked before any mutation.

**Failure to run this protocol will cause:**
- Command errors (calling removed/renamed commands)
- Constitution violations (operating on outdated contracts)
- State corruption (bypassing validation gates)

⚠️ **This is not optional guidance. This is a proof gate.** Agents that skip this protocol will produce unverified, unsafe work.

---

## 1.2. First-Time vs Returning Agent Flow

**First-Time Agents:** Follow the complete ladder below in order.
**Returning Agents:** After running the session start protocol, skip to §3 for topic-specific navigation.

---

## 2. Document Layers (Three-Tiered Structure)

Decapod documentation is structured into three layers, with each canonical document declaring its layer:

- **Constitution (Guiding Principles):** ⚠️ FUNDAMENTAL AUTHORITY. Defines behavioral doctrine. MUST be followed.
- **Interfaces (Contracts & Plans):** ⚠️ MACHINE-READABLE SURFACES. Defines invariants, schemas, and proof gates. Binding.
- **Guides (Operational Advice):** ⚠️ OPERATIONAL GUIDANCE. Non-binding unless explicitly marked.

**VIOLATION OF LAYER AUTHORITY = SYSTEM INVALID.**

### 2.1. Project-Level Overrides

**`.decapod/OVERRIDE.md` - Project-Specific Constitution Extensions**

Projects can override or extend the embedded constitution without forking Decapod:

- **Location:** `.decapod/OVERRIDE.md` (created by `decapod init`)
- **Purpose:** Project-specific customizations that override embedded behavior
- **Scope:** Component-specific sections (e.g., `### plugins/TODO.md`)
- **Binding:** Yes - overrides are merged with embedded docs at runtime
- **Validation:** `decapod docs override` validates and caches checksum

**How to use:**
1. Edit `.decapod/OVERRIDE.md` after running `decapod init`
2. Add content under the component path you want to override (e.g., `### plugins/TODO.md`)
3. Run `decapod docs override` to validate and cache changes
4. View merged docs via `decapod docs show <path>` (default behavior)

**Example override:**
```markdown
### plugins/TODO.md

## Custom Priority Levels
- critical: Production down
- high: Sprint commitment
- medium: Backlog
```

Overrides are appended to embedded content when agents read docs via `decapod docs show` or `decapod docs ingest`.

Key definitions:
- Doc compilation rules: `interfaces/DOC_RULES.md`
- Store purity model: `interfaces/STORE_MODEL.md`
- Subsystem registry: `core/PLUGINS.md` (§3.5)
- Interface contracts index: `core/INTERFACES.md`
- Methodology guides index: `core/METHODOLOGY.md`
- Change control (amendments): `specs/AMENDMENTS.md`
- Claims ledger (recorded promises): `interfaces/CLAIMS.md`
- Deprecation + migration: `core/DEPRECATION.md`
- Glossary of terms: `interfaces/GLOSSARY.md`

---

## 3. Navigation by Topic (Optimized Reading Order)

### Phase 1: Authority — READ FIRST (Constitution Layer)
⚠️ These documents are **BINDING** and form the foundation. Read in order:

1. **`specs/INTENT.md`** — **METHODOLOGY CONTRACT (READ FIRST)**
   Intent-first flow, choice protocol, proof doctrine
   
2. **`specs/SYSTEM.md`** — System definition, authority hierarchy, proof doctrine
   
3. **`specs/SECURITY.md`** — **Security philosophy, credential architecture, threat model**
   READ BEFORE HANDLING CREDENTIALS
   
4. **`specs/GIT.md`** — **Git etiquette, branching, commits, push policies**
   BINDING FOR ALL GIT OPERATIONS
   
5. **`specs/AMENDMENTS.md`** — Change control for binding documents

### Phase 2: Registry — Core Indices (Router Layer)
These route you to the right subsystem:

6. **`core/PLUGINS.md`** — Subsystem registry + truth labels (REAL/STUB/SPEC)
   
7. **`core/INTERFACES.md`** — Interface contracts index
   
8. **`core/METHODOLOGY.md`** — Methodology guides index
   
9. **`core/DEPRECATION.md`** — Deprecation and migration contract
   
10. **`core/DEMANDS.md`** — User demand system
    
11. **`core/GAPS.md`** — Gap analysis methodology

### Phase 3: Contracts — BINDING SURFACES (Interfaces Layer)
Machine-readable contracts and invariants:

12. **`interfaces/CONTROL_PLANE.md`** — Agent sequencing patterns
    
13. **`interfaces/DOC_RULES.md`** — Doc compilation rules, truth labels
    
14. **`interfaces/STORE_MODEL.md`** — Store semantics and purity model
    
15. **`interfaces/CLAIMS.md`** — Promises ledger with proof surfaces
    
16. **`interfaces/GLOSSARY.md`** — Normative term definitions
    
17. **`interfaces/TESTING.md`** — Testing and verification contract

### Phase 4: Practice — Operational Guidance (Guides Layer)
Non-binding but highly recommended:

18. **`methodology/SOUL.md`** — Agent identity and behavioral style

19. **`methodology/ARCHITECTURE.md`** — Architecture practice and tradeoffs

20. **`methodology/KNOWLEDGE.md`** — Knowledge curation practice

21. **`methodology/MEMORY.md`** — Memory hygiene and retrieval practice

### Phase 5: Domain Architecture — Specialized Patterns (Architecture Layer)
Domain-specific architectural guidance:

22. **`architecture/UI.md`** — **UI architecture patterns, component design, interaction models**

23. **`architecture/FRONTEND.md`** — Frontend architecture patterns

24. **`architecture/WEB.md`** — Web architecture patterns

25. **`architecture/DATA.md`** — Data architecture patterns

26. **`architecture/SECURITY.md`** — Security architecture patterns

27. **`architecture/CLOUD.md`** — Cloud deployment patterns

### Phase 6: Operations — Daily Use (Plugins Layer)
Start here for specific tasks:

28. **`plugins/TODO.md`** — **PRIMARY: Work tracking and task lifecycle**

29. **`plugins/VERIFY.md`** — Validation and verification subsystem

30. **`plugins/MANIFEST.md`** — Canonical vs derived vs state

31. **`plugins/EMERGENCY_PROTOCOL.md`** — Emergency stop-the-line procedures

---

## 4. The Thin Waist (Effective Integration)

Decapod aims for effective subsystem integration through a uniform interface:
- Stable command groups (`decapod <subsystem> ...`).
- Stable JSON envelopes (for agents).
- Store-aware operation.
- Schema/discovery mechanisms.
- **Proof gates (`decapod validate`) — MUST PASS BEFORE CLAIMING COMPLETION.**

The source of truth for subsystem details is the registry in `core/PLUGINS.md`.

If there's uncertainty about store mutation, it is advisable to clarify. Store purity is a consideration for system integrity.

---

## 5. Agent Entry Contract (Hard Rules)

All agents operating in this workspace MUST adhere to the following:

1. **Follow the Ladder**: Read `specs/INTENT.md` → `methodology/ARCHITECTURE.md` → `specs/SYSTEM.md` before acting. No exceptions.

2. **Obey Validate**: Never claim a change is correct unless `decapod validate` passes.

3. **Propose, Don't Fiat**: Do not write directly to canonical documents (core/, specs/, root README). Propose diffs or use `decapod govern feedback propose`.

4. **Record Proofs**: Every meaningful change needs a proof event (`decapod govern proof record`). No proof, no health promotion.

5. **Respect Budgets**: Monitor context token usage via `decapod data context audit`. Use `decapod data context pack` to archive history instead of silent truncation.

6. **Consult Policy**: For high-risk or irreversible actions, use `decapod govern policy eval` and await an `APPROVAL_EVENT`.

7. **Preserve Interface Abstraction**: Treat Decapod as internal agent infrastructure. Operator-facing outputs should remain semantic (intent/actions/outcomes), with command-surface details reserved for explicit diagnostic requests.

8. **Rely on invocation heartbeat**: Presence is expected to auto-refresh on each Decapod command invocation. Do not bypass the CLI for shared-state work.

9. **Clarification Gate (Stop Guessing)**: Before taking ambiguous, high-risk, irreversible, or user-visible actions, ask concise clarifying question(s) and wait for confirmation. If scope, target ID, command surface, or success criteria are unclear, do not guess.

10. **Command Surface Comprehension Gate**: Before mutating any subsystem state, agents MUST verify the active CLI surfaces via `decapod data schema --subsystem command_registry --deterministic` and read the target subsystem schema/docs (for example: `decapod data schema --subsystem todo`, `decapod data schema --subsystem policy`, etc.). Do not invent commands or subcommands.

---

## Links

### Phase 1: Authority (Constitution)
- `specs/INTENT.md` — **Methodology contract (READ FIRST)**
- `specs/SYSTEM.md` — System definition and authority doctrine
- `specs/SECURITY.md` — **Security contract (credentials, threat model)**
- `specs/GIT.md` — **Git etiquette contract**
- `specs/AMENDMENTS.md` — Change control for binding documents

### Phase 2: Registry (Core Indices)
- `core/PLUGINS.md` — Subsystem registry and truth labels
- `core/INTERFACES.md` — **Interface contracts index**
- `core/METHODOLOGY.md` — **Methodology guides index**
- `core/DEPRECATION.md` — Deprecation and migration contract
- `core/DEMANDS.md` — User demand system
- `core/GAPS.md` — Gap analysis methodology

### Phase 3: Contracts (Interfaces)
- `interfaces/CONTROL_PLANE.md` — Agent sequencing patterns
- `interfaces/DOC_RULES.md` — Doc compilation rules
- `interfaces/STORE_MODEL.md` — Store semantics and purity model
- `interfaces/CLAIMS.md` — Promises ledger
- `interfaces/GLOSSARY.md` — Normative term definitions
- `interfaces/TESTING.md` — Testing contract

### Phase 4: Practice (Methodology)
- `methodology/SOUL.md` — Agent identity and behavioral style
- `methodology/ARCHITECTURE.md` — Architecture practice
- `methodology/KNOWLEDGE.md` — Knowledge curation
- `methodology/MEMORY.md` — Memory and learning

### Phase 5: Domain Architecture
- `architecture/UI.md` — **UI architecture patterns and component design**
- `architecture/FRONTEND.md` — Frontend architecture patterns
- `architecture/WEB.md` — Web architecture patterns
- `architecture/DATA.md` — Data architecture patterns
- `architecture/SECURITY.md` — Security architecture patterns
- `architecture/CLOUD.md` — Cloud deployment patterns

### Phase 6: Operations (Plugins)
- `plugins/TODO.md` — **Work tracking (PRIMARY)**
- `plugins/VERIFY.md` — Validation subsystem
- `plugins/MANIFEST.md` — Canonical vs derived vs state
- `plugins/EMERGENCY_PROTOCOL.md` — Emergency protocols
- `plugins/DB_BROKER.md` — Database broker (planned)
