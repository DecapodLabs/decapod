# Interfaces

## Interface Overview

Decapod's interfaces connect human intent, agent work, repository state, and
proof. The human supplies intent and reviews and merges the resulting pull request,
resolving exceptional decisions when needed. The
agent interprets that request, performs the work, and calls Decapod at planning,
coordination, validation, and publication boundaries. Decapod exposes the state,
constraints, evidence, and blockers that govern those transitions. Each CLI or
RPC invocation is ephemeral; repository state preserves continuity between calls.

The optional post-validation Vega request uses `validation-results.v3` in
`core::vega_readiness::project`. It retains the plan's declared proof hooks,
unknowns, human and deferred questions, stop conditions, contradictions and
phase entry/exit gates alongside the intent and actual validation observations.
Gate descriptions, artifacts, checks and satisfaction flags are declarations,
not independent execution evidence. Administrative plan metadata and raw logs
are omitted. The complete serialized input must fit the existing 32 KiB and
token limits; an oversized obligation produces abstention rather than a clipped
or selectively reassuring assessment. The existing final material/governance
identity recheck still rejects an assessment made stale by a changed plan.
`ProjectionContext` deserializes the existing governed plan/trajectory types;
`EvaluationSnapshot` adds the independently derived expected proof plan, complete
suite, binding and dispositions for private replay. Its `project` method checks
deterministic evidence before calling the same production builder. It does not
authenticate unsigned inputs or authorize live publication. New shared-path and
wire-response regressions await the next prescribed validation.

`vega_readiness::permits_decision` is the shared final boolean predicate used by
live receipts/publication and native replay diagnostics. Failed proof dominates
every model result. In required mode only a positive, nonabstaining decision may
pass this predicate; current authenticated proof, custody, installation and
human authority are still established by the live caller, never replay metadata.

Private Vega numerical exports record raw block samples only when `--proof-output` is requested; normal requests return pooled features. Each actual and independent reference export retains its 1 MiB bound. `reference_backbone` accepts at most 4,096 fresh unpadded pure-text tokens. Candidate `30015be0` executed all three fixed 138/314/4,096-token Metal and CPU comparisons, including the exact boundary and 4,097-token rejection. These fixtures use the independent tokenizer, fixed repeated sentences and bounded literal suffixes; token/span/mask/layer comparisons and frozen numerical tolerances remain exact or unchanged. This is fixed-fixture evidence, not all-input equivalence. Full production loader-companion fallback remains separate from the exercised unavailable-CUDA-to-CPU component path.

The private evaluation corpus and input receipt use v2 for the pre-execution addition of contradictory semantic-evidence cases. `benchmark_units` counts original benchmark tasks, without asserting statistical independence. Model output records distinguish correct required abstentions, incorrectly decisive answers and unobserved cases. Existing project split, underlying artifact hash and model/calibration pins remain unchanged; v1 construction remains in Git and no model result informed the revision.

The standalone `--evaluate-readiness --split holdout --backend auto --proof-output` command is now historical v2 diagnostic evaluation, not an untouched final holdout. The configured eight-run recipe retains numerical collection, six final-admission collection slots and a mandatory aggregate, all with fresh kernel proof paths. Numerical collection additionally calls the real native worker on fixed production-protocol development fixtures through shared projection and `Response::validated_outcome`. Constructed observations are explicit and supply no independent labels. The final slots bind the reviewed source-admission ledger and report zero admitted production units, without rerunning exposed v2. The aggregate independently rechecks numerical exports and remains non-ready while the required production study is missing. Old v2 code, hashes and failed results remain intact; risk/coverage, token limits, model/calibration and kernel deadlines are unchanged. See `docs/vega-reference-contract.md` for exact prior numerical measurements and `docs/vega-completion-checklist.md` for current proof gaps.

## Human and agent entrypoints

The separate scientific worker validates its own ephemeral nonce format locally (sha256: plus64lowercase hexadecimal digits). It does not call or expose the main crate's private model-installation digest helper. Candidate4cdcd25d native compilation rejected that cross-crate call with E0603 before any new test or model execution; repair preserves the abstraction boundary and leaves all receipt/acceptance checks intact. A canonical nonce detects malformed bookkeeping, not authenticated global ownership.

`docs_cli::build_docs` projects the optional Vega contract from `docs/agent/repository-readiness.md` into generated `docs/agent/config-schema.md`. Editing the source contract requires regenerating and reviewing that projection; a stale generated copy fails documentation source proof before native collection. Candidate132fd505 retained this failure in validation063311Z/Buildkite2225. The prescribed source-built documentation generator repairs Markdown only; installed official v0.119.1 continues to govern ownership and validation. Generation success is not a test, numerical result or publication permission.

Native probes report attention_metal_gdn_source_sha256 separately from the upstream attention revision, with null on CPU. Actual aa66c8e4 Metal primitives and two model fixtures passed; status is not a general parity certificate. Scientific context/envelope JSON is private and scoped to one kernel evidence directory; source/runtime/model/corpus bindings detect stale or incomplete collection, not malicious same-user forgery or global task ownership. Private context and phase files require local ownership, restrictive permissions, single links, exclusive creation, bounded parsing and safe filenames. Production request/decision schemas and deterministic authority remain unchanged: failed, missing or stale proof returns before optional Vega assessment.

### CI-attested execution declaration

