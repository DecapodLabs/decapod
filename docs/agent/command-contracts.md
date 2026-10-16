# Command Contracts

This document defines the normative operational contracts for the Decapod CLI.

## `decapod eval`
- **Intent:** Evaluate an untrusted agent prompt before any repository or tool action

## `decapod activate`
- **Intent:** Activate local control plane state and run startup migrations

## `decapod init`
- **Intent:** Bootstrap system and manage lifecycle

## `decapod setup`
- **Intent:** Configure repository (hooks, settings)

## `decapod session`
- **Intent:** Session token management (required for agent operation)

## `decapod cloud`
- **Intent:** Optional cloud credential and Propodus integration commands

## `decapod constitution`
- **Intent:** Embedded Constitution Graph queries and lookups

## `decapod docs`
- **Intent:** Access agent-facing methodology documentation (restricted to docs/agent/)
- **Restriction:** Only handles documents under `docs/agent/`.

## `decapod todo`
- **Intent:** Track tasks and work items
- **Preconditions:** Agent must have an active session.
- **State Transition:** Managed via `todo.db`.

## `decapod obligation`
- **Intent:** Governance-native obligation graph

## `decapod validate`
- **Intent:** Prove complete current-workbranch readiness before pull request creation
- **Subject:** The exact current work branch, its accepted target tip, source and governance material, reviewed CI/release definitions, toolchain and applicable complete check matrix.
- **Outcome:** Exit 0 only after all required evidence passes. Missing, empty, stale, running, skipped-required or failed proof returns nonzero. Any relevant branch or target change invalidates earlier success.
- **Publication loop:** Work branch pushes and iteration are allowed before preflight. Hold PR creation (including a draft) until intent is converged, the branch is mergeable into the accepted target, and full validation succeeds against the exact current candidate and accepted target. Follow the validation report's actionable instructions, repair reported root causes on the work branch, and rerun exact-candidate validation until it succeeds; never bypass a reported gate or fabricate evidence. `workspace publish` combines branch push and PR creation, so it remains held at this readiness boundary; earlier standalone branch pushes are allowed. RPC `validate.run` enforces the same validation contract.
- **CI context:** Inside GitHub Actions (`GITHUB_ACTIONS=true`) or Buildkite (`BUILDKITE=true`), public CLI/alias/RPC validation executes the real bounded local gates and ignores remote preflight configuration. It reports `scope=ci_local` and `publication_ready=false`, never writes a full-readiness receipt or waits for its own suite, and rejects snapshot transport arguments. Provider environment flags alone cannot authorize publication: the publisher independently invokes the complete workbranch readiness boundary.
- **Prerequisites:** Once source and required governance material are ready, checkpoint and commit the candidate, then run plain `decapod validate`. Validation owns configured build prerequisites and tests; do not run a separate manual build or shadow preflight. `decapod govern project-check` runs bounded project/governance prerequisites for bootstrap and finite native/CI recipe execution. Its partial success never grants publication readiness. Complete workbranch validation runs outside CI and aggregates actual source checks without recursion.
- **Providers:** `proof.validation_buildkite_preflights = false` selects native execution; true selects remote execution on the current Decapod workspace branch. The `decapod.pr-readiness.v1` profile retains the complete finite source-CI contract on either route. Consumer `repo.pr-readiness.v1` instead proves explicitly reviewed required local commands with `scope=repository_local_readiness`; it cannot select Buildkite or attest hosted CI, release or deployment execution. Missing capabilities and unknown or unreconciled contracts fail explicitly.
- **Automatic provider execution:** With Buildkite validation enabled outside CI, `validate` pushes or reuses the exact current workspace branch commit, waits for every required imported Buildkite workflow, collects authenticated execution artifacts once, and rechecks the candidate, accepted target and provider attempt identities before returning success. The required project checks execute on Buildkite; this route neither runs a duplicate local prerequisite suite nor requires a fresh local validation receipt before submission. It uses already configured official Git access, repository check URLs only to locate Buildkite runs, and the reviewed Buildkite reader; missing tools, access denial, timeout, cancellation, failed jobs or incomplete custody return nonzero. It never installs credentials or falls back after an authorization failure.
- **Host handoff:** `validate --snapshot-prepare` prepares private metadata for the existing workspace branch and returns nonzero while proof is pending. An authorized host may push only the same exact candidate commit to that branch. `validate --snapshot-resume <HANDOFF> --snapshot-adopt <H>` independently verifies the unchanged source/tree/target and the exact remote branch tip; it cannot accept a different commit or a temporary branch. If a host object API changes commit identity, first checkpoint and review that real source identity before preparing a new handoff. Never edit a handoff to assert success. `validate --snapshot-resume <HANDOFF>` independently verifies Buildkite identities, raw artifact digests and execution evidence. No per-run branch or synthetic commit is created, and validation cleanup never deletes the workspace branch. Existing GitHub workflows may run independently; their results and artifacts neither satisfy nor block this Buildkite profile. This is not a compatibility claim for a separate `bk preflight` command.
- **Boundaries:** Validation may perform the authorized non-forced push of its existing workspace branch. It does not open a PR, create a release/tag, upload a package/image or deploy. Remote CI submission needs existing authorized access and remains subject to external-action policy. The core source-PR profile requires actual compile, test and build execution. Release-only packaging is outside this profile and has no packaging-assurance claim; publish/deploy dispositions describe simulation only, never actual delivery.
- **Recovery:** Do not ask again before an already-authorized configured validation push or a supported, safely replayable retry to the same repository and workspace branch. Recheck the exact source and accepted target; stop on explicit cancellation, access or authorization denial, or a change that requires new authority. Follow the validation report's actionable recovery instructions, preserve blockers requiring human input, repair root causes or missing proof, and rerun against the exact candidate. For supported remote source-CI contracts, validation uses the accepted target’s independently verified executor to build and test the candidate, even when candidate helper bytes change. The controller keeps the required graph and final verdict; candidate output cannot authorize itself. `CONTROLLER_CONTRACT_UNSUPPORTED` identifies an unsupported carrier, protocol or accepted executor. Rerunning the same controller, repinning configuration or running a manual build cannot resolve that blocker; use an independently supported controller for this exact contract, and preserve the blocker if none is available. No automatic controller replacement is performed. Do not mark unexecuted checks green, weaken the required profile to pass, or replace unavailable provider evidence with a local receipt.

