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

## Runtime recovery proof obligations

The durability slice requires adversarial proof at real domain boundaries:
transaction failure must roll back task state and journal together; duplicate
identical operations are idempotent; conflicting IDs, predecessor gaps, corrupt
snapshots, generation regressions and divergent live state fail closed. Modern
claim, renew, yield, handoff, release, dependency and proof state must round-trip
without changing verification bytes or their task-revision binding. A legacy
baseline must visibly preserve its incomplete-history limitation.

Workunit tests exercise concurrent read-modify-write, stale prepared generations,
interruption between durable append and JSON publication, repeated recovery, and
current-source/intent/approval drift at resume. Atomic-file tests inject directory
sync failure after rename and verify that a complete file remains visible while
the operation reports unconfirmed durability. Unix tests reject symlinked parent
sync targets. These are required checks, not claims that arbitrary hardware
power-loss behavior or every external runtime has been measured.

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

The integration inventory adapter preserves a failed supervised process status
and bounded, escaped stdout and stderr excerpts. An executable-launch failure
remains a failure; the diagnostic path neither retries it nor changes file
permissions. This keeps operating-system details available for the agent to
diagnose before proposing a repair (`assets/tools/integration_shards.rs`).

A provider-declared new ordinary branch at the accepted target has no candidate
delta. Its context verifier still checks complete history, exact commit/tree,
clean source and the captured target. The constitution workflow runs the same
Ajv schema recipe, rechecks that identity around execution, and emits no PR
readiness receipt. Fixtures cover creation metadata, SHA-1/SHA-256 identity,
changed and unrelated heads, hidden/staged source, missing bases and workflow
failure propagation. Those fixtures do not establish a live release event result.

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

`tests/unit/core/task_transfer_tests.rs` exercises two separate local stores and
real Git repositories: identity/intent/history preservation, source yield and
actor checks, source seal rejection of later custody mutations, exact private
artifact recovery, replay/conflicting-history refusal, repository/receiver/commit
binding, malformed/truncated journal rejection, explicit operator acceptance,
digest/byte bounds, and shared/connected-task rejection. Historical passed proof
must remain only in provenance, with unavailable current proof and a new revision.
These library regressions run through the existing required Supabase library-test
recipe. Their presence is not passing evidence until normal validation executes
them. Synthetic stores cannot prove source authenticity or global exclusivity;
the trust contract requires the operator's authenticated source handoff.

Transfer regressions use synthetic task identities only. Candidate transfer
commands cannot authorize custody of the real feature task before this controller
is independently accepted and released. The unchanged accepted source executor
and complete configured proof graph remain responsible for compiling and testing
the candidate; refreshing a reviewed inventory declaration supplies no proof.
Compiling the production binary alone does not execute the transfer regressions.
The required library-test build must also compile their real repository fixtures
and run them; fixture compilation failure leaves the transfer claim unsupported.

## Obligation current-evidence regression proof

`tests/plugins/obligation.rs` covers transitive and diamond completion without
status-cache writes; uncached passing proofs; poisoned or stale `VERIFIED`
cache entries; newer failure and expiry; malformed requirement arrays; dangling
and orphan edges; cycles; deep chains; atomic add failures; and public CLI JSON
plus failure exit codes. These tests run in the existing required plugins suite.
`tests/unit/core/validate_tests.rs` checks completion attempts and legacy `met`
projections while preserving unrelated open backlog.

HealthEngine integration and unit tests exercise numeric chronology, tied
outcome permutations, conservative tied expiry, invalid timestamps/results/SLAs,
missing claims/tables, alias ambiguity, exact-ID precedence, storage-error
propagation, and the cache-free connection helper. They remain in the required
plugins and library suites. Source inspection and a reproduction against a
released binary describe the defect; only execution against the current
candidate establishes that the repair passes. Plain full `decapod validate`
owns the configured build and test evidence before publication.

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

