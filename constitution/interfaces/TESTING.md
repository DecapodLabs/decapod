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

### 5.1 Validate Liveness Invariant (`claim.validate.bounded_termination`)

`decapod validate` MUST terminate in bounded time.

If DB contention prevents progress, validate MUST fail with a typed error marker:

- `VALIDATE_TIMEOUT_OR_LOCK`

and MUST provide remediation guidance (retry with backoff / inspect concurrent processes).

### 5.2 Variance Eval Proof Surfaces

For frontend/backend non-deterministic promotion paths, the following deterministic tests are required:

1. Golden aggregation determinism:
   - fixed synthetic run/verdict set -> deterministic aggregate delta + CI + gate decision.
2. Judge contract validation:
   - malformed judge JSON fails with `EVAL_JUDGE_JSON_CONTRACT_ERROR`.
3. Judge bounded execution:
   - timeout path fails with `EVAL_JUDGE_TIMEOUT` and blocks eval gate.
4. Reproducibility lineage:
   - changing critical plan settings changes `plan_hash`;
   - cross-plan comparison fails unless explicit acknowledge flag is provided.

### 5.3 Eval Gate Contract

When eval gating is marked required, `decapod validate` and workspace publish MUST fail unless:

1. Referenced aggregate artifact exists.
2. Minimum run count criteria are met.
3. Bootstrap CI is present.
4. No gate-level regression condition is triggered.
5. Judge timeout failures are zero.

### 5.4 Skill Governance Proof Surfaces

For skill ingestion/resolution to be promotion-relevant, the following checks are required:

1. SKILL.md import determinism:
   - same SKILL.md source content -> identical `skill_card.card_hash`.
2. Skill resolution determinism:
   - same query + same skill store state -> identical `skill_resolution.resolution_hash`.
3. Artifact integrity:
   - tampered `skill_card` or `skill_resolution` hash fails `decapod validate`.
4. Bounded authority:
   - unmanaged external skill text cannot silently become promotion authority without control-plane artifacts.

---

## Links

- `core/INTERFACES.md` - Interface contracts registry
- `core/PLUGINS.md` - Subsystem proof surfaces
- `specs/INTENT.md` - Intent proof doctrine
- `plugins/VERIFY.md` - Validation subsystem
