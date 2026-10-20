# Architecture

## Architecture Overview

Decapod is a Rust governance kernel invoked by coding agents through a CLI and
structured RPC. It connects human intent, project authority, coordinated
execution, and evidence in repository-owned state. Each invocation is finite;
the repository carries the work between invocations.

Vega's separate native worker keeps evidence collection and final acceptance distinct. Under the maintainer-approved 2026-10-20 functional contract, the eight-run recipe collects unchanged numerical comparisons, six pairs of fixed functional cases on Metal/CPU, and a required aggregate through the existing kernel-owned mechanism. `production_evaluation::execute_contract_checks` actually exercises bounded candidate gate, tokenizer or calibration code before constructing their observations; labels and fixture provenance remain outside the model input. `functional_snapshot` calls the shared production projection, then the collector uses the standard request/inference/response/final predicate. The reducer binds every case/backend, private context, runtime, model, projected input, raw typed response and expected outcome, and rechecks numerical exports. Missing/failed/altered functional cases cannot become collection success. The old exposed full-patch study and failed statistical criterion remain historical, not a current admission prerequisite or passing accuracy evidence. Fresh recurrent state, artifact verification, byte/token rejection and original 1,800-second command/7,200-second overall bounds remain. Unsigned replay never replaces live authentication, custody or final material recheck. No new control plane, Python, orchestrator, inactive/advisory substitute or model/threshold change is introduced.

The production assessment retains declared plan obligations as well as intent:
proof hooks, unresolved/deferred questions, contradictions, stop conditions and
phase gates. These declarations are distinct from execution evidence and are
serialized without clipping under the existing byte/token bounds. Final
governance custody remains authoritative. A separate verification-only Rust
source audit (`assets/tools/vega-native/public_review_sources.rs`) prepares
prospective public review inputs without fitting or inference. Its independently
collected report cannot satisfy numerical or readiness-calibration acceptance;
missing source research cannot suppress independent Metal/CPU diagnostics.

## Direction

The d3ebad07 eight-run recipe actually compiled and passed40native tests and
real Metal probe/32recurrence comparisons. Pinned download failures56/28
prevented all three numerical model comparisons and most evaluation phases.
The last CPU shard executed22cases, with4false-ready proxy outcomes,
11abstentions and1unsupported-length case; this is incomplete evaluation.
The final aggregate remains strict, but now retains independently collected
phase diagnostics even if an earlier missing numeric export stops scientific
verification. An error report is not acceptance or new model-equivalence proof.

The native worker's pinned Metal recurrence implementation is isolated in assets/tools/vega-native/vendor/metal-kernels, preserving upstream f22e42c API, arithmetic and other kernels. Three end-token barriers repair shared-key synchronization between SIMD groups. Under official1191 candidate aa66c8e4 passed all30native tests,32fresh-state primitive repetitions per diagnostic and138/314-token actual Metal/CPU model comparisons with the separate F64 reference. The earlier19combined-engine failures remain preserved. Probe provenance binds patched shader SHA3ba973f4 separately from upstream revision. This execution establishes only recorded fixtures, not all-context equivalence or readiness calibration. The vendored license now uses the official standard Apache text instead of a copied project-specific appendix; shader bytes and external model/calibration assets are unchanged.

Verification-only `assets/tools/vega-native/reference_backbone.rs` computes selected raw blocks 13/19, pooling and engine inputs with its own scalar F64 equations from pinned upstream sources, outside candidate tensor/model kernels. Shared generic parsing/hash infrastructure, weights and same-agent authorship limit independence; no separate human review or empirical HF execution is claimed. Its cap is 4,096 fresh unpadded pure-text tokens. Candidate `30015be0` executed all three predeclared fixed fixtures, including all 16 memory slots and the 4,096/4,097-token boundaries. Production Candle/candle-vllm/CPU paths and normal pooled-only responses remain separate. See `docs/vega-reference-contract.md` for measured errors, timing and the actual supported scope.

The product is a local-first command-line runtime with a structured process RPC
surface. Agents invoke it at decision and state-transition boundaries. Durable
repository records provide continuity instead of a resident orchestration service.

## Current Facts

- Rust implements the `decapod` binary and its supporting library
- The binary embeds Constitution data and agent documentation
- Local runtime state uses Dactyl-backed SQLite; cloud routing is optional
- Git worktrees and supported containers provide task execution isolation
- CLI and Decapod-specific process RPC are the current integration surfaces

## Topology

The runtime is organized by responsibility:

```text
Agent or harness process
  decapod CLI / structured RPC invocation
    Authority and context
    Task custody and workspace lifecycle
    Governance records and proof gates
    Storage facade and bounded provider adapters
Repository
  Source and Git history
  Authored specifications and tracked governance
  Runtime datastore and managed evidence
```

## Component Responsibility Matrix