After source and required governance material are ready, the agent checkpoints
and commits the candidate, then invokes plain validation. Configured build
prerequisites and tests belong inside that route. Canonical agent templates in
`src/decapod/core/assets.rs` forbid a separate manual build or shadow preflight.
An unreviewed inventory requires reconciliation. A reviewed inventory can change candidate helper inputs while the remote
adapter uses its independently supported accepted executor. An unsupported
carrier, protocol or accepted executor instead reports
`CONTROLLER_CONTRACT_UNSUPPORTED` and requires an independently supported
controller. The current runtime does not automatically build or promote a new
controller to resolve that incompatibility. Neither outcome grants readiness.

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

The checked workspace-publication path binds its push request to the exact
repository, workspace, candidate and branch. This validation-owned request does
not require a separate human operator receipt. Generic external writes retain
their approval requirements, and validation still enforces the configured
executable allowlist and actual provider access. Altering the sealed command or
using another ref cannot acquire this validation-specific authorization.

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

Independent build envelopes and each build's envelope, steps, and jobs inventory
are fetched in ordered batches through the same sealed public reader. The
collection adapter forwards those batches instead of serializing them. Every
live metadata read remains fresh, including the final complete run-set recheck;
batching changes transport scheduling, not which checks establish readiness.
An interrupted controller has no final verdict, even when provider jobs passed.
No transport timing or process heartbeat can replace the completed report.

## Authority, specifications, and proof currency

The specs manifest binds both project authority inputs, `config.toml` and
`OVERRIDE.md`, as well as supported release-bound projections. Validation checks
actual entrypoint content, declared fingerprints, compiled expectations, and the
managed Dockerfile pin. Inheriting intact current material is permitted; every
commit need not manufacture a change to every governed path.

The full readiness controller independently checks two acceptance invariants in
`validation_runtime::load_project` before execution selection and again during
final discovery. `plan_governance::enforce_scope_paths` applies declared forbidden
path prefixes and file-touch budgets to the complete accepted-base/candidate Git
delta. Deleted paths and both sides of renames count; a clean worktree and passing
compilation do not waive scope. Optional execution/project checks additionally
include staged, unstaged, and untracked paths, preserve NUL-framed filenames, and
fail closed when scope evidence cannot be read. These are explicit path/budget
constraints, not an automated interpretation of arbitrary natural-language intent.

`entrypoint_integrity::validate_committed_history` reuses immutable Git history
and canonical template validation for every feature-branch entrypoint rewrite.
Restoring canonical final bytes cannot conceal an earlier unauthorized rewrite.
The full controller calls this check independently of CI-local Git-gate skip
flags, on native and provider routes; a provider's green health check alone is
insufficient. Existing accepted-template lineage and installed-binary authority
remain unchanged.

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
evidence is preserved; the agent commits the complete prior governance journal
before initializing a replacement run and obtaining fresh proof, rather than
relabeling an old receipt. Uncommitted replacement fails with
`TRAJECTORY_UNARCHIVED_WORK`; continuing the current run remains available. Same-epoch validation is idempotent.

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

Task-proof revision regressions exercise current binding, missing/malformed
bindings, changed completed intent, dependency claim/completion denial, aggregate
selection, observational comments, custody edits, and conservative legacy replay.
Verification fixtures prove post-transition capture, current-revision recapture,
stale pass/failure rejection, prune ABA protection, and transaction rollback
for result/recovery/prune events. Batch recovery must not partly rewrite proof
when any captured task or baseline changes. Final aggregate failure, a stale
second target, an event-insertion failure, or drift in output/material/epoch
must leave the whole recovery batch pending. Temporary admission exemptions must
restore their previous value after success, errors, and unwinding. These live in the existing selected
unit/plugin test targets; fixture validation callbacks are test-only and cannot
manufacture production completion evidence.

Integration preparation builds selected executables once; workers consume those
artifacts without compiling. Shards must form the exact selected inventory with
no overlap, use executable libtest names, preserve normal ignored-test behavior,
and propagate every child failure. The repository's selective integration steps
retain their 300-second execution budget. Raising a timeout, omitting a test, or
passing one isolated case does not prove the complete suite fits that budget.