`proof.ci_attested.workflows` maps each supported workflow's native step keys to
the compiled required checks. Its optional `pipeline_trigger` declaration names
the finite `buildkite-gha-0.102.7` contract, exact workflow name and, when present,
the supported authored concurrency expression. The configured executor is trusted
to supply one keyless importer; metadata verifies its terminal success and actual
job UUID, not the plugin implementation. Derived concurrency gates and the reserved
bootstrap evidence identity are distinct from required logical validation checks.
Collection reads only build, complete steps and complete jobs metadata. It rejects
failed, retried, missing, duplicate or unexpected jobs and mismatched source/run
identity. Optional configured workflows are collected only when the compiled
catalog requires them. Exact accepted-target policy equality remains mandatory.

The activation PR uses an explicitly approved one-time direct-check and exact-CI
bootstrap review. It cannot report released-controller publication readiness for
its own new policy. Subsequent changes use the accepted policy and released
controller through ordinary validation.

Human setup is `cargo binstall decapod && decapod init` in the project, followed
by a task given to the person's coding agent. The README keeps this path short.
Operational commands belong in the agent contract and agent documentation.

The human-facing README is a brief product overview: its existing visual layout
and section order make capabilities easy to scan, while the introduction
explains agent-owned engineering work. It distinguishes parallel owned tasks
from continuing one pull request in another tool. Validation stays in prose as
an agent responsibility; its optional remote execution applies to that phase,
not to Decapod as a whole. The README does not add operating instructions or
historical proof reports to the single-command setup path.

`CONTRIBUTING.md` gives human contributors the review expectations and project
layout. The agent contract and linked technical references carry the operational
workflow. Contribution guidance assigns build orchestration, verification and
repair to the agent, without requiring a human to expand credential permissions
or run a manual prerequisite suite.

Agents discover the installed interface with `capabilities`, ingest its command
contracts with `docs ingest`, and retrieve engineering guidance with
`constitution get` or constitution search. `docs` serves `docs/agent/`; the
Constitution supplies broader engineering doctrine. Version, capabilities,
Constitution lookup, and embedded documentation remain available when project
state needs recovery. Documentation ingestion can update an existing local
awareness marker; it is not an implementation or completion proof.

`eval --stdin --format json` accepts the complete incoming prompt. The agent
proceeds only after exit 0 and `status: allow`. A blocked result or nonzero exit
requires human review. This gate applies at startup and to each new prompt.

## Inbound Contracts

The CLI is the operational surface. `decapod rpc` provides Decapod-specific
JSON request and response envelopes over process stdin/stdout. The binary does
not currently provide an MCP server, MCP lifecycle, or MCP discovery. Future
adapters are separate contracts.

An RPC request contains `op`, `params`, a correlation `id`, and an optional
`session`; omitted parameters default to JSON null and omitted IDs are generated. The response includes `id`, `success`, a `receipt`, and
`allowed_next_ops`. Depending on the operation it also carries `result`,
`mandates`, `context_capsule`, `blocked_by`, `interlock`, `advisory`,
`attestation`, or `error`. A receipt records the operation, time, input/output
hashes, touched paths, and governing anchors. An operation receipt establishes
what that call recorded, not the correctness of the whole task.

## Failure Semantics

Blockers distinguish `missing_answer`, `missing_proof`, `unauthorized`,
`conflict`, `validation_failed`, `workspace_required`, and `protected_branch`.
Errors carry a code, message, and optional details. Callers inspect these fields
rather than infer domain error kinds from exit status:

- `0`: the requested operation succeeded within its reported scope
- `1`: a domain, validation, configuration, session, I/O, or storage failure
- `2`: CLI syntax rejected before execution
- `127`: the calling shell could not locate a command; this is not a Decapod
  domain error

The CLI, aliases, and RPC share their underlying execution gates. In particular,
a failed `validate.run` retains its structured response and returns nonzero.
Agents use the installed command contracts and schemas rather than inventing
operations or treating a listed next operation as approval to bypass its gates.

## Intent, authority, and context

The prompt, conversation, and explicit human clarifications establish request
meaning. An isolated workspace supplies the execution filesystem; a todo
supplies ownership and coordination. Entering a workspace does not erase the
agent's request context.

`infer orientation` exposes the goal, task binding, governed plan, allowed and
forbidden scope, relevant areas, proof requirements, known unknowns, decision
gates, and next action. `context.resolve`, capsule queries, and context bundles
carry selected project specifications and Constitution material with their
sources. A retrieved recommendation or advisory observation is not permission
to cross an execution boundary.

Project authority comes from `.decapod/config.toml` and `.decapod/OVERRIDE.md`.
Each registered override directive owns a fenced source body. Resolution
preserves nested Markdown and examples, removes the outer wrapper, and exposes
`directive_id`, `source`, `source_hash`, `body_hash`, `byte_count`, and
`precedence`. Duplicate exact directives, unclosed wrappers, and non-empty
unknown Decapod-namespaced directives invalidate the whole overlay. Empty
retired generated directives remain upgrade-compatible. Partial application
must not conceal ambiguous authority.

## Sessions, tasks, and workspaces

Most mutations require an active repository-local session. Local session
credentials establish custody and correlation; they do not authenticate a model
provider or human principal. Cloud authentication credentials are separate,
machine-local material. Session records and credentials must not become tracked
project content.

Todos have durable IDs, lifecycle state, and explicit ownership. Exclusive
claim mode takes the assignment; shared mode joins as a secondary owner. Claim,
yield, handoff, heartbeat, and completion use governed operations. Conditional
status changes distinguish success, not-found, and conflict, and emit their event
only when the state transition wins. External trackers may remain the
organizational system of record; they do not replace Decapod execution claims.

