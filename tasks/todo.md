# Task Tracking

Primary task tracking lives in `decapod todo` (event-sourced in `.decapod/data/todo.db`).
This file tracks higher-level initiatives and their proof status.

---

## Active Initiatives

### Phase 3 LCM + Map Operators
- **Status**: In Progress
- **Scope**: Wire LCM/Map into RPC, capabilities, schema; add rebuild command; add validation gates
- **Completed**:
  - [x] Add LCM/Map to `decapod capabilities` output
  - [x] Add LCM/Map to `decapod data schema` output
  - [x] Add `decapod lcm rebuild` command
  - [x] Add LCM rebuild validation gate
  - [x] Register LCM claims in CLAIMS registry
  - [x] Add map.events.jsonl to flight-recorder timeline
  - [x] Add lcm.events.jsonl to flight-recorder timeline
  - [x] Worktree exemption for schema commands
- **In Progress**:
  - [ ] Context pack integration (LCM summaries in context packs)
- **Gate**: `decapod validate` must pass

### ObligationNode Evolution
- **Status**: In progress
- **Plan**: `IMPLEMENTATION_TASK_PLAN.md`
- **Tasks**: `OBLIGATION_TASKS.md`
- **Gate**: `decapod validate` must pass after each task

### Instruction Stack Completion
- **Status**: In progress
- **Scope**: CLAUDE.md, IDENTITY.md, TOOLS.md, PLAYBOOK.md, AGENTS.md updates
- **Gate**: `decapod validate` (entrypoint gate checks for presence)

### Co-Player Policy Tightening
- **Status**: Implemented
- **Scope**: `src/core/coplayer.rs` — `derive_policy()` + `CoPlayerPolicy`
- **ADR**: `tasks/decisions/002-coplayer-policy-tightening.md`
- **Gate**: `validate_coplayer_policy_tightening` in `src/core/validate.rs`
- **Proof**: Unit test `test_policy_only_tightens` + validate gate

### Proof Hardening
- **Status**: Tracking
- **Scope**: CLAIMS registry → gate mapping completeness
- **Items**:
  - [ ] `claim.todo.claim_before_work` → needs automated enforcement (currently procedural)
  - [ ] `claim.broker.is_spec` → DB Broker graduation from SPEC to REAL
  - [ ] `claim.risk_policy.single_contract_source` → machine-readable risk tiers
  - [ ] `claim.context_pack.canonical_layout` → validate gate for context pack
  - [ ] `claim.memory.append_only_logs` → append-only validation checks
  - [ ] `claim.memory.distill_proof_required` → deterministic distill proof
  - [x] `claim.coplayer.only_tightens` → validate gate enforces only-tighten invariant

---

## Commitments Ledger (Audit Summary)

### ENFORCED (Gate exists, blocks on failure)

| Commitment | Gate | Proof Path |
|-----------|------|------------|
| No work on main/master | `decapod validate` Git Protected Branch Gate | `src/core/validate.rs` |
| CLI-only `.decapod` access | Four Invariants Gate | `src/core/validate.rs` |
| Blank-slate user store | `decapod validate --store user` | `src/core/validate.rs` |
| No repo→user auto-seeding | `decapod validate --store user` | `src/core/validate.rs` |
| Doc graph reachability | `decapod validate` doc graph gate | `src/core/validate.rs` |
| Mandatory tests for code changes | `cargo test` + CI merge gate | `.github/workflows/ci.yml` |
| Federation store-scoped | `decapod validate` federation.store_purity | `src/plugins/federation.rs` |
| Federation append-only critical | `decapod validate` federation.write_safety | `src/plugins/federation.rs` |
| Validation determinism | Same repo state → same results | `src/core/validate.rs` |
| Secret redaction in traces | Pattern-based redaction | `src/core/trace.rs` |
| Entrypoint files exist | Entrypoint gate | `src/core/validate.rs` |
| Co-player policy only tightens | Co-Player Policy Tightening Gate | `src/core/validate.rs` |
| LCM append-only ledger | `decapod validate` LCM Immutability Gate | `src/core/validate.rs` |
| LCM index rebuildable | `decapod validate` LCM Rebuild Gate | `src/core/validate.rs` |
| LCM content hash deterministic | `decapod validate` LCM Immutability Gate | `src/core/validate.rs` |
| Map scope-reduction invariant | `decapod map agentic --retain` required | CLI argument parsing |

### PARTIALLY ENFORCED (Gate exists but has procedural gaps)

| Commitment | Current State | Path to Full Enforcement |
|-----------|---------------|--------------------------|
| Claim before work | `decapod todo claim` records ownership | Need validate gate that checks claim exists before PR merge |
| Session password required | `session.acquire` + password check | Broader cryptographic hardening planned |
| Container workspace required | Runtime preflight exists | Policy-level enforcement future work |
| Store selection explicit | Validate gate exists | CLI footguns remain |
| Co-player diff limits | `derive_policy()` computes limits | Runtime enforcement in workspace gate needed |

### NOT YET ENFORCED (Aspirational)

| Commitment | Tracking | Priority |
|-----------|----------|----------|
| Risk policy gates (machine-readable) | `claim.risk_policy.single_contract_source` | Medium |
| Context-pack layout validation | `claim.context_pack.canonical_layout` | Medium |
| Memory append-only verification | `claim.memory.append_only_logs` | Low |
| Memory distill proof | `claim.memory.distill_proof_required` | Low |
| Doc contradiction detection | Planned | Low |
| Duplicate authority detection | Planned | Low |

---

## Drift Risks (Top 5)

1. **CLAIMS registry drift**: New guarantees added to docs without corresponding CLAIMS.md entries. Mitigation: validate gate checks claim registration.
2. **Constitution doc staleness**: Embedded docs diverge from reality as code evolves. Mitigation: `decapod validate` doc graph checks.
3. **Event log / SQLite divergence**: SQLite state diverges from JSONL event log. Mitigation: `decapod lcm rebuild` deterministic replay. Needs regular CI assertion.
4. **Test coverage gaps on validation gates**: New gates added without regression tests. Mitigation: `tests/entrypoint_correctness.rs` covers entrypoints; needs expansion.
5. **Session TTL silently breaking background operations**: Long-running batch operations fail partway through. Mitigation: documented in `tasks/lessons.md`; needs retry logic in CLI.
