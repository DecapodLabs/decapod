---
id: ADR-002
title: Co-Player Policy Tightening Invariant
status: accepted
date: 2026-02-21
scope: Multi-agent governance and trust
---

# ADR-002: Co-Player Policy Tightening Invariant

## Context

Decapod tracks agent interactions via an append-only trace ledger and derives
co-player snapshots (reliability scores, risk profiles) from that data. These
snapshots need to drive coordination constraints (diff limits, proof
requirements, handshake mandates).

The critical invariant: snapshot-derived policies MUST only tighten constraints
as reliability decreases. A less-reliable agent must never receive looser
constraints than a more-reliable one. No snapshot should ever bypass proof gates.

## Decision

1. Add `CoPlayerPolicy` struct and `derive_policy()` function to `src/core/coplayer.rs`.
2. Policy derivation is fully deterministic: fixed thresholds, no stochastic scoring.
3. `require_validation` is hardcoded to `true` for ALL agents regardless of reliability.
4. Unknown agents get mandatory handshake + smallest diff limits (100 lines).
5. High-risk agents get extra proof requirements + broad refactor prohibition.
6. Add `validate_coplayer_policy_tightening` gate to `src/core/validate.rs`.
7. Gate tests the invariant structurally: generates policies for all risk profiles
   and verifies monotonic tightening.

## Alternatives Considered

- **Soft/advisory policies**: Rejected. Policies that don't constrain behavior provide
  no governance value.
- **Dynamic thresholds based on project context**: Rejected. Violates determinism
  invariant. Thresholds are fixed in code.
- **Trust delegation (high-reliability agents can vouch for others)**: Rejected.
  Violates "proof over persuasion" — only evidence counts, not endorsements.

## Consequences

- Every agent, regardless of track record, must pass `decapod validate`.
- Unknown agents face strict constraints until they build a trace history.
- The mentor/obligations engine can consume `CoPlayerPolicy` to generate
  adaptive MUST obligations based on co-player risk profiles.
- Adding new risk tiers requires updating `derive_policy()` and the validation gate.

## Proof Impact

- Gate: `validate_coplayer_policy_tightening` in `src/core/validate.rs`
- Unit tests: `test_policy_only_tightens` in `src/core/coplayer.rs`
- Claim: `claim.coplayer.only_tightens` (partially_enforced — gate validates
  structural invariant; runtime enforcement of diff limits is future work)