Captured task verification artifacts include `task_id` and `task_revision`.
A passing legacy record without this binding, a malformed binding, or a different
task/revision projects `validation_status: stale`; a done task then projects
`status: blocked`. Dependency admission and completion reject that evidence.
Proof references remain historical references rather than renewed permission.
`qa diagnose` recommends `decapod qa verify regen <id>` for stale binding;
`qa verify --stale` includes revision drift as well as age. Explicit recapture
runs real validation before binding new evidence. Batch recovery first leaves
its refreshed evidence `CLAIMED` and requires per-task and aggregate proof.

`STALE_TASK_VERIFICATION` reports a task or verification baseline changed while
verification was running. The stale result is discarded without replacing the
newer evidence. Inspect the current task and rerun verification or recapture.
Strict revision binding also invalidates proof after custody changes such as
release or owner edits; comments alone do not change task revision.

`workspace ensure` requires a claimed task and creates or reuses a matching
isolated workspace. It starts from committed source, preserves dirty root files
and the root index, and never imports uncommitted root changes implicitly.
Existing destinations require verified ownership, Git registration, and branch
identity. Unknown or symlinked destinations fail with preservation guidance.
`root_isolation` metadata describes this separation; it is not permission to
copy root material. `workspace status` is bounded observation and does not run
validation or refresh specifications.

A committed claim is acknowledged before optional container preparation. The
preparation runs outside broker/datastore locks; its failure is a warning on the
successful claim, not a rollback of ownership. A durable request-bound receipt
prevents automatic duplicate launches. `DECAPOD_CLAIM_AUTORUN=0` disables this
optional preparation. Interrupted preparation requires supported workspace
recovery before an explicit retry.

`TODO_PROJECTION_MISSING` means the selected control plane lacks the expected
coordination projection. It reports `kind: coordination_projection_missing` and
`execution_ready: false`; it is distinct from missing human intent and from a
storage read failure. Preserve the request, inspect the original claim and
workspace through Decapod, resolve ownership or store selection, and rerun the
execution gate. Never manufacture a duplicate task or copy a database to hide
this discrepancy.

The explicit local transfer commands preserve an original standalone task:
`todo transfer export --id <id> --to <receiver> --output <absolute-private-file>`
requires an actual yield by the source actor and seals source mutation.
`transfer recover --id <id> --transfer-id <id> --output <new-private-file>`
recovers identical bytes after interrupted publication. Recovery selects the
immutable export event by task and transfer identity instead of recapturing the
entire subject journal. The canonical event read enforces its existing 8 MiB
record bound before payload materialization, and recovery revalidates the
repository, exporter, task, transfer, envelope and artifact digest bindings.
Successful exports at the 4,095/4,096-event boundary remain recoverable after
the export/checkpoint append and after later comments. This does not relax the
4 MiB artifact or 4,096-event historical-bundle limits.
`transfer import --input <absolute-private-file> --expected-digest <sha256:digest>
--accept-source-transfer` requires a trusted source handoff, the same GitHub
origin and intended receiver, fetched source commit and absent local authority.
The `decapod.task-transfer.v1` artifact is bounded to 4 MiB and 4,096 subject
events. Sessions and unrelated tasks are excluded. Export supports only yielded
tasks without shared owners, parent/dependency edges or incoming relationships.
Import returns `execution_ready: false`; it neither claims the task nor imports
current-machine proof. `TODO_PROJECTION_MISSING` remains blocking until the real
import, claim and applicable execution gates succeed.
The source seal assumes all cooperating writers use a supporting controller.
An older executable, privileged database writer or disconnected copy can ignore
that fence; the artifact does not establish an online exclusive-ownership lease.

Cleanup requires ownership evidence. Task-like names, old timestamps, and an
empty inventory from a different container engine do not establish permission
to delete. Managed invocation receipts bind the exact path, directory generation,
lease, and creating runtime; uncertain resources are preserved for review.
Workspace/control subprocesses have a 15-second deadline and image builds a
600-second deadline, with bounded output and owned-child termination/reaping.
A timeout does not prove that a task, branch, or container is absent.

`container run` reports `workspace_cleanup` in successful summaries: `removed`,
`preserved_by_request`, `preserved_unsynced_branch`, `preserved_unexpected_head`,
`preserved_hidden_index`, `preserved_unreturned_refs`, or
`preserved_dirty_workspace`. Failed executions retain their failure status and
workspace path; runtime termination retains files after invocation-owned runtime
cleanup. `--keep-worktree` adds an unconditional retention request. Git inspection
errors report `container_workspace_status_unavailable`; finalization failures
report `container_workspace_finalization_failed` with the underlying diagnostic
and retained path. Foldback rejects divergent host history without forcing it.

`--push` publishes the returned branch only after successful execution. `--pr`
uses the common governed workspace publisher on the clone, including full
readiness and verified draft-PR creation/reuse; its result appears as
`publication` in the summary. Combining `--push --pr` does not cause a separate
unvalidated push. Publication precedes final foldback because the publisher may
commit checkpointed material. Publication failure retains the clone; repair the
reported gate, then run `decapod validate` and `decapod workspace publish` there.
A failed command never invokes either publication action.

`--local-only` cannot combine with `--push` or `--pr`; the shared boundary rejects
that conflict before starting a runtime or creating a clone. Omit the publication
flags to retain local work, or omit `--local-only` to return the actual candidate.
An explicit `--pr-base` differing from the configured governed publication target
returns `container_publication_base_mismatch`. Align the intended configuration,
review the accepted target and revalidate from the retained workspace rather
than silently publishing against another base.

## Governance and living specifications

`.decapod/governance.json` is the sole tracked governance document. Its plan,
trajectory, claims, validation, and optional decision-provider sections share
normalized intent and epoch state. Agents update them through CLI/RPC in
work-sized batches: intent and scope, claims and proof hooks, work and evidence,
then validation and checkpoints. Unchanged semantic updates are no-ops.