`src/main.rs` enters the library runtime. `src/decapod/cli.rs` defines command
surfaces; `src/decapod/lib.rs` dispatches them. The core implements governance,
state, task custody, workspaces, context, and proof. Plugin modules provide
capabilities such as knowledge, federation, policy, health, and bounded
automation. Embedded Constitution data and agent documentation ship with the
binary.

The agent interprets the request, plans and performs work, configures the
project's build, test and CI paths as needed, writes living specifications, and
gathers evidence. Decapod steers those responsibilities through project rules,
state and actionable reports, evaluates supported transitions, and persists
their state. Validation is one phase in that lifecycle. The runtime does not
own a model loop or keep a background service running for a task.

## Execution Path

An invocation parses a command or RPC request and resolves its project context.
The applicable session, authority, claim, workspace, and policy checks run before
the governed operation. Stateful paths reconcile supported migrations and use
the canonical storage boundary. The operation records its required evidence and
returns a scoped result or actionable blocker. Read-only discovery remains
available for diagnosing missing or damaged state.

## Intent, authority, and context

The human request and clarifications establish meaning. The agent records its
accepted interpretation, scope, assumptions, and proof expectations in governed
project artifacts. This context remains valid when execution moves into an
isolated workspace.

The Constitution provides structured engineering guidance. `core/DECAPOD`
routes broad intent toward the smallest relevant doctrine surface. Project
configuration and recognized directive bodies in `.decapod/OVERRIDE.md`
specialize that authority. Resolution records the applied directive, source,
hashes, byte count, and precedence; a malformed or conflicting overlay fails
visibly.

Orientation and context capsules bind selected sources and policy to the
task. Capsules use canonical serialization and hashes so consumers can verify
their recorded inputs. A hash establishes content identity, not the truth or
completeness of the selected context. Repository mapping respects tracked and
non-ignored inputs rather than traversing dependency and build trees without
bounds.

## Concurrency and Runtime Model

Sessions identify local agent custody. Todos and their claims coordinate
assignments, leases, dependencies, and handoff. Exclusive claims reject active
ownership conflicts; supported shared ownership remains explicit.

Workspace creation requires the applicable claim and builds an isolated Git
worktree or supported container-backed workspace from an accepted committed
base. The root checkout's unrelated staged, tracked, untracked, and ignored
changes remain in place. Branch names and directory names alone never establish
ownership.

The canonical governance root resolves shared runtime coordination for managed
worktrees. Authored specs, generated workspace projections, and the active
tracked governance document belong to the work branch. This separates shared
coordination from the candidate files being reviewed.

Container and workspace lifecycle receipts bind the invocation, path, directory
identity, and creating runtime where applicable. Bounded subprocess helpers
capture output and terminate and reap owned children on failure. Cleanup acts
only on established ownership and preserves uncertain resources with a recovery
diagnostic.

`plugins/container.rs::finalize_container_branch` separates child success,
publication, foldback and disposal. Failed commands may return their branch for
recovery but never invoke push or PR publication. Successful PR requests call
`workspace::publish_workspace` on the owned clone, using the same governance,
exact-candidate validation, normal push and verified draft-PR path as the public
workspace command. Publication may create a checkpointed commit; foldback occurs
afterward so the host receives the final candidate. Push-only execution returns
the branch first. A local-only/publication option conflict fails before runtime
startup, and an inconsistent explicit/configured publication base fails with
retained-workspace recovery instructions.

Foldback uses Git's ordinary non-forcing ref update, including its atomic rejection
of non-fast-forward host divergence. A checked-out host branch uses the existing
fast-forward-only path. Neither recovery nor success authorizes overwriting
independently advanced host history.

`cleanup_workspace_clone` separates runtime removal from workspace disposal.
Failed or interrupted commands keep the clone even if foldback succeeded.
Successful disposal requires foldback, the expected attached branch, an ordinary
visible index, no added or changed refs outside the returned branch, and the
bounded shared `workspace::worktree_is_dirty` check. `prepare_workspace_clone`
captures inherited refs after initialization; comparing raw ref OIDs preserves
new stash, side-branch and annotated-tag custody without treating unchanged
inherited refs as new work. Index inspection is NUL-delimited and never clears
assume-unchanged or skip-worktree flags. Unknown Git state fails closed with the
retained path; ownership is rechecked before removal. These are cooperative,
post-execution checks, not an atomic filesystem snapshot or reflog preservation.

## Deployment Topology

The installed product is an on-demand executable on the host or in a managed
execution container. Its managed Dockerfile supplies the agent's execution
environment; an application's own Dockerfile remains a separate product artifact.
Git, the selected container runtime, optional hosted storage, and optional remote
providers have independent availability and access requirements. Selecting an
adapter does not establish that its external service was provisioned or verified.

## Data and Contracts

Decapod separates several kinds of state:

- `.decapod/config.toml` records project configuration
- `.decapod/OVERRIDE.md` records human-editable project authority
- `.decapod/managed/specs/` holds the agent-authored product contract plus
  explicitly marked generated projections
