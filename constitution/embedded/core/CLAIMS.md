# CLAIMS.md - Claims Ledger (Promises and Proof Surfaces)

**Authority:** interface (registry of guarantees and their proof surfaces)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** table-driven ledger of explicit guarantees/invariants and where they are proven/enforced
**Non-goals:** replacing specs; this is an index of promises, not the full spec text

This ledger exists to prevent "forgotten invariants" and accidental promise drift.

Rule: if a canonical doc makes a guarantee/invariant, it MUST be registered here with a claim-id.

---

## 1. Table Schema

Columns:

- Claim ID: stable identifier (`claim.<domain>.<name>`).
- Claim (normative): the promise, phrased as a single sentence.
- Owner Doc: where the claim is specified (the full text and any caveats live there).
- Enforcement: `enforced` | `partially_enforced` | `not_enforced`.
- Proof Surface: named, runnable surface(s) that can detect drift (e.g. `decapod validate`, schema checks).
- Notes: brief context, limitations, or migration pointers.

---

## 2. Claims (Binding Registry)

| Claim ID | Claim (normative) | Owner Doc | Enforcement | Proof Surface | Notes |
|---|---|---|---|---|---|
| claim.doc.decapod_is_router_only | `embedded/core/MAESTRO.md` routes and prioritizes canonical docs but does not define or override behavioral rules. | `embedded/core/MAESTRO.md` | partially_enforced | `decapod validate` (doc graph + canon headers) | Social + doc-layer boundary; code enforcement is limited. |
| claim.doc.no_shadow_policy | If a rule is not declared in canonical docs, it is not enforceable. | `embedded/core/DOC_RULES.md` | partially_enforced | `decapod validate` (doc graph) | Enforcement of "shadow policy" is largely procedural. |
| claim.doc.real_requires_proof | Any `REAL` interface claim requires a named proof surface; otherwise it must be `STUB` or `SPEC`. | `embedded/core/DOC_RULES.md` | not_enforced | planned: validate checks for proof surface annotations | Current enforcement is doc-level; future validate gate can check. |
| claim.doc.decapod_reaches_all_canonical | `embedded/core/MAESTRO.md` reaches every canonical doc via the `## Links` graph. | `embedded/core/DOC_RULES.md` | enforced | `decapod validate` (doc graph gate) | Prevents buried canonical law and unreachable contracts. |
| claim.doc.no_duplicate_authority | No requirement may be defined in multiple canonical docs; duplicates must defer to the owner doc. | `embedded/core/DOC_RULES.md` | not_enforced | planned: validate checks for duplicated requirements | Procedural today; becomes enforceable only with additional tooling. |
| claim.doc.no_contradicting_canon | If two canonical binding docs appear to disagree, the system is invalid; resolution is amendment, not interpretation. | `embedded/specs/AMENDMENTS.md` | not_enforced | `decapod validate` (planned: contradiction checks) | Humans must treat contradictions as a stop condition. |
| claim.store.blank_slate | A fresh user store contains no TODOs unless the user adds them. | `embedded/core/STORE_MODEL.md` | enforced | `decapod validate --store user` | Protects user-store privacy and blank slate semantics. |
| claim.store.no_auto_seeding | Repo store content must never appear in the user store automatically. | `embedded/core/STORE_MODEL.md` | enforced | `decapod validate --store user` | Prevents cross-store contamination. |
| claim.store.explicit_store_selection | Mutating commands must be treated as undefined unless store context is explicit; `--store` is preferred and `--root` is dangerous. | `embedded/core/STORE_MODEL.md` | partially_enforced | `decapod validate` (store invariants) | CLI behavior may still allow footguns; treated as a red-line constraint. |
| claim.proof.executable_check | A "proof" is an executable check that can fail loudly (tests, linters, validators, etc). No new DSL. | `embedded/core/PLUGINS.md` | enforced | `decapod validate` | Definition is normative; proof registry (Epoch 1) will formalize. |
| claim.concurrency.no_git_solve | Decapod does not "solve" Git merge conflicts; it reduces collisions via work partitioning and proof gates. | `embedded/core/PLUGINS.md` | enforced | N/A (doc-level constraint) | Prevents over-claiming on concurrency. |
| claim.broker.is_spec | DB Broker (serialized writes, audit) is SPEC, not REAL. Do not claim it is implemented. | `embedded/core/PLUGINS.md` | enforced | `decapod validate` (truth label check) | Will graduate to REAL in Epoch 4. |
| claim.test.mandatory | Every code change must have corresponding tests. No exceptions. | `embedded/specs/ARCHITECTURE.md` | enforced | `cargo test` + CI | Tests gate merge; untested code is rejected. |

---

## 3. Workflow: Registering/Updating a Claim

When adding or changing a guarantee:

1. Add/update the claim row here.
2. Ensure the owner doc references the claim-id near the guarantee.
3. Ensure the claim has a proof surface, or do not label it `REAL`.
4. If the change deprecates older binding meaning, follow `embedded/core/DEPRECATION.md`.
5. Record the change as an amendment: `embedded/specs/AMENDMENTS.md`.

---

## Links

- `embedded/core/MAESTRO.md`
- `embedded/core/DOC_RULES.md`
- `embedded/core/DEPRECATION.md`
- `embedded/specs/AMENDMENTS.md`
- `embedded/core/STORE_MODEL.md`
- `embedded/core/PLUGINS.md`
