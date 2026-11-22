# Agent Governance Envelope Profile v1 (`decapod.governance-envelope:v1`)

Status: normative v1 specification for transport-neutral governance envelope exchange (`#870`, `#871`, `#1733`).

## Four Independently Versioned Semantic Planes

`GovernanceEnvelopeV1` composes four independently versioned planes into one declared profile (`decapod.governance-envelope:v1`):

1. **`intent` (`plane_versions.intent`)**: Repository, tenant, task, run, operation, actor, recipient, replay domain, lease generation, original human request hash, agent interpretation summary, and whether the human request was independently observed.
2. **`authority` (`plane_versions.authority`)**: Requested operation, canonical parameters, exact approved call binding, explicit separation between authenticated human approval and agent plan approval, issuance/expiry timestamps, optional narrowing delegation grant, and mandatory vs optional extensions.
3. **`evidence` (`plane_versions.evidence`)**: Producer identity and trust class (`independent_executor`, `trusted_ci`, `self_declared`, `untrusted`), exact source tree/head/base commits, proof-plan epoch, claimed receipt digests, raw receipt bytes, environment tamper signals, contiguous history sequence, optional independently held terminal sequence, and independent effect observation flag.
4. **`outcome` (`plane_versions.outcome`)**: Claim kind (`attempt`, `effect`, `precommit_containment`, `goal_completion`), declared vs observed status, assertion count, optional HTTP status, separate process-path authorization vs behavioral pass flags, and target URI/path ambiguity checks.

## Authority Isolation and Import Rules

- **Authority is never portable by default.** `import_external_envelope` always returns `imported_authority_granted = false`. Schema validity, envelope digest validity, or producer authentication establishes external evidence only; local mutation, completion, or publication requires explicit repository-local acceptance and kernel reauthorization.
- **Version compatibility:** Any major version mismatch on any plane (`major != 1`) or unknown **mandatory** extension fails closed. Unknown **optional** extensions degrade safely and are reported sorted in `omitted_optional_extensions`.
- **Replay and tenant isolation:** Envelopes are scoped to `(replay_domain, repository_id, tenant_id, run_id, op_id)` and bound to `expected_recipient_id`. Exact replays, retired run IDs after restart, and duplicate `op_id` with mutated payloads are rejected (`replay_detected`).
- **Summary-fiction rejection:** Claimed 64-hex digests or self-generated checksum lists without verified raw receipt bytes are never upgraded to verified proof (`summary_fiction_missing_raw_receipts`). Mutated raw bytes fail with `raw_receipt_digest_mismatch`.
- **Delegation narrowing:** Child grants that widen parent action, recipient, expiry, path prefix, or budget are refused (`authority_widening_denied`).

## Decision-Time 4-Disposition Projection

`project_decision_disposition` maps decision boundaries read-only to four dispositions (`admit`, `repair_required`, `authority_required`, `reject`), keeping `invalid`, `unknown`, and `unavailable` evaluation states strictly separate from implicit admission and rejecting proposed repairs that expand scope or alter arguments outside bounds.

## Independent Consumer Conformance

Pinned raw vectors live in `tests/fixtures/agent_interop_vectors.v1.json` and are evaluated by:
1. Decapod's `core::agent_interop::import_external_envelope` engine;
2. The standalone Python standard-library consumer `tests/fixtures/agent_interop_reference_consumer.py` (no Decapod imports, DB, or CLI dependency);
3. An independent raw-JSON Rust verifier in `tests/agent_interop_conformance.rs`.

All independent consumers must agree on both normalized verdict and decisive `reason_class`.