- `.decapod/governance.json` is the single tracked governance document for
  the accepted baseline and active change, including claims, checkpoints,
  plan, trajectory, validation, and optional observations
- `.decapod/data/decapod.db` is the canonical local runtime datastore for
  coordination and event evidence
- Ignored workunits, context capsules, and managed artifacts hold current
  execution state and evidence whose custody is governed by their contracts
- Git preserves accepted source, specifications, and governance history

A fresh work branch receives a fresh active-change journal while carrying
accepted policy and unresolved obligations forward. Ordinary repeated calls,
renames, and rebases preserve the active workstream. An accepted merge and a
published release are separate facts.

Starting another trajectory run is a guarded journal transition. Under the same
governance-document lock, `trajectory::init_trajectory` proves that the complete
predecessor document is recoverable from Git before replacing trajectory and
intent custody. Historical Jev observations remain read-only and preserved. An uncommitted failed or interrupted run
cannot be discarded by choosing another run ID; the agent continues that run
or checkpoints and commits it before starting another. Git remains the history
store rather than a second snapshot database.

`governance_document::checkpoint` checks unstaged material and ID reuse inside
the existing document lock, before and after loading current authority and
measuring material respectively. This prevents a cooperating writer from
classifying another checkpoint’s temporary publication file as authored input. It preserves the
first record, rejects conflicting content before saving, and treats an identical
request as a no-op. Proof references are sorted and deduplicated before comparison.
The material digest includes staged source and governed semantic inputs, but
excludes the checkpoint list itself. Cooperating document writers serialize;
this does not claim isolation from an unrelated process mutating the Git index.

Governance document writers lock the complete read-modify-write operation and
write atomically. Related sections share intent and epoch references instead
of creating competing copies. Generated views remain projections of their
inputs and never become a second authority.

Plan acceptance is bound to its execution contract through the existing locked
mutation boundary in `src/decapod/plan_governance.rs`. Material plan, phase, or
gate definition edits return the plan to `DRAFT` and clear every phase/gate
progress flag and timestamp. Exact no-op updates preserve progress. The
conservative all-phase reset prevents later phases from retaining evidence that
depended on an earlier contract. Reapproval is a separate declared transition;
phase entry/completion cannot manufacture approval from draft state.
`validate_plan_progress` is shared by explicit transitions, execution checks,
and the publication validation gate to reject inconsistent completed state.
This adds no separate storage, authority service, or harness-specific runtime.

## Persistence boundary

Decapod owns domain schemas, transition rules, migration order, identity,
retry policy, and the meaning of events. All application storage access crosses
the Dactyl facade in `core::dactyl_db`. Dactyl owns physical execution, access
mode, atomicity, normalized results, and its supported integrity and recovery
operations.

Local connections use a bounded advisory sidecar lock. Cooperating Decapod
processes conservatively serialize connection lifetimes across host and
container access. The lock is released by the operating system; a persistent
lock filename is not a stale lease to delete.

Material task and agent-state mutations commit with required event evidence.
Task revisions and conditional updates reject stale transitions. Canonical
events use named streams with unique event identity and per-stream sequence.
Consumers use the semantic event interface; legacy JSONL and split stores are
migration inputs rather than live alternate stores.

Optional cloud routing is repository-scoped. Decapod derives repository identity
from the supported Git origin and accepts an authenticated opaque datastore
route. The hosted service owns its authentication and tenancy boundary; Dactyl
owns transport and physical execution. A local transaction test does not prove
that a deployed service implements the same contract.

Task proof keeps its subject in the existing verification-artifact JSON: the
stable `task_id` and the exact `task_revision` accepted by the completion
transaction. `PreparedTodoVerification::persist_on_connection` binds the final
revision after `done`; explicit recapture binds the current completed revision.
`todo::effective_verification_status` is the shared eligibility boundary for
work claims, dependency admission, exclusive completion, aggregate verification,
and read-only diagnostics. Historical proof remains visible when its binding is
missing or stale, but cannot authorize current work.

Verification and batch recovery carry the task and baseline they actually
inspected. Under one broker transaction they compare status, revision, proof-plan
bytes, artifact bytes, and the previous verification outcome before recording
new results and canonical events. Batch refresh returns its exact written
snapshots rather than selecting newly appeared tasks into an earlier validation.
Recovery keeps every refreshed row pending while aggregate validation runs with
exactly those task-proof admissions scoped to the recovery. The final transaction
checks each admission against current task, epoch, output, and file evidence,
then promotes the complete batch atomically; an error cannot leave partial passes.
The exemption affects only selected proof admission; completed-task counting
still enforces the required governed plan. A scope guard restores temporary
verification exemptions on every return, including recapture and completion's
nested validator calls.
Prune deletes proof, reopens the task, increments revision, and records its event
atomically. The legacy replay helper lacks complete revision history and removes
revision authority from its projection while retaining the original events.
This uses existing task revisions and SQLite/Dactyl transactions, not another
state store or execution runtime.

