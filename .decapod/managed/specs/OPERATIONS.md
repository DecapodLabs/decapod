# Operations

## Operations Overview

Decapod runs on demand as a versioned CLI or structured RPC process. The agent
operates it throughout a task; no daemon owns the task after an invocation exits.
The repository retains intent, custody, specifications and evidence so another
agent or reviewer can continue without the original conversation.

## Human setup and agent responsibility

The human starts in the project with:

```sh
cargo binstall decapod && decapod init
```

Then they use their coding agent normally. The agent owns orientation, task
coordination, workspace use, specification authoring, proof collection, supported
recovery and governed publication. The human supplies intent, reviews meaningful
outcomes and resolves decisions the agent cannot safely make.

The commands below describe agent operations and exceptional maintenance. They
are not a manual workflow the human must run for every task. Agents obtain current
syntax and preconditions from `decapod docs ingest`; they must not guess arguments
or invent unavailable capabilities.

## Atomic task completion and custody

Task completion in `src/decapod/core/todo.rs::update_status` checks dependency
readiness and exclusive-lease proof under the same transaction as the revision
compare-and-swap and event append. The public mutation, CLI and worker loop share
this boundary. A precheck outside the transaction is not completion authority.

Validated completion prepares real verification through
`src/decapod/plugins/verify.rs::prepare_verified_completion` before committing
`done`. The opaque prepared result binds the observed task revision, verification
epoch and captured artifacts. The final transaction rechecks that material and
commits the transition, passing verification record and events together. Failed
capture attempts remain durable history. The worker loop must keep the task
incomplete and skip automatic archiving when proof fails.

`todo release` compares the caller with the current assignee before clearing
local custody. Unclaimed retries are idempotent. Cache invalidation preserves the
stored task status rather than reporting a completed or archived task as open.
These are cooperative kernel boundaries over the existing SQLite/Dactyl path;
arbitrary database writes and actor impersonation remain outside the guarantee.

## The working lifecycle

1. Evaluate the incoming prompt with `decapod eval`. A nonzero exit or blocked
   result stops work for human review.
2. Load the agent contract and relevant constitution guidance before committing
   to a design. Read project configuration and overrides; expose unresolved
   intent, scope, boundary and proof questions.
3. Acquire the local session, initialize the agent and establish task ownership.
   Claim the task before creating or using its managed workspace. Enter that
   workspace before edits; preserve the root checkout.
4. Record the accepted interpretation, scope, assumptions, falsifiable claims
   and planned checks through Decapod. Author living-spec prose as understanding
   changes. Record inspected and changed files, actions and evidence at durable
   work boundaries.
5. Checkpoint the staged material and governance before committing. Refresh
   supported generated projections through `rpc --op specs.refresh` or
   `validate --refresh-specs`; review the result.
6. Complete validation on the exact candidate and accepted target. Repair
   recoverable failures on the same work branch and revalidate. Escalate decision
   gates, unavailable access and unsupported recovery with the exact blocker.
7. Create the PR only after full readiness succeeds. Preserve proof and complete
   the task through the governed lifecycle. A completion statement alone is
   insufficient.

A task can span many sessions and invocations. Prompt and conversation context
remain valid inside an isolated workspace. `TODO_PROJECTION_MISSING` requires
coordination recovery and a durable discrepancy record; it neither erases clear
intent nor establishes permission to proceed without ownership.

## Durable state and continuity

`.decapod/governance.json` is the tracked plan, trajectory, claims, validation and
checkpoint document. `.decapod/managed/specs/` contains the agent's reviewable
interpretation. Project configuration and overrides supply local policy.
Canonical local events and projections live in `.decapod/data/decapod.db` and are
accessed through Decapod. Generated projections do not become independent
sources of truth.

Agents edit semantic spec prose directly in the workspace. The CLI owns generated
blocks, manifests and other control-plane state. A fingerprint refresh alone
fails the material-spec requirement with `FINGERPRINT_ONLY_SPECS`. Refresh that
would discard authored content stops with `SPEC_REFRESH_AUTHORED_CONTENT_LOSS`;
preserve the document and resolve the boundary problem rather than delete its
obligations.

Fresh task workspaces seed a current governance template. Repeated calls, branch
renames and rebases preserve the active journal; accepted history remains in Git.
An explicit new-PR boundary is a deliberate reset. Handoffs retain uncertainty,
unresolved assumptions, proof references and the exact candidate under review.

