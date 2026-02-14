# GLOSSARY.md - Loaded Terms (Normative)

**Authority:** interface (normative term definitions)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** defines loaded terms used across the doc stack to prevent semantic drift
**Non-goals:** tutorials; this is a reference

This glossary is binding: if a term is defined here, other canonical docs MUST use it consistently.

---

## 1. Terms

- Canonical:
  - The repo-relative path in `**Canonical:** ...` identifies the authoritative location of a document.
  - Canonical does not imply binding; it implies "this path is the source-of-truth for the text."
- Binding:
  - `**Binding:** Yes` means the document defines requirements, invariants, or interfaces.
  - `**Binding:** No` means guidance only; if it conflicts with binding docs, it is wrong.
- Layer:
  - Constitution: authority and behavioral doctrine.
  - Interfaces: machine surfaces, schemas, invariants, safety gates.
  - Guides: operational advice; non-binding.
- Authority (header field):
  - A short statement describing what the document is allowed to define (e.g. routing vs interface vs constitution).
- Router (routing authority):
  - A document that routes readers to canonical sources.
  - A router does not create new behavioral requirements (see Delegation Charter in `embedded/core/DECAPOD.md`).
- Proof surface:
  - A named, runnable mechanism that can detect drift or validate invariants (e.g. `decapod validate`, schema checks).
- Claim:
  - A registered promise/guarantee/invariant with a stable claim-id, tracked in `embedded/interfaces/CLAIMS.md`.
- Enforcement:
  - Whether a claim is checked by a proof surface (`enforced`), partly checked (`partially_enforced`), or only documented (`not_enforced`).
- Store:
  - A state root that scopes reads/writes (see `embedded/interfaces/STORE_MODEL.md`).
  - User store: `~/.decapod`
  - Repo store: `<repo>/.decapod/project`
- Subsystem:
  - A first-class Decapod surface with a CLI group and schema/proof hooks (see `embedded/core/PLUGINS.md`).
- Plugin-grade:
  - Meets the thin-waist requirements in `embedded/core/PLUGINS.md` (stable CLI group, schema/discovery, store-awareness, proof hooks).
- Derived (artifact/state):
  - Computed output that must not be treated as source-of-truth (see `embedded/plugins/MANIFEST.md`).
- Validate:
  - The primary proof surface (`decapod validate`) that checks documented invariants and drift gates.
- Amendment:
  - A binding meaning change governed by `embedded/specs/AMENDMENTS.md`.
- Deprecation:
  - A non-binding marker on old meaning governed by `embedded/core/DEPRECATION.md`, with replacement + sunset.

---

## Links

- `embedded/core/DECAPOD.md` - Router and navigation charter
- `embedded/core/INTERFACES.md` - Interface contracts index
- `embedded/interfaces/DOC_RULES.md` - Doc compilation rules
- `embedded/interfaces/CLAIMS.md` - Promises ledger
- `embedded/interfaces/STORE_MODEL.md` - Store semantics
- `embedded/core/PLUGINS.md` - Subsystem registry
- `embedded/core/DEPRECATION.md` - Deprecation contract
- `embedded/specs/AMENDMENTS.md` - Change control
- `embedded/specs/SYSTEM.md` - System definition
