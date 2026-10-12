# Validation

## Validation Overview

Decapod validates whether identified work satisfies the project's stated
boundaries and required proof. Validation preserves the difference between what
the human requested, what the agent understood, what ran, and what the evidence
supports. It cannot establish correctness outside the configured requirements or
control commands an agent runs outside Decapod.

The agent owns the operational loop: establish intent and scope, work in a
claimed workspace, record falsifiable claims, gather evidence, repair supported
failures, and revalidate. Human decisions and unavailable proof remain explicit
blockers. Human onboarding requires only installation and initialization; the
agent performs validation and coordination.

## Evidence Artifacts

Evidence must identify the input state, evaluator, executed check, result, and
relevant artifact. Plans, checkpoint records, provider probabilities, wrapper
success, and an agent saying “done” cannot substitute for execution evidence.

- `passed` means the identified check has supporting successful evidence
- `failed` means a check ran and did not satisfy its condition
- `partial` means only part of the required proof is available
- `unavailable` means the required evidence could not be obtained
- `no_checks_run` is a computed absence of proof, with an unsupported completion
  verdict; it is not a passing check input

A required check that is missing, empty, duplicate, stale, running, canceled,
failed, or skipped cannot establish readiness. Non-applicability must be derived
from the current immutable source range and reviewed policy, independently of a
provider's skipped result. Mocked or simulated stages retain that disposition.
Required check invocations must actually execute. Reuse of compiled artifacts
follows the declared, source-bound proof contract.

Verification loops require non-empty proof references for `grader_result: pass`
and non-empty feedback for `grader_result: fail`. Loop result `pass`/`fail` is
separate from lifecycle status `passed`/`failed`. Retry attempts remain contiguous,
and feedback is bounded. Failed historical attempts remain visible when new
successful evidence replaces their blocking condition.

## Agent steering

Validation reports identify this phase, the agent as operator, the entry command,
and the next repair or review action. Reconciliation assigns missing build, test
and CI configuration to the agent and names validation as the resume command.
The agent follows those actions within the accepted intent and authority. A
decision gate or unavailable authorization/access remains a human-input boundary.

Configured candidate builds and downstream consumers belong inside the selected
validation route. Required consumers must be bound to the exact authenticated
producer artifact; missing, empty, duplicate or substituted inputs fail. The
command-execution fixtures prove ordering and failure propagation with controlled
tools. Only actual applicable execution and provider evidence can establish
readiness for the product candidate.

## Governance invariants

The agent contract requires prompt evaluation before repository/tool actions.
The agent must respect the result and stop for human review when blocked. The
validator cannot certify every prior action taken through an external tool.

Validation checks repository evidence for these product contracts:

1. Human intent, the agent's recorded interpretation, task binding, allowed scope,
   unresolved questions, and proof hooks remain consistent enough to execute
2. Mutations use an active session and the correct owned workspace. Root files,
   the root index, other task workspaces, and unowned resources remain untouched
3. Required approvals and decision gates remain binding. Advisory observations,
   inferred intent, and missing coordination state cannot satisfy them
4. Plans, current claims, trajectories, receipts, and unresolved obligations
   remain loadable and preserve their custody across restarts and handoffs
5. Specifications reflect the agent's current interpretation; generated metadata
   is current and never replaces substantive authored content
6. Completion and publication bind their proof to the actual candidate and active
   policy, rather than a historical receipt or a different source tree
7. Execution and recovery remain bounded, with owned subprocesses terminated and
   reaped and uncertainty preserved on timeout or interruption

`TODO_PROJECTION_MISSING` is a coordination blocker with
`execution_ready: false`, not lost request meaning. Its recovery must prove the
intended task, agent ownership, lease, and control plane before execution resumes.
A storage error must not be relabeled as an absent projection.

## Promotion Gates

Full `decapod validate` outside CI proves the exact current workbranch against
its accepted target and complete applicable profile. It binds source, governance,
reviewed CI/release definitions, toolchain, and execution identities. Changes to
those inputs or the accepted target invalidate prior readiness.