Fresh governed branches receive a task-bound DRAFT plan without inherited
completion claims or validation receipts. Repeated initialization and branch
renames preserve the active journal. Explicit `begin-pr` verifies acceptance of
the previous work before resetting it; unarchived work, ambiguous task/base
identity, and conflicting authorities block replacement. Unresolved obligations
survive until explicit evidence-bound resolution. Accepted history stays in Git.

A current-PR claim has a statement, falsifier, status, and proof references.
Health Engine runtime claims remain a separate datastore surface. A trajectory
records intent, boundaries, assumptions, inspected/modified files, commands,
checks, and typed loop attempts. Verification passes require non-empty proof
references; failed verification requires feedback. Attempts start at 1 and are
contiguous. Event or improvement records do not apply their mutation proposals.

`govern artifacts checkpoint` binds staged material and semantic inputs before
a commit. `verify-checkpoints` checks the immutable range, including merges and
the exact head; `--target-base` can identify the accepted target separately.
An existing checkpoint ID is immutable within the active PR. The checkpoint
mutation returns `GOVERNANCE_CHECKPOINT_ID_CONFLICT` without saving if its summary,
normalized proof references, or current material digest differ. An exact retry
returns the current document without a write, including after committing the
original checkpoint. Changed material requires a new ID; historical verification
remains an independent publication gate.
Checkpoints do not substitute for tests, human decisions, or missing earlier
checkpoints. `review-spec` binds contract review to current code and authored
INTERFACES, ARCHITECTURE, and SECURITY hashes. `requires-decision` remains open
until the explicit resolution surface records a human decision reference and
rationale; that record does not authenticate the human or prove semantic truth.

Agents directly author semantic prose in `.decapod/managed/specs/*.md` inside
their isolated workspace. Generated attestations, capability blocks, overlays,
fingerprints, and manifests remain CLI-owned, as does all other machine-owned
control-plane state. Supported refresh is `rpc --op specs.refresh` or
`validate --refresh-specs`; there is no top-level `specs.refresh` command.
Refresh preserves authored content and refuses `SPEC_REFRESH_AUTHORED_CONTENT_LOSS`
before writing any specification or manifest. A material authored change remains
required where the publication profile requires living-spec proof.

## Data Ownership

Application persistence crosses the Dactyl-backed facade. Decapod owns domain
schemas, conditional mutations, event identities, migrations, and policy;
Dactyl owns physical execution, access mode, typed results, and maintenance.
No raw SQLite connection or alternate driver is an application interface.
Read-only callers do not perform schema writes or migration repair.

The local backend uses the canonical `.decapod/data/decapod.db`. Shared workspace
lookup resolves its owning control plane rather than creating a divergent task
store. Local connection factories hold a bounded exclusive sidecar coordination
lock; contention reports `STORAGE_LOCK_TIMEOUT`. Locks are descriptor-owned and
released by the operating system, never deleted based on age. This coordinates
cooperating Decapod processes, not arbitrary external SQLite clients.

State and its canonical event commit together at the local transaction boundary.
Event replay with the same identity and content is idempotent; divergent content
fails with `EVENT_ID_CONFLICT`. Canonical streams and projections live in the
datastore. Legacy JSONL and split databases are migration inputs, not parallel
runtime authority. Migration is lossless, bounded, and idempotent; conflicting or
malformed evidence blocks rather than being silently discarded.

Runtime continuity is exposed through the existing domain commands. Task journal
commands require the local backend; cloud selection fails closed without a local
fallback. Workunit mutations require an initialized local runtime; missing or
partial policy tables fail before journal mutation. Verification returns a nonzero
exit status for projection conflict:

- `todo journal checkpoint` captures an explicit current-state baseline;
  `todo journal verify` checks the journal without restoring state;
  `todo journal restore --expected-current-digest <digest>` requires the observed
  live projection precondition and refuses divergent authority
- `govern workunit checkpoint --task-id <id> --operation-id <id>` records a
  versioned checkpoint; optional `--expected-generation` fences a known head
- `govern workunit journal-status --task-id <id>` reports integrity and projection
  consistency; `recover` restores exact journaled manifest bytes
- `govern workunit resume --task-id <id> --operation-id <id>` verifies current
  execution authority before recording continuation; it does not run work or
  reissue proof, and accepts the same generation precondition as checkpoint
- `govern trajectory checkpoint --run-id <id> --checkpoint-id <id>` captures
  current source, task custody and policy, with repeated `--approval-id` references
- `govern trajectory verify-resume --run-id <id> --checkpoint-id <id>` checks
  continuation without changing the recorded checkpoint or granting approvals

Operation identifiers bind immutable requests. Identical replay is idempotent;
a changed request must use a new identifier and a valid predecessor. Missing,
unsupported, corrupt or contradictory history is a typed blocker. A migration
baseline states that earlier history is incomplete. Neither projection recovery
nor a baseline grants a lease, approval, successful check, or current proof.

`data db verify` reports healthy, unavailable, locked, corrupt, or unsupported
integrity results without repairing them. Backup produces a verified standalone
snapshot. Recovery is an explicit operator-selected archive-and-reload action
requiring writer quiescence. `data broker verify` checks audit lifecycle, not
physical database integrity. Orphan audit repair previews the exact event,
requires a stopped writer and a reason, rejects recent/future/completed targets,
and appends an idempotent abandonment acknowledgment while retaining
`original_outcome: unknown`. It neither retries the original mutation nor proves
the database healthy.

## Validation, publication, and completion

