# ADR: Agent Interoperability, Bounded Transport Adapters, and Offline Capability Governance

Cross-references: `#870`, `#871`, `#872`, `#873`, `#875`, `#878`, `#1715`, `#1716`, `#1717`, `#1718`, `#1724`, `#1725`, `#1733`, `#1735`.

## Decision

Keep Decapod's local repository state, custody leases, and validation proof policy as the **sole authority** while exposing modular, offline, fail-closed interoperability primitives under `src/decapod/core/agent_interop.rs`:

- **Transport-neutral governance envelope (`#870`, `#1733`)**: Four independently versioned planes (`intent`, `authority`, `evidence`, `outcome`), replay/tenant isolation, delegation narrowing, summary-fiction rejection, and independent-consumer vector parity (`assets/schemas/agent-interop-envelope-v1.schema.json`, `tests/fixtures/agent_interop_vectors.v1.json`, `tests/fixtures/agent_interop_reference_consumer.py`).
- **Truthful conformance receipts and two-way drift (`#871`)**: Static `builtin_manifest()` capabilities remain `not_run` with source-coverage references. Observed execution receipts (`ObservedConformanceReceipt`) bind verified/failed promotions to exact binary SHA, source commit, schema/profile version, transport, accepted producer trust class, raw evidence bytes, and freshness TTL. Two-way drift detection (`check_dispatcher_authorization_drift`) audits advertised manifest entries, actual RPC dispatch routes, session guards, and read-only profile allowlists.
- **Bounded transport adapters (`#872`, `#873`)**:
  - `decapod mcp serve|schema|config`: finite stdio JSON-RPC 2.0 adapter with frame/response caps, secret/rubric redaction, and CLI/RPC gate parity; daemonless exit on EOF/`--one-shot`/`--max-requests`/`shutdown`.
  - `decapod a2a card|handoff`: truthful A2A Agent Card advertising only executable governance skills with separate core-A2A vs Decapod-extension reporting, plus fail-closed handoff validation.
- **Offline capability, skill, manifest, composition, rule, and overlap engines (`#875`, `#878`, `#1715`–`#1718`, `#1724`, `#1725`)**: Offline registry with license/provenance quarantine and cycle detection, token-budgeted skill compiler with A/B evaluation, canonical manifest compiler with source maps and policy-conflict detection, safe fragment composition denying prompt injection and privilege escalation, 8-domain machine-readable coding-rule taxonomy (zero third-party product names), typed rule precedence evaluator, and 6-state semantic task-scope overlap classification with separate task-local vs combined integration proof.

## Honest Assurance Classification (`#1735` Proof Bar)

| Surface | Classification | Evidence / Remaining External Scope |
| --- | --- | --- |
| Offline capability registry, skill discovery/bundle compiler, manifest compiler, fragment composition, coding-rule taxonomy & metadata evaluator, 6-state task overlap (`#878`, `#1715`–`#1718`, `#1724`, `#1725`) | **Proven** (local deterministic) | Unit tests in `src/decapod/core/agent_interop.rs` + integration suite `tests/agent_interop_conformance.rs`. |
| 4-plane governance envelope v1, replay/import/authority isolation, summary-fiction rejection, independent Python + Rust consumer parity (`#870`, `#1733`) | **Partial** (local & independent-consumer proven; cross-org/CI external runs ongoing) | `tests/agent_interop_conformance.rs`, `tests/fixtures/agent_interop_vectors.v1.json`, `tests/fixtures/agent_interop_reference_consumer.py`. |
| Two-way dispatcher/authorization drift & observed-conformance receipt projection (`#871`) | **Partial** (local receipt binding & drift detection proven; hosted CI producer signing external) | `check_dispatcher_authorization_drift` + `project_manifest_with_receipt` tests. |
| Bounded stdio MCP & A2A adapters (`#872`, `#873`) | **Partial** (protocol framing, redaction, gate parity, Agent Card & handoff validation proven locally; live external host activation tracked separately in `#880`) | `decapod mcp` / `decapod a2a` CLI & black-box tests in `tests/agent_interop_conformance.rs`. |
| Harness-native packaging & compatibility matrix (`#875`) | **Partial** (offline package generation, conflict-safe merge/rollback, and honest matrix proven; live real-host matrix runs remain external) | `harness_compatibility_matrix()` explicitly records `live_host_verified: false` for all 6 hosts. |
| End-to-end multi-agent cold-context continuation campaign (`#1735`) | **Target** (architectural contract & falsifiers documented; live external agent B continuation remains tracked by `#1367`/`#1758`) | Cross-referenced without premature closure claims. |