Workbranch pushes and iteration may precede validation. Creating a PR, including
a draft, requires converged intent, mergeability, and successful full validation.
A focused test, passing `govern project-check`, empty proof registry, or green
provider summary is insufficient. The publisher invokes the full boundary
independently before its combined push-and-PR operation.

The source-readiness profile is finite and reviewed. Its source inventory,
required platform/matrix instances, and selected integration shards must match
the current repository. Missing adapters, unknown repositories/profiles, altered
workflow contracts, unproved exclusions, or self-dependent validation fail
explicitly. Profile declarations are reviewed policy and cannot be silently
updated to match whatever evidence happens to exist.

The current core profile reports `source_pr_readiness`. Release-only
four-platform packaging is outside that profile. A passing source report makes
no packaging or delivery claim. Publishing and deployment simulations must be
labeled; they are not actual uploads, tags, releases, or deployments.

## Local and remote execution scopes

`govern project-check` executes bounded project/governance prerequisites. It is
useful during bootstrap and inside finite recipes, but grants no publication
readiness on its own.

Inside GitHub Actions or Buildkite, public CLI, alias, and RPC validation run
real bounded CI-local checks. Reports state `scope=ci_local` and
`publication_ready=false`. They do not write a full-readiness receipt, recursively
submit another suite, or accept snapshot transport arguments. Provider environment
flags alone cannot authorize publication.

Native execution and optional Buildkite execution enforce the same complete
required profile. The canonical `proof.validation_buildkite_preflights` setting
selects the route. The Buildkite route executes its prerequisites remotely and
reports local project checks as `not_run_provider_owned`. It pushes or reuses the
exact existing workspace-branch commit and collects authenticated execution
artifacts. GitHub Actions results neither satisfy nor block that selected profile.

Remote proof must authenticate the expected event, build, attempt, logical job,
artifact producer, raw bytes, and reviewed execution recipe. Once a PR exists,
independent PR/manual runs for the same head do not replace required push-event
execution. Duplicate matching runs and changed attempt bindings are errors.
Imported runtime identity and native provider identity remain distinct; a receipt
must not invent native fields the importer cannot expose.

Collection uses bounded authorized reads, a shared deadline, cancellation,
explicit capture limits, and final uncached identity checks. Failed or denied
transfers cannot yield partial-batch proof or authorize another route. Timing
reports are diagnostics, not evidence of correctness; overlapping request work
must not be summed as wall time. Fixture collectors do not prove live provider
acceptance.

## Authority, specifications, and proof currency

The specs manifest binds both project authority inputs, `config.toml` and
`OVERRIDE.md`, as well as supported release-bound projections. Validation checks
actual entrypoint content, declared fingerprints, compiled expectations, and the
managed Dockerfile pin. Inheriting intact current material is permitted; every
commit need not manufacture a change to every governed path.

`PUBLICATION_BUNDLE_CURRENCY` proves required HEAD participation and loadability.
Sibling gates separately prove fingerprint integrity, authored material change,
epoch currency, and receipt binding. Presence alone is insufficient.

Refresh runs only in the supported isolated workspace and preserves authored
content. Supported surfaces are `rpc --op specs.refresh`, validation refresh,
and workspace creation; fresh initialization may scaffold absent specifications.
A protected-root refresh reports `workspace_required` and leaves existing root
material untouched. `workspace status` never refreshes. Malformed generated
boundaries that risk prose loss report `SPEC_REFRESH_AUTHORED_CONTENT_LOSS` before
any spec or manifest write.

A governed source change requires a material authored living-spec update against
the PR base. Attestations, capability declarations, and capability overlays are
excluded from the material comparison. Fingerprint-only change fails with
`FINGERPRINT_ONLY_SPECS`. Not every spec must change, but affected contracts must
be accurate. Release-only treatment depends on strict immutable change
classification, never a label or branch name.

`STALE_CONFIG_INPUT_HASH`, `OUT_OF_SYNC_SPECS`, and `STALE_SPECS_FINGERPRINT`
identify projection/authority drift and require supported refresh and revalidation.
`STALE_VALIDATION_EVIDENCE` identifies a different active validation epoch. Prior
evidence is preserved; the agent initializes a new run and obtains fresh proof
rather than relabeling an old receipt. Same-epoch validation is idempotent.

