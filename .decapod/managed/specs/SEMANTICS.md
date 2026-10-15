# Semantics

## Semantics Overview

Decapod gives intent, coordination, execution, evidence, and publication distinct
meanings. A successful transition in one does not silently establish the others.
The product's durable records must preserve those distinctions across agents,
processes, and retries.

## Authority and interpretation

The human request and explicit clarifications establish request meaning. The
agent's plan and living specifications record its interpretation. Configuration
and recognized project overrides define local governance inputs. Decapod
resolves and checks these inputs through its supported interfaces.

Workspace isolation changes where work executes. It does not erase the prompt.
A missing local todo projection is a coordination discrepancy; it neither
invalidates clear intent nor grants ownership. Recovery must establish the
required coordination and execution gates before work continues.

Generated attestations, manifests, capsules, and receipts report bounded facts
about their inputs and operations. They do not independently establish the
semantic correctness of authored prose.

## State Machines

A todo records the accepted work and its lifecycle. Task status, assignment,
ownership records, and lease state are related but separate fields. Claiming
a task establishes custody; it does not mean the task is complete or verified.
Exclusive claims reject a different active owner. Shared claims use explicit
secondary ownership rather than silently replacing the primary owner.

Material lifecycle mutations use revision-aware conditional updates and commit
required events atomically with state. A stale revision or lost ownership is a
conflict. It must not emit a successful mutation event. Reclaim and handoff
follow the lease and session rules rather than an agent's assumption that an
old owner has disappeared.

An active managed workspace binds a task, an agent, and an execution location.
Creation starts from a committed base and preserves unrelated source-checkout
changes. A task claim may succeed even if optional post-claim container
preparation fails; the response preserves the warning and the durable claim.
A retry must inspect the preparation receipt instead of launching a duplicate
container operation.

Cleanup requires verified ownership. Unknown directories, replaced directory
identities, symlinked paths, unresolved Git state, and unavailable creating
runtimes remain preserved. Timeout is uncertainty, not proof of absence.

## Current work and historical evidence

`.decapod/governance.json` records one active change and one current trajectory
for the workspace. Subagent work can be recorded as loops in that trajectory.
The document retains current-change checkpoints; Git retains accepted history.

A proven fresh branch boundary initializes the next change from the accepted
base, carrying durable policy and unresolved obligations forward. Repeating a
command, renaming a branch, or rebasing does not itself create new intent or
authorize erasing the journal. Explicit lifecycle recovery preserves evidence
before replacing inherited or malformed state.

Trajectory loop attempts have stable identities and contiguous attempt
numbers. A passing verification loop requires proof references; a failed one
requires feedback. Event and improvement loops record observations or proposals.
A proposal does not apply its requested mutation.

## Transition Contract

Trajectory checks use `passed`, `failed`, `partial`, and `unavailable`.
`no_checks_run` is a computed proof status. A completion claim with no checks
has an `unsupported` completion verdict. These records summarize supplied
evidence and do not authorize publication by themselves.

Workunits have the concrete states `DRAFT`, `EXECUTING`, `CLAIMED`, and
`VERIFIED`. The normal progression follows that order. `EXECUTING` may return
to `DRAFT`, and same-state transitions are supported. `CLAIMED` here describes
the workunit's proof lifecycle; it is not a replacement for todo ownership.

The transition to `VERIFIED` requires a nonempty proof plan, a passing result
for each planned gate, and a living-spec reference when that surface exists.
Completion evidence additionally binds the task, workunit, context capsule,
repository material, validation epoch, and referenced artifacts. Structural
integrity and hashes do not authenticate an unconfigured external producer.

A passing task-verification row is current only when its artifact binds the
same task ID and current nonnegative revision. Changing a completed task does
not erase historical proof or change its `done` row, but removes its eligibility
to appear complete or satisfy dependent work. Returning a task to a prior
lifecycle state does not restore earlier proof: prune advances revision, and
new completion/recapture must establish fresh custody. Legacy replay never
assigns a current revision to historical evidence whose revision provenance
cannot be reconstructed.

Obligation status is derived from current transitive dependencies, required
proofs, and a nonempty state-commit root. Stored `open` or `met` values cannot
substitute for those inputs. Cycles, missing dependencies, malformed proof
requirements, absent claims, expired proofs, and current contradictions cannot
produce `Met`. Completion attempts are identified by a recorded root; legacy
`met` rows are also rechecked. Unattempted open backlog is not a publication gate.

HealthEngine proof order uses numeric epoch seconds. Because timestamp ties do
not establish event order, any failure at the latest second dominates tied
passes; all tied passes must still satisfy their SLAs. Expiry remains inclusive
at the expiry second. A strictly newer passing event can recover from an older
failure. Unknown latest results, future or malformed timestamps, invalid SLAs,
and overflow cannot establish `VERIFIED`. An unorderable timestamp invalidates
the derivation instead of being discarded. Current claim/event evidence remains
authoritative regardless of `health_cache` contents or inspection history.

Full publication readiness concerns the exact workbranch candidate and accepted
target under the applicable current contract. CI-local success, project
prerequisite checks, branch push, task status, and provider observations each
have narrower meanings. Relevant source, target, policy, workflow, toolchain,
or epoch changes require current proof before publication.