## Living-spec refresh surfaces

- **RPC:** `decapod rpc --op specs.refresh` refreshes supported spec fingerprints,
  attestations, overlays, generated boundaries and the manifest.
- **Validation flag:** `decapod validate --refresh-specs` requests the same
  supported refresh while running validation. Validation remains authoritative;
  this flag does not make unrelated gates pass.
- **Preconditions:** Use an active session and the claimed, isolated workspace.
  Review and author the material living-spec prose before refreshing it.
- **Authored scope:** Living specs describe the configured project from purpose
  to architecture to implementation depth. Lead with the whole product, then
  explain components and source-grounded behavior. A narrow task updates the
  relevant part without replacing project-wide identity. Cite repository paths
  and symbols; distinguish implemented behavior, plans, assumptions, and gaps.
  Templates and capability declarations are obligations, not runtime evidence.
- **Boundary:** Refresh does not rewrite authored intent or architecture and
  cannot substitute for the required material spec change. It does not approve
  a plan or mark a todo complete.
- **Recovery:** There is no top-level `decapod specs.refresh` command. Use one of
  the two invocations above, inspect the refreshed files, and rerun validation.

## `decapod govern`
- **Intent:** Governance: policy, health, proofs, audits

### `decapod govern plan` contract revisions

- Plan acceptance applies to the recorded execution contract. A real change to
  title, intent, task bindings, proof hooks, questions, stop conditions,
  contradictions, deferrals, scope constraints, or phase/gate definitions returns
  the plan to `DRAFT` and clears all phase and gate progress, including timestamps.
  Phase ordering can make later evidence depend on earlier definitions, so a
  revision conservatively requires replay of every declared phase.
- Exact no-op updates preserve acceptance and progress. Spec-review records are
  retained, including unresolved human decisions; their independent content-bound
  review rules still apply.
- After a material edit, review the revised contract and use the separate
  `decapod govern plan approve` transition. Editing and accepting a changed
  contract in the same low-level patch is rejected without writing the document.
  An explicit state transition remains a declared approval, not authenticated
  proof of a human identity or semantic correctness.
- `enter-phase` and `exit-phase` require an approved or executing plan. Completing
  phases cannot promote a draft or annotating plan to `DONE`. Reapproval does not
  restore progress erased by a contract revision; satisfy the current gates and
  replay the phases in declaration order.
- Execution and publication reject `DONE` plans with incomplete phases and
  completed phases with unsatisfied gates. These structural checks supplement
  exact-candidate validation and do not substitute for actual proof execution.

### `decapod govern trajectory record`