`validate` outside CI is the complete current-workbranch readiness boundary.
It binds the exact candidate, accepted target, source and governance material,
reviewed workflow definitions, toolchain, and complete applicable check matrix.
`govern project-check` is a bounded prerequisite check and cannot establish PR
readiness. Under `GITHUB_ACTIONS=true` or `BUILDKITE=true`, public validation runs
bounded local gates, reports `scope=ci_local` and `publication_ready=false`, and
never submits or waits for its own preflight.

Readiness reports expose `agent_guidance` for the validation phase: agent
ownership, the plain validation entry command, the next action, and the
conditions that require human input. Reconciliation repair tasks expose `owner`
and `resume_command`. These fields steer the agent; they do not change the
readiness verdict, grant authorization, or replace execution evidence. Missing
build/test configuration and unsupported adapters remain explicit blockers.

For supported remote execution, validation captures an accepted executor R
independently from candidate source H. Versioned preparation and execution
custody bind R’s revision, tree and profile closure separately from H’s actual
execution revision and tree. The required graph and final verdict remain in the
installed controller. Authenticated artifact-only workers inherit the measured
R executor through their verified preparation dependency.

`CONTROLLER_CONTRACT_UNSUPPORTED` distinguishes a reviewed inventory from a
carrier, protocol or accepted executor the running controller cannot admit. Its
`next_action` is `obtain_supported_controller`, and readiness remains false.
Repeating validation with that controller, changing the declaration or building
manually cannot repair this incompatibility. The current runtime does not
bootstrap a replacement controller or accept a candidate's own readiness claim.

`proof.validation_buildkite_preflights=false` selects native execution; `true`
selects the optional Buildkite adapter for the same required profile. Unknown
profiles, unavailable adapters, and unreconciled source contracts fail explicitly.
The remote route pushes or reuses the exact existing workspace branch, executes
its project checks remotely, and verifies authenticated build, attempt, artifact,
and final source/target identities. It does not run duplicate local prerequisites,
create a temporary branch, or treat GitHub Actions results as Buildkite proof.
Configured validation does not ask the human for a separate local approval to
push that checked work branch. The bounded validation push authorization does
not grant permission for unrelated Git writes, merges, releases or deployments.
Existing authorized provider access is required; denial does not authorize
credential installation or fallback.

Snapshot preparation/resume is a private evidence handoff on that same branch.
Preparation remains non-ready. Resume reconstructs source/tree/target identity
and rechecks actual provider evidence. Adoption cannot assert a different source
commit into validity; a changed host object identity requires checkpointed source
and a fresh handoff. Publishing rejects adoption arguments. The transport does
not claim compatibility with a separate `bk preflight` command.

Branch pushes may precede readiness. PR creation, including draft creation, waits
for converged intent, mergeability, and full exact-candidate proof. Material,
history, target, policy, workflow, or toolchain changes invalidate earlier proof.
`workspace publish` independently enforces this boundary, performs a normal
non-forced push, and verifies remote head/base and the complete PR material.
It reports `remote_verified`, `pr_verified`, and warnings separately. Confirmed
push with incomplete PR verification returns `PUBLICATION_INCOMPLETE`; an
ambiguous push acknowledgment remains an unknown outcome. Reconcile and retry
idempotently without destructive rollback.

Workunit states are `DRAFT`, `EXECUTING`, `CLAIMED`, and `VERIFIED`. A `VERIFIED`
workunit requires passed proof-plan gates. Verified completion binds task, intent,
specifications, proof plan/results,
context capsule, repository identity, and validation epoch. `passed`, `failed`,
`partial`, `unavailable`, and `no_checks_run` remain distinct. A claim of completion
is not evidence, and no checks produce an unsupported verdict. Portable evidence
retains source identity and artifact digests; the receiving repository makes its
own acceptance decision.

The CI context action exposes a required boolean `no_change` disposition. It is
true only after an explicit created push with an exact zero before value names
an ordinary branch whose head equals the captured accepted target. Missing or
malformed event data, unavailable base history, dirty or hidden source, and
unrelated heads fail verification. Consumers must not infer this disposition
from a release-looking branch name or equal caller-supplied IDs. Constitution
schema validation runs for both dispositions; only real ranges use candidate
custody. Full validation and measured receipt APIs still reject equal endpoints.

## Obligation and HealthEngine verification

`obligation verify --id` prints its structured current derivation and returns a
nonzero exit when the obligation is unmet. `obligation validate-graph` prints the
whole graph result and exits nonzero for any unmet node, cycle, or missing
endpoint. `dependency_errors` exposes dangling edges; `total_edges` counts raw
stored edges. Malformed requirement arrays are errors, never empty proof lists.
`get` and `list` retain the stored status field for compatibility; that projection
cannot authorize readiness.

`obligation complete --id --commit` records a completion attempt using a nonempty
root and prints current readiness. It does not assert `Met`; missing IDs or empty
roots fail. Full validation rechecks recorded roots and legacy `met` projections,
including their transitive evidence, while unattempted open obligations remain
backlog. A public whole-graph check intentionally asks about every node.

Health claim references resolve an exact ID first, then a unique subject alias.
Duplicate subjects return `HEALTH_CLAIM_AMBIGUOUS`; a storage failure is not
reported as a missing claim. `govern health get` may refresh the advisory cache,
but obligation verification does not depend on that refresh. These behaviors are
implemented in `core/obligation.rs` and `plugins/health.rs`.

## Outbound Dependencies

Cloud storage uses an explicit versioned storage context, Git-derived repository
routing, and an authenticated opaque route. Caller-supplied repository identifiers
are routing hints; Propodus owns effective authorization. Credentials are never
serialized into that context or repository state. Unsupported versions and
missing authentication fail before I/O. Cloud `todo done --validated` is
unsupported until the remote contract supplies proof-capture data.