## Runtime reconstruction and execution continuity

`core::todo::task_journal` extends the existing canonical TODO event stream with
versioned projection checkpoints. Task scalars, revision, lease generation,
ownership, dependencies, and proof bytes are captured by the task transaction;
legacy event logs are not assumed to contain those complete preimages. An
explicit baseline records existing authority without inventing earlier custody.
Chain validation checks identity, predecessor and projection digests before
restoring a projection. Divergent live authority is not overwritten.
A surviving mutation beyond the last checkpoint blocks reconstruction. Complete
journal-tail removal cannot be detected without independent evidence; this is
not an externally sealed tamper-proof log. Post-baseline legacy imports and
sequence renumbering require an explicit migration instead of silent repair.
The optional cloud task schema has no reconstruction adapter in this contract.

`core::todo::task_journal::transfer` (implemented in
`src/decapod/core/task_transfer.rs`) adds explicit bounded custody transfer for
standalone yielded local tasks. A source transaction records the exact transfer
artifact and increments its revision; the shared task mutation boundary refuses
later mutations on that sealed subject. Artifact publication follows the durable
record and supports explicit byte-identical recovery. Import validates the source
checkpoint chain through the same pure journal verifier, binds GitHub repository,
receiver and source commit, then atomically creates the original task identity and
a local observed baseline. Foreign events remain nested source provenance inside
the canonical TODO stream, preserving their sequence numbers without pretending
they executed locally. No new datastore or lease service is added.

Only task records/history cross this boundary. Current verification artifacts and
pass timestamps are cleared; exact old bytes remain in the provenance record.
Destination revision advances, requiring fresh claim, orientation and proof.
Connected/shared tasks and any preexisting destination task/hash/history fail
closed. Operator acceptance of an independently obtained digest supplies source
trust. A content digest does not authenticate a sender; local source sealing cannot
prevent divergent clones or privileged writers from continuing. Global exclusive
custody would require an additional trusted authority.

`core::workunit` journals complete manifest snapshots in the canonical `workunit`
event stream. The JSON file remains the ignored, recoverable projection. Journal
commit precedes file publication; interruption can leave the projection behind
its durable head, which explicit recovery reconciles without changing proof.
Read-modify-write helpers serialize the whole operation; external prepared writes
require the expected journal generation. An identical operation retry is a no-op;
conflicting identity or predecessor reuse is an error.

`core::trajectory::execution` binds checkpoints to current repository material,
accepted intent and plan authority using existing material and validation-epoch
owners. Recovery and resume are separate operations. Restoring a manifest does
not authorize resumed execution or establish publication readiness. The full
validation and publisher remain the only source of their own readiness verdicts.

Atomic artifact publication syncs file contents before rename. On Unix it also
syncs the parent directory. A directory-sync error is an unconfirmed durability
outcome even if the complete new file is visible; recovery inspects the journal
and file instead of treating that error as proof of rollback. Other platforms
retain atomic visibility without a claimed directory-fsync guarantee.

## Current obligation evidence

`core::obligation::Derivation` walks dependencies iteratively on one canonical
broker-held read connection, with one captured time and memoized node results.
The existing Dactyl/SQLite storage lock keeps cooperating local writers outside
that read. Dependency readiness uses current derived results rather than stored
`obligations.status`; deep chains and diamonds need no recursive call stack or
cache mutation. Direct edge reads retain missing endpoints and cycles as
verification failures. Whole-graph inspection counts every stored edge, including
orphan sources. New obligations validate dependencies before transactional insert.

`plugins::health::derive_health_from_connection` reads claims and proof events
on that same connection. It never reads or updates `health_cache`. Health
inspection uses the same derivation before refreshing that advisory cache.
Exact claim IDs take precedence; subject aliases must be unique. Missing proof
state is unsatisfied, while malformed storage and ambiguous aliases remain
errors. No additional state store, daemon, or completion ledger is introduced.

## Proof and publication

Plans, claims, trajectories, workunits, obligations, context bindings, validation
epochs, and completion evidence describe different parts of the same work.
The proof layer checks their consistency and required evidence instead of
equating one successful command with completion.

Validation is bounded and returns explicit failures and supported recovery.
Full workbranch readiness binds the exact candidate, accepted target, applicable
profile, policy, and execution evidence. CI-local validation has a narrower
scope and cannot authorize publication. Candidate or authority changes can
invalidate earlier proof.

The native validation route and optional Buildkite route implement required
check profiles. Buildkite is a provider of execution evidence. Its presence
does not change the ownership of intent, policy, or proof requirements.
Provider details belong in [VALIDATION.md](VALIDATION.md).

The controller's finite source adapters are separate from the configured
project's reviewed inventory. `readiness_profile::required_source_catalog`
checks both before execution selection. A matching declaration cannot authorize
different compiled workflow or helper semantics; this case returns a typed
controller-support blocker rather than asking the agent to repin and repeat.
There is no independently pinned remote controller-bootstrap harness today.

