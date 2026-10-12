# Interfaces

## Interface Overview

Decapod's interfaces connect human intent, agent work, repository state, and
proof. The human supplies the requested outcome and resolves decisions. The
agent interprets that request, performs the work, and calls Decapod at planning,
coordination, validation, and publication boundaries. Decapod exposes the state,
constraints, evidence, and blockers that govern those transitions. Each CLI or
RPC invocation is ephemeral; repository state preserves continuity between calls.

## Human and agent entrypoints

Human setup is `cargo binstall decapod && decapod init` in the project, followed
by a task given to the person's coding agent. The README keeps this path short.
Operational commands belong in the agent contract and agent documentation.

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

Cleanup requires ownership evidence. Task-like names, old timestamps, and an
empty inventory from a different container engine do not establish permission
to delete. Managed invocation receipts bind the exact path, directory generation,
lease, and creating runtime; uncertain resources are preserved for review.
Workspace/control subprocesses have a 15-second deadline and image builds a
600-second deadline, with bounded output and owned-child termination/reaping.
A timeout does not prove that a task, branch, or container is absent.

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

`proof.validation_buildkite_preflights=false` selects native execution; `true`
selects the optional Buildkite adapter for the same required profile. Unknown
profiles, unavailable adapters, and unreconciled source contracts fail explicitly.
The remote route pushes or reuses the exact existing workspace branch, executes
its project checks remotely, and verifies authenticated build, attempt, artifact,
and final source/target identities. It does not run duplicate local prerequisites,
create a temporary branch, or treat GitHub Actions results as Buildkite proof.
Existing authorized access is required; denial does not authorize credential
installation or fallback.

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

`DecisionProvider::observe` returns a typed `trajectory_satisfies_intent`
probability or `no_observation`. The default `none` provider makes no external
call. Jev is optional; missing credentials, timeout, transport errors, and invalid
responses remain no-observation results. Observations cannot approve an action,
clear an interlock, rewrite authority, satisfy proof, or declare completion.

Local handshake hashes make declarations tamper-evident. Self-declared model,
provider, and agent identity remain unverified without a configured trust root.
Policy approvals retain exact action/target scope and expiry; category approval
and risk-zone approval are independent gates. No provider or approval record
expands Decapod's control to commands the agent executes outside its interfaces.

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

- Repository signal fingerprint: `667a48279b6f65c06fbc010723f7c053401dbeac2ba1c589e0f17865949ef801`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (21 files), `docs/` (1 files), `src/` (180 files), `tests/` (186 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->
