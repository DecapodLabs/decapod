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
| claim.doc.decapod_is_router_only | `core/DECAPOD.md` routes and prioritizes canonical docs but does not define or override behavioral rules. | `core/DECAPOD.md` | partially_enforced | `decapod validate` (doc graph + canon headers) | Social + doc-layer boundary; code enforcement is limited. |
| claim.doc.no_shadow_policy | If a rule is not declared in canonical docs, it is not enforceable. | `interfaces/DOC_RULES.md` | partially_enforced | `decapod validate` (doc graph) | Enforcement of "shadow policy" is largely procedural. |
| claim.doc.real_requires_proof | Any `REAL` interface claim requires a named proof surface; otherwise it must be `STUB` or `SPEC`. | `interfaces/DOC_RULES.md` | not_enforced | planned: validate checks for proof surface annotations | Current enforcement is doc-level; future validate gate can check. |
| claim.doc.decapod_reaches_all_canonical | `core/DECAPOD.md` reaches every canonical doc via the `## Links` graph. | `interfaces/DOC_RULES.md` | enforced | `decapod validate` (doc graph gate) | Prevents buried canonical law and unreachable contracts. |
| claim.doc.no_duplicate_authority | No requirement may be defined in multiple canonical docs; duplicates must defer to the owner doc. | `interfaces/DOC_RULES.md` | not_enforced | planned: validate checks for duplicated requirements | Procedural today; becomes enforceable only with additional tooling. |
| claim.doc.no_contradicting_canon | If two canonical binding docs appear to disagree, the system is invalid; resolution is amendment, not interpretation. | `specs/AMENDMENTS.md` | not_enforced | `decapod validate` (planned: contradiction checks) | Humans must treat contradictions as a stop condition. |
| claim.store.blank_slate | A fresh user store contains no TODOs unless the user adds them. | `interfaces/STORE_MODEL.md` | enforced | `decapod validate --store user` | Protects user-store privacy and blank slate semantics. |
| claim.store.no_auto_seeding | Repo store content must never appear in the user store automatically. | `interfaces/STORE_MODEL.md` | enforced | `decapod validate --store user` | Prevents cross-store contamination. |
| claim.store.explicit_store_selection | Mutating commands must be treated as undefined unless store context is explicit; `--store` is preferred and `--root` is dangerous. | `interfaces/STORE_MODEL.md` | partially_enforced | `decapod validate` (store invariants) | CLI behavior may still allow footguns; treated as a red-line constraint. |
| claim.store.decapod_cli_only | Agents must not read/write `<repo>/.decapod/*` files directly; access must go through `decapod` CLI surfaces. | `interfaces/STORE_MODEL.md` | enforced | `decapod validate` (Four Invariants Gate marker checks) | Prevents jailbreak-style state tampering and out-of-band mutation. |
| claim.foundation.intent_state_proof_primitives | Decapod governance is anchored on explicit intent, explicit state boundaries, and executable proof surfaces. | `core/DECAPOD.md` | partially_enforced | `decapod validate` + canonical doc graph gates | Foundation doctrine is explicit; full semantic enforcement remains incremental. |
| claim.foundation.daemonless_repo_native_canonicality | Decapod remains daemonless and repo-native for promotion-relevant state and evidence. | `specs/SYSTEM.md` | partially_enforced | `decapod validate` + repo-native manifest/provenance gates | Operationally enforced in current control plane; hardening continues through gate expansion. |
| claim.foundation.proof_gated_promotion | Promotion-relevant outcomes are invalid without executable proof and machine-verifiable artifacts. | `specs/SYSTEM.md` | partially_enforced | `decapod validate` + workspace publish proof gates | Publish paths enforce this today; broader policy coupling is still evolving. |
| claim.proof.executable_check | A "proof" is an executable check that can fail loudly (tests, linters, validators, etc). No new DSL. | `core/PLUGINS.md` | enforced | `decapod validate` | Definition is normative; proof registry (Epoch 1) will formalize. |
| claim.concurrency.no_git_solve | Decapod does not "solve" Git merge conflicts; it reduces collisions via work partitioning and proof gates. | `core/PLUGINS.md` | partially_enforced | `decapod validate` (workspace/protected-branch gates) | Prevents over-claiming on concurrency; residual merge semantics remain Git-native. |
| claim.broker.is_spec | DB Broker (serialized writes, audit) is SPEC, not REAL. Do not claim it is implemented. | `core/PLUGINS.md` | enforced | `decapod validate` (truth label check) | Will graduate to REAL in Epoch 4. |
| claim.test.mandatory | Every code change must have corresponding tests. No exceptions. | `methodology/ARCHITECTURE.md` | enforced | `cargo test` + CI | Tests gate merge; untested code is rejected. |
| claim.federation.store_scoped | Federation data exists only under the selected store root. | `plugins/FEDERATION.md` | enforced | `decapod validate` (federation.store_purity gate) | Prevents cross-store contamination. |
| claim.federation.provenance_required_for_critical | Critical federation nodes must have ≥1 valid provenance source with scheme prefix. | `plugins/FEDERATION.md` | enforced | `decapod validate` (federation.provenance gate) | Prevents hallucination anchors. |
| claim.federation.append_only_critical | Critical types (decision, commitment) cannot be edited in place; must be superseded. | `plugins/FEDERATION.md` | enforced | `decapod validate` (federation.write_safety gate) | Write-safety for operational truth. |
| claim.federation.lifecycle_dag_no_cycles | The supersedes edge graph contains no cycles. | `plugins/FEDERATION.md` | enforced | `decapod validate` (federation.lifecycle_dag gate) | Prevents infinite supersession loops. |
| claim.risk_policy.single_contract_source | Risk tiers, required checks, docs drift, and evidence requirements are defined in one machine-readable contract source. | `interfaces/RISK_POLICY_GATE.md` | not_enforced | planned: `risk-policy-gate` + `decapod validate` contract-shape checks | SPEC until runtime gate consumes contract as source of truth. |
| claim.risk_policy.preflight_before_fanout | Risk-policy preflight must complete successfully before expensive CI fanout starts. | `interfaces/RISK_POLICY_GATE.md` | not_enforced | planned: `risk-policy-gate` | SPEC pending CI orchestration enforcement. |
| claim.review.sha_freshness_required | Review-agent state is valid only when tied to current PR head SHA. | `interfaces/RISK_POLICY_GATE.md` | not_enforced | planned: review check-run head SHA verifier | SPEC pending implementation. |
| claim.review.single_rerun_writer | Exactly one canonical rerun writer may request review reruns, deduped by marker plus head SHA. | `interfaces/RISK_POLICY_GATE.md` | not_enforced | planned: rerun-writer dedupe gate | SPEC pending enforcement surface. |
| claim.review.remediation_loop_reenters_policy | Automated remediation must push to the same PR branch and re-enter policy gates; bypass is forbidden. | `interfaces/RISK_POLICY_GATE.md` | not_enforced | planned: remediation workflow policy gate | SPEC pending deterministic remediation implementation. |
| claim.evidence.manifest_required_for_ui | UI and critical flow changes require machine-verifiable evidence manifests and verifier checks. | `interfaces/RISK_POLICY_GATE.md` | not_enforced | planned: `browser-evidence-verify` + `decapod validate` marker checks | SPEC until artifact verifier is mandatory. |
| claim.harness.incident_to_case_loop | Production regressions must map to harness-gap cases and tracked follow-up. | `interfaces/RISK_POLICY_GATE.md` | not_enforced | planned: harness-gap lifecycle checks | SPEC pending workflow linkage automation. |
| claim.context_pack.canonical_layout | Agent context pack uses canonical `.decapod/context` and `.decapod/memory` layout, not root file sprawl. | `interfaces/AGENT_CONTEXT_PACK.md` | not_enforced | planned: `decapod validate` context-pack layout gate | SPEC pending directory/shape enforcement. |
| claim.context_pack.deterministic_load_order | Context pack load order is deterministic across runners. | `interfaces/AGENT_CONTEXT_PACK.md` | not_enforced | planned: load-order validation gate | SPEC pending loader checks. |
| claim.context_pack.mutation_authority_rules | High-authority context files require human-owned or explicit approval updates. | `interfaces/AGENT_CONTEXT_PACK.md` | not_enforced | planned: mutation-policy enforcement gate | SPEC pending policy engine integration. |
| claim.memory.append_only_logs | Operational memory logs are append-first and cannot be silently erased in place. | `interfaces/AGENT_CONTEXT_PACK.md` | not_enforced | planned: append-only validation checks | SPEC pending log write-policy enforcement. |
| claim.memory.distill_proof_required | `memory.md` must be produced by deterministic distillation with a named proof surface. | `interfaces/AGENT_CONTEXT_PACK.md` | not_enforced | planned: deterministic distill proof check | SPEC pending distill command/proof surface. |
| claim.context_pack.security_scoped_loading | Sensitive context-pack memory is scope-gated and not auto-loaded into broad shared contexts. | `interfaces/AGENT_CONTEXT_PACK.md` | not_enforced | planned: scoped-load policy checks | SPEC pending runtime loader policy enforcement. |
| claim.context_pack.correction_loop_governed | Corrections must be persisted through control-plane artifacts and proofed, not mental notes. | `interfaces/AGENT_CONTEXT_PACK.md` | not_enforced | planned: correction-to-proof audit gate | SPEC pending end-to-end trace enforcement. |
| claim.agent.intent_refinement_required | Agents MUST ask clarifying questions and refine requirements with the user BEFORE burning tokens on inference/implementation. | `core/INTERFACES.md` | not_enforced | planned: intent-refinement gate | SPEC pending: agent must produce a refined design doc before code generation. |
| claim.lcm.append_only_ledger | LCM events are stored in append-only JSONL ledger (`lcm.events.jsonl`) and never mutated or deleted. | `interfaces/LCM.md` | enforced | `decapod validate` (LCM Immutability Gate) | Enforced via validate_lcm_immutability gate. |
| claim.lcm.content_hash_deterministic | Content hash is SHA256 of raw content bytes — deterministic across runs. | `interfaces/LCM.md` | enforced | `decapod validate` (LCM Immutability Gate) | Enforced via validate_lcm_immutability gate. |
| claim.lcm.index_rebuildable | LCM SQLite index (`lcm.db`) is always rebuildable from `lcm.events.jsonl`. | `interfaces/LCM.md` | enforced | `decapod lcm rebuild --validate` + `decapod validate` (LCM Rebuild Gate) | Enforced via validate_lcm_rebuild_gate. |
| claim.lcm.summary_deterministic | Same originals in timestamp order produce the same summary hash across runs. | `interfaces/LCM.md` | enforced | `decapod lcm summarize` produces stable hash | Deterministic by construction. |
| claim.map.scope_reduction_invariant | Agentic map delegation MUST declare retained scope; empty retain is rejected. | `interfaces/LCM.md` | enforced | `decapod map agentic --retain` required | Enforced in CLI argument parsing. |
| claim.todo.claim_before_work | Agents must claim a TODO before substantive implementation work on that task. | `interfaces/CONTROL_PLANE.md` | partially_enforced | `decapod todo claim` ownership records + procedural review | Enforced by process today; future validate gate may enforce ownership-before-mutation traces. |
| claim.git.container_workspace_required | Git-tracked implementation work must execute in Docker-isolated git workspaces, not direct host worktree edits. | `specs/GIT.md` | enforced | `decapod validate` (Git Workspace Context Gate) | Enforced via validate gate checking container signals and worktree isolation. |
| claim.git.no_direct_main_push | Direct commits/pushes to protected branches (master/main/production/stable/release/*) are forbidden; work must happen in working branches. | `specs/GIT.md` | enforced | `decapod validate` (Git Protected Branch Gate) | Enforced via validate gate checking current branch and unpushed commits. |
| claim.git.container_runtime_preflight_required | Container workspace runs must pass runtime-access preflight and fail loudly with elevated-permission remediation when access is denied. | `specs/GIT.md` | partially_enforced | `container.run` runtime `info` preflight + permission-aware error diagnostics | Enforced in container runtime preflight; broader policy-level enforcement remains future work. |
| claim.session.agent_password_required | Session access requires agent identity plus an ephemeral per-session password; expired sessions trigger cleanup and assignment eviction. | `specs/SECURITY.md` | partially_enforced | `session.acquire` credential issuance + `ensure_session_valid` password check + stale-session cleanup hook | Enforced for active command auth path; stronger cryptographic hardening may be added later. |
| claim.validate.bounded_termination | `decapod validate` MUST terminate in bounded time and return a typed failure under DB lock contention. | `interfaces/TESTING.md` | enforced | `tests/validate_termination.rs` + `DECAPOD_VALIDATE_TIMEOUT_SECS` timeout path | Prevents proof-gate hangs from becoming cultural bypass. |
| claim.validate.no_cross_turn_lock_residency | No single agent session may hold validation-related datastore locks across multiple turns/commands. | `interfaces/CONTROL_PLANE.md` | partially_enforced | `tests/validate_termination.rs` + contention integration tests | Locking discipline is implemented in command-scoped paths; broader contention coverage remains in progress. |
| claim.knowledge.provenance_required | Every procedural memory entry must cite evidence (commit, PR, doc, test, or transcript). | `interfaces/KNOWLEDGE_STORE.md` | enforced | `decapod validate` (Knowledge Integrity Gate) | Enforced via validate_knowledge_integrity gate. |
| claim.knowledge.directional_flow | Episodic observations cannot flow directly into procedural/semantic memory. Must use explicit promotion artifact + human approval. | `interfaces/KNOWLEDGE_STORE.md` | not_enforced | planned: gate in knowledge promote | Blocks direct friction→procedural writes. |
| claim.knowledge.versioned_schema | Knowledge store uses versioned schemas. No breaking changes without migration path. | `interfaces/KNOWLEDGE_STORE.md` | not_enforced | planned: schema migration validation | Readers never break on writes. |

---

## 3. Workflow: Registering/Updating a Claim

When adding or changing a guarantee:

1. Add/update the claim row here.
2. Ensure the owner doc references the claim-id near the guarantee.
3. Ensure the claim has a proof surface, or do not label it `REAL`.
4. If the change deprecates older binding meaning, follow `core/DEPRECATION.md`.

---

## Links

### Core Router
- `core/DECAPOD.md` - **Router and navigation charter (START HERE)**

### Authority (Constitution Layer)
- `specs/INTENT.md` - **Methodology contract (READ FIRST)**
- `specs/SYSTEM.md` - System definition and authority doctrine
- `specs/AMENDMENTS.md` - Change control

### Registry (Core Indices)
- `core/PLUGINS.md` - Subsystem registry
- `core/INTERFACES.md` - Interface contracts index
- `core/DEPRECATION.md` - Deprecation contract

### Contracts (Interfaces Layer - This Document)
- `interfaces/DOC_RULES.md` - Doc compilation rules
- `interfaces/STORE_MODEL.md` - Store semantics
- `interfaces/CONTROL_PLANE.md` - Sequencing patterns
- `interfaces/GLOSSARY.md` - Term definitions
- `interfaces/TESTING.md` - Testing contract