Governed publication requires current readiness and uses a normal non-forced
push. Remote divergence requires reconciliation and fresh proof. Branch push,
pull-request creation, merge, packaging, release, and deployment remain
distinct external outcomes. The human supplies intent and reviews and merges
the prepared PR; the agent owns the engineering work and validation phase. The
checked validation push uses narrowly bound authorization rather than a separate
local operator-approval ceremony.

The dependency-free `assets/tools/ci_event_context.rs` distinguishes a real
change range from a verified ordinary branch creation at the captured accepted
tip. Only the latter emits `no_change=true`; workspace/pre-PR and ordinary update
contexts retain distinct immutable endpoints. The constitution workflow rechecks
the same accepted commit and tree before and after running its schema command.
This no-delta execution produces no measured candidate custody receipt.

## Optional vega readiness assessment

`core::validation_runtime::run` finishes the complete deterministic proof profile
before calling `vega_readiness::assess`. Required mode additionally needs an
uncertainty-free positive singleton and a second exact-state freshness check.
The assessor cannot supply proof, waive failed checks or publish a PR. Disabled
mode preserves ordinary deterministic validation; there is no selectable provider
architecture or active Jev transport/credential path.

The bounded `validation-results.v2` projection interns repeated observations and
stage definitions, retaining all check identities, outcomes, exclusions, simulated
stages, blockers and adverse trajectory facts. It supplies minimal declared intent
and changed paths, without full plans, diffs or logs. Opaque custody hashes bind
the complete evidence in the request/receipt instead of consuming model context.
Unknown log warnings and untested intent remain explicit. Overflow abstains.

The separate `assets/tools/vega-native` worker uses pinned Candle Apple Metal for
the Qwen3.5-0.8B text backbone, F32 pooled block 13/19 features and at most 4,096
tokens. A finite numerical Metal probe must succeed; OS labels are insufficient.
The worker alone owns the tokenizer dependency and concrete tokenizer adapter;
the root library retains tokenizer-neutral prompt/span construction and bounds.
This keeps native-only dependency metadata out of the root validation inventory
without omitting the worker's locked tokenizer or changing inference semantics.
Its portable `vega-native/decoder.rs` also owns SafeTensors payload decoding;
the root exposes the shared checkpoint byte bound and checked weight schema,
and retains configuration, setup header validation and typed scalar execution.
The decoder preserves SafeTensors offset validation, preallocation name/shape
checks, F32/F16/BF16 conversion and final typed finiteness/resource checks.
Portable decoder fixtures must run through the separate worker's test target;
root CI alone cannot establish their execution or native backend acceptance.
Binary16 fixture obligations include both signs of zero and subnormals, normal
and maximum finite values, infinities and NaNs, with explicit 32-bit normalization.
The Rust scalar engine uses F32 and calibration uses F64 probabilities. Required
calibration, unbound/OOD and prediction-set semantics are preserved. Runtime
loads only explicitly pinned local artifacts, resets recurrent state per request,
and has no downloads or cross-request state reuse. Shared `backend = "auto"`
prefers Metal on macOS and CUDA elsewhere; explicit metal/cuda/cpu preferences
and `cpu_fallback = false` are supported. Metal builds retain the pinned
`forward_collecting_layers` path; optional non-Apple `cuda` builds use that same
API and pinned kernel closure. Unsupported GPU builds/devices return a typed
hardware-unavailable result. The controller may then launch one fresh CPU process,
under the original deadline and frozen model/runtime pins. A negative, abstaining,
malformed or failed numerical result is never a fallback trigger.

The pinned attention dependency has no CPU GDN kernels. The separate worker's
`cpu_backbone.rs` implements the dense, text-only profile in Rust with Candle CPU
GEMMs, causal grouped attention, partial RoPE and scalar gated delta recurrence.
It upcasts the same BF16 weights to F32 and captures raw blocks 13/19, stopping
after block 19 because later blocks/final norm/logits do not affect those features.
State is request-local. CPU recurrence and RoPE fixtures establish mathematical
test obligations only: this new CPU path is not an independently reviewed oracle,
and CPU/CUDA/Metal execution and cross-backend parity remain unverified.
Receipts retain actual device and per-attempt bindings/errors; a CPU result cannot
count as evidence that Metal ran. Existing scalar engine and calibration remain
CPU F32 and F64 respectively. No model identities change during fallback.

Project policy and machine installation have different owners. Team enablement
belongs in tracked configuration; each maintainer's verified cache/runtime binding
belongs in existing private local storage outside Git. Resolving that binding must
select an immutable configured model identity, never a floating active pointer,
and recapture local runtime identity before accepting a decision. This separation
is implemented by the shared `model_manifest_digest` and private `vega_runtime`
binding, registered only by explicit setup. Legacy absolute `[vega.installation]`
bindings remain supported without being generated for a new team policy. The
validation runtime captures local custody before deterministic proof, refuses a
changed binding before inference and rechecks runtime/model bytes before consuming
an acceptance. Missing setup produces a post-proof abstention. Local registration
pins operator-selected executable bytes; it does not prove build provenance or
native execution. These new storage/freshness regressions require execution.