- **Preconditions:** Use an active session and initialize the run with `decapod govern trajectory init --run-id <run-id> --original-intent "..." --derived-intent "..."`.
- **Inputs:** Repeat `--loop-json '<object>'` for typed loop attempts and `--check "name=status"` for named check results. Each loop input is one JSON object, not an array.
- **Schema:** The tables and required fields below are generated by `decapod docs build` from [trajectory.schema.json](../../assets/schemas/trajectory.schema.json), `$defs.loop` and `$defs.check`. They are embedded in `decapod docs ingest` and `decapod docs show command-contracts --source embedded`.
- **Required loop fields:** `intent_id`, `trajectory_id`, `loop_id`, `loop_type`, `attempt`, `trigger`, `grader_result`, `mutation_proposal`, `status`.

| Input | Canonical values |
| --- | --- |
| `grader_result` | `pass`, `fail`, `skipped` |
| `loop_type` | `agent`, `verification`, `event`, `improvement` |
| `mutation_proposal` | `none`, `prompt`, `tool`, `config`, `rubric`, `policy` |
| `status` | `open`, `retrying`, `passed`, `failed`, `blocked` |
| `trigger` | `human`, `webhook`, `cron`, `agent`, `grader`, `engine` |
| `--check status` | `passed`, `failed`, `partial`, `unavailable` |

