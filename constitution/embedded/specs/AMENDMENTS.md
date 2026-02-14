# AMENDMENTS.md - Change Control for Binding Docs

**Authority:** constitution (how binding text may change)
**Layer:** Constitution
**Binding:** Yes
**Scope:** defines what counts as an amendment, required co-updates, and required records
**Non-goals:** specifying system behavior; this document only governs changes to binding docs

This document defines how binding documents may change without creating silent consensus rewrites.

If a binding doc changes without following this process, the system is in an invalid governance state.

---

## 1. Definitions

- Binding doc: any doc with `**Binding:** Yes`.
- Amendment: any change that modifies binding meaning.
  - Includes: changing MUST/SHALL/NEVER language, changing invariants, changing interfaces, changing decision rights, changing layer/authority/scope, introducing or removing a claim.
  - Excludes: pure spelling/formatting changes that do not alter meaning.
- Record: a durable entry describing what changed, why, and what proof surface was used.

---

## 2. Amendment Process (Required)

An amendment is valid only if all of the following are true:

1. The change is explicit.
   - Update the binding doc text (no "implied" policy).
2. The change is routed.
   - Ensure `embedded/core/DECAPOD.md` reaches the updated/added canonical docs via `## Links`.
3. The change is recorded.
   - Add an entry to the Amendment Log in this document (§6).
4. The change is claim-safe.
   - If the change introduces/updates a guarantee, register/update the claim in `embedded/interfaces/CLAIMS.md`.
5. The change is deprecation-safe.
   - If the change replaces or retires binding meaning, follow `embedded/core/DEPRECATION.md`.
6. The change is validated.
   - Run `decapod validate` for the relevant store(s) and record it in the log entry.

---

## 3. Required Co-Updates (No Drift)

When a binding doc change touches these areas, the following co-updates are required:

- Doc graph and canon:
  - Update `embedded/core/DECAPOD.md` routing as needed.
  - Regenerate `docs/DOC_MAP.md` (derived; do not hand-edit).
- Doc compiler and authority routing:
   - If header fields, layers, truth labels, reachability, or decision rights change: update `embedded/interfaces/DOC_RULES.md`.
- Subsystems and extensibility:
  - If a subsystem is added/removed/renamed/status-changed: update `embedded/core/PLUGINS.md`.
  - If shipped CLI surfaces change: ensure `decapod validate` gates cover the drift.
- Store semantics and safety:
   - If store selection or purity model changes: update `embedded/interfaces/STORE_MODEL.md`.
- Claims and promises:
   - If a guarantee/invariant changes: update `embedded/interfaces/CLAIMS.md`.
- Deprecations and migrations:
  - If anything is being retired: update `embedded/core/DEPRECATION.md`.

---

## 4. No "Interpretation" As Resolution

If two canonical binding docs appear to disagree, the system is in an invalid state.

Resolution is not interpretation; resolution is an amendment to eliminate the disagreement (claim: claim.doc.no_contradicting_canon).

---

## 5. Emergency Changes

If urgent work must proceed while governance is unclear:

- Follow `embedded/plugins/EMERGENCY_PROTOCOL.md`.
- Do not mutate stores or ship new requirements based on assumption.
- Record an amendment entry that flags `EMERGENCY` and describes the risk and follow-up.

---

## 6. Amendment Log (Append-Only)

Each entry MUST include:

- Date (YYYY-MM-DD)
- Docs changed
- Summary of binding meaning change
- Claims added/changed (claim-ids)
- Deprecations added/updated (if any)
- Proof surface run (`decapod validate` store(s), plus any other named proofs)

### 2026-02-09

- Docs changed:
  - `embedded/specs/AMENDMENTS.md` (introduced)
  - `embedded/core/CLAIMS.md` (introduced)
  - `embedded/core/DEPRECATION.md` (introduced)
  - `embedded/core/GLOSSARY.md` (introduced)
  - `embedded/plugins/EMERGENCY_PROTOCOL.md` (introduced)
  - `embedded/core/DECAPOD.md` (delegation charter + routing)
  - `embedded/core/DOC_RULES.md` (decision rights + truth label constraints)
- Summary:
  - Established explicit change control, claims ledger, and deprecation contract as binding governance surfaces.
- Claims added/changed:
  - `claim.doc.real_requires_proof`
  - `claim.doc.no_shadow_policy`
  - `claim.doc.no_contradicting_canon`
  - `claim.doc.decapod_is_router_only`
  - `claim.store.blank_slate`
  - `claim.store.no_auto_seeding`
  - `claim.store.explicit_store_selection`
- Deprecations:
  - None.
- Proof surface run:
  - `decapod validate` (expected; record exact store(s) when run)

---

## Links

- `embedded/core/DECAPOD.md` - Router and navigation charter
- `embedded/interfaces/DOC_RULES.md` - Doc compilation rules
- `embedded/interfaces/CLAIMS.md` - Promises ledger
- `embedded/core/DEPRECATION.md` - Deprecation contract
- `embedded/plugins/EMERGENCY_PROTOCOL.md` - Emergency protocols
- `embedded/core/PLUGINS.md` - Subsystem registry
- `embedded/interfaces/STORE_MODEL.md` - Store semantics
- `embedded/specs/SYSTEM.md` - System definition
- `embedded/specs/INTENT.md` - Intent contract
- `embedded/specs/SECURITY.md` - Security doctrine
- `embedded/specs/GIT.md` - Git workflow