## Local runtime and workspace operations

Local storage uses the published Dactyl dependency and a host SQLite shared
library. Startup honors `DACTYL_SQLITE_LIBRARY`, then machine-local runtime
configuration, then host discovery. A discovered path is saved outside the
repository. Missing support produces `LOCAL_SQLITE_RUNTIME_REQUIRED` with the
supported installation or library-path remedy. Cloud-backed startup does not
probe or require local SQLite.

## Service Level Objectives

The implemented operational limits apply to bounded invocations. This daemonless
product does not define a measured service-availability or request-latency SLO.
Hosted dependencies need their own project-specific objectives and evidence.

Workspace/control subprocesses have a 15-second deadline; image builds have a
600-second deadline. Output is drained without pipe backpressure and capped at
64 MiB per stream. Failure terminates and reaps owned children. A timeout is a
recovery error, never evidence that a branch, task or container is absent.

## Workspace preparation and cleanup

Task claim acknowledgment precedes optional container preparation, which runs on
the requesting client without a broker election or datastore lock. Preparation
failure remains a warning on the successful claim. A durable receipt prevents a
retry from repeating an already-started launch; inspect recovery before retrying.
`DECAPOD_CLAIM_AUTORUN=0` disables this optional preparation.

On macOS/Windows, an unreachable detected Podman service may use the bounded,
noninteractive `podman machine start --quiet --no-info --update-connection=false`
path, followed by another service probe. This does not launch Podman Desktop or
change the default connection.

The explicit container-workspace launch hint makes a verified online backup in a
private temporary directory, mounts it read-only and seeds container-local tmpfs.
Backup failure prevents launch. Container-copy writes are ephemeral and are not
merged into host state. Existing automatic container-run datastore sharing is a
separate supported route.

Cleanup preserves unknown or unverified resources. A force-prune request still
requires exact ownership, directory identity and lifecycle evidence. Active
claims protect local clones as well as registered worktrees. Container recovery
uses the recorded Docker/Podman backend and immutable container identities; an
unavailable runtime preserves the resource for later recovery. Process mocks do
not establish real container-interruption behavior.

## Validation and publication

When source and required governance material are ready, the agent enters the
validation phase through plain `decapod validate`. The configured flow owns build
prerequisites, verification and tests. The agent does not run a separate build
as an entry requirement. It follows reported configuration, source or evidence
repairs, checkpoints the repaired candidate when required, and re-enters
validation. Human intent, decisions and access constraints still govern the work.

`decapod validate` evaluates the exact work branch, accepted target, governance,
reviewed workflow definitions, toolchain and complete applicable check profile.
Missing, empty, stale, running, skipped-required or failed evidence returns
nonzero. Material, history, policy, workflow, toolchain or target changes invalidate
an earlier readiness result.

Branches may be pushed during iteration. PR creation, including drafts, remains
held until intent is converged, the branch is mergeable and full validation
succeeds. `govern project-check` proves bounded prerequisites only. In GitHub
Actions or Buildkite, validation runs bounded local gates with `scope=ci_local`
and `publication_ready=false`; it never submits or waits for another preflight.
A CI flag cannot authorize publication.

Native execution is the default validation route. Enabling
`proof.validation_buildkite_preflights` moves the required checks to Buildkite on
the same claimed workspace branch. Validation uses non-forced push or exact-tip
reuse, collects authenticated execution evidence and rechecks candidate, target
and attempts. It creates no temporary branch, does not run a duplicate local
prerequisite suite, and never deletes the work branch. Independent GitHub Actions
results neither satisfy nor block this Buildkite profile.

Host handoff may transport the same exact candidate to the same branch. A pending
handoff returns nonzero; adoption independently checks unchanged source, tree,
target and remote tip. Different commit identity requires checkpointing and a new
handoff. Edited success records cannot substitute for provider custody.

The source-PR profile proves actual required compile, test and build execution.
Release-only packaging is outside that profile; simulated publish/deploy steps
never prove delivery. Validation itself does not open a PR, create a release/tag,
upload packages or images, or deploy.

## Monitoring