The real-CLI workunit recovery cases live in
`tests/support/cli_workunit_recovery.rs`, included by the required `cli_contracts`
target. The selective CLI matrix still executes that target alongside
`cli_contract_enforcement` and `entrypoint_correctness`; no recovery assertion or
target is excluded. This balances the aggregate real-process suites without
extending their 300-second budgets. A successful contracts job alone does not
prove the complete selective CLI matrix passed.
The enforcement target also runs exclusively with respect to sibling Bazel test
targets: its real CLI/validation processes repeatedly passed alone but exhausted
the same 300-second budget under sibling load. Serialization preserves all cases,
commands and finite per-target limits; current execution must verify the repair.

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

Container boundary regressions in `tests/unit/plugins/container_tests.rs` exercise
real temporary Git repositories for non-fast-forward rejection, checked-out-host
fast-forward behavior, hidden index flags, stash/side-branch/tag preservation and
unchanged inherited-ref cleanup. Side-effect spies exercise the production
finalization helper's failure, publication, final-foldback and push ordering.
A real common-publisher call rejects a clone missing governance before network
publication; target mismatches also fail closed. These tests do not claim a live
Docker run or a real external PR. Existing full publisher/readiness contracts
remain independently required. Full `decapod validate` executes the prescribed
suite; authored tests and static inspection alone are not passing receipts.

SQLite, Neon, Supabase HTTP fixtures, actual PostgreSQL transactions, and hosted
Propodus authorization are separate proof scopes. Cloud fixtures cannot establish
server-side tenant isolation, deployment membership policy, or event atomicity.
Missing host SQLite runtime is an explicit availability failure; it does not
activate a bundled fallback driver.

Vega requires Rust regressions for positive-singleton-only acceptance, negative,
ambiguous, abstaining, unbound/OOD and malformed output; stale source, intent,
evidence, policy, executable and compound model identities; failed-proof refusal;
and exact retention of adverse outcomes, exclusions and unknowns during prompt
compaction. Setup/cache tests must cover pin validation, unchanged-backbone reuse,
atomic refresh/rollback and no runtime downloads. No new Python test path exists.

Cache regression coverage additionally requires one download across separately
verified single-link installations, corruption and unsafe-hardlink rejection,
cancelled-copy cleanup and retry, no-clobber publication, and failed installation
preserving the previous active identity and reusable cache. Init coverage checks
both entry points, read-only dry-run, idempotent migration of known retired
selectors, extension preservation and rejection of ambiguous provider settings.
The init policy selector additionally needs both command surfaces, explicit true
and false, preservation on omitted flags, read-only dry-run, unchanged authored
pins/extensions and proof that selection performs no inference or model download.
Portable execution needs real CPU inference and bounded accelerator-failure
recovery, platform-specific device selection, exact model/runtime binding after
fallback, and independent numerical checks for every claimed backend. Declared
support, a CPU enum variant or mocked accelerator errors do not prove that the
pinned backbone executes on CPU or CUDA.
The CPU/GPU worker source and v2 hardware-only retry protocol are now authored.
The scalar CPU recurrence fixture uses hand-calculated decayed state and query
scaling, and RoPE fixtures independently constrain zero-position/tail behavior.
These are not checkpoint parity or independently reviewed reference evidence.
The parent classification tests must reject retries for any answer (including
negative/abstaining answers), inference errors, malformed messages and CPU retries.
Init-refresh coverage also requires omitted portable defaults and an empty model
pin to remain omitted from legacy TOML; explicitly authored backend/fallback
settings must survive refresh. All current source changes need executed CI, while
native build/probe/inference remains a separate missing validation recipe.
The worker manifest scopes `parking_lot` to Metal and optional CUDA builds;
CPU-only targets do not need GPU loader synchronization. Dependency lint must
recognize this target/feature boundary without adding an unused-dependency waiver.
Local-binding regressions must exercise independent per-maintainer paths for one
shared model pin, private permissions, malformed/oversized/wrong-identity records,
failed registration preserving prior bytes, unsafe executable links/formats and
cancellation, and frozen-custody rejection before model access. Native-format
fixtures are never executed and provide no Metal/CPU inference evidence. Recheck
local custody after the independent deterministic freshness check; changed runtime
or assets must invalidate the model acceptance without relabelling passed source
proof as failed model inference.

