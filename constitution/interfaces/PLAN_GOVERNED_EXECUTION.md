# PLAN_GOVERNED_EXECUTION.md

**Authority:** binding  
**Layer:** Interfaces  
**Binding:** Yes  
**Scope:** Plan-governed execution pushback contract  
**Non-goals:** Agent orchestration loops, UI, memory systems

## 1. Contract

Decapod MUST enforce an execution boundary:

`RESEARCH -> PLAN -> ANNOTATE -> APPROVE -> EXECUTE -> PROVE -> PROMOTE`

This interface standardizes the first kernel slice with deterministic pushback.

## 2. Governed Artifacts

- `PLAN`: store: `<repo>/.decapod/governance/plan.json`
- `WORK_UNIT`: store: `<repo>/.decapod/governance/workunits/<task_id>.json`
- `TODO`: existing task ledger (`todo.db`) with proof metadata (`task_verification`)

`PLAN.state` values are:

- `DRAFT`
- `ANNOTATING`
- `APPROVED`
- `EXECUTING`
- `DONE`

`WORK_UNIT` required fields are:

- `task_id` (string)
- `intent_ref` (string)
- `spec_refs` (array of strings)
- `state_refs` (array of strings)
- `proof_plan` (array of strings)
- `proof_results` (array of proof result records)
- `status` (`DRAFT` | `EXECUTING` | `CLAIMED` | `VERIFIED`)

`WORK_UNIT.status` allowed transitions are:

- `DRAFT -> EXECUTING`
- `EXECUTING -> CLAIMED`
- `CLAIMED -> VERIFIED`
- `EXECUTING -> DRAFT` (explicit rollback before claim)

`VERIFIED` contract meaning:

- Every proof in `proof_plan` has a corresponding `proof_results` record.
- Every required proof result is `pass`.
- A deterministic context capsule artifact must exist at `.decapod/generated/context/<task_id>.json`.
- The capsule must carry non-empty policy lineage fields (`risk_tier`, `policy_hash`, `policy_version`, `policy_path`, `repo_revision`).
- `WORK_UNIT.state_refs` must include the capsule artifact path (`.decapod/generated/context/<task_id>.json`) to make lineage explicit and machine-checkable.
- Promotion-relevant commands (`validate`, `workspace publish`) treat non-`VERIFIED` work units as blocking.

## 3. Mandatory Pushback Markers

Decapod MUST return typed, machine-readable failure markers:

- `NEEDS_PLAN_APPROVAL`
- `NEEDS_HUMAN_INPUT`
- `SCOPE_VIOLATION`
- `PROOF_HOOK_FAILED`
- `VALIDATE_TIMEOUT_OR_LOCK`

`NEEDS_HUMAN_INPUT` MUST include a payload with exact questions.

## 4. Threshold Rule for Human Input

Execution MUST be blocked when any condition is true:

- PLAN intent is empty.
- PLAN unknowns is non-empty.
- PLAN human_questions is non-empty.
- No executable TODO is selected or resolvable.

## 5. Agent Reaction Contract

When Decapod returns `NEEDS_HUMAN_INPUT`, an agent MUST:

1. Ask the human the provided questions verbatim.
2. Update PLAN via `decapod govern plan update ...`.
3. Re-run `decapod govern plan check-execute`.

## 6. Proof Semantics for TODO Completion

- TODO completion without verified proof hooks is `CLAIMED` (not promotion-ready).
- TODO becomes `VERIFIED` only when proof checks pass (`last_verified_status in {"VERIFIED","pass"}`).
- Promotion path (`validate` and `workspace publish`) MUST block on unverified done TODOs.