Agents inspect supported command status, task/workspace state, trajectories,
validation reports and maintenance diagnostics at work boundaries. Retain the
exact result, candidate identity, failing gate and proof reference so a later
invocation can distinguish a current blocker from historical evidence. A missing
check, unavailable provider or timed-out operation remains visible; it is never
recorded as a passing health signal.

## Release and Migration Readiness

After an upgrade, the next normal governed command performs protected,
idempotent migration and legacy-event reconciliation before consuming runtime
evidence. Existing-project initialization uses the same path. Successful
migration receipts retire legacy JSONL inputs; they are not rescanned as a live
authority. Human-authored overrides are validated, never mechanically rewritten.
Conflicts preserve source artifacts and stop with an actionable error.

When a migration notice appears, inspect the managed ledger and catalog through
Decapod and confirm completion and verification. The notice alone is not proof.
Failed migration follows supported backup/restore diagnostics; unresolved
corruption or unsupported recovery requires human judgment.

Release artifact jobs use `assets/tools/release_cpu.rs` through the scoped
`.github/scripts/release-build.sh` wrapper. Rust estimates available parallelism
from runner affinity and accessible CPU quotas; a lower positive runner
`CARGO_BUILD_JOBS` budget is preserved and discovery failure selects one job.
Hidden resource limits and memory pressure are not inferred. The ordinary Cargo
job cap and the four declared artifact targets remain separate from this
release-job override. Only observed artifact builds can establish a throughput
improvement; source validation alone makes no four-platform packaging claim.

## Incident Response

Preserve the failing operation's diagnostics and affected state. Stop dependent
mutations when ownership, integrity or authority is unresolved. Use the supported
recovery path, then verify its result before resuming; escalate unsupported
repair or a required human decision with the exact blocker.

The maintenance interface distinguishes three operations:

- `decapod data db verify`: read-only typed integrity diagnostics
- `decapod data db backup --destination <unused-path>`: verified SQLite online
  backup, including WAL/SHM consistency, published atomically as a standalone file
- `decapod data db recover --preserve-original-at <unused-sibling-path>`: explicit
  logical rebuild with the original and sidecars preserved, followed by verified
  activation; never implicit startup repair

Recovery requires writer quiescence and no same-process Dactyl connection to the
target. Successful recovery preserves application data, schema, `user_version`
and `application_id`, and activates DELETE journal mode. Contention, unavailable
runtime/storage, malformed or corrupt databases, unsupported capabilities and
recovery/rollback failures remain distinct diagnostics with Dactyl's
`failure_code`. A blocked rollback may leave the original at its archive path.
Inspect preserved paths before resuming.

`STORAGE_UNSAFE_PERMISSIONS` stops without changing modes. Review ownership,
sharing and links rather than recursively chmodding state or deleting locks.
External SQLite writers and filesystems with unreliable locking or rename
semantics remain outside the coordination guarantee. Details are in
`docs/storage-permissions.md`.

## Optional services

Vega is disabled by default. Explicit setup caches verified external model data;
normal builds and inference never download models. Check updates using
`decapod setup vega --check-updates`, refresh an explicitly selected compatible
revision using `--refresh`, or restore a verified cached manifest with
`--rollback`. Unchanged backbone files are reused by content hash. The native
Rust worker must be built from a reviewed source release on Apple-arm64, checked
with `--probe`, and explicitly pinned before required mode can run. This is not
permission to bypass repository validation for development builds.

Required inference is bounded and one-shot, with fresh recurrent state and no
background process. Unsupported hardware, missing/changed artifacts, timeout,
overflow or ambiguous output abstains. Current calibration is general upstream
decision calibration, not an established code-readiness risk guarantee. Historical
Jev evidence is readable but never appended, reset or used for new readiness.

Cloud storage is explicitly configured and uses separate cloud authentication.
Availability of a client library or registry package does not prove a hosted
service works. External access denials require resolution within existing
authority, never automatic credential changes or permission expansion.

## Maintaining Decapod's distribution

The reviewed `.github/workflows/*.yml` files define this repository's CI and
release behavior. Buildkite can import those definitions; it does not require a
second repository pipeline. Adapter runner, matrix, artifact, secret, environment
and deployment capabilities need independent proof. A green GitHub run does not
prove Buildkite publication permissions, and a successful importer does not prove
release or Pages delivery.