Cross-machine custody is established separately from execution evidence. The
released v0.118.0 task-transfer importer preserves the original task identity and
source journal as provenance, but its successful import and fresh local claim
do not carry forward validation or native results. The Mac continuation must
regenerate complete candidate proof and record its actual Metal, independent
parity and readiness-calibration results before publication.

The standalone CI recipe regenerates release-bound entrypoints, the managed
Dockerfile and the spec manifest with its evaluating binary, then requires a
clean tracked projection. Commit supported regeneration after a controller
upgrade; passing project checks alongside projection drift is still failure.

Synthetic fixtures and source review are not native acceptance. Required measured
evidence remains Apple-arm64 compilation, genuine Metal numerical dispatch, real
pinned checkpoint loading, independent reference agreement through pooling,
engine and calibrated decision output, readiness-domain calibration evaluation,
latency/peak memory and repeated-request cleanup. Exercise 4,096/4,097-token
boundaries. The private numerical export records observations with parity
unverified and supplies no fabricated threshold or pass. No compatible native
executor or independent reference outputs are currently established for this work.
Ordinary governed full validation is still required; activation's one-time
bootstrap permission does not authorize feature builds/tests or publication.

### Native execution boundary

The released v0.118.0 controller's `validation_runtime::source_invocation`
supports a finite source-check catalog and the main candidate binary build;
it has no recipe for the separate `assets/tools/vega-native/Cargo.toml` worker,
its Metal probe, model installation or `--proof-output` inference. The strict
Buildkite profile does not execute arbitrary `proof.commands`. Selecting the
consumer repository-local profile would downgrade the accepted source contract
and is rejected. `proof::execute_proof_cli` resolves configuration and execution
from the canonical datastore's project root, rather than the claimed worktree;
it is not a supported workaround for this host-native task.

The smallest legitimate controller prerequisite is an independently accepted
and released supplemental host-proof recipe that retains the complete source-CI
profile. It must bind the exact candidate and accepted target, worker manifest
and lock, measured Rust/Xcode/Metal toolchain, immutable verified model inputs,
bounded child execution and private proof output. It must execute the separate
decoder tests, actual synchronized Metal probe and pinned-model requests, preserve
failed attempts and refuse completion when either source proof or native proof
is missing. An unmerged candidate must not authorize this new recipe. Hardware
availability alone does not close this execution-contract gap.

### Readiness evaluation protocol

Freeze the calibration and model/runtime pins before evaluating. The initial
acceptance criterion is zero false-ready outcomes, with a one-sided 95% exact
binomial upper bound at most 5% for false-ready among accepted decisions. With
zero errors this requires at least 59 independent accepted evaluation cases;
zero observations never establishes safety. Require at least 50% nonabstaining
coverage overall and at least 50% acceptance of independently labelled ready
cases, so rejecting or abstaining on everything cannot pass. These engineering
criteria do not reinterpret upstream conformal levels as readiness guarantees.

Evaluation must use real, independently adjudicated task/PR snapshots with
explicit requirements and validation evidence as available at the decision time.
Labels must be fixed without looking at this implementation's outputs. Include
ready cases and passing tests with unmet explicit requirements, incomplete work,
contradictions, stale/missing evidence and ambiguous cases requiring abstention.
Group related task revisions into one split; keep the evaluation holdout apart
from any fitting or threshold selection and record corpus provenance, split
digest, label adjudication and deduplication. No readiness fitting or real holdout
evaluation has yet occurred. If fitting becomes necessary, pin it separately and
evaluate on a new untouched holdout; do not silently replace upstream calibration.

