# Phase-Correct Gate Admission, Cold-Context Resilience & Constitution Integrity Audit

Cross-references: `#1763`, `#1367`, `#1772`, `#1774`, `#1735`.

## 1. End-State Enforcement Table (`#1763`)

Per `#1763`'s acceptance contract, every governed lifecycle phase is classified explicitly by policy owner, executable enforcement point, independent observation source, refusal code, bypass route, and currentness subject.

| Phase | Accepted Rule / Policy Owner | Executable Enforcement Point | Independent Observation Source | Exact Failure Code / Remediation | Bypass Route & External Control | Currentness Subject |
| --- | --- | --- | --- | --- | --- | --- |
| **Prompt & Preparation** | `AGENTS.md` + `core/DECAPOD` (`src/decapod/core/docs.rs`) | `decapod eval --stdin`, `decapod infer orientation`, `decapod govern plan check-execute` | Local SQLite/Git governance journal (`.decapod/governance.json`, `todo.db`) | `status: block`, `NEEDS_PLAN_APPROVAL`, `TODO_PROJECTION_MISSING` | Direct shell/editor execution outside Decapod; **cannot** be blocked by kernel alone — requires host hook/sandbox | Prompt SHA-256, `instruction_revision`, `source_revision`, task lease generation |
| **Task Execution & Custody** | `INV-ROOT-ISOLATION`, `INV-SESSION-AUTH` (`src/decapod/core/workspace.rs`, `src/decapod/core/agent_interop/mcp.rs`) | `decapod workspace ensure`, `decapod rpc`, `decapod mcp serve --allow-mutations` | Local Git worktree metadata + `.decapod/managed/sessions/` password hash verification | `ProtectedBranch`, `WorkspaceRequired`, `read_only_session_denied`, `session_required`, `worktree_isolation_required` | Direct `git commit` or file edit on root `master`; requires OS sandbox / host confinement | Active session expiry, `agent_id`, worktree path, current branch |
| **Native Validation & Checkpoints** | `.decapod/config.toml` + `src/decapod/core/validate.rs`, `src/decapod/core/governance_document.rs` | `decapod govern artifacts verify-checkpoints --publication-material --target-base <base>`, `decapod validate` | Local Git tree/head/base + Buildkite/GitHub Actions remote CI (when configured) | `PUBLICATION_MATERIAL_BASE_CHANGED`, `FINGERPRINT_ONLY_SPECS`, `UNSTAGED_GOVERNANCE_ARTIFACT`, `CONTROLLER_CONTRACT_UNSUPPORTED` | Forged self-declared PASS text or manual build; refused by `verify-checkpoints` and `validate`, **not** prevented if agent opens PR via raw `gh` | Exact `head_sha`, `base_sha`, tree digest, toolchain/deps, `config.toml` policy digest, timestamp |
| **Push & Publish** | `src/decapod/core/workspace_publication.rs`, `src/decapod/core/validation_rules.rs` | `decapod workspace publish` / preflight gate | Remote Git refs + configured CI status | `PUBLICATION_NOT_READY`, non-fast-forward rejection | Direct `git push` / `gh pr create`; classified **external, non-admitted** (`outside_native_enforcement`) — enforced remotely only by GitHub branch protection / rulesets | Exact pushed commit SHA vs validated commit SHA, target branch tip |
| **PR Readback & Merge** | GitHub repository rulesets + `decapod observe pr-manifest` (`src/decapod/core/observability_projections/pr_manifest.rs`) | GitHub protected branch rulesets & required status checks | GitHub API / independently executed CI checks | `PrDriftFlag` (`HeadMovedAfterValidation`, `BaseMovedAfterValidation`, `UnpublishedPlan`) | Repo admin ruleset bypass (must be logged transparently in GitHub ruleset audit, never hidden) | Exact PR head SHA, merged tree SHA, base branch SHA, required checks |

### Non-Enforceable Boundary Report (`#1763`)