Epoch bindings cover plan/review, claims, authored spec digests, entrypoints, and
managed Dockerfile while excluding the receipt itself. A proof-only receipt commit
must not invalidate its own inputs. Checkpoints bind staged material and semantic
inputs and are verified through the exact head, including merges. A later
checkpoint cannot retroactively prove an earlier unchecked commit.

## Regression Guardrails

CI source selection uses a complete immutable event/base/head context. Required
contexts reject missing, partial, malformed, unresolvable, or checkout-inconsistent
evidence, including a missing accepted-base ref. Base ancestry and synthetic
merge parent/tree identity must be verified. Provider context recognition is
scoped to the actual checkout and its owned validation clone so inherited
variables cannot classify unrelated test repositories.

Release-only classification permits an advancing root package version, its
matching root lockfile version, and optional changelog content. Source,
dependency, feature, registry, checksum, file-mode, add/delete, or mixed-range
changes retain normal checks. Version-only releases still compile the candidate
and execute actual validation. A narrow dependency-metadata projection may
normalize only the matching local root version; malformed, duplicate,
mismatched, or registry-sourced root records fail closed. Original Cargo inputs
remain authoritative for product version and release classification.

Proof execution must preserve its bound checkout, index, and non-ignored source
material. A stable HEAD is insufficient. Drift checks reject staged-only changes
and staged changes hidden by restoring the worktree. Any permitted generated
release drift must be reproduced byte-for-byte by the actual projection generator
from accepted inputs, without normalizing authored prose or unrelated surfaces.

Provider governance checks use a newly owned private clone with exact Git
identity and full history. A supported merge-checkout normalization must verify
ordered accepted-base/authored-head parents and exact head-tree parity, then run
strict checkpoint/material verification. It must not mutate the provider checkout
or ignore source/private-clone races.

## Evidence Matrix

Required test selection comes from reviewed project policy. These proof areas
must remain covered as implementations evolve:

Public-provider timeout tests exercise the production retry decision with
controlled process results and waits. They distinguish the three-attempt limit
from exhaustion of the shared deadline, while real execution retains its bounded
process capture, backoff, cancellation, and authorization checks. Scheduler load
must not decide which independent bound a unit test expects to observe.

- Interfaces: prompt evaluation, CLI/RPC envelopes, typed failures, bootstrap
  availability, schema compatibility, and accurate agent documentation
- Intent and context: plan interlocks, task/scope binding, override precedence,
  malformed authority rejection, deterministic capsules, and evidence portability
- Coordination: exclusive claims, conditional conflicts, event-atomic mutation,
  retries, competing clients, crash replay, and committed-claim/container follow-up
- Isolation: dirty-root preservation, correct canonical store resolution,
  branch-local governance, destination collisions, symlink rejection, invocation
  ownership, and safe cleanup after interruption
- Persistence: explicit IDs, transaction rollback, read-only enforcement,
  close/reopen durability, idempotent event migration, replay determinism,
  integrity diagnostics, bounded locking, backup, and operator-directed recovery
- Specifications: material-change detection, release-bound integrity, authored
  prose preservation, idempotent refresh, config drift, and epoch invalidation
- Publication and completion: full range/checkpoint verification, remote head/base
  and PR-file verification, partial/unknown outcomes, idempotent retry, current
  workunit proof, and rejection of stale or omitted living-spec references
- Proof execution: complete check inventories, real exits/logs/artifacts,
  toolchain custody, deadlines/cancellation, unchanged source, adversarial provider
  bindings, missing or duplicate evidence, and independently proved exclusions

Integration preparation builds selected executables once; workers consume those
artifacts without compiling. Shards must form the exact selected inventory with
no overlap, use executable libtest names, preserve normal ignored-test behavior,
and propagate every child failure. The repository's selective integration steps
retain their 300-second execution budget. Raising a timeout, omitting a test, or
passing one isolated case does not prove the complete suite fits that budget.

