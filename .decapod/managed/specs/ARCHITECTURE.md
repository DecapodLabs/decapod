# Architecture

## Checkout custody and non-Git diagnostics (#1661)

`core::authoring` enforces one live checkout per task lease transactionally with
the existing task journal and workspace lifecycle events. Renew advances the
binding generation; yield, release, reclaim, handoff, terminal status and prune
retire it. `core::fleet_coord` treats `resource:<name>` as a resource reservation,
not a repository path. Release and publication consume the corresponding gate.

`workspace::get_workspace_status` remains fail closed for non-worktrees and
unavailable custody evidence. `lib::run_validate_command_local` permits non-Git
projects to execute only explicitly partial diagnostic scopes without workspace
admission. A successful project check has no publication judgment. Full validation
still requires a registered, governed candidate. Host custody remains cooperative
and is not an OS sandbox against direct filesystem or Git access.

## Composed reflection continuation

`core/mentor/reflection.rs` composes the existing mentor ADR detector, canonical
plan constraints, execution-custody capture and governance-document lock/atomic
writer. Orientation and `mentor.obligations` share this implementation. One
optional journal in `GovernedPlan` preserves at most three compact receipts and
a terminal stop; it adds no database table, index, event stream or transcript.
Current plan/accepted-base, policy, ADR and supplied source identities invalidate
dependent conclusions. The existing plan execution, trajectory checkpoint/resume
and validation boundaries reject binding unresolved or stale reflection.
The unchanged approval and exact-candidate publication gates remain necessary.
The reflection contract hash reuses the plan's existing acceptance/progress
reset on a clone. Approval and measured phase/gate progress do not consume
reflection rounds; changes to the actual phase/gate definitions still invalidate
convergence. The original execution and proof gates remain authoritative.
Mentor text matches stay advisory. `state::project_federation_decision` now
honors lifecycle before labels and preserves confidence, scope and provenance;
an inferred/imported accepted tag cannot become a human-confirmed decision.
ADR context identity and potential-conflict detection consume the same bounded
captured bytes, followed by current-source revalidation before the atomic write.
Case-folded proposed paths and challenge identities prevent spelling-only
aliases from being counted as progress on case-insensitive workspaces.

## Registry dependency closure (#1638)

`Cargo.toml` pins Dactyl 0.14.0 while retaining `sqlite` and `neon`, and the
existing default `supabase-cloud` feature still forwards `dactyl-db/supabase`.
Cargo resolves the registry checksum and patched Quinn protocol floor; pinned
Bazel regenerates its crate-universe metadata from those same committed Cargo
inputs. Nix consumes `Cargo.lock` directly and needs no Git output hash or
separate package pin. The SDK requires Rust 1.85, below the existing Decapod
1.96.1 toolchain. HTTP/3 stays disabled. No cloud/session/authorization code
is changed by this dependency alignment.

## Managed cloud connector and crash-safe custody (#1628)

The agent-facing handoff carries a validated `authorization_url` separately from
recovery text. `core::cloud_client::pending` directs the caller to poll
`decapod cloud login` at two-second intervals for at most five minutes and then
resume its original task. Individual invocations stay bounded and daemonless.
`cloud login --restart` is the explicit agent recovery entrypoint for uncertain
one-time state; it never repeats the interrupted exchange. `core::todo` retains
the original add operation ID and browser URL when connection preflight fails.

`core::cloud_client::connect` composes the public Dactyl `CloudConnector` with
the private `core::cloud_session::SessionStore`. Dactyl resolves discovery and
the physical route; the active init/session/TODO path does not use the legacy
Propodus HTTP client or ambient provider/token selectors. Decapod supplies the
Git-origin repository target and an opaque bearer to the managed bridge.

The session binding contains the validated origin, repository, and canonical
main-checkout identity. Linked worktrees share a binding; distinct clones and
origins do not. Private Unix files, no-follow opened descriptors, ownership and
link checks, bounded records, a stable exclusive lock, and synchronized atomic
replacement protect durable custody. Credentials and pending bootstrap state
are never stored in the project. Every persisted phase validates its required
session/bootstrap/code shape before orchestration uses it.

Preparing, code exchange, session exchange, and refresh persist an in-flight
marker before dispatch. Reopening an interrupted one-time phase yields
`RestartRequired`; no uncertain mutation is replayed. Status polling and
same-session account completion resume safely. Received codes and rotations
are durable before later requests. Explicit bearer rejection invalidates that
same bearer under the refresh lock without discarding a newer concurrent
rotation. Denied or unresolved entitlement retains a safely retryable account
completion. `DactylTodoStore::check_read_access` uses an existing read-only,
service-authorized task query; health checks never stand in for authorization.

Unsupported cloud repository-datastore commands fail before local migration,
presence, group-broker, or SQLite startup. Explicit file-only controls retain
their own path; this is a capability boundary, not remote kernel parity.
`tests/cloud_managed_session.rs` exercises the binary through local fixtures;
it cannot establish server RLS, PostgreSQL atomicity, or protected hosted policy.