Source CI requests read-only repository access. Disposable governance fixtures
use private exact-tree checkouts and process-local permission isolation. Parallel
integration runners consume their own commit's relocated test artifact; shard
identifiers select known groups. Empty or malformed inventories fail rather than
silently reduce coverage. Release/deployment retries require their own authority.

A verified version-only range may reuse source lint/test coverage from the prior
code merge. Candidate build, Decapod validation and release-packaging checks remain
active. The complete immutable range must contain only package-version, matching
root lock-version and changelog metadata; mixed source/release ranges and
unprovable history take the ordinary source-check path. Release-only material-spec
and projection handling use exact proofs, never branch names or labels.

Code PRs carry release-bound entrypoints, the managed Dockerfile pin, specifications
and governance. Release automation compiles and publishes approved artifacts;
it does not directly mutate master or open a projection-healing PR. A later agent
iteration refreshes projections for the installed release in its claimed workspace.
Documentation drift workflows report changes without committing to PR branches.

The generated universal agent contract must fit its 120-line validation budget.
When an embedded template changes, the agent can use the exact candidate binary
built during validation to regenerate projections through Decapod, checkpoint
the coherent result, and validate again. An intermediate work-branch build is
not publication readiness; the final source, projections, and proof must agree.

`Cargo.toml` owns the package version. `dist.toml` selects only the `decapod` product
binary for release archives; feature-gated contributor tools are not release
products. The locked registry dependency must support package and publish dry-run
checks. Nix uses the locked Rust toolchain and dependency source; CI does not update
`flake.lock`. Native Nix checks cover `x86_64-linux` and `aarch64-darwin`; exposed
`x86_64-darwin` and `aarch64-linux` outputs are not continuous CI proof.

<!-- decapod:capability-overlay:background-processing:start -->

Generated capability obligations: `background-processing` is declared for this project. These are decisions and proof requirements, not claims of implemented behavior. Resolve each against project source and tests; record unsupported or non-applicable mechanisms explicitly.

## Background Processing Operations Overlay

### Queue Visibility
- Queue depth, processing rate, and latency MUST be monitored
- If a dead-letter queue exists, define visibility and alerting obligations for it
- Worker health and processing rate metrics required

### Shutdown Behavior
- Graceful shutdown: stop accepting new work, finish current job
- Drain behavior and timeout MUST be selected for the deployment
- Termination and requeue behavior MUST be selected and proven for the deployment

### Worker Health
- Worker liveness and readiness probes
- Queue depth alerts for backpressure detection
- Processing latency percentiles (p50, p95, p99)
<!-- decapod:capability-overlay:background-processing:end -->

<!-- decapod:capability-overlay:persistent-state:start -->

Generated capability obligations: `persistent-state` is declared for this project. These are decisions and proof requirements, not claims of implemented behavior. Resolve each against project source and tests; record unsupported or non-applicable mechanisms explicitly.

## Persistent State Operations Overlay

### Backup & Recovery
- Backup scope, schedule, retention, and restore evidence MUST be selected for the project
- Recovery point objectives MUST be explicit project decisions, not assumed values
- Recovery time objectives MUST be explicit project decisions, not assumed values
- Restore verification cadence MUST be recorded with the operational proof plan

### Migration Operations
- All schema changes via migration files
- Migration rollback procedures documented
- Migration downtime and availability effects MUST be declared; zero downtime requires project-specific proof
- Migration health checks and rollback triggers
<!-- decapod:capability-overlay:persistent-state:end -->

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `16ba47ed87ae6c4f26efe540c20a139f9d0317cda85d454ff12be8444c71971b`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (40 files), `docs/` (1 files), `src/` (210 files), `tests/` (194 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->

## Source validation and documentation delivery

Constitution schema validation in `.github/workflows/constitution-sync.yml` runs
on ordinary branch pushes and pull requests. A release tag names an already
validated source commit; it does not provide a branch comparison range. The
workflow excludes tag pushes while retaining the immutable source-context
verifier and both ordinary and accepted-tip schema checks.

Documentation builds remain measured source checks on GitHub Actions and
Buildkite. In `.github/workflows/docs.yml`, the build job records a positive
native hosted-runner identity before uploading a Pages artifact. The deployment
job requires that output and the master branch before it can be scheduled.
GitHub-compatible environment variables or missing Buildkite markers alone do
not establish native ownership. Imported builds retain a generic site artifact
and validation custody, without starting the GitHub Pages deployment job.