`vega_setup` owns explicit bounded discovery/download, independently pinned
engine/backbone/tokenizer/config/license identities, content-addressed download
reuse and atomic activation/rollback. Installation copies each component into a
private temporary artifact, synchronizes and independently verifies the copy,
then installs without clobbering another file. Cache and installation files each
retain one link; offline reuse saves downloads while requiring separate disk
space. Full checkpoint verification still precedes atomic directory activation.
The active pointer is never runtime authority.
Private numerical diagnostics support future measurements but do not establish
parity. Actual Apple execution, independent reference agreement, readiness-domain
calibration and latency/memory measurements remain required acceptance evidence.

## Architectural constraints

Decapod remains daemonless, local-first, and agent-operated. Supported mutations
require the applicable session, ownership, policy, and isolation gates.
Machine-owned state changes through the CLI or RPC; authored living-spec prose
is the explicit direct-edit exception.

Recovery preserves uncertainty and original evidence. A timeout, missing
projection, unreachable service, or interrupted client cannot establish that
an operation never happened. Unsupported repair and unresolved ownership stop
the affected transition for review.

## Delivery Plan

Changes to Decapod begin with the affected user outcome and contract. The agent
records scope and proof expectations, implements a bounded change in its claimed
workspace, updates the owning specifications, and checks the actual behavior.
Compatibility and migration consequences are reviewed before exact-candidate
validation and governed publication. Release and deployment require their own
evidence and authority after source readiness.

## Risks and Mitigations

- Misunderstood intent remains possible. Authored specifications and explicit
  unresolved questions make it reviewable before publication
- Coordination races can lose custody. Conditional mutations, leases, canonical
  events, and isolated workspaces reject or expose conflicts
- Crashes and remote timeouts can leave unknown outcomes. Durable operation
  identity and conservative recovery preserve that uncertainty
- Provider drift can invalidate proof. Reviewed profiles, immutable source and
  attempt bindings, and fresh final checks reject mismatched evidence
- Direct commands can bypass Decapod. The agent contract and human review remain
  necessary outside the runtime's enforceable interface boundary

<!-- decapod:capability-overlay:persistent-state:start -->

Generated capability obligations: `persistent-state` is declared for this project. These are decisions and proof requirements, not claims of implemented behavior. Resolve each against project source and tests; record unsupported or non-applicable mechanisms explicitly.

## Persistent State Architecture Overlay

### State Ownership
- Each entity type MUST have a designated state owner
- State ownership boundaries MUST be explicitly documented
- Cross-boundary state access MUST go through defined interfaces

### Transaction Boundaries
- Multi-entity mutation atomicity and partial-failure treatment MUST be documented and proven for the selected store
- Transaction boundaries MUST be documented in ARCHITECTURE.md
- Document compensation or other partial-failure treatment if distributed operations exist