## Agent-invoked authority across local and cloud repositories

The repository's agent remains Decapod's operational caller. Solo/local work
uses its existing cooperative session and workspace custody; no external
account is required. In cloud mode, Decapod carries a managed session bearer
to Propodus through Dactyl's opaque storage context. Propodus independently
resolves the authenticated GitHub-linked principal, live membership,
repository grant and entitlement. A client-provided agent ID or local RBAC
role label can identify the executing agent but cannot elevate cloud authority.

The coordinating backend maintains `read`, `write`, `maintainer`, and
`owner` tiers; a contractor is a user with a bounded repository-scoped
grant, rather than a special class of agent. Cloud task custody couples an
agent label with its authenticated principal, so the same label supplied
under a different principal cannot release or complete the claim. Reviewed
cross-principal repair remains a separate privileged operation. These
backend guarantees are developed and proved in the companion Propodus
migration; this repository's client fixtures prove only request routing and
denial propagation, not remote PostgreSQL authorization or Git merge rights.

## Storage-independent version dispatch

`src/decapod/lib.rs::run` dispatches `system version` alongside the bootstrap
commands before project discovery, session acquisition and migration. Binary
version metadata has no project-store dependency. Normal project commands still
initialize storage and report a detected migration once; version reporting
neither consumes that notice nor repairs unsafe datastore permissions.

## Bounded macOS epoch enumeration

`core::validation_epoch::input::mac_directory` enumerates a pinned directory
through `fdopendir`, using a new `openat` description for an independent cursor.
Entry classification uses `fstatat` with `AT_SYMLINK_NOFOLLOW` relative to that
same stream. Owned names and type flags feed the existing repository-signal
walker; file selection, hashes, entry/depth/byte/time bounds and safe reads remain
owned by their existing functions. macOS never reopens an observed pathname or
uses `/dev/fd` for enumeration. Linux retains its `/proc/self/fd` reader. This
internal iterator adjustment adds no public command, authorization or datastore
contract. Regression coverage replaces an enumerated path with a hostile symlink
and drops the original handle while verifying the stream remains pinned.

## Governed transition judgment

`src/decapod/core/governed_transition.rs` is the kernel that decides whether an
attempted change is admissible and whether a completion or publication claim is
proven. It does not choose the next unit of work. Canonical head, rules
fingerprint, and verifier come from `state::canonical_snapshot`, which is the
same revision and provenance resolver used by `state.resolve`. The judgment
does not keep a second definition of that identity.

A task verification row is an index. `judge_stored_verification` re-hashes the
bound files and checks that the proof digest matches that reconstruction under
the canonical snapshot. A pass status or a digest written only into SQLite is
not `proven`. Material decisions are appended to the existing `traces` event
stream and rendered by the flight recorder.

Canonical order is Git identity, then task revision, lease, and claim scope in
the datastore, then evidence bound to that identity. Projections such as
`project_validation_report` derive a judgment from those records. Task comments,
verification notes, `passes: true`, and an agent-written `publication_ready`
boolean are not authority.

`effective_verification_status` returns `unproven` when a passing status is
bound to a task revision but `proof_plan_results` has no output digest.
`work_claim::from_todo` then stays blocked. `state_commit::verify` rejects an
empty scope record or a missing expected root. `state_commit::freshness`
reports `stale` when the committed head, rules fingerprint, or verifier differs
from the current identity, and `unknown` when either identity is incomplete.

Validation rules no longer drop an Error or Critical rule because a forbidden
path changed. `evaluate_workunit_rule` does not pass because a manifest key
exists. Project and task overlays cannot replace a `NonOverridable` rule body.
The publication-phase rule `decapod.publication.fresh_proof` runs only when the
caller sets `include_publication`.

Verifier integrity, runtime capability attestation, and blocker classification
live in the same kernel. Each isolation axis on `VerifierEnvelope` is an
`IsolationAttestation` with its own observation, source, and evidence
reference. A reference on one axis cannot establish another. A native-trace or
external-observation label without a usable reference stays `unknown`.
Integrity also compares `canonical_input_hash` with the completion evidence
`inputs_hash`. `CapabilityAttestation` keeps observed support only when a
native or external evidence reference is usable and the `StateIdentity` is
current. It is not `capabilities::CapabilityRegistry`. `classify_blocker` uses
`AdmissionCause` and optional `ProofCause`. It ignores reason prose.
`transition_trace` adds `blocker_class` and `required_capability` only when a
class is known. `capability_gap` is a pure record: runtime for context, state,
tool semantics, coordination, policy, verification, and mechanical work, and
frontier only for inference. No second state machine, harness adapter, or
capability router was added.

## Claim scope command coverage

The `claim_boundaries` module in `tests/cli_contracts.rs` runs distinct CLI
processes from a main checkout and a separate Git worktree. A barrier starts
two disjoint explicit-file claims together, then read commands from both
worktrees verify canonical-store identity and persisted assignment. The test
also submits an overlapping claim and verifies that rejection leaves it
unassigned. No database state is injected to construct either scenario.

## Historical proof replay identity