Report total and per-category sample counts, false-ready counts and denominators,
coverage, abstention, malformed/error outcomes and confidence intervals. Separate
deterministic gate refusals from model assessments. Representative sampling and
independent-case assumptions must be disclosed; a convenience corpus cannot
prove population risk or out-of-distribution coverage. Synthetic fixtures only
test arithmetic and fail-closed gates. Independent parity additionally needs
authenticated pinned inputs and token/span, raw block 13/19, pooled feature,
normalization, engine and calibrated-output reference values, or a separately
reviewed Rust reference with stage-specific tolerances fixed before comparison.

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

## Release mutation ownership

Release ownership regression fixtures embed the workflow as a declared Bazel
compile input, so sandboxed test compilation and execution inspect the same
reviewed configuration. Synthetic checkout fixtures bind their own executor
root explicitly, including when a checkout-less shard inherits a different
provider root. A subprocess regression verifies this isolation without mutating
other tests' process environment. Release-policy assertions follow the actual
executor-aware entrypoint and its preparation/execution context propagation.

The release workflow (`.github/workflows/release.yml`) selects native GitHub
Actions as its sole mutation owner in a credential-free job. Ownership requires
`GITHUB_ACTIONS=true` and the protected `RUNNER_ENVIRONMENT=github-hosted` value
used by its `ubuntu-latest` owner job, with no Buildkite host markers. The
importer also exposes GitHub-compatible variables, so absent Buildkite variables
alone cannot establish ownership. Its sanitized `self-hosted` identity, missing
or unknown runner identity, and conflicting provider markers must all be rejected
by the exact-script regression. Future native self-hosted release runners require
an explicitly reviewed policy change. Every release job requires that positive
ownership result, including planning, artifact building,
container publication, hosting and announcement. Imported Buildkite execution
therefore skips those jobs before their ordinary secret retrieval. These skips
are not release or publication evidence. Provider resolver capability checks can
still run before the condition is evaluated.

Native pushes retain the release-plz action's release-PR creation followed by
release behavior. A manual dispatch may explicitly select `release_pr_only` to
create or update the release PR without invoking package publication; the default
remains the normal release sequence. This code does not repair a provider that
never receives a postmerge event. Missing external trigger delivery remains a
separate operational blocker, and validation does not authorize distribution.

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

