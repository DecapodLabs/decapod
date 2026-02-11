# DOC_RULES.md - Doc Compiler Contract

**Authority:** interface (doc compilation rules)
**Layer:** Interfaces
**Binding:** Yes

This document defines how markdown behaves as a machine interface in Decapod-managed repos.

If a rule is not declared here, it is not enforceable (claim: claim.doc.no_shadow_policy). If it is declared here, it is intended to become enforceable (via `decapod validate`).

---

## 1. Canonical Doc Header (Required)

Every canonical doc under `docs/` MUST include the following header fields (exact spelling):

- `**Canonical:** <repo-relative path>`
- `**Version:** vX.Y.Z`
- `**Authority:** <short role>`
- `**Layer:** Constitution | Interfaces | Guides`
- `**Binding:** Yes | No`

Optional but encouraged:

- `**Scope:** <what this doc is allowed to define>`
- `**Non-goals:** <what it must not define>`

---

## 2. Layers (Meaning)

### 2.1 Constitution (Binding)

Defines authority and behavior. Rarely edited. Short by design.

Allowed:
- authority hierarchy
- proof doctrine
- agent persona/interaction contract
- methodology contract (intent-first flow)

Forbidden:
- enumerating subsystem commands
- describing storage layouts in detail
- describing planned features as if implemented

### 2.2 Interfaces (Binding or Planned)

Defines machine surfaces: commands, schemas, store semantics, invariants, and safety gates.

Allowed:
- subsystem registry and truth labeling
- interface envelopes and schema surfaces
- store selection and purity model
- validate taxonomy and coverage matrix

Forbidden:
- tutorial prose that introduces new requirements (route to Guides instead)

### 2.3 Guides (Non-binding)

Operational guidance only. Guides may be verbose.

Allowed:
- suggested workflows
- examples and operator steps

Forbidden:
- new requirements (no "MUST", "NEVER", "REQUIRED")

Guides MUST include a disclaimer: if a guide conflicts with Constitution/Interfaces, the guide is wrong.

---

## 3. Links Footer (Graph Contract)

The canonical markdown dependency graph is defined exclusively by `## Links` footers.

Rules:
- Every canonical doc MUST have a `## Links` footer.
- Links SHOULD be repo-relative paths in backticks (e.g. `.decapod/constitution/core/MAESTRO.md`).
- `.decapod/constitution/core/MAESTRO.md` MUST reach every canonical doc via the `## Links` graph (reachability) (claim: claim.doc.decapod_reaches_all_canonical).
- Constitution hop constraint (intended invariant):
  - Every Constitution doc with `**Binding:** Yes` SHOULD be linked directly from `.decapod/constitution/core/MAESTRO.md` (no buried law).
- Interfaces hop constraint (intended invariant):
  - Every Interfaces doc with `**Binding:** Yes` SHOULD be reachable from `.decapod/constitution/core/MAESTRO.md` within 2 hops (directly or via a single router doc).
- `docs/DOC_MAP.md` is derived from this graph and MUST NOT be edited by hand.

---

## 4. Subsystem Truth (Single Source)

The only canonical place allowed to list subsystems and their statuses is:

- `.decapod/constitution/core/PLUGINS.md` (Subsystem Registry)

Any other doc that needs to refer to subsystems MUST point to the registry instead of restating it.

---

## 5. Truth Labels (For Interfaces)

Any interface statement that looks like an API (commands, schemas, guarantees) MUST be tagged with one of:

- `REAL`: implemented and working now, with a named proof surface
- `STUB`: surface exists, behavior incomplete
- `SPEC`: intended interface; not implemented
- `IDEA`: exploratory; not a commitment
- `DEPRECATED`: do not use

Constraint:
- `REAL` requires a named proof surface.
  - If no proof surface exists, the statement MUST be labeled `STUB` or `SPEC` instead.
  - This is a claim: (claim: claim.doc.real_requires_proof).

Truth labels are required in:
- subsystem registry rows
- command lists (if present)
- schema descriptions (if present)

---

## 6. No Duplicate Authority

No requirement may be defined in multiple places (claim: claim.doc.no_duplicate_authority).

If two docs define the same requirement:
- Constitution wins
- Interfaces must defer (reference Constitution)
- Guides must delete or soften the statement (guidance only)

Meta-rule:
- If two canonical binding docs appear to disagree, the system is in an invalid state.
  - Resolution is not interpretation; resolution is amendment (see `.decapod/constitution/specs/AMENDMENTS.md`).

---

## 7. Claims Ledger (Promises Must Be Registered)

Any guarantee/invariant in a canonical doc MUST:

- include a claim-id (e.g. `(claim: claim.store.blank_slate)`) near the guarantee
- be registered in `.decapod/constitution/core/CLAIMS.md`
- declare its proof surface if labeled `REAL` (see §5)

If a guarantee is not registered, treat it as non-existent for enforcement purposes.

---

## 8. Decision Rights Matrix (Authority Routing, Not RACI)

This matrix defines which canonical doc owns which type of decision. If you need to change a decision, amend the owner doc (see `.decapod/constitution/specs/AMENDMENTS.md`).

| Decision Type | Owner Doc (Single Source) |
|---|---|
| Authority hierarchy, proof doctrine, contradiction handling | `.decapod/constitution/specs/SYSTEM.md` |
| Change control for binding docs | `.decapod/constitution/specs/AMENDMENTS.md` |
| Methodology contract (how agents should work) | `.decapod/constitution/specs/INTENT.md` |
| Agent persona/interaction constraints | `.decapod/constitution/core/SOUL.md` |
| Doc compilation rules, graph semantics, truth labels, claims registration | `.decapod/constitution/core/DOC_RULES.md` |
| Claims registry (what we promise + proof surfaces) | `.decapod/constitution/core/CLAIMS.md` |
| Store semantics and purity model | `.decapod/constitution/core/STORE_MODEL.md` |
| Subsystem existence/status/truth labels registry | `.decapod/constitution/core/PLUGINS.md` |
| Control-plane sequencing patterns | `.decapod/constitution/core/CONTROL_PLANE.md` |
| Deprecation and migration contract | `.decapod/constitution/core/DEPRECATION.md` |
| Loaded-term definitions | `.decapod/constitution/core/GLOSSARY.md` |

---

## Links

- `.decapod/constitution/core/MAESTRO.md`
- `.decapod/constitution/core/PLUGINS.md`
- `.decapod/constitution/specs/SYSTEM.md`
- `.decapod/constitution/specs/AMENDMENTS.md`
- `.decapod/constitution/core/CLAIMS.md`
- `.decapod/constitution/core/DEPRECATION.md`
- `.decapod/constitution/core/GLOSSARY.md`
- `docs/DOC_MAP.md`