`core::validation_history` preserves successful full-readiness evidence in the
existing local canonical verification event stream, indexed by the validated commit.
It adds no parallel datastore or background service. `lib.rs` captures the
task binding before proof starts and rechecks it at the durable append boundary.
Native, repository-local and provider execution share that boundary; partial
CI checks, snapshot preparation and failed proof cannot create successful
history. An append failure revokes the returned publication readiness.
Cloud-backed validation keeps its existing authority path; this local history
implementation does not claim cloud task completion support.

The archived record retains the complete bounded check plan, execution results,
material identity and optional decision assessment. It survives deletion of the
temporary execution directory without promising permanent raw build logs.
Full proof remains outside `.decapod/governance.json` and the frozen candidate,
so recording it does not change the commit it proves. The tracked validation
section retains its existing prerequisite-only meaning.

`plugins::verify` uses a distinct historical completion proof. It checks the
original task contract, owner and lease lineage against the canonical task
journal, then verifies integration into the accepted branch. The validated
candidate, its original accepted base and the observed integration are separate
identities. Task completion and proof repair use the existing atomic revision,
dependency, verification and journal transaction. A completed but unverified
task retains its original completion timestamp and revision when repaired.

Historical replay does not run current-workspace validation or bind unrelated
later work to the old task. It cannot satisfy the full current-candidate gate
used by `workspace publish`. Missing, corrupt, ambiguous or mismatched evidence
fails closed rather than selecting a convenient successful run.

## Candidate and accepted-target custody

Source-CI validation binds execution evidence to the immutable candidate and
accepted target revisions. When master advances, the worktree must incorporate
that accepted revision and generate fresh candidate-bound evidence; a push proof
that names an older base is a custody failure even when the Rust tests passed.
The #1524 validation failures were this target mismatch, not evidence of a test
failure. Release projection refreshes remain required when the installed
Decapod version changes.
The validation trajectory remains bound to the original claimed PR task while
supporting reconciliation work is attached to the same governed plan; auxiliary
work items must not replace the identity used to authorize publication.
The v0.126.2 master update changed only the Decapod package version in the
reviewed `Cargo.toml`/`Cargo.lock` inventory. The workflow, action and adapter
contracts were unchanged, so the required-profile source digest is updated to
the complete current inventory reported by `decapod init --proof --dry-run`.

## Consumer inventory test boundary

`tests/support/justfile_inventory.rs` copies the Rust fixture under
`tests/fixtures/justfile_consumer/` into an isolated Git repository and runs the
actual initialization CLI. It compares the inventory's `Justfile` digest with
exact index bytes. The macOS test checks APFS and inode aliasing before exercising
the same path, failing instead of skipping incompatible filesystems. Fixture
module tests are compiled into `cli_contracts`, avoiding nested builds.
`source_ci_profile::commands` runs that module in the existing required macOS
job as well as the Linux CLI target; normal custody and candidate binding apply.

## Broker caller custody

The finite group broker in `src/decapod/core/group_broker.rs` serializes local
mutations without adopting the elected leader's identity. Protocol version 3
requires an explicit caller context: canonical working directory and a closed
allowlist of agent, session, task, correlation, idempotency and claim-autorun
fields. Each child command clears those inherited fields before installing the
request values; absent caller metadata cannot fall back to the leader's values.
The ordinary session and operation-policy checks still run in that child.
Context conveys the caller's execution inputs, not a grant of trust or approval.

The active session password is transported only on the existing restricted local
socket, without a debug representation, response echo or durable request storage.
Dedupe stores only the result and a structured digest of arguments and non-secret
caller context. That digest binds identity and worktree while permitting password
rotation for an already authenticated same-caller retry. Unknown context fields
and inconsistent payload hashes fail closed; old context-free broker clients
cannot silently execute under a leader's identity. Caller isolation remains process-based. Election and storage ownership have finite
foreground lifetimes; this adds no daemon or service.

`tests/group_broker.rs` exercises two callers claiming distinct tasks through a
third caller's live broker and checks returned and persisted ownership; the
unit suite covers actor/workspace replay separation, credential rotation,
rejected context keys and removal of absent leader metadata. These are required
regressions, not a claim that unexecuted candidate tests have passed.

## Native completion custody

`validation_runtime::write_native_completion` writes and syncs native receipt
and verdict into a private temporary directory outside the frozen checkout and
Git metadata, then publishes the pair with one directory rename. No fallible
operation follows that publication point. Creation, partial-write or sync
failure leaves no published successful pair and the returned report fails closed.
`workspace::ensure_required_governance_artifacts_in_pr` checks required intent
and trajectory material independently of legacy preparation receipts; the full
readiness boundary remains responsible for all selected proof and final custody.

## Architecture Overview

Decapod is a Rust governance kernel invoked by coding agents through a CLI and
structured RPC. It connects human intent, project authority, coordinated
execution, and evidence in repository-owned state. Each invocation is finite;
the repository carries the work between invocations.

