# TESTING.md - Testing and Proof Interface Contract

**Authority:** interface (proof-surface contract)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** minimum testing/proof requirements for claiming verified work
**Non-goals:** test framework tutorials

---

## 1. Verification Claim Rule

Claims such as "verified", "compliant", "ready", or equivalent require a passed proof surface.

If proof cannot run, output MUST explicitly state "unverified" and include blocker details.

---

## 2. Minimum Proof Sequence

For meaningful repo mutations:
1. Run the narrowest relevant tests/checks.
2. Run `decapod validate` before final completion claims.
3. Report pass/fail with exact command names.

---

## 3. Failure Semantics

- Any non-zero exit is proof failure.
- Partial execution without clear status is unverified.
- Silent skips are prohibited.

---

## 4. Coverage Expectations

At least one falsifiable check should exist for:
- changed behavior
- changed interfaces
- changed invariants/document contracts

When no proof exists, create the smallest new gate that can fail loudly.

---

## 5. Proof Surfaces in Decapod

Primary cross-cutting gate:
- `decapod validate`

Subsystem gates are defined by owner docs and registry entries in `core/PLUGINS.md`.

---

## Links

- `core/INTERFACES.md` - Interface contracts registry
- `core/PLUGINS.md` - Subsystem proof surfaces
- `specs/INTENT.md` - Intent proof doctrine
- `plugins/VERIFY.md` - Validation subsystem