Source-owned shell, checkout, setup, release, and dependency-policy regressions
must exercise the actual authored helper or workflow body. Tool setup verifies
pinned official bytes and exact version output before adding tools to PATH.
Regular-file release staging preserves executable bytes. Tests do not publish
real packages merely to establish routing or archive policy.

## Proof Surfaces

Process tests and container mocks cannot establish behavior against a Docker
daemon. Actual interrupted-container recovery requires a provisioned runtime and
checks receipt-bound cleanup while preserving unrelated files and containers.
Absent runtime evidence remains unavailable.

SQLite, Neon, Supabase HTTP fixtures, actual PostgreSQL transactions, and hosted
Propodus authorization are separate proof scopes. Cloud fixtures cannot establish
server-side tenant isolation, deployment membership policy, or event atomicity.
Missing host SQLite runtime is an explicit availability failure; it does not
activate a bundled fallback driver.

The optional Jev adapter requires tests for disabled operation, missing or invalid
state, credentials, malformed/out-of-range observations, transport failure, and
interlock preservation. A live provider corpus is opt-in and must report its own
execution; it does not establish a production approval threshold.

Platform proof must state where it executed. Native, emulated, and cross-compiled
results are distinct. Packaging checks must build and run the promised artifact
on the applicable native or explicitly emulated target; evaluating metadata or
selecting an archive is insufficient. Unrun release targets remain unproven.

## Recovery and final acceptance

Validation failures leave the task open. Follow the report's supported repair,
preserve failed evidence, and rerun against the exact candidate. Stop for human
judgment when a decision is required, access is unavailable, authority conflicts,
or remediation is unsupported. Never weaken the profile, fabricate green checks,
edit machine-owned evidence, or use a local receipt to replace missing remote
proof.

Audit abandonment retains an unknown original mutation outcome. Integrity probes
do not authorize database repair, and failed runtime inspection does not authorize
resource deletion. Timeout diagnostics identify the active stage when available;
no universal short wall-time claim covers a full provider suite.

Final acceptance requires current complete evidence, satisfied execution and
publication gates, verified remote publication when requested, and proof-bound
completion. Report the exact scope and unresolved limitations. A successful
check of this specification, or a previous candidate, makes no claim that a later
candidate has passed.

<!-- decapod:capability-overlay:background-processing:start -->

Generated capability obligations: `background-processing` is declared for this project. These are decisions and proof requirements, not claims of implemented behavior. Resolve each against project source and tests; record unsupported or non-applicable mechanisms explicitly.

## Background Processing Validation Overlay

### Duplicate Delivery Tests
- Test repeated delivery against the declared duplicate-handling semantics, including side effects and limitations
- Idempotency key verification
- Verify the declared delivery guarantee; do not claim exactly-once behavior without proof

### Retry Tests
- Configured retry/backoff policy verified
- Configured retry bound or unbounded policy verified
- Poison-work handling verified when the project declares it

### Shutdown Tests
- Graceful drain on signal
- In-flight job completion or safe requeue
- Verify the declared durability and loss boundary on forced termination; record unsupported guarantees
<!-- decapod:capability-overlay:background-processing:end -->

<!-- decapod:capability-overlay:persistent-state:start -->

Generated capability obligations: `persistent-state` is declared for this project. These are decisions and proof requirements, not claims of implemented behavior. Resolve each against project source and tests; record unsupported or non-applicable mechanisms explicitly.

## Persistent State Validation Overlay

### Migration Proof Command
- Configure `repo.migration_validation.command` and its arguments as the executable migration proof; file presence is not proof
- The configured command MUST define its working directory, timeout, expected exit code, and evidence output

### Migration Tests
- All migrations MUST have integration tests
- Rollback procedures MUST be tested
- Data integrity checks post-migration

### Persistence Integration Tests
- Repository abstraction tested against real database
- Transaction boundary tests
- Concurrency conflict tests
- Data integrity validation after recovery
<!-- decapod:capability-overlay:persistent-state:end -->

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `0531ff30ad77e32e256de7e0db7ea313ff89b0c1981a700e06ec3107aff11076`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (21 files), `docs/` (1 files), `src/` (180 files), `tests/` (186 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->
