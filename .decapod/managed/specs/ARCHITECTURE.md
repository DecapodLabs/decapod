# Architecture

## Architecture Overview

Decapod is a Rust governance kernel invoked by coding agents through a CLI and
structured RPC. It connects human intent, project authority, coordinated
execution, and evidence in repository-owned state. Each invocation is finite;
the repository carries the work between invocations.

## Direction

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

Governance document writers lock the complete read-modify-write operation and
write atomically. Related sections share intent and epoch references instead
of creating competing copies. Generated views remain projections of their
inputs and never become a second authority.

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

Governed publication requires current readiness and uses a normal non-forced
push. Remote divergence requires reconciliation and fresh proof. Branch push,
pull-request creation, merge, packaging, release, and deployment remain
distinct external outcomes. The human supplies intent and reviews and merges
the prepared PR; the agent owns the engineering work and validation phase. The
checked validation push uses narrowly bound authorization rather than a separate
local operator-approval ceremony.

## Optional advisory intelligence

The assurance engine reconstructs obligations and workspace state before
calling the provider-neutral decision seam. With Jev enabled, the adapter
receives declared intent, the proposed trajectory, obligations, and labeled
snapshots of governed artifacts. The default provider is `none`.

The result is a typed observation or `no_observation`. An observation remains
advisory: it cannot resolve an interlock, waive proof, change boundaries, or
mark work complete. Provider transport, credentials, and parsing stay inside
the adapter boundary.

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

- Repository signal fingerprint: `7878f6ed8607d2a1102eb78788025fb7ecdd4c76b313a8bd4107f457e19bd161`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (21 files), `docs/` (1 files), `src/` (180 files), `tests/` (186 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->
