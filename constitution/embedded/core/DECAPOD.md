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

Change control for key documents is outlined in: `embedded/specs/AMENDMENTS.md`.

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
decapod --help                      # Check available commands
decapod docs show core/DECAPOD.md  # Refresh constitution (this file)
decapod validate                    # Verify system state
decapod todo list                   # Check active work
```

**Why this is non-negotiable:**

1. **Version drift**: The decapod binary evolves. Commands change, new features ship, old syntax becomes invalid.
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
- Doc compilation rules: `embedded/core/DOC_RULES.md`
- Store purity model: `embedded/core/STORE_MODEL.md`
- Subsystem registry: `embedded/core/PLUGINS.md` (§3.5)
- Change control (amendments): `embedded/specs/AMENDMENTS.md`
- Claims ledger (recorded promises): `embedded/core/CLAIMS.md`
- Deprecation + migration: `embedded/core/DEPRECATION.md`
- Glossary of terms: `embedded/core/GLOSSARY.md`

---

## 3. Navigation by Topic

**Constitution (Guiding Principles) - ABSOLUTE AUTHORITY:**
- Authority and proof doctrine: `embedded/specs/SYSTEM.md`
- ⚠️ **METHODOLOGY CONTRACT: `embedded/specs/INTENT.md` — READ THIS FIRST. ALWAYS.**
- ⚠️ **SECURITY CONTRACT: `embedded/specs/SECURITY.md` — Security philosophy, credential architecture, threat model. READ BEFORE HANDLING CREDENTIALS.**
- Change control (amendments): `embedded/specs/AMENDMENTS.md`
- Agent persona/interaction guidelines: `embedded/core/SOUL.md`

**Interfaces (Contracts & Plans) - BINDING SURFACES:**
- Agent<->Decapod sequencing: `embedded/core/CONTROL_PLANE.md`
- Subsystem registry + truth labels: `embedded/core/PLUGINS.md`
- Store purity model: `embedded/core/STORE_MODEL.md`
- Doc compiler contract: `embedded/core/DOC_RULES.md`
- Claims ledger (promises + proof surfaces): `embedded/core/CLAIMS.md`
- Deprecation + migration contract: `embedded/core/DEPRECATION.md`
- Glossary of loaded terms (normative): `embedded/core/GLOSSARY.md`
- Planned broker interface: `embedded/plugins/DB_BROKER.md`

**Guides (Operational Advice) - REFERENCE ONLY:**
- **TODO Subsystem (PRIMARY):** `embedded/plugins/TODO.md` — **Use this for all work tracking**
- Operating loop: `embedded/plugins/WORKFLOW.md`
- Canonical vs derived vs state: `embedded/plugins/MANIFEST.md`
- Known gaps: `embedded/plugins/METHODOLOGY_GAPS.md`
- Agent checklist (docs only): `embedded/plugins/TODO_USER.md`
- Emergency protocol (stop-the-line): `embedded/plugins/EMERGENCY_PROTOCOL.md`

---

## 4. The Thin Waist (Effective Integration)

Decapod aims for effective subsystem integration through a uniform interface:
- Stable command groups (`decapod <subsystem> ...`).
- Stable JSON envelopes (for agents).
- Store-aware operation.
- Schema/discovery mechanisms.
- **Proof gates (`decapod validate`) — MUST PASS BEFORE CLAIMING COMPLETION.**

The source of truth for subsystem details is the registry in `embedded/core/PLUGINS.md`.

If there's uncertainty about store mutation, it is advisable to clarify. Store purity is a consideration for system integrity.

---

## 5. Agent Entry Contract (Hard Rules)

All agents operating in this workspace MUST adhere to the following:

1. **Follow the Ladder**: Read `embedded/specs/INTENT.md` → `embedded/specs/ARCHITECTURE.md` → `embedded/specs/SYSTEM.md` before acting. No exceptions.

2. **Obey Validate**: Never claim a change is correct unless `decapod validate` passes.

3. **Propose, Don't Fiat**: Do not write directly to canonical documents (embedded/core, embedded/specs, root README). Propose diffs or use `decapod govern feedback propose`.

4. **Record Proofs**: Every meaningful change needs a proof event (`decapod govern proof record`). No proof, no health promotion.

5. **Respect Budgets**: Monitor context token usage via `decapod data context audit`. Use `decapod data context pack` to archive history instead of silent truncation.

6. **Consult Policy**: For high-risk or irreversible actions, use `decapod govern policy eval` and await an `APPROVAL_EVENT`.

---

## Links

- `embedded/specs/SECURITY.md` — **Security contract (credential handling, threat model, incident response)**
- `embedded/plugins/TODO.md` — **TODO subsystem (start here for work tracking)**
- `embedded/plugins/MANIFEST.md`
- `embedded/plugins/TODO_USER.md`
- `embedded/plugins/WORKFLOW.md`
- `embedded/plugins/DB_BROKER.md`
- `embedded/plugins/METHODOLOGY_GAPS.md`
- `embedded/plugins/EMERGENCY_PROTOCOL.md`
- `embedded/specs/INTENT.md`
- `embedded/specs/ARCHITECTURE.md`
- `embedded/specs/SYSTEM.md`
- `embedded/specs/AMENDMENTS.md`
- `embedded/core/PLUGINS.md`
- `embedded/core/CONTROL_PLANE.md`
- `embedded/core/DOC_RULES.md`
- `embedded/core/STORE_MODEL.md`
- `embedded/core/SOUL.md`
- `embedded/core/MEMORY.md`
- `embedded/core/KNOWLEDGE.md`
- `embedded/core/CLAIMS.md`
- `embedded/core/DEPRECATION.md`
- `embedded/core/GLOSSARY.md`
