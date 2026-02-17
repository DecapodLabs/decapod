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
   - Ensure `core/DECAPOD.md` reaches the updated/added canonical docs via `## Links`.
3. The change is recorded.
   - Add an entry to the Amendment Log in this document (§6).
4. The change is claim-safe.
   - If the change introduces/updates a guarantee, register/update the claim in `interfaces/CLAIMS.md`.
5. The change is deprecation-safe.
   - If the change replaces or retires binding meaning, follow `core/DEPRECATION.md`.
6. The change is validated.
   - Run `decapod validate` for the relevant store(s) and record it in the log entry.

---

## 3. Required Co-Updates (No Drift)

When a binding doc change touches these areas, the following co-updates are required:

- Doc graph and canon:
  - Update `core/DECAPOD.md` routing as needed.
  - Regenerate `docs/DOC_MAP.md` (derived; do not hand-edit).
- Doc compiler and authority routing:
   - If header fields, layers, truth labels, reachability, or decision rights change: update `interfaces/DOC_RULES.md`.
- Subsystems and extensibility:
  - If a subsystem is added/removed/renamed/status-changed: update `core/PLUGINS.md`.
  - If shipped CLI surfaces change: ensure `decapod validate` gates cover the drift.
- Store semantics and safety:
   - If store selection or purity model changes: update `interfaces/STORE_MODEL.md`.
- Claims and promises:
   - If a guarantee/invariant changes: update `interfaces/CLAIMS.md`.
- Deprecations and migrations:
  - If anything is being retired: update `core/DEPRECATION.md`.

---

## 4. No "Interpretation" As Resolution

If two canonical binding docs appear to disagree, the system is in an invalid state.

Resolution is not interpretation; resolution is an amendment to eliminate the disagreement (claim: claim.doc.no_contradicting_canon).

---

## 5. Emergency Changes

If urgent work must proceed while governance is unclear:

- Follow `plugins/EMERGENCY_PROTOCOL.md`.
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
  - `specs/AMENDMENTS.md` (introduced)
  - `core/CLAIMS.md` (introduced)
  - `core/DEPRECATION.md` (introduced)
  - `core/GLOSSARY.md` (introduced)
  - `plugins/EMERGENCY_PROTOCOL.md` (introduced)
  - `core/DECAPOD.md` (delegation charter + routing)
  - `core/DOC_RULES.md` (decision rights + truth label constraints)
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

### 2026-02-17

- Docs changed:
  - `interfaces/RISK_POLICY_GATE.md` (introduced)
  - `interfaces/AGENT_CONTEXT_PACK.md` (introduced)
  - `interfaces/CLAIMS.md` (claims added for risk-policy and context-pack contracts)
  - `core/INTERFACES.md` (registry routing updated)
  - `contracts/risk-policy.example.json` (introduced machine-readable template)
  - `src/core/validate.rs` (presence/structure gate for new interfaces and template)
- Summary:
  - Added binding interface contracts for deterministic PR risk-policy gating and Decapod-native agent context-pack governance.
  - Registered new SPEC claims and added minimal loud-fail validation for required contract artifacts and section markers.
- Claims added/changed:
  - `claim.risk_policy.single_contract_source`
  - `claim.risk_policy.preflight_before_fanout`
  - `claim.review.sha_freshness_required`
  - `claim.review.single_rerun_writer`
  - `claim.review.remediation_loop_reenters_policy`
  - `claim.evidence.manifest_required_for_ui`
  - `claim.harness.incident_to_case_loop`
  - `claim.context_pack.canonical_layout`
  - `claim.context_pack.deterministic_load_order`
  - `claim.context_pack.mutation_authority_rules`
  - `claim.memory.append_only_logs`
  - `claim.memory.distill_proof_required`
  - `claim.context_pack.security_scoped_loading`
  - `claim.context_pack.correction_loop_governed`
- Deprecations:
  - None.
- Proof surface run:
  - `decapod validate` (attempted in repo store; currently fails due `RusqliteError(SystemIoFailure, "disk I/O error")`)

---

## Links

### Core Router
- `core/DECAPOD.md` - **Router and navigation charter (START HERE)**

### Authority (Constitution Layer)
- `specs/INTENT.md` - **Methodology contract (READ FIRST)**
- `specs/SYSTEM.md` - System definition and authority doctrine
- `specs/SECURITY.md` - Security contract
- `specs/GIT.md` - Git etiquette contract

### Registry (Core Indices)
- `core/PLUGINS.md` - Subsystem registry
- `core/INTERFACES.md` - Interface contracts index
- `core/DEPRECATION.md` - Deprecation contract

### Contracts (Interfaces Layer)
- `interfaces/DOC_RULES.md` - Doc compilation rules
- `interfaces/CLAIMS.md` - Promises ledger
- `interfaces/STORE_MODEL.md` - Store semantics
- `interfaces/GLOSSARY.md` - Term definitions

### Operations (Plugins Layer)
- `plugins/EMERGENCY_PROTOCOL.md` - Emergency protocols
- `plugins/TODO.md` - Work tracking