- Repository signal fingerprint: `fcc58a4d0ae4c739b1f0f1a2682be685cdd4e673c6be911f3aae4bd05a346ec1`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (27 files), `docs/` (1 files), `src/` (210 files), `tests/` (193 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->

### Validation-owned candidate execution

For the supported remote source-CI profile, plain validation captures the
accepted executor contract before selecting the provider. Candidate helper
changes do not require a separate local compile or a shadow preflight when the
accepted R closure and unchanged carrier are supported by the controller.
Validation pushes or reuses the same governed work branch, requires the full
candidate build and applicable test graph, authenticates their artifacts, and
returns its own final report. Build failure, missing custody, altered R/H
identity, or a moved accepted target leaves readiness false.

The source executor’s dual-root preparation.v2 and execution.v3 envelopes bind
R, its tree and profile closure separately from the actual subject revision and
tree. Their schema is not authorization: the importer constructs expectations
from independent source capture and requires matching final closure. Legacy
preparation.v1/execution.v2 cannot carry an ignored dual-root field. Authenticated
integration workers retain their artifact-only mode and inherit the verified R
executor through the measured preparation dependency. No candidate receipt can
choose the expected recipe, required suite or readiness outcome.

The protocol is finite and provider-specific. Changed workflow/local-action
semantics, new providers and unsupported accepted executor contracts require a
reviewed controller adapter. Native validation retains its existing strict
contract. Initial rollout uses the strict legacy path until the accepted target
contains this protocol; automatic replacement of an already installed old
controller is not claimed.

The Linux source-policy regression also exercises a real, dependency-free Cargo
fixture through the unchanged production golden-vector recipe. Its accepted Git
executor R and edited candidate H are separate clean worktrees. R compiles H,
runs its assertions and executes the resulting ELF binary; the consumer then
applies the full preparation policy and verifies the actual artifact and logs.
Compile errors, failing assertions, changed executor material, and tampered
artifacts or receipts must be rejected. The fixture uses the installed declared
Rust toolchain and bounded child-process capture, with no successful skip path.
Fixture setup generates the dependency-free lockfile offline before the accepted
R commit, so compilation must leave the complete tracked candidate unchanged.

This regression runs in the required Supabase library-test recipe. It proves
local runner and independent custody behavior, not provider authentication or a
complete hosted dual-root rollout. Provider authentication remains covered by
its own checks. A hosted opt-in proof requires a subsequent ordinary work branch
whose accepted target already contains the protocol, followed by full plain
validation against that branch; release dispatch is unnecessary.

## Consumer local-proof regressions

The repository-local profile is tested in fresh non-Cargo Git repositories using
actual source-inspecting native commands, including success and nonzero failure.
Contract tests cover unknown IDs, stale workflow and command pins, full mapping,
requiredness, strict-profile downgrade rejection and immutable execution/target
boundaries. The native profile does not attest hosted CI or Nix daemon/deployment
behavior. Init regressions distinguish top-level and `init with`, preserve external
tracker/proof choices across forced refreshes, and verify generated workflow event
context and nonshallow history. Full Decapod validation still owns this project's
build/test/source-CI proof and publication gate.

The required `cli_contracts` target includes the consumer-readiness and both init
regression modules explicitly, with their source/compile-data inputs declared in
the Bazel target. Standalone Cargo test discovery is not evidence that the finite
source-CI graph executed a new suite. Provider logs must show those module tests
passing, alongside a separate genuine public CLI acceptance run.

Security rejection assertions inspect the complete structured validation failure
list, not the compact human summary's first issue. A placeholder-spec failure
must not hide or substitute for the specific protected-path traversal rejection.

## Reviewed importer compatibility maintenance

Importer upgrades require independent official source and executable-member
review, followed by negative tests for old, observed-but-unreviewed, unknown,
mixed and platform-swapped identities. The fixed reviewed tuple is 0.102.7.
A provider's own successful output cannot authorize its compiler bytes. When a
released controller cannot import newer provider evidence, that failed result
remains failed; a changed source pin alone does not repair the running binary.
Any explicit engineering selection of a replacement controller must independently
verify its accepted source-CI executor, exact candidate identity and downloaded
artifact digest, then rerun complete readiness. This is not automatic controller
adoption or acceptance of candidate-produced JSON as a proof verdict. No separate
local prerequisite build or reduced check graph is introduced.

## CI-attested activation regression coverage

The finite provider adapter is checked against projected public metadata from
Buildkite builds 1840 (successful commit `c444ff9b03ae569a268f348c898f0dccfa08b5e4`)
and 1870 (failed commit `bcb5842ff5d1dba6a8697b1cffe5deb1d135f701`). Fixture
provenance distinguishes retained identity/status fields from redacted command
content. Required keys originate in the reviewed workflow/catalog mapping, never
from whichever observed jobs passed. Tests must exercise the ordinary three-read
collector, branch-dependent concurrency identities, exactly one keyless bootstrap,
negative source/run/job mutations, and checked-in policy mapping completeness.
Only metadata routes are permitted by the fixture reader; no archives or logs are
used as evidence for this policy.

This activation change is checked under a specifically approved one-time bootstrap
exception: direct repository build/lint/tests and exact-commit CI, followed by a
PR that discloses those results and the absence of current-controller readiness.
Maintainer merge and a controller release establish the accepted policy for future
ordinary validation. Passing fixture tests alone does not prove native execution,
vega readiness calibration, or the absence of mutable provider configuration.

The Gatling completion regression exercises the public CLI's rejection of
`todo done --validated` when its minimal fixture lacks the mandatory full proof
profile, retaining the open task and unchanged revision. Existing completion-unit
fixtures separately cover successful prepared proof capture and atomic status
commit. The ignored positive public-CLI full-readiness fixture remains an explicit
coverage gap, not a passing acceptance test. Completion/runtime edits additionally
select Gatling so future changes cannot silently omit this regression.

## Separate CPU executable regression obligations

The CUDA-loader correction has authored, unexecuted regressions for strict
model-free capability envelopes, rejected outcome injection, contradictory
availability/error pairs, wrong backend/schema, malformed and oversized errors,
CPU-pin changes before model access, and atomic private binding retention after
failed CPU registration. Setup rejects an orphan CPU option before cache/network
access. Existing unsafe-hardlink and inference-negative/abstention retry rejection
coverage remains required. Rustfmt and whitespace checks pass; these are not
executed Rust tests. Actual loader fallback, native worker compilation, Metal
probe/model inference, independent backbone parity and readiness holdout proof
remain outstanding and must run through the accepted released controller.

## Supplemental Rust worker verification

Official v0.119.0 was published and installed from its locked crates.io package;
its VCS identity is release commit `201b504485d64f9a09a7fc0e0560fdff035e8daf`.
The separate worker recipe now requires native tests and a private automatic
capability diagnostic. Source CI, successful compilation and CPU arithmetic
remain distinct from Metal/model/parity/calibration verification.

The candidate corrects #1455's supplemental discovery deadline overrun by
passing the original absolute deadline into discovery. A regression compiles
and launches a real Rust slow probe with half a second left of an original
120-second budget; it requires dispatch, interrupted execution, and refusal to
start any probe after expiry. That test is authored and remains unexecuted until
the prescribed current-candidate validation runs. The installed v0.119.0
controller still has its released discovery implementation; candidate source
does not replace its authority.

The supplemental Rust-worker prerequisite must pass plain official-controller
`decapod validate` with its new recipe inactive. It must not use an unmerged
candidate controller to validate or activate itself. Preserve all strict source
checks, current accepted-target identity, failures and normal branch history.

New regression obligations cover malformed/oversized/duplicate recipes, stale
review pins, accepted-recipe removal/reinterpretation, relative path aliases and
escape attempts, argv separation, proof-output destination control, and finite
read-only Metal tool probes. A real dependency-free Rust fixture must compile,
test and execute through the immutable runner, retain a private JSON diagnostic,
preserve an intentional failure, stop dependent runs and leave original source
and Git state unchanged. That fixture is CPU arithmetic, not fake Metal, a model
oracle or calibration data. The real Rust fixture passed in the first candidate source CI, including preserved
intentional failure and source/ref integrity. The first candidate ran the remaining
regressions in source CI; its private-output
permission fixture failed because the general storage check permits read access.
A stricter proof-only privacy check and its new variant regression require rerun.
The same attempt also found a malformed fixture insertion and a Clippy failure.
Its full validation remained non-ready (Buildkite 2032); prior evidence is retained.
Existing native-runner regressions remain required for source/tool/target drift,
timeout, cancellation, output limits and cleanup. Actual Vega Metal/model/parity
and independent readiness holdout measurements remain separate issue #1445 gates.

### Permanent recovery provenance

The prepared source was carried from `d8cc8305d1e06c7d7f761ba8e7699a2e1b0d9d8c`
after its branch was rejected for the missing historical checkpoint in
`a31ac5d1377e5c11f268f7952753543d46261b15`. The rejected branch, full
Buildkite 2032/2042 failures and source-CI history remain preserved. The human
authorized this permanent replacement PR branch under the same prerequisite task.
No old governance identity/checkpoints were imported or retrospectively asserted.
Fresh governance and released-controller validation must bind this exact candidate;
prior passing code jobs do not establish its publication readiness.
