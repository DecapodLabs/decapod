# DECAPOD.md - How Decapod Works (Top-Level Index)

**Authority:** guidance (this file helps agents navigate the system)
**Layer:** Interfaces
**Binding:** No
**Scope:** bootloader/router for the doc stack and CLI thin waist
**Non-goals:** restating contracts; defining authority; listing subsystems (route instead)

Decapod is a Project OS for Machines. You are an agent operating within a shared, deterministic environment. Humans steer; you execute.

This is the top-level document for how Decapod works. It serves as the primary internal guide for an agent in a Decapod-managed repo.

Agent entrypoints (e.g., from `embedded/templates/*`) should generally link here to help route to relevant information.

---

## 0. Navigation Charter

This document helps agents navigate the Decapod documentation. Its primary roles are to:

- Route to canonical documents.
- Prioritize reading order.
- Indicate which document is canonical for a given topic.

This document does not aim to:

- Define specific behavioral rules, invariants, or interfaces.
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

A key guideline: agents should interact with Decapod through its commands, not by directly manipulating internal state if a command exists for it.

---

## 2. Document Layers (Three-Tiered Structure)

Decapod documentation is structured into three layers, with each canonical document declaring its layer:

- **Constitution (Guiding Principles):** Outlines fundamental authority and behavioral suggestions.
- **Interfaces (Contracts & Plans):** Details machine-readable surfaces (CLI, schemas, stores, invariants), some of which may be binding.
- **Guides (Operational Advice):** Provides practical operational guidance.

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

**Constitution (Guiding Principles):**
- Authority and proof doctrine: `embedded/specs/SYSTEM.md`
- Methodology contract: `embedded/specs/INTENT.md`
- Change control (amendments): `embedded/specs/AMENDMENTS.md`
- Agent persona/interaction guidelines: `embedded/core/SOUL.md`

**Interfaces (Contracts & Plans):**
- Agent<->Decapod sequencing: `embedded/core/CONTROL_PLANE.md`
- Subsystem registry + truth labels: `embedded/core/PLUGINS.md`
- Store purity model: `embedded/core/STORE_MODEL.md`
- Doc compiler contract: `embedded/core/DOC_RULES.md`
- Claims ledger (promises + proof surfaces): `embedded/core/CLAIMS.md`
- Deprecation + migration contract: `embedded/core/DEPRECATION.md`
- Glossary of loaded terms (normative): `embedded/core/GLOSSARY.md`
- Planned broker interface: `embedded/plugins/DB_BROKER.md`

**Guides (Operational Advice):**
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
- Proof gates (`decapod validate`).

The source of truth for subsystem details is the registry in `embedded/core/PLUGINS.md`.

If there's uncertainty about store mutation, it's advisable to clarify. Store purity is a consideration for system integrity.

---

## 5. Agent Interaction Guidelines

Agents operating in this workspace are encouraged to consider the following:

1. **Follow the Ladder**: Review `INTENT.md` -> `ARCHITECTURE.md` -> `SYSTEM.md` before acting.
2. **Utilize Validate**: Aim for `decapod validate` to pass when making changes.
3. **Propose Changes**: For canonical documents (core/specs/root README), consider proposing diffs or using `decapod feedback propose`.
4. **Record Proofs**: For significant changes, documenting a proof event (`decapod proof record`) can be beneficial for health promotion.
5. **Consider Budgets**: Be mindful of context token usage. `decapod context pack` can help manage history without silent truncation.
6. **Consult Policy**: For high-risk or irreversible actions, `decapod policy eval` can provide insights, and an `APPROVAL_EVENT` may be sought.

---

## Links

- `embedded/plugins/MANIFEST.md`
- `embedded/specs/ARCHITECTURE.md`
- `embedded/specs/AMENDMENTS.md`
- `embedded/specs/INTENT.md`
- `embedded/specs/SYSTEM.md`
- `embedded/core/CONTROL_PLANE.md`
- `embedded/core/DOC_RULES.md`
- `embedded/core/PLUGINS.md`
- `embedded/core/STORE_MODEL.md`
- `embedded/core/SOUL.md`
- `embedded/core/MEMORY.md`
- `embedded/core/KNOWLEDGE.md`
- `embedded/core/CLAIMS.md`
- `embedded/core/DEPRECATION.md`
- `embedded/core/GLOSSARY.md`
- `embedded/plugins/METHODOLOGY_GAPS.md`
- `embedded/plugins/TODO_USER.md`
- `embedded/plugins/WORKFLOW.md`
- `embedded/plugins/DB_BROKER.md`
- `embedded/plugins/EMERGENCY_PROTOCOL.md`