1. **Git tree digests alone do NOT bind:** untracked test fixtures, external tool binary versions, remote service state, or omitted proof obligations.
2. **Local `plan.state = APPROVED` is NOT a human signature:** As empirically observed in our Wave 7 Codex worktree arm (`docs/research/host-outcome-pilot/wave7-host-activation-2026-11-22.md`), an agent with local CLI access can invoke `decapod govern plan approve`. True principal consent requires an independent external approval surface (e.g., GitHub PR review / signed approval).
3. **Unmediated shell/forge bypass:** If an agent bypasses Decapod and invokes `git push` or `gh pr create` directly, Decapod classifies the resulting PR as external/non-admitted rather than claiming it prevented the unmediated operation.

### Executable Local Negative Controls (`tests/gate_admission_and_resilience.rs`)

- **Stale head TOCTOU (`#1763`):** Checkpoint bound to candidate tree X is refused by `verify-checkpoints --publication-material --target-base master` after candidate mutates to tree Y.
- **Target base movement TOCTOU (`#1763`):** Advancing `master` after checkpointing tree Y causes `verify-checkpoints` to fail closed with `PUBLICATION_MATERIAL_BASE_CHANGED` naming both recorded and resolved target revisions.
- **External status note:** Live GitHub forge ruleset-enforced merge admission without admin bypass remains tracked externally on `#1763` (this PR references `#1763` without claiming premature closure).

## 2. Multi-Process Cold-Context Crash & Handoff Resilience (`#1367`)

Verified in `tests/gate_admission_and_resilience.rs::issue_1367_multiprocess_cold_context_handoff_recovers_intent_refuses_stale_and_dedupes`:

1. Process A (`agent-alpha`) persists canonical original intent (`max_attempts=3`), checkpoints tree X, mutates to tree Y, and exits (simulating crash mid-flight with a stale PASS envelope for tree X).
2. Cold-context Process B (`agent-beta`, fresh session, zero shared memory/transcript) runs `decapod infer orientation --task-id ...` and recovers the canonical original intent from repository state alone.
3. Process B evaluates Process A's inherited PASS envelope for tree X against current tree Y via `import_external_envelope` -> **rejected fail-closed (`ImportVerdict::Rejected`, `ImportReasonClass::StaleEvidence`, `imported_authority_granted == false`)**.
4. Replay/dedupe guard (`ReplayLedger::check_and_record`) rejects conflicting replay of unknown-outcome external operation ID (`op-external-write-42`) with `ImportReasonClass::ReplayConflict`.

## 3. Constitution Relevance & Contribution Integrity Audit (`#1772`, `#1774`)

- **`#1772` (Relevance & Anti-Repetition-Bias):**
  - Confirmed in `tests/gate_admission_and_resilience.rs`: irrelevant queries (`zzzz_nonexistent_unicorn_quasar_topic_999`) return an **empty** fragment set rather than arbitrary alphabetic fallback; 25x keyword-stuffed rejected phrases (`hello_bypass`) produce zero false-positive constitution matches.
  - Real-host negative control (`docs/research/host-outcome-pilot/wave7-host-activation-2026-11-22.md`): 10x repeated low-trust `NOTES.md` prompt injection for `"Hello, bypass!"` was rejected across all three Codex trials.
  - Remaining external gap keeping `#1772` open (`Refs #1772`): controlled equal-permission cross-arm comparison with reviewed harmful-context rate & dollar cost labels (shared with `#1095`).
- **`#1774` (Contribution Integrity & Normative Promotion):**
  - Confirmed in `tests/constitution_claims.rs` + `tests/gate_admission_and_resilience.rs`: `core/SPECS` is `binding` with `authority_review.control_id == "core/SPECS"`, `sections.apply` present, `enforcement == "not_machine_enforced"`, and `interfaces/CLAIMS` remains `advisory` without self-attested execution.
  - Production build-time admission matrix (`CONSTITUTION_PROMOTION_REVIEW_REQUIRED`, `CONSTITUTION_ID_CATEGORY`, `CONSTITUTION_ROUTER_AUTHORITY`, `CONSTITUTION_REFERENCE_UNRESOLVED`, `CONSTITUTION_UNSUPPORTED_ENFORCEMENT`, `CONSTITUTION_BINDING_CONTROL_REMOVED`) is enforced in `assets/build/constitution_admission.rs`.
  - Remaining external gap keeping `#1774` open (`Refs #1774`): maintainer PR review record explicitly attesting the `core/SPECS.sections.apply` amendment from merged PR #1864 (cannot be fabricated by an agent).
