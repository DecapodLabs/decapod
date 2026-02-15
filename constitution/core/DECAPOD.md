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

⚠️ **ABSOLUTE REQUIREMENT:** Agents MUST interact with Decapod through its commands, not by directly manipulating internal state if a command exists for it.

**Bypassing the CLI = unverified, unsafe work.**

---

## 1.1. Mandatory Session Start Protocol

⚠️ **ABSOLUTE REQUIREMENT:** Every agent session MUST begin with this sequence:

```bash
decapod --version                   # Verify installed binary version
decapod update                      # Update if repo version is newer (safe no-op if current)
decapod --help                      # Check available commands
decapod docs show core/DECAPOD.md  # Refresh constitution (this file)
decapod validate                    # Verify system state
decapod todo list                   # Check active work
```

**Why this is non-negotiable:**

1. **Version drift**: The decapod binary evolves. Commands change, new features ship, old syntax becomes invalid. If version mismatch exists, you MUST run `decapod update` before continuing.
2. **Constitution updates**: The embedded documentation is updated independently of your knowledge cutoff.
3. **Command availability**: What worked yesterday may have moved (e.g., `decapod heartbeat` → `decapod govern health summary`).
4. **State verification**: System health and validation gates must be checked before any mutation.

**Failure to run this protocol will cause:**
- Command errors (calling removed/renamed commands)
- Constitution violations (operating on outdated contracts)
- State corruption (bypassing validation gates)

⚠️ **This is not optional guidance. This is a proof gate.** Agents that skip this protocol will produce unverified, unsafe work.

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

## 3. Navigation by Topic

**Constitution (Guiding Principles) - ABSOLUTE AUTHORITY:**
- Authority and proof doctrine: `specs/SYSTEM.md`
- ⚠️ **METHODOLOGY CONTRACT: `specs/INTENT.md` — READ THIS FIRST. ALWAYS.**
- ⚠️ **SECURITY CONTRACT: `specs/SECURITY.md` — Security philosophy, credential architecture, threat model. READ BEFORE HANDLING CREDENTIALS.**
- ⚠️ **GIT CONTRACT: `specs/GIT.md` — Git etiquette, branching strategy, commit conventions, push policies. BINDING FOR ALL GIT OPERATIONS.**
- Change control (amendments): `specs/AMENDMENTS.md`
- Agent persona/interaction guidelines: `methodology/SOUL.md`

**Interfaces (Contracts & Plans) - BINDING SURFACES:**
- **Interface contracts index: `core/INTERFACES.md` — START HERE for all interface contracts**
- Agent<->Decapod sequencing: `interfaces/CONTROL_PLANE.md`
- Subsystem registry + truth labels: `core/PLUGINS.md`
- Store purity model: `interfaces/STORE_MODEL.md`
- Doc compiler contract: `interfaces/DOC_RULES.md`
- Claims ledger (promises + proof surfaces): `interfaces/CLAIMS.md`
- Deprecation + migration contract: `core/DEPRECATION.md`
- Glossary of loaded terms (normative): `interfaces/GLOSSARY.md`
- Planned broker interface: `plugins/DB_BROKER.md`

**Methodology (How-To Guides) - REFERENCE ONLY:**
- **Methodology guides index: `core/METHODOLOGY.md` — START HERE for all methodology**
- Architecture practice: `methodology/ARCHITECTURE.md`
- Agent persona: `methodology/SOUL.md`
- Knowledge management: `methodology/KNOWLEDGE.md`
- Memory/learning: `methodology/MEMORY.md`

**Plugins (Operational Subsystems):**
- **TODO Subsystem (PRIMARY):** `plugins/TODO.md` — **Use this for all work tracking**
- Operating loop: `plugins/TODO.md`
- Canonical vs derived vs state: `plugins/MANIFEST.md`
- Verification and drift checks: `plugins/VERIFY.md`
- Emergency protocol (stop-the-line): `plugins/EMERGENCY_PROTOCOL.md`

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

---

## Links

- `specs/SECURITY.md` — **Security contract (credential handling, threat model, incident response)**
- `specs/GIT.md` — **Git etiquette contract (branching, commits, push policies)**
- `plugins/TODO.md` — **TODO subsystem (start here for work tracking)**
- `plugins/MANIFEST.md`
- `plugins/VERIFY.md`
- `plugins/DB_BROKER.md`
- `plugins/EMERGENCY_PROTOCOL.md`
- `specs/INTENT.md`
- `specs/SYSTEM.md`
- `specs/AMENDMENTS.md`
- `core/PLUGINS.md`
- `core/DEPRECATION.md`
- `core/INTERFACES.md` — **Interface contracts index**
- `interfaces/CONTROL_PLANE.md`
- `interfaces/DOC_RULES.md`
- `interfaces/STORE_MODEL.md`
- `interfaces/CLAIMS.md`
- `interfaces/GLOSSARY.md`
- `core/METHODOLOGY.md` — **Methodology guides index**
- `methodology/ARCHITECTURE.md`
- `methodology/SOUL.md`
- `methodology/MEMORY.md`
- `methodology/KNOWLEDGE.md`
