# STORE_MODEL.md - Store Purity and Threat Model

**Authority:** interface (store semantics + safety model)
**Layer:** Interfaces
**Binding:** Yes

This document defines store selection semantics and the safety model for preventing cross-store contamination.

---

## 1. Stores

Decapod has two stores (state roots):

- user store: `~/.decapod`
- repo store: `<repo>/.decapod/project`

The store is part of the request context. A command that mutates state is not well-defined unless the store is well-defined.

---

## 2. Assets (What We Protect)

- user store privacy: a user starts blank and should not inherit repo ideology or backlog
- repo store reproducibility: repo state should be deterministically rebuildable from repo-tracked artifacts where declared
- derived state integrity: derived artifacts should never be treated as source-of-truth
- provenance: every mutation should be attributable to an actor and a store context (planned: audit trail)

---

## 3. Threats (How Systems Die)

- accidental contamination: repo dogfood tasks appearing in user store
- ghost state: agent writes to a store without intending to (wrong root, implicit defaults)
- split brain: multiple "canonical" stores or parallel tooling
- provenance loss: mutations without a record of who/when/why

---

## 4. Guarantees (Contract)

All guarantees here are registered in `embedded/core/CLAIMS.md`.

- blank-slate (claim: claim.store.blank_slate): a fresh user store has no tasks unless the user adds them
- no auto-seeding (claim: claim.store.no_auto_seeding): repo store content must never appear in the user store automatically
- explicit store selection (claim: claim.store.explicit_store_selection): `--store` is the preferred selector; `--root` is an escape hatch and must be treated as dangerous

---

## 5. Red Lines (Unacceptable Behavior)

- writing repo backlog into user store
- silently switching stores mid-session
- creating alternate state roots outside `.decapod`
- claiming compliance/verification without running a proof surface

---

## 6. Routing (Where This Is Used)

- Control plane patterns: `embedded/core/CONTROL_PLANE.md`
- Subsystem surfaces: `embedded/core/PLUGINS.md`
- Proof doctrine and authority: `embedded/specs/SYSTEM.md`

---

## Links

- `embedded/core/CONTROL_PLANE.md`
- `embedded/core/DECAPOD.md`
- `embedded/core/CLAIMS.md`
- `embedded/core/PLUGINS.md`
- `embedded/core/DOC_RULES.md`
- `embedded/specs/AMENDMENTS.md`
- `embedded/specs/SYSTEM.md`
- `docs/REPO_MAP.md`