## Events and replay

Canonical runtime events live in the datastore and expose stream, event ID,
timestamp, sequence, event type, payload, and actor through `core::events`.
Replay uses canonical stream sequence; bounded query pages preserve the full
required history rather than sampling it.

An identical event identity with identical semantic content is duplicate-safe
where the event contract permits it. Conflicting content under the same
identity fails visibly. A unique per-stream sequence prevents ambiguous replay.
Sequence normalization is transactional and must establish uniqueness before
committing.

Legacy JSONL and old store rows are imported through supported migrations.
Successfully retired inputs remain historical evidence and are not reread as
live authority. New conflicts or migration failures stop the affected operation;
they do not authorize direct database editing or a silent second store.

## Retry and recovery

Retry safety belongs to the specific operation. Decapod does not promise
exactly-once execution for arbitrary external commands.

Cloud task creation reserves an operation identity before remote mutation.
Resuming that operation preserves task and creation-event identity across
retries and agent handoff. A fresh identical request remains a new operation.
The original creation event retains its actor attribution.

An unknown submission outcome remains unknown through later authentication or
readback failures until an authorized exact creation-event receipt establishes
the result. Diagnostic operation status and current-attempt failure remain
separate. Recovery metadata excludes task text and credentials.

Local datastore locks are bounded, advisory, and released by the operating
system. Contention does not authorize deleting a lock or repairing the store.
Broker audit abandonment records that an incident was acknowledged while
preserving the original event and unknown mutation outcome; it does not undo
the operation or certify database health.

## Projections and optional observations

Spec refresh updates only supported generated material and preserves authored
prose. Refresh must reject a transformation that would lose authored content
before writing specs or the manifest. A fingerprint-only change cannot satisfy
the material authored-spec requirement.

Jev observations belong to the active trajectory and retain their typed
`trajectory_satisfies_intent` meaning. Missing, invalid, and present governance
inputs remain distinct. Provider or persistence failure yields
`no_observation`; a probability is neither approval nor proof.

## Failure reporting

Failures must preserve the violated condition, relevant state, and a supported
next action. Missing evidence, failed proof, stale evidence, unavailable access,
ownership conflict, and a human decision gate are different outcomes.
Recoverable failures leave work open for repair and revalidation. An
unsupported recovery path leaves a visible blocker.

## Invariants

- Each entity has an identified authority; generated views do not create a
  competing source of truth
- Supported mutations honor the applicable session, policy, ownership, and
  workspace gates
- Required lifecycle evidence commits with the corresponding state change
- Stale updates and conflicting event identities cannot become successful
  transitions
- Verification requires its declared proof; an assertion or optional observation
  cannot satisfy a missing gate
- Current readiness is bound to its actual candidate, accepted target, and
  governing inputs
- Recovery preserves original evidence and user work when ownership or outcome
  cannot be established
- Invocations and owned subprocesses remain bounded; persistent state carries
  task continuity after they exit

<!-- decapod:capability-overlay:background-processing:start -->

Generated capability obligations: `background-processing` is declared for this project. These are decisions and proof requirements, not claims of implemented behavior. Resolve each against project source and tests; record unsupported or non-applicable mechanisms explicitly.

## Background Processing Semantics Overlay

### Retry Semantics
- Retry and backoff behavior MUST be selected and documented for each work class
- Poison-work handling MUST be selected and documented for each work class
- Retry MUST preserve the declared side-effect and idempotency semantics

### Idempotency
- Each job MUST declare whether it is idempotent, transactional, compensating, or otherwise duplicate-safe
- Deduplication or compensation mechanisms are project decisions and require proof
- Duplicate execution MUST follow the job's declared duplicate-handling semantics

### Poison Message Handling
- Document the actual terminal-failure behavior for exhausted or non-retryable work; a dead-letter queue is not implied
- If a dead-letter queue exists, document its monitoring, alerting, and ownership
- If manual replay is supported, document authorization and duplicate-side-effect proof; otherwise record the limitation
<!-- decapod:capability-overlay:background-processing:end -->

<!-- decapod:capability-overlay:persistent-state:start -->

Generated capability obligations: `persistent-state` is declared for this project. These are decisions and proof requirements, not claims of implemented behavior. Resolve each against project source and tests; record unsupported or non-applicable mechanisms explicitly.

## Persistent State Semantics Overlay

### Transaction Semantics
- Atomicity boundaries and any non-atomic operations MUST be declared with source and test evidence
- Document observed read-after-write behavior; do not infer consistency from a capability declaration
- Eventual consistency windows MUST be documented

### Migration Semantics
- Migration compatibility, any breaking changes, and their recovery path MUST be explicit and tested
- Migration rollback procedures MUST be documented
- Data integrity checks post-migration

### Recovery Semantics
- Document supported recovery mechanisms and evidence; point-in-time recovery is not implied by persistent-state
- Recovery objectives MUST be selected for the project and recorded as proof obligations
- Recovery test cadence MUST be selected for the project and recorded as a proof obligation
<!-- decapod:capability-overlay:persistent-state:end -->

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `20cb0a82971ddc723a3d45c985bf1ce38591b45ac1481c29bbd1541386d08c64`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (22 files), `docs/` (1 files), `src/` (188 files), `tests/` (193 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->