### Storage Abstraction
- Storage ownership, consistency behavior, and access boundaries MUST be explicit
- Portability or swappable implementations are project decisions, not universal requirements
- Migration and rollback treatment MUST match the selected storage technology
<!-- decapod:capability-overlay:persistent-state:end -->

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `16ba47ed87ae6c4f26efe540c20a139f9d0317cda85d454ff12be8444c71971b`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (40 files), `docs/` (1 files), `src/` (210 files), `tests/` (194 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->

## Accepted executor and candidate source

The remote validation adapter separates the accepted executor revision R from
candidate source H. `validation_runtime/executor_contract.rs` captures R from
the accepted target’s immutable Git objects before provider dispatch. Its entire
helper, selector, aggregation and recipe closure must match the installed
controller. The candidate’s workflow, local-action and carrier-script bytes stay
independently pinned. Updating a reviewed inventory does not admit a new carrier
or remove a required check.

The existing source-CI carrier stages R separately and compiles its executor.
Fixed R commands build candidate H with H’s configured build inputs and execute
its tests. The integration preparation job packages the measured R helpers with
H’s built test artifacts; downstream workers authenticate that producer chain
rather than compiling a different verifier. Versioned custody binds executor and
subject identities before and after execution. The original controller imports
provider-authenticated results and rechecks the current source, target and full
expected graph before reporting readiness.

This is a reviewed-transitive-code boundary. Separate directories and closure
measurements prevent accidental authority substitution; they are not an OS
sandbox against hostile build scripts sharing the provider account. The trusted
provider importer compiles the pinned workflow, and its authenticated producer
records remain part of the trust model. Validation does not accept a candidate
CLI’s JSON as its verdict or install that candidate as its controller.

The protocol preserves strict single-root operation for the initial rollout
when the accepted base predates dual-root support. That path still requires H’s
helper closure to match the controller. Unknown carrier semantics, unsupported
R, and native-controller mismatches remain explicit blockers. A released binary
cannot acquire new protocol code merely because its repository source changed.

## Repository-declared local proof

Consumer repositories select `repo.pr-readiness.v1` through the canonical proof
configuration. `core/repository_profile.rs` checks complete discovered workflow
mapping, required commands and explicit execution declarations against reviewed
source and command digests. `validation_runtime/repository_host.rs` translates
that reviewed local contract into the existing bounded native runner; it does not
interpret workflow YAML or introduce an orchestration engine. Every command runs
in the frozen source snapshot, with byte-measured declared tool inputs and fresh
accepted-target queries. Source/configuration/tool/target drift invalidates proof.

The resulting `repository_local_readiness` scope attests those local commands,
not hosted workflow jobs, platform matrices or release/deployment execution. The
repository authors the semantic coverage and transitive toolchain declaration;
the kernel enforces freshness, execution and custody mechanically. Existing
Decapod source-CI adapters retain their stricter authority and cannot be downgraded
by a candidate switching to local proof. Unsupported remote/daemon operations
remain explicit limitations, rather than fabricated native evidence.

### Optional CI-attested validation boundary

Validation establishes whether the agent's work has converged on the user's
declared intent and satisfies the conditions for integration. Assurance remains
bounded by the requirements and checks actually established. A later readiness
decision evaluates that result; the agent publishes through the applicable gate,
and a human merges.

`validation_runtime::snapshot::ci_attested` implements an opt-in alternative to
archive custody. The policy must equal the accepted target's policy, with
Buildkite validation enabled on both target and candidate. The existing compiled
check catalog remains authoritative. `snapshot_provider::public_buildkite` binds
complete successful native-job metadata to the exact repository, pipeline,
workflow, branch and commit. `readiness_evidence::buildkite_ci` verifies distinct
CI-attested observations bound to material, intent and check definitions.

The trust boundary is the configured CI executor and provider-owned HTTPS
metadata, rather than independently downloaded measurements of remote outputs.
No artifact, log, toolchain or binary measurement is invented. Projects without
an accepted CI-attested policy retain archive custody. This repository selects
the CI-attested policy in `.decapod/config.toml`; its activation still requires
maintainer acceptance and a controller containing the corrected adapter.

The finite `buildkite-gha-0.102.7` Pipeline Trigger declaration trusts the
configured Buildkite pipeline to supply exactly one keyless importer. Its label
and count are consistency checks, not independent proof of plugin identity or
unchanged remote pipeline settings. Evidence retains the actual provider job
UUID under a reserved bootstrap identity. Concurrency identities are derived
from the reviewed workflow name, authored expression, repository and exact push
branch using the pinned importer's algorithm. Unknown, duplicate, failed or
retried jobs remain rejection conditions. The complete required catalog chooses
which configured workflows must run; an inapplicable documentation-sync workflow
does not become an extra execution obligation. This change establishes neither
vega inference nor native Apple GPU readiness.

## CPU startup across separate executable pins

`vega_readiness::Installation` may bind a separate `cpu_runtime: RuntimePin`.
`vega_setup` verifies both executable hashes before atomically registering the
private local binding; validation freezes and rechecks both pins. This handles
CUDA dependencies that can prevent a GPU worker from reaching its CPU code.
A model-free `vega_infer --backend-check` precedes supplying inference state,
and uses the existing real GPU arithmetic probe. It cannot produce a readiness
outcome. Typed hardware unavailability may select CPU; absent interpreter or
empty loader exit 127 requires the separate CPU pin. Invalid reports and all
other process/numerical failures remain non-ready. Every attempt stays within
the same deadline and immutable model/calibration binding. These source changes
are not yet executed native evidence.

## Supplemental Rust worker verification

For issue #1445, the combined order is complete source validation, required
native worker tests/runs, fresh deterministic source/target verification, then
Vega assessment and another freshness check. A worker failure returns before
Vega runs. The assessment cannot turn native or source failure into a pass.
Supplemental discovery receives the outer absolute deadline; its 120-second
discovery cap can shorten that deadline but cannot restart the original budget.

`validation_runtime::rust_worker` adds a required supplemental phase to the strict
source profile. `run` first verifies the full primary source verdict, then executes
every configured worker test/run through the existing `native_proof` immutable
snapshot and bounded process runner. It independently recaptures source, policy,
primary tooling and a fresh accepted-target observation afterwards before final
readiness. A native failure leaves private failure evidence and keeps readiness
false. No alternate source profile or partial-source publication route is added.

`proof.rust_workers` contains reviewed relative Cargo manifests, features and run
arguments. Installed Rust/Cargo/linker discovery remains controller-owned; shared
recipes contain no installed tool paths. Cargo tests and runs use `--release` and
`--locked`, fresh private HOME/CARGO_HOME, and an owned target directory outside
source. Snapshot source and original Git refs/index remain protected. Optional
Metal-toolchain inventory on macOS measures xcrun, selected metal/metallib tools
and macOS/iOS SDK metadata. These measurements identify the declared compiler
closure, not every OS/runtime dependency or physical GPU execution.

The supplemental subplan reuses the existing local-command evidence kind/verifier
inside an independently preserved outer strict source plan. It is never a
replacement `repo.pr-readiness.v1` project selection. Only trusted compiled hosts
can provide the existing native external-action authorizer. Recipe declarations
and worker outputs cannot mint that authority. Numerical/model claims still need
their independent reference and evaluation evidence.

### Native diagnostic dependency boundary (#1445)

`assets/tools/vega_infer.rs::diagnose_native` runs the real preferred-device probe, installs only the compiled immutable baseline through `vega_setup::run` into the validation worker's private HOME, and attempts both preferred-device and explicit CPU model inference. Independent attempts are collected even if another backend fails; failed installation is a genuine shared dependency. Collection has no numerical-reference or evaluation-file prerequisite and never fits calibration. Execution errors fail the required worker run after writing diagnostics. The existing accepted controller, source CI, recipe integrity and final freshness checks remain authoritative. This separates evidence collection from the agent's still-unsatisfied parity/calibration publication conditions without changing the kernel or waiving checks.

The separate native worker owns verification-only `reference_backbone.rs`, `reference_engine.rs`, `reference_tokenizer.rs` and `evaluation_corpus.rs`. Reference engine outputs use separately coded F64 equations on pinned weights; conditional comparisons take supplied candidate features as inputs, while combined comparisons take the scalar backbone's independently computed features. Corpus construction decodes an integrity-pinned public Rust benchmark subset without executing its scripts. These modules do not participate in production readiness inference. Official v0.119.1 resolved the historical golden-fixture snapshot rejection and actually compiled/executed the worker on `2f785c82`. Its limited CPU/reference and real Metal arithmetic results are recorded in `docs/vega-reference-contract.md`; Metal model execution, later repairs and readiness calibration remain distinct obligations. Public CI bootstrap failure is an execution dependency, not model evidence or permission to replace required source proof.

### Exact source aliases in native snapshots (#1457)

`src/decapod/core/native_proof/snapshot.rs::reviewed_alias` recognizes only the existing Rust toolchain alias and `tests/fixtures/state_commit_repo/symlink_test -> target.txt`. The fixture destination is the regular in-repository `tests/fixtures/state_commit_repo/target.txt`. Index and HEAD link blobs must match exact expected bytes and both planes must contain the regular target. Working capture verifies the link and target types and hashes; fixture parents must be directories, not symlinks. Copying recreates the link without following it, checks its frozen digest, charges link bytes to existing budgets, and verifies the resulting snapshot with the existing material identities. Arbitrary aliases, submodules, special files and source changes remain rejected. No fixture content or immutable golden vector is changed.

### Vega native runtime compatibility revision

The separate Rust worker explicitly pins candle-vllm b1055eca63de76b548d0d4b0daa96ff8cd1d0596, Candle23a6f388a82a71c88067ad2b0d64ac9d00f24aac and attentionf22e42cf27dd31efd2d1b838bd9e4cbf85bd2305 after the first actual native compile exposed upstream Metal DFlash symbol gating. The candle-vllm successor changes only dependency pins, preserving the reviewed Qwen3.5 block-capture implementation. No model or calibration artifact changes. Rust runtime revisions are diagnostic provenance; they do not prove native execution or numerical equivalence.

### One-shot GPU ownership and independent verification collection

Candidate76e87da2 subsequently executed actual Metal and CPU model inference and the138-token independent numerical subset under official1191, including GPU one-shot reuse rejection. The314-token fixture exposed a reference-only omission: memory vectors were hardcoded zero. The independent scalar reference now computes floor-assigned chunk means from its own block19 rows using pinned vegaml21a17/config12f4f737; no candidate pooling helper or expected values are reused. Analytic boundary/offset/finite guards are authored and require the next validation. Max512 reference tokens, all model/calibration pins and frozen tolerances remain. Both partial heldout backend phases executed and failed proxy-label risk/coverage; broader context parity, production fallback routing and calibration remain unsatisfied. Measured details and preserved failure references are in docs/vega-reference-contract.md.

Measured Metal model load failed because upstream reset_all uses CUDA-only zero_, despite successful real Metal arithmetic. The worker now consumes an owned fresh GPU model for one pooling attempt and discards its complete recurrent state on success, failure or unwind. It synchronizes completion and resource release and refuses a second attempt on that instance; production already uses a fresh worker per request. This changes no persistent installation/task/storage responsibilities and does not ignore a reset failure. The existing Rust recipe collects diagnostics and automatic/CPU holdout evidence independently before a required aggregate verdict, so missing parity or a negative calibration result cannot suppress unrelated collection. The kernel retains the hard deadline and publication authority. Exact prior CPU/model/reference measurements and their single138-token scope are recorded in `docs/vega-reference-contract.md`; new GPU/fallback/collector behavior remains unverified until executed.