Source adoption inventory in `src/decapod/core/init_reconciliation/mod.rs` binds
actual directory-entry paths, rather than successful filesystem probe spellings.
The `source_entry_path` resolver checks source components and the existing
reviewed symlink policy first, then resolves each existing discovery root to its
exact entry spelling before `Discovery::visit` hashes it. This keeps a single
`Justfile` from producing a phantom `justfile` on case-insensitive filesystems;
distinct entries on case-sensitive filesystems remain separate even when their
contents or underlying inode are identical. Discovery still includes untracked
files and works before Git initialization, so a Git-only listing cannot hide new
build inputs. Snapshot binding remains exact and does not forgive case-only
working-tree drift from the tracked candidate.

Spelling resolution reads at most 8,192 entries per containing directory and
rejects ambiguous or unresolved aliases, while the existing source entry,
depth, per-file and total-byte bounds remain in force. Failure returns an
incomplete-discovery blocker rather than a partial accepted inventory. Tests in
`init_reconciliation/tests.rs` cover both Justfile spellings without Git,
same-content and hardlinked distinct names, nested spelling resolution and the
lookup bound; their filesystem-sensitive branches exercise the host's actual
case behavior and do not claim emulated APFS or Windows execution on Linux.

`snapshot_provider::public_buildkite::CurlPublicReader` retains its typed,
anonymous GET authorization and bounded redirect loop. Artifact redirects admit
the exact documented `https://buildkiteartifacts.com/` origin and legacy owned
S3 bucket, then apply the same four-UUID layout, producer-job and safe object-path
checks before following. Provider metadata and content hashes still bind the
download. No arbitrary host, authentication, alternate downloader or new backend
is introduced. This controller prerequisite keeps the accepted active evidence
policy; the separate stricter rollout waits for independent controller release.

Functional qualification and product permission remain distinct. Following the
maintainer's delegated decision, the native reducer allows valid conservative
abstention on a ready control while retaining the original expected-accept
mismatch. Both actual backends must demonstrate genuine ready-work approval and
every adverse/protocol control must pass. Raw response and numerical verification
remain mandatory. The shared live predicate continues denying abstaining work;
qualification cannot convert uncertainty into publication permission or claim
all oracle labels matched.

## Direction

The d3ebad07 eight-run recipe actually compiled and passed40native tests and
real Metal probe/32recurrence comparisons. Pinned download failures56/28
prevented all three numerical model comparisons and most evaluation phases.
The last CPU shard executed22cases, with4false-ready proxy outcomes,
11abstentions and1unsupported-length case; this is incomplete evaluation.
The final aggregate remains strict, but now retains independently collected
phase diagnostics even if an earlier missing numeric export stops scientific
verification. An error report is not acceptance or new model-equivalence proof.

The product is a local-first command-line runtime with a structured process RPC
surface. Agents invoke it at decision and state-transition boundaries. Durable
repository records provide continuity instead of a resident orchestration service.

## Current Facts