With the cloud backend selected, Supabase is the default runtime selector and
Neon is an explicit alternative. Supabase requires its compiled feature and an
explicit compatible authenticated Propodus endpoint; a database DSN or Supabase
REST URL is not that endpoint. Endpoint-scoped sessions cannot reuse another
endpoint's credentials, and Supabase authentication/data transports reject
redirects. Fixture tests alone do not establish hosted authorization or atomicity.

`[vega]` selects disabled or required assessment, with explicit alpha/risk keys.
After required local or Buildkite proof passes, `validation_runtime::ValidationGuidance`
distinguishes the optional post-validation assessment from validation repair.
Required Vega acceptance instructs the already-authorized agent to create the PR
on the exact validated branch, subject to freshness and existing delivery authority.
Rejection instructs the agent to inspect unmet intent and requirements, complete
work and revalidate; abstention instructs it to inspect the receipt and resolve
evidence or installation/runtime gaps. A boolean verdict supplies no invented
defect explanation. Failed or stale proof always returns validation repair guidance,
including when a model previously accepted. These instructions grant no merge or
release authority and leave disabled-mode delivery guidance unchanged. Both the
local-route top-level JSON/text renderer in `src/decapod/lib.rs` and the Buildkite
route expose the final report's guidance; local rendering must not replace a
post-validation Vega instruction with generic build/test repair advice.
`ValidationReport::finish` gives every execution error precedence over an earlier
in-memory readiness result. In particular, failure to create or synchronize the
final verdict clears `ready`, restores repair guidance and yields a failing exit
code, even after proof and Vega acceptance. Completed proof and earlier evidence
remain diagnostic records; they cannot authorize publication after that failure.
Both `init --vega-decisions[=true|false]` and `init with --vega-decisions[=true|false]`
select that policy. Interactive init offers the same choice; refresh preserves it
unless explicitly changed. Init does not install models or execute inference.
New selection contains no machine-specific installation path; previously authored
pins and unrelated settings remain intact when the mode changes. These new init
regressions are authored but require execution before a passing claim.
`[vega].model_manifest_digest` pins the shared compound model identity. Explicit
`setup vega --runtime <path>` verifies the installed worker's native file format
and byte identity, then atomically records this maintainer's local binding in
private cache metadata. It executes no worker and does not certify build source.
Legacy `[vega.installation]` still binds canonical directory, manifest digest,
native worker path and SHA-256; both model pins must agree if present. These
installation identities are frozen before proof and rechecked before acceptance.
The old provider flag and credential lookup
are removed. Historical Jev evidence remains read-only compatibility, without
current trajectory or readiness authority.

Shared `[vega]` policy resolves each maintainer's private local installation.
`backend` accepts auto (default), metal, cuda and cpu; `cpu_fallback` defaults true.
Auto prefers Metal on macOS and CUDA elsewhere; CPU-only builds on other systems
can return a typed unavailable-GPU response and then execute CPU inference.
CUDA currently targets NVIDIA via the worker's explicit `cuda` build feature;
AMD/Intel acceleration is not implemented. These defaults do not prove runtime
availability. Backend choice/fallback are part of the frozen policy and cannot
weaken an accepted required policy through candidate changes.

Init refresh removes only the obsolete provider selector when its value is
`none` or `jev`, deleting the section only when empty. Authored extension keys
remain intact. Unknown or malformed selectors require review and leave the
configuration unchanged; dry-run performs the same checks without writing it.

`decapod setup vega` supports `--cache-directory`, `--check-updates`, `--refresh`,
`--offline`, `--rollback <manifest-digest>` and explicit `--runtime <path>` local
registration. A read-only update check cannot register a runtime. Its JSON report
prints verified model pins and disabled portable TOML; `runtime_registered` is
distinct from `runtime_installed`, and it does not build or install an executable.
Report serialization failures propagate through the CLI's typed validation error path
instead of panicking or claiming a successful setup response.
The generated configuration reference includes the exact vega configuration
declaration and the complete authored readiness/setup guidance; source-derived
documentation corrections still require the CI renderer's equality check.
The worker's bounded `decapod.vega-worker.v2` request/response additionally binds
the explicitly requested physical backend. The parent validates actual backend
and precision labels before consuming an answer or considering fallback. A typed
`hardware_unavailable` error with no outcome/tokens permits one fresh CPU process
when policy allows; inference errors, invalid IPC, timeout, crash and all model
decisions terminate the attempt without retry. Pins are rechecked between attempts,
each request has its own digest and the original deadline bounds both. Receipt
`backend_attempts` preserves each requested backend, request ID and error.
This protocol needs execution tests; declarations alone are not portability proof.
Outcomes preserve [no, yes] probabilities, prediction set,
abstention, unbound/OOD, physical diagnostics and explicit devices/precision.
Only positive singleton yes accepts; negative rejects and uncertainty abstains.
The validation report separates `proof_ready`, final `ready`, and the exact-bound
`decapod.vega-readiness.v1` receipt. No result supplies independent proof authority.

Worker `--probe` performs a real finite Metal numerical check without models.
Optional `--proof-output` writes private bounded numerical diagnostics, including
tokens/spans, pooled vectors, readouts, outcomes and measured durations; parity
remains unverified until independently authenticated reference comparison.

Local handshake hashes make declarations tamper-evident. Self-declared model,
provider, and agent identity remain unverified without a configured trust root.
Policy approvals retain exact action/target scope and expiry; category approval
and risk-zone approval are independent gates. No provider or approval record
expands Decapod's control to commands the agent executes outside its interfaces.