- **Exact JSON values:** Loop enum strings are case-sensitive. In particular, `grader_result` uses `pass` / `fail`, while loop `status` uses `passed` / `failed`. Arbitrary trigger descriptions are not enum values.
- **Check aliases:** The check parser additionally accepts `pass` as `passed` and `fail` as `failed`, ignoring status case and surrounding whitespace. Check names must be non-empty. `no_checks_run` is a computed proof status, not an accepted check input.
- **Identity and attempts:** Non-empty `intent_id`, `trajectory_id`, and `loop_id` are required. `trajectory_id` must equal `--run-id`; `intent_id` must match the initialized intent (by default `intent:<run-id>`). Attempts for each `loop_id` start at 1 and remain contiguous, without duplicates.
- **Verification evidence:** A verification loop with `grader_result: "pass"` requires at least one `proof_refs` entry, and every entry must be non-empty; `grader_result: "fail"` requires non-empty `feedback`. Feedback on every loop is limited to 2048 bytes. A passing enum value is not evidence by itself.
- **Optional loop fields:** `parent_loop_id`, `tool_calls`, `observations`, `feedback`, `proof_refs`, and `custody_event_id`. Event and improvement loops record evidence; a mutation proposal does not apply a change.
- **Example:** See [the minimal trajectory record](payload-examples.md#trajectory-record) for an initialized run and an unverified inspection.


## `decapod data`
- **Intent:** Data: archives, knowledge, context, schemas

## `decapod auto`
- **Intent:** Automation: scheduled and event-driven

## `decapod qa`
- **Intent:** Quality assurance: verification and checks

## `decapod decide`
- **Intent:** Architecture decision prompting

## `decapod workspace`
- **Intent:** Agent workspace management
- **Preconditions:** Task must be claimed.
- **State Transition:** Creates git worktrees/containers.
- **Publication:** Publication is a governed transition and remains blocked while required validation or evidence is unsatisfied.

## `decapod rpc`
- **Intent:** Decapod-specific structured RPC interface for agents

## `decapod release`
- **Intent:** Release lifecycle checks and guards

## `decapod capabilities`
- **Intent:** Show Decapod capabilities (for agent discovery)

## `decapod infer`
- **Intent:** Inference governance: shape context before model, validate after

## `decapod trace`
- **Intent:** Local trace management

## `decapod system`
- **Intent:** System: capabilities, version, doctor

## `decapod context`
- **Intent:** Context: infer, lcm, internalize, preflight, impact

# RPC Operations (Auto-generated)

### Operation: `AgentInit`
### Operation: `WorkspaceStatus`
### Operation: `WorkspaceEnsure`
### Operation: `WorkspacePublish`
### Operation: `ContextResolve`
### Operation: `ContextCapsuleQuery`
### Operation: `ContextBundleExport`
### Operation: `ContextBundleImport`
### Operation: `ContextBindings`
### Operation: `ConstitutionGet`
### Operation: `ConstitutionLinksQuery`
### Operation: `ConstitutionLinksNavigate`
### Operation: `SpecsRefresh`
### Operation: `ConstitutionMigrate`
### Operation: `AgentRegistryQuery`
### Operation: `SchemaGet`
### Operation: `StoreUpsert`
### Operation: `StoreQuery`
### Operation: `ValidateRun`
### Operation: `ScaffoldNextQuestion`
### Operation: `ScaffoldApplyAnswer`
### Operation: `ScaffoldGenerateArtifacts`
### Operation: `StandardsResolve`
### Operation: `MentorObligations`
### Operation: `AssuranceEvaluate`

# Runtime Checkpoints and Recovery

Runtime journals use the existing canonical datastore. They do not replace Git
as the authority for authored governance or grant publication readiness.
Workunit mutations require an initialized local runtime (`decapod init`). A
missing or partial policy store fails before any workunit journal append.

- `decapod todo journal checkpoint` records the selected store's current task
  state as an explicit baseline or current checkpoint
- `decapod todo journal verify` inspects integrity without repairing state
- `decapod todo journal restore --expected-current-digest <digest>` verifies
  the complete chain and the observed live-state precondition before restoring
  an absent or known predecessor projection; divergent authority is preserved
- `decapod govern workunit checkpoint --task-id <id> --operation-id <id>`
  records a checkpoint, with optional `--expected-generation <n>`
- `decapod govern workunit journal-status --task-id <id>` inspects the journal
  and JSON projection; `recover --task-id <id>` restores the exact recorded
  manifest without refreshing verification
- `decapod govern workunit resume --task-id <id> --operation-id <id>` checks
  current execution authority before continuing, with the same optional
  generation precondition

- `decapod govern trajectory checkpoint --run-id <id> --checkpoint-id <id>`
  records bounded source, intent, plan, trajectory, task custody, and policy
  bindings; repeat `--approval-id <id>` for explicit required approval references
- `decapod govern trajectory verify-resume --run-id <id> --checkpoint-id <id>`
  rechecks those bindings and current approval validity without mutating them

An approval reference never issues a grant. Resume requires live task custody
and current dependency readiness. Checkpoint-only commits may descend from the
captured commit; branch changes, rewinds, changed source, or changed authority
require review and a fresh checkpoint. Existing execution checkpoints cannot
silently drop required approval references.

Reuse an operation ID only for an identical request. A failed file publication
may follow a successful durable journal append; inspect status and reconcile
that operation rather than submitting a changed request with the old ID. Legacy
baseline records describe observed current state, not reconstructed prior
history. Neither successful replay nor a readable manifest proves current
source, task, approval, or validation freshness. Full `decapod validate` and the
governed publisher retain their existing independent proof requirements.

# Governance Artifact Ownership

`.decapod/governance.json` is the sole tracked governance document. Its logical
plan, trajectory, validation, and claims sections share normalized
intent and epoch records. Health Engine operational claims remain separate in
`.decapod/data/decapod.db`; workunit runtime manifests live in `.decapod/data/workunits/`.
Do not hand-edit either control-plane authority.

Use `decapod govern artifacts migrate` for lossless legacy adoption and
`begin-pr --id <change> --base-branch <branch>` for an explicit new-PR boundary.
Normal `workspace ensure`, `agent.init`, and validation initialize a proven fresh
worktree branch from the embedded governance template and current claimed task.
The plan starts DRAFT and the trajectory has no checks or completion proof; only
actual successful validation writes a receipt. Protected refs and read-only
status/inventory never trigger initialization. Repeats, renames, and rebases keep
the active change identity and journal. Dirty inherited or ambiguous context
fails closed rather than being overwritten.
The boundary requires discarded state to be recoverable from Git, keeps explicit
unresolved obligations, and leaves accepted completed claims in Git history.
Use `claim` for actual current-PR claims and `resolve-obligation` with proof
references to retire a carried obligation explicitly. Status and
`verify-checkpoints` are read-only and never migrate state.

Populate in order: stable PR identity → intent/scope plan → actual current claims
and planned checks → work/evidence trajectory → validation → staged-material
checkpoint per commit. Persist meaningful boundaries as work progresses; do not
wait until the final response or repeatedly rewrite unchanged content.

Before each authored commit, stage code/spec changes, run
`decapod govern artifacts checkpoint --id <unique-id> --summary "..." --proof-ref <evidence>`,
then stage governance.json and commit. Publication checks exact material and
cumulative checkpoint preservation across the entire commit graph, including merges.
Checkpoint IDs are immutable within the active PR. Repeating an ID succeeds
without a write only when the summary, normalized proof-reference set, and
current material digest match its original record. Different code, governed
inputs, summary, or proof references return `GOVERNANCE_CHECKPOINT_ID_CONFLICT`
without replacing history. Record changed work with a new ID. This check runs
under the existing governance-document lock; independent history verification
still rejects erased or rewritten records in authored commits.
`inventory --claims-note "..."` records a current-PR checkpoint, not a fake claim
or a policy-history append. `inventory --compact` performs lossless legacy
normalization. Neither surface repairs malformed evidence by overwriting it.