- Rust implements the `decapod` binary and its supporting library
- The binary embeds Constitution data and agent documentation
- Local runtime state uses Dactyl-backed SQLite; cloud routing is optional. The
  SQLite shared library is loaded dynamically from the host, and the Nix flake
  package carries it in its runtime closure (`packages.sqlite-runtime`, wired into
  `packages.default` via `LD_LIBRARY_PATH`) so `nix run` needs no host libsqlite3 (#1536)
- `packages.default` is reproducible on `x86_64-linux`: two builds from the same
  locked inputs are byte-identical (`nix build --rebuild`). On `aarch64-darwin`,
  upstream Nix rebuilds can still differ in the Mach-O `LC_UUID` and its one
  code-signature page hash; Lix rebuilds are byte-identical. The Nix flake
  workflow checks the `x86_64-linux` rebuild on each release merge. The
  constitution build script
  (`assets/build/compress_constitution.rs`) serializes only `BTreeMap`s, so the
  embedded JSON keeps one key order, and the flake maps the build directory to
  `/build`, because on Darwin it is random and rustc embeds it (#1176)
- The Nix flake workflow has credential-free release identification and build
  proof, followed by a separate native GitHub Actions-only Cachix publisher.
  The publisher executes native locked builds, smoke checks and Linux rebuild
  proof again before sending `packages.default` for `x86_64-linux` and
  `aarch64-darwin` to `decapod`. It requires a valid `CACHIX_AUTH_TOKEN` and
  fails closed if missing; Buildkite import, manual runs, ordinary merges and
  pull requests cannot publish (#1177). The reviewed workflow hash in the
  compiled readiness profile and the source-inventory digest must both match
  this reviewed source definition.
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

## Native command sequences

`core::native_proof::Invocation` retains the existing reviewed direct command
and may carry up to 63 ordered `InvocationStep` successors. This extends the
same bounded runner rather than introducing a workflow interpreter. All commands
share one owned scratch directory, the original check deadline and cumulative
output allowance. Each executable joins the measured toolchain closure; the
invocation digest binds the complete ordered argument, directory and environment
declarations. Failed or unexecuted successors cannot satisfy the parent check.
The source adapter uses this for finite Cargo recipes when their existing
platform and toolchain prerequisites are met. mdbook still needs its independent
version-resolution adapter. Shell, Docker, generated executable,
foreign-platform and derivative adapters remain explicit obligations; sequence
support alone does not establish complete native/Buildkite equivalence.

`assets/tools/build_lifecycle.rs` supervises Bazel (`--run_under=//:build_lifecycle`)
and Cargo build/test invocations inside an atomically allocated per-invocation
scratch directory. After child exit and bounded process-group teardown,
`Scratch::clean` verifies parent and scratch directory identity before removing
the owned scratch tree. On Unix `PermissionDenied` (including mode-`0o000`
rootless container overlay directories left under a disposable `HOME`),
`Scratch::recover_owned_permissions` restores owner `0o700` traversal only on
verified same-UID, same-device non-symlink directories within bounded depth and
entry limits, leaving `cleanup_diagnostics` read-only and external caches or
symlink targets untouched.

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

Local physical connections hold shared advisory lifecycle guards, preventing
file replacement while SQLite owns transaction locking. Maintenance takes an
exclusive guard on the same canonical, stable sidecar inode. Same-thread
maintenance connections can nest; unrelated threads cannot inherit exclusive
access, and shared-to-exclusive upgrades fail until handles close. No persistent
lock filename is a stale lease to delete.

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

## Optional Laya readiness assessment

`core/laya_readiness.rs` runs only after required deterministic proof succeeds.
`core/readiness_assessment.rs` owns the bounded production projection and common
binding types; `core/laya_protocol.rs` defines the external worker protocol.
Disabled mode is the default. Required mode needs the reviewed manifest and a
private, verified runtime registration. Only a current affirmative decision can
permit the already-authorized publication step; abstention and rejection block.
No replay fixture or model response establishes build, custody or merge authority.

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

## Foreground storage ownership and caller isolation

The caller-isolation requirement in issue #1476 applies to returned claim results,
persisted task assignees, and the metadata attached to each queued operation.
`group_broker::CallerContext` continues to capture the originating caller and
replace inherited identity/session fields in isolated command children. The
storage-owner transition does not dispatch CLI handlers inside the leader or
swap its process-global environment between callers.

The opt-in `DECAPOD_STORAGE_OWNER=1` path starts a supervised storage child owned by
the elected foreground broker. `storage_owner::Owner` opens the canonical
Dactyl connection once and retains it across successive proxy leases. The
connection is created, used, and dropped on the worker main thread because its
facade contains thread-local `Rc` state. A lifetime pipe ends the worker on
parent loss, even during SQL. Normal shutdown stops new leases and gives an
established lease up to five seconds to finish before killing/reaping a
nonresponsive worker.
There is no detached process or independently managed daemon.
Its lifetime is the current foreground election, not an authentication session.
The broker stops admission after 120 seconds or the configured idle period,
finishes the bounded active request, and drains storage for at most five seconds.
Callers in every worktree discover the canonical datastore's endpoint. An
initialized owner lets its isolated children omit redundant startup migrations;
the optimization is bound to the exact canonical database, and a lost required
owner is an error rather than permission to reopen locally.

All four canonical `db` factories discover the owner before acquiring a physical
sidecar lock. When the storage parent does not exist, discovery leaves secure
first-open directory creation to the normal factory rather than rejecting a new
database. Existing parents and storage files still undergo permission checks.
An isolated child retains its caller context and executes the
normal policy and audit code, while its facade forwards storage operations.
Reads of task backlog/status can use that same physical connection. One logical
stream lease owns transaction affinity until its last facade handle closes.
Same-thread, same-context nested handles reuse that lease; differing context or
access mode is rejected rather than silently inherited. Other callers queue.
Abandoned transactions are rolled back before another caller is admitted.

The shared sidecar guard protects file lifetime even while the owner is idle;
it holds no SQLite write lock. Election uses the separate stable broker.election
inode. SQLite transactions and the active stream lease provide mutation isolation.
Claim completion releases its lease before optional container preparation; a
claiming leader also releases its election before preparation. Maintenance bridges
and migration rollback hold exclusive lifecycle access across backup/restore.
Initial migrations precede owner creation. Unfinished transactions roll back and
temporary schema objects are removed before the next lease. SQL configuration,
attachment, connection-local counters in reads or writes (including stored view
definitions), and unsupported transaction controls are rejected; read-only leases
cannot disable query-only mode. Typed SQL failures survive transport unchanged.

Group requests have one 120-second queue/execution budget, a bounded kernel
admission backlog, bounded frame size, and a one-second request-read deadline.
Execution is serial in accepted order, without FIFO guarantees across concurrent
connectors or elections. A failed acknowledgement is activity, not an idle timeout.
Connection failure before any request bytes may retry admission; after send,
transport loss is UNKNOWN and does not automatically elect, fall back or replay.
Before launch, a durable UNKNOWN receipt binds request ID to arguments and caller.
A final receipt replaces it after completion. Same-ID retrieval returns a saved
result or uncertainty; a crash in the commit/receipt gap never causes replay.
This is conservative at-most-one launch per recorded request, not exactly-once
mutation. Operators must inspect uncertain persisted state before deciding on
new work. Command failure may follow partial commit and is also uncertain.

The owner remains opt-in through DECAPOD_STORAGE_OWNER=1 because its deliberately
restricted SQL facade and cooperative local scope should not silently change all
consumers. Normal CI exercises this path unconditionally in owner unit tests and
the eight-worktree, twenty-mutator and exclusive-claim broker regressions. Socket
probe failure is failure in these regressions. Physical-open instrumentation sits
at the underlying Dactyl open, not logical proxy creation. Both Linux and Darwin
normalize accepted sockets to blocking I/O before applying bounded timeouts.

core/local_socket.rs preserves short historical addresses and maps overlong
canonical paths to a SHA-256-derived directory under sticky /tmp. Private UID or
explicit shared datastore GID scopes that directory. It rejects unsafe existing
permissions, symlink/non-socket endpoints and live peers; cleanup checks endpoint
inode identity. Lock files remain beside canonical storage and are never unlinked.
This avoids both Linux and Darwin sun_path limits without shortening test roots
or masking permission/protocol failures. Empty runtime directories are reusable.

## Intent Traceability Read Model

<!-- decapod:requirement REQ-INTENT-TRACEABILITY -->
`core/intent_reconcile.rs` derives a read-only reconciliation view from
`plan_governance`, `core/project_specs`, the spec manifest, and current
repository code fingerprints. It stores no secondary project authority and
requires no persistent database migration or agent scheduler. The calling
harness remains responsible for interpreting findings and proposing repairs.

## Bounded Immutable Impact Read

<!-- decapod:requirement REQ-INTENT-CHANGE-IMPACT -->
`core/intent_reconcile.rs` optionally reads an immutable ancestor commit and
the current HEAD through bounded local Git calls. It reuses
`core/project_specs::material_specs_change_between_refs` for authored versus
generated-only living-spec deltas, with the same canonical specification list
and material-body stripping rules. NUL-delimited change paths preserve renames
and whitespace. Oversized, unsupported or unavailable observations fail closed
or explicitly mark the impact incomplete. No new persistence, scheduler, proof
record or cross-document authority is introduced.

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

- Repository signal fingerprint: `fbccc9cfb5bf2b38e6ef970f60ff59a1d618c570d9b96d317b9f5bd0c40da905`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (38 files), `docs/` (1 files), `src/` (234 files), `tests/` (232 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->

## Native inventory boundary

`core::storage_inventory` owns semantic measurement, sampling and redaction;
`DactylBridge::inspect_snapshot` delegates physical reads to released Dactyl's
bounded read-only transaction. CLI/RPC inspection dispatches before mutating
startup. Filesystem observations remain separate from SQL snapshot consistency.
Exact checkpoint/other totals and a distinct checkpoint sample avoid prefix
blindness, with shared scan/time/VM budgets. No retention or authority changes.

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

### External engine tooling

The repository-local proof host executes reviewed local commands only. Decapod
has no Dagger-specific engine adapter, authorization map, engine inspection, or
lifecycle integration. Agents may use Dagger through their own skills or project
CI outside this kernel. Such execution does not itself satisfy a Decapod proof
gate or replace independently required Buildkite execution evidence. Existing
native receipt custody and receipt-independent prerequisites remain unchanged.

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

## CPU startup across separate executable pins

Laya installation metadata may include a separately hashed CPU companion.
`laya_runtime` checks canonical native executable identity and private bindings;
backend fallback follows the configured policy without granting readiness on an
unusable response. Local runtime-binding schema version 2 requires explicit
registration. Older registrations fail with re-registration instructions; model
manifests and historical proof bodies are unchanged.

## Supplemental Rust worker verification

`validation_runtime/rust_worker.rs` provides generic, explicitly declared Rust
proof prerequisites. It retains locked dependency execution, bounded processes,
source custody and recorded outcomes. These are deterministic project checks,
not an alternate post-validation model evaluator. This repository declares no
mandatory supplemental worker. Laya remains separately configured and optional.

## Immutable target configuration in source CI

The accepted CI context action continues to bind workbranch pushes to the
regular committed configuration blob, exact checkout and accepted ancestry.
Its carrier bytes remain unchanged because the installed controller independently
pins them. A prepared Rust-only bootstrap migration is retained privately and is
not an active feature or proof of this PR. No Python implementation or runtime is
added. Actual candidate-command regressions remain Rust and invoke the existing
validator/projection interfaces. Neither context diagnostics nor CI-local checks
claim provider freshness or publication readiness.

The managed Dockerfile release refresh in `plugins/container.rs` changes only
the two release-bound ARG values. Its preservation of line endings, trailing
newline state, authored commands and blank lines keeps the projection verifier's
expected bytes consistent with actual command-level release validation. Existing
historical generator verification continues to bind earlier entrypoints to their
own immutable generator source rather than retroactively applying this template.

## Precise claim paths and unchanged trust

The bounded file-scope change addresses the disjoint-work portion of issue 1480.
Trust-grant administration remains unresolved and is not added by this change.
There is no new config trust mapping, elevation API, trust-row mutation or human
CLI requirement. Existing caller names and local approvals remain cooperative
assertions, not authenticated principals or independent proof of human consent.

`fleet_coord::declared_paths` recognizes bounded ASCII literal JSON file-set scopes.
Explicit footprint matching conservatively folds ASCII case on every platform;
Unicode scopes are rejected rather than assuming filesystem alias semantics.
`todo::add_task` rejects traversal/symlink scopes and avoids fuzzy consolidation
of explicitly scoped tasks. Claim/handoff overlap checks, fleet diagnostics and
`work_claim::from_todo_with_lease` use the same footprints. Legacy/invalid scopes
remain conservative. File sets do not bypass independent category ownership,
dependency, lease, shared-claim trust or approval gates and do not establish
verified isolated-worktree custody. Tests cover disjoint/intersecting/parent paths,
malformed declarations, unchanged shared-claim trust and journaled scopes;
source declarations are not a claim that tests have run.

## Task custody and observed checkout admission

The local claim/journal transaction in `core/todo.rs` retains exclusive task
ownership, dependencies, revision and lease generation while treating source
path/category overlap as metadata. Workspace creation selects the destination
branch's task before examining another actor's source branch. The version-one
`authoring_bindings` table lives in the existing canonical datastore, accessed
through `DbBroker` and Dactyl. `core/authoring.rs` observes registered Git and
filesystem identity, checks the live actor/lease, and commits binding plus audit
atomically. The same physical checkout cannot acquire incompatible active task
bindings, including under the same actor. Admission is rechecked at governed
CLI/RPC authoring entrypoints; it is not an OS write sandbox.

Repository-scoped cloud task state stays behind Dactyl and Propodus's trusted
identity/authorization boundary. Host-local checkout identities are never sent
as cloud authorization. The released compatibility task/event profile remains;
resource reservation, filesystem bindings, lease handoff and validated remote
completion are unsupported there. Local labels cannot grant cloud roles or
entitlements. Native RPC and hosted integration need their separate proofs.

## Immutable history recovery assessment

`core/governance_document/checkpoint_history.rs` contains the per-commit checker
shared by strict checkpoint admission and read-only JSON assessment. It binds
resolved base/head identities, classifies affected obligations and retains the
strict material, identity, parent-record and imported-lineage requirements.
`governance_document::record_history_recovery` records an observation in the
fresh workstream's carried obligations, under the existing document lock.
The source commit, tree and governance blob remain immutable; no database,
scheduler or second history authority is introduced. Source reauthoring and
current independent proof are distinct from reconstruction of past evidence.

## Datastore physical revision 2

Decapod retains one local SQLite authority and routes physical execution through
Dactyl. `core/schemas.rs::EVENTS_TABLE_SCHEMA_V2` retains event columns and proof
encodings while avoiding the duplicate nonunique `(stream,seq)` tree. The forward
`events.compact_sequence_index.v002` migration verifies both index definitions
under `BEGIN IMMEDIATE` and removes only the redundant tree. Missing uniqueness
is established additively; duplicate historical sequences reject the upgrade
without normalization. No historical payload, identity or hash is rewritten.
Freed pages are reusable; file shrink requires separately controlled maintenance.
The measured catalog and unresolved cross-backend contracts live in
`docs/datastore-v2/`; native Propodus RPC supports only a partial task model.

## Current evaluator boundary

Laya is the sole optional post-validation evaluator. Shared projection and asset
helpers contain no evaluator compatibility layer. `ValidationReport` exposes one
assessment field, `laya_readiness`; generic historical replay normalization is
unchanged. Retired model code and research corpora are absent from the current
source tree; their immutable history remains in Git.

## Guarded relational index compaction

`core::migration::migrate_compact_redundant_relational_indexes` runs one
`BEGIN IMMEDIATE` transaction over local `decapod.db`. For each of five
nonunique indexes it verifies exact index shape and the full ascending/BINARY
UNIQUE replacement before DROP. Category/risk-zone names already have UNIQUE
constraints; task-owner/dependency keys have matching composite UNIQUE
indexes; preferences use `UNIQUE(category,key)`. Runtime initializers stop
recreating redundant trees. Reverse-key indexes remain. No data or proof
encodings change; cloud PostgreSQL migration ownership stays with Propodus.

## Terminal setup over the managed cloud boundary

`src/decapod/onboarding.rs` owns terminal sizing, presentation, keyboard input and
terminal restoration. `lib.rs::{enrich_repo_context_interactive,customize_setup,
review_setup}` collects a configuration draft; `prepare_init_directory_and_git`
and `run_init_apply` execute only after interactive approval. Changed configuration
is backed up privately before replacement. Preview and cancellation do not create
project state. Unattended initialization retains its direct execution path.

Cloud initialization then calls the accepted `bootstrap_cloud_session` and
`core::cloud_client::connect` path. Session custody and Dactyl transport retain
their existing owners. The terminal does not implement billing, duplicate account
policy, or grant authority from local settings. Errors leave setup non-successful;
the ready receipt follows successful service-confirmed access only. Real PTY
coverage verifies that reviewing/canceling cloud setup does not contact even the
loopback fixture service before consent. The six Unix PTY cases live in the
private `terminal_onboarding` module of `tests/cli_contracts.rs`, which the
finite source-CI CLI selector schedules. Declaring a standalone integration
target alone does not establish required execution.

## Validation proof lifecycle

Full validation has separate admission and terminal evidence boundaries. `core/validation_rules.rs` checks source/governance inputs before execution; rules requiring proof outputs run in `validation_runtime::run` only after the complete required suite has passed. The runtime passes its owned `CompletedReadinessEvidence`, bound to exact candidate, accepted target, source, configuration, plan, governance and trajectory. No prior prerequisite receipt or string marker can manufacture that completion. Terminal failures precede external readiness receipts and canonical completion history.

Both pre-execution (`core/validate.rs::get_current_task_type`) and terminal publication (`core/validation_rules.rs::evaluate_terminal_rules`) normalize task categories and task ID prefixes through `validation_rules::normalize_rule_task_type` (`feature`, `bugfix`, `refactor`, `docs`, `test`) and populate canonical Git references through `validation_rules::canonical_git_ref` (`refs/heads/<branch>`). Store lookups inspect only an existing `decapod.db` without creating sidecar databases or tripping unmigrated `todo.db` stubs, falling back to branch task-ID prefixes when local store rows are absent. Administrative categories (`code`, `cicd`, `secu`, `task`) map to `None` so built-in `task_types` rules remain applicable rather than exempted, while task-scoped and ref-scoped `EvaluationPhase::Publication` overrides filter against actual branch and task identity.

The terminal boundary shares the original invocation deadline. It checks the remaining budget around evidence reads and before issuing proof; native success files remain private until an atomic bundle publication that rechecks that deadline. Remote ready verdicts likewise stage privately before publication. A deadline expiry cannot turn completed checks into late publication authority.

A fresh worktree may lack the ignored context-capsule policy cache. The policy loader materializes and validates that cache when needed; its absence is not a missing source scaffold and must not trigger a rewrite of the committed specs manifest before proof. Missing tracked scaffold surfaces retain their repair path.

## Explicit managed project routing

`core/cloud_project.rs` owns bounded project-directory selection and the pending creation receipt. `SessionGuard` stores this auxiliary record under the same stable lock and exact checkout/service custody binding, without changing the credential record format or refresh state machine. `cloud_client::acquire` reuses accepted authentication; `connect` resolves the selected project after refresh. The todo factory uses that resolved data reference when opening the consumer connection. No data or authentication transport is implemented outside Dactyl.

The resolved native project also supplies the repository field of newly created tasks. Before the first possible write dispatch, the existing machine-private todo operation journal atomically binds the operation to a digest of its resolved service/project route. Preflight login and initial project selection retain the same operation ID; subsequent selection changes cannot replay an uncertain or successful write into another project. Older unknown records remain bound to their original canonical repository semantics. The real PostgreSQL consumer exposed the former Git-label write mismatch; that failed candidate supplies no successful native proof.

## Versioned cloud mutation receipts

The machine-private cloud todo receipt file uses a version-two envelope around its bounded records. Legacy flat arrays migrate under the existing stable file lock; array-only older clients cannot parse and rewrite the new envelope, so they cannot silently remove resolved-project bindings. This changes neither credential custody nor the server data protocol.

## PR 1666: accepted v0.143.2 source integration

`snapshot_provider::github_quota::GithubQuota` is shared by the existing Budget. Validated successful HTTP response hints pace cold GitHub locator discovery; known Buildkite envelopes retain their normal polling interval. The final binding and fresh-registry recheck remain authoritative after removal of a redundant earlier binding read.

## PR1666 current-master integration

The shared GithubQuota/Budget mechanism continues to pace only anonymous discovery, preserve final verification capacity, and honor bounded deadlines. Accepted custody and workunit runtime ownership changes remain in place; this integration adds no competing verifier or provider policy.

## PR1663 integrated storage behavior

The integrated `todo` module exports both accepted authoring/resource custody functions and component-aware journal verification. Keep initialized runtime ownership and task binding retirement alongside storage format guards, compact event reconstruction, content-addressed components and snapshot-consistent verification. The coordination kernel is the published0.144.1 source with only the previously scoped inert-placeholder repair; it does not contain or execute this proposed storage migration.

## PR 1662: accepted v0.143.2 source integration

`core::db_maintenance` centralizes capability admission against canonical accepted project configuration. `storage_lock` retains OS-backed descriptor custody, and Dactyl remains the physical backup/verification boundary. CLI and RPC dispatch this narrow surface before startup migration without adding a second datastore authority.

## PR1662 current-master preparation

The db_maintenance module resolves accepted canonical-repository policy and uses existing Dactyl and StorageLock primitives. It does not replace the datastore or migration authority. Retain current-master storage format guards, task custody, provider verification and runtime ownership while integrating this operation boundary.