Configured validation uses existing authority for its exact non-forced workspace
branch push, including a supported retry whose outcome can be safely rechecked.
Agent guidance explicitly avoids asking for that same permission again. It still
requires renewed authority after explicit cancellation or a material scope change,
and it preserves access and authorization denials. Source and accepted-target
checks precede reuse; guidance is not an authorization receipt. A same-head
retry can reuse the exact published branch after fresh binding checks; it does
not approve arbitrary shell commands, a different ref, or changed source under
an earlier receipt.

## Compatibility Matrix

Compatibility is established by the exposed operation and versioned data
contract, rather than by an agent's chosen provider or harness:

- CLI and process JSON RPC use the installed command schemas and the same gates
- Storage context version 1 separates local and authenticated cloud targets;
  unknown versions fail before I/O
- Native and Buildkite readiness implement the reviewed required profile;
  missing platform or provider capabilities remain blockers
- Local handshake declarations provide tamper evidence without authenticated
  provider identity; MCP and other planned adapters are not current binary APIs
- Cloud task completion and local verified completion remain distinct while the
  cloud contract lacks proof-capture support

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `2bdd681d64f0037b96df12be8e5e44d054a8e2cccfb3ebb14c8a1196fcd84fdd`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (40 files), `docs/` (1 files), `src/` (210 files), `tests/` (194 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->

## Plan contract revision boundary

`govern plan update`, `phase-add`, `phase-update`, `add-entry-gate`,
`add-exit-gate`, and `update-gate` revoke acceptance and all phase/gate progress
when their proposed contract actually differs. Repeating identical values leaves
acceptance and progress intact. Review the revised contract and explicitly
`govern plan approve`, then satisfy and replay its phases; `enter-phase` and
`exit-phase` return `NEEDS_PLAN_APPROVAL` for draft or annotating plans.

`plan_governance::patch_plan` rejects a combined contract change and accepted
state with `PLAN_CHANGED_REQUIRES_APPROVAL`, preserving the previous document.
Final proposed state, rather than the pre-patch phase list, determines whether
`DONE` is structurally valid. `validate_plan_progress` enforces incomplete-phase
and unsatisfied-completed-gate rejection for execution and publication as well.
Spec-review decisions are preserved by these editors and retain their separate
review and resolution rules. Explicit approval still records declared authority;
it does not authenticate the approver or independently judge natural language.

The public plan CLI regressions live in the existing branch-enforced
`tests/cli_contract_enforcement.rs::plan_governed_execution` module. Their
standalone predecessor was not selected by the source-PR graph; keeping all
three cases in this executed target makes CLI routing, reapproval, and phase
replay behavior part of the normal required proof without changing its adapter.

## Consumer readiness and repeatable initialization

`proof.required_profile.id = "repo.pr-readiness.v1"` is the supported local consumer
profile. `proof.repository_contract` declares a reviewed command digest,
`source_coverage` rows and per-command `executions`. Every discovered workflow has
exactly one explicit mapping to real required commands; all configured commands
execute. Absolute installed programs, declared runtime/tool files, exact environment,
working directory and timeout are configuration-bound. Unknown IDs, stale pins,
empty/optional commands and missing or duplicate coverage fail closed. Init exposes
both discovered digests for review without updating them or implying proof success.

`init` and `init with` share option-provenance-aware preservation: omitted defaults
retain authored configuration even under `--force`; explicit arguments still change
it. The generated GitHub validation workflow fetches complete history and supplies
immutable `CHANGE_EVENT`, `CHANGE_BASE` and `CHANGE_HEAD` to CI-local validation.

## Local CPU companion and model-free startup protocol

`setup vega --runtime <native-worker> --cpu-runtime <cpu-only-worker>` registers
optional separate CPU bytes privately. The CPU option requires the primary
runtime and conflicts with read-only update checks. Legacy bindings omit this
optional field without changing their serialized identity. A loader failure
cannot be retried through the same unavailable GPU executable.

`decapod.vega-backend-check.v1` binds a requested GPU backend to an availability
boolean and optional typed bounded error; it has no outcome, model path or state.
Unknown fields, contradictory status, wrong schema/backend and compute errors
are rejected. A successful capability check is not a readiness verdict.
`BackendAttempt` records capability/inference phase and executable SHA-256; the
receipt's runtime pin identifies the final selected inference executable.

## Supplemental Rust worker verification

The Vega work branch selects a reviewed `vega-native` recipe using the separate
`assets/tools/vega-native/Cargo.toml` and its lock. Tests precede
`--probe-auto --proof-output <private-controller-path>`. Automatic capability
diagnostics select Metal on Apple or CUDA elsewhere, with real CPU tensor
arithmetic only after typed hardware unavailability. The JSON names the selected
backend and fallback reason and explicitly says model inference was not executed.
The explicit `--probe` remains a Metal-only check with no CPU retry.

Optional `[proof.rust_workers]` has `reviewed_recipes_digest` and `workers`.
Each worker has a unique `name` and a repository-relative `manifest` ending in
`Cargo.toml`, explicit `timeout_seconds` (1–1800 per test/run), `features` (default
empty), `metal_toolchain` (default false) and nonempty `runs`. Each run has literal
`arguments` and optional `proof_output` (default false). The controller always runs
the worker's tests first. Run arguments follow Cargo's `--` separator. The worker
must be a separately locked Cargo workspace with explicit `package.rust-version`.
A manifest may have distinct feature-set variants; exact or reordered duplicate
variants are rejected. Each variant has independent build scratch. At most four workers, sixteen unique simple feature names and eight runs per
worker are accepted, with bounded argument counts/bytes and the original overall
validation deadline. Paths, duplicate declarations, unsupported fields and stale
review pins fail closed. An accepted recipe cannot be removed or changed by a
candidate; additional recipes only add required proof.

`rpc agent.init` reports `rust_worker_recipes_digest` as a read-only review aid.
It neither writes nor approves the recipe pin. Init refresh retains authored
recipes. A configured recipe requires the strict Decapod source profile and the
complete existing source proof before native execution. Absent host authorization
is an explicit unsupported-adapter error. This prerequisite's installed controller
continues to be v0.118.0; candidate support cannot authorize its own adoption.

`proof_output=true` appends `--proof-output <fresh-private-path>` to that run. The
controller retains a bounded private JSON object (at most 1 MiB), hashes it, and
retains command stdout/stderr, timing, termination and failures under the existing
validation evidence directory. A run cannot supply its own output destination.
This is diagnostic custody, not independent validation of model semantics. The
outer validation report is the only publication-readiness result; supplemental
command records explicitly carry `publication_ready=false`.

### Private native collection interface (#1445)

`verify_scientific_collection` always writes its bounded private final failure
report after loading a valid local collection context, including available phase
diagnostics when strict verification encounters a missing numeric export. Each
phase independently retains its context binding or an explicit read error;
binding verification does not mean its scientific contents passed. Missing,
unsafe or malformed evidence still fails the required command and cannot be
replaced by later diagnostic success. The d3ebad07 run exposed this reporting
gap after public download failures: all40 native tests passed, but only the
last CPU shard produced model outcomes, and no complete numerical result was
available. Installation errors now identify the immutable public component pin
and HTTP status where available, preserving the original I/O error kind and
access-denial behavior. These diagnostic fields add no retry or transport route.

The final reducer checks the required numerical collection disposition before
model setup. A known failed numerical collection immediately yields a non-ready
failure report with all available current-context diagnostics; it cannot cause
another download or full asset rehash. This does not skip independent work:
the numerical collection and all six evaluation shards precede the reducer.
Production failed-proof handling remains the separate earlier kernel gate,
which never invokes Vega to assess a failed required validation result.

`decapod-vega-infer --diagnose-native --proof-output <fresh-private-path>` is a diagnostic-only worker command for the reviewed native recipe. It records pinned installation identities, actual per-backend inference outputs/errors, synchronized stage timings and lifetime-process RSS high water (macOS bytes; other Unix ru_maxrss converted from KiB). It reports `publication_ready=false`, parity and readiness calibration unverified. Explicit CPU execution does not itself establish production fallback routing. No model paths, weights, credentials or private input are committed. The 1 MiB private-export cap remains unchanged. Preferred and CPU attempts share a process, so RSS includes earlier attempts and is not isolated GPU/model allocation. Production request/response, deterministic gates and abstention behavior are unchanged.

Verification-only `--prepare-evaluation --proof-output <fresh-private-path>` writes a bounded private input receipt and an 8 MiB capped sibling corpus without clobbering. The pinned dataset, grouped split, agent-authored label scope, benchmark-reported evidence and constructed conditions are explicit. `--diagnose-native` collects it independently of model setup and adds separate tokenization/engine reference comparisons with frozen tolerances; neither command claims full-backbone parity or readiness calibration. See `docs/vega-reference-contract.md` for exact hashes, provenance, commands and unsatisfied criteria.

### Native snapshot alias boundary (#1457)

Candidate76 receipts now establish26worker tests and the short Metal/CPU model/reference subset; both heldout evaluations returned nonaccepting results. reference_backbone::compute additionally receives exact pinned engine-config bytes and verifies SHA12f4f737 before independently calculating active memory chunks, with source revision/hash recorded in private reference exports. It retains its512-token pure-text bound and exact masks, existing mixed numerical tolerances and fail-closed unsupported inputs. The correction remains unexecuted until fresh validation; neither old receipts nor conditional engine agreement establish nonempty chunk or long-context parity. No production IPC authority, receipt size, timeout, calibration or model pin changes.

The existing supplemental Rust-worker recipe and private evidence contracts are unchanged. Native snapshot preparation accepts the exact already reviewed golden-vector fixture symlink in addition to the reviewed toolchain alias. There is no generic symlink permission or caller-controlled target map. `SnapshotReview.denied_paths` remains an additional deny list and takes precedence even for a recognized alias. Missing or retargeted HEAD/index/working links or nonregular targets fail closed; no out-of-repository referent is copied. Official published controller v0.119.1 contains this repair and successfully compiled/executed candidate `2f785c82` through the required recipe. Its real Metal arithmetic and limited CPU/reference receipts do not establish successful Metal model execution or the later repair. The accepted released controller retains validation authority; public provider logs are diagnostic inputs, never substitute proof or publication permission.

Native device-probe receipts now include the exact attention source revision alongside Candle provenance. Source compatibility is explicitly advanced to candle-vllm b1055eca/Candle23a6f38/attentionf22e42c; external model/tokenizer/engine/calibration identities and selected raw blocks13/19 remain fixed. Unsupported model modes and malformed/abstaining/stale proof still cannot authorize readiness.

The worker's `--collect-native-verification --proof-output <new-private-path>` runs native diagnostics and heldout automatic/explicit-CPU evaluation independently. It records bounded private child receipts with digest references, then fails if any required phase failed or a receipt is missing/malformed. Each phase retains its scientific acceptance checks; collection is not readiness. The existing controller timeout still bounds the whole command, and evaluation retains its600-second soft dispatch window. `Response::permits_cpu_retry` is a public pure classifier shared by production and native diagnostics, never proof/publication authority. Diagnostic fallback uses a real unsupported alternate GPU API; it does not claim available Metal failed or that kernel custody/loader companion routing executed. Private GPU model proofs also require consumed-model reuse rejection.
