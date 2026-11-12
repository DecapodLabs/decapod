# Validation

## Recovery and accepted-target reconciliation (#1628)

The client continues from both original checkpoints, including
`d85650bafa610dc952fbc91acd91ba5e28b329b7`, with the accepted billing/release
history merged into the same work branch. The newer mandatory project roster,
embedded price and client-built billing URL conflict with the backend-only
contract and are replaced by the managed Dactyl sign-in/account flow. The fresh
headless initialization fixture also asserts that these project fields are absent.

Dactyl's unchanged `86ed55dbf4ff78bb3c6c6aa52832949101d9ea91` candidate has
verified native evidence from run `43131221323`: both mandatory proof groups,
all fourteen checks, a ready verdict and its exact-candidate readiness receipt.
Its wrapper failed afterward during PR publication/readback. This upstream
offline proof does not establish client compilation or protected hosted access.
At that observation the registry exposed only `dactyl-db 0.11.1`. During
continuation Dactyl PR #102 and release #103 merged, publishing `0.12.0` from
`598415953b5ee568f6d42652982897c58b4433cc`. The crate SHA256 was verified as
`b9d8eda3a7bad7490d23dbca5c468bf89b28de4c8b4fd566034b342334ba481e`; its
public API and packaged source match that release. The existing client now
uses its supported Serde/status/code/completion contract. The original stronger
repository-response API is not assumed to exist in the registry release.
On registry candidate `0bff2528d498c7423783656b45a47fde8cd92dbc`, the
independent GitHub diagnostics passed 1,674 library tests (eight ignored),
including all eighteen private-custody unit tests. The consumer integration
suite failed because successful RPC fixtures omitted the released protocol's
required `generated_keys` field, and three human-diagnostic assertions assumed
lowercase status text. Complete successful fixtures now supply an empty key
list; the diagnostic assertions match the released transport classification
and actual status spelling. Existing negative, no-fallback, bounded timeout
and redaction assertions remain required. This diagnostic run was not Buildkite
readiness and had not executed the twelve managed sign-in scenarios. Their
later successful diagnostic execution is recorded below; the current exact
candidate still requires complete native proof.
The released connector classifies a dropped non-timeout authentication response
as `Unavailable`. Completion recovery must retain the saved session and return
`onboarding_pending` for this kind as well as transport/timeout failures; the
existing lost-completion fixture verifies the repeated idempotent confirmation
and the absence of repeated one-time exchanges.
The pending-account scenario exercises the published `entitlement_required`,
`seat_unassigned` and `quote_confirmation_required` reasons individually. Each
must retain the issued session, return action-required, avoid the repository
read and avoid replaying either exchange until the service confirms completion.
The 0.12.0 lockfile also downgrades inactive HTTP/3 dependencies under Dactyl
MSRV bounds; the required dependency audit must pass without exemptions.

The first registry candidate reached actual Buildkite execution. Independent
GitHub diagnostics on that same candidate passed all-feature clippy and
dependency lint, but Bazel setup rejected the stale `MODULE.bazel.lock` before
the Bazel tests could run. The reported recovery, repository-pinned Bazel 9.3.0
`mod deps --lockfile_mode=update`, regenerates the extension's Cargo inputs.
The committed build policy remains `lockfile_mode=error`; no test/build gate is
replaced by this metadata preparation. Fresh Buildkite proof on the current
exact candidate remains required.

The generated lock was 1,053,030 bytes, exceeding bounded discovery by 4,454
bytes. Reformatting its complete JSON with one-space indentation yields
1,029,040 bytes; parsed JSON equality verifies that no value was removed or
changed. Pinned Bazel accepts the result with `lockfile_mode=error`. Source and
runtime discovery limits remain unchanged. This is metadata consistency proof,
not an executed client test or full-readiness result.

The current registry version and checksum are established by Cargo and
`tests/release_dependency_policy.rs`. Existing versioned SQLite recovery
documentation describes the unchanged recovery primitive; those historical
capability references do not establish the managed connector's release or proof.

The first current-target validation stopped before Buildkite because the
immutable candidate still carried v0.138.2 entrypoint headers while the installed
released controller was v0.138.3. Supported initialization regenerates the four
entrypoints and managed image pin; spec refresh updates their projections.
The original journal, reviewed project configuration and required proof coverage
are preserved. This projection repair supplies no compilation or test evidence.

## Managed connector execution prerequisites (#1628)

The managed connector requires a published Dactyl dependency containing its
public API and successful execution of the complete required source checks.
No governed source-overlay recipe is configured for integration diagnostics
before those checks pass. Source review, formatting, native checkpoints and
workbranch pushes do not establish compilation, test execution or readiness.

The registry manifest and lockfile, `tests/release_dependency_policy.rs` and
complete required source profile remain authoritative. Dependency alignment
and package publication remain the separate
[#1380](https://github.com/DecapodLabs/decapod/issues/1380) handoff; no path/Git
fallback, dependency override or relaxed proof gate supplies that evidence.

## Managed cloud client proof and package prerequisite (#1628)

The configured complete source-CI profile remains required. New
`tests/cloud_managed_session.rs` fixtures exercise the actual binary through
headless sign-in, durable continuation, credential refresh, interrupted mutation,
explicit restart and authenticated read readiness. The cloud backend, CLI,
Supabase boundary, TODO resume and init regression suites preserve local
independence and fail-closed cloud behavior. Unit coverage in
`tests/unit/core/cloud_session_tests.rs` exercises private record permissions,
path rejection, exact binding, phase invariants and serialized custody.

These tests must run on the exact checkpointed candidate. Static review and
source formatting do not establish compilation or execution. The managed API
is consumed from the published Dactyl 0.12.0 registry dependency.
The accepted source-CI executor retains its own required recipe. Its diagnostic
record on `ee288c86` showed that a candidate helper edit did not add a new target
to that recipe. The existing required `cloud_supabase_boundary` target now
includes the shared `common/cloud_managed_scenarios.rs` test module, so all
twelve scenarios execute under the accepted recipe without changing executor
authority. The standalone `cloud_managed_session` entry uses the same scenarios.
Both entries include the private seed helper once at their crate root; this
avoids the `clippy::duplicate_mod` failure observed on `2f01156c` without a lint
exemption. Every existing required command/target remains intact.

That diagnostic run passed 1,674 library tests (eight ignored) and nineteen of
twenty existing boundary tests. Its remaining failure showed that unsupported
`todo done --validated` reached discovery before rejecting unavailable proof
capture. A shared guard now rejects it before connector construction, while
retaining the direct-store guard and the existing no-network/no-mutation test.
The full native validation of that superseded candidate was interrupted; it
does not supply a final readiness verdict.

On `2f01156cfebd9622cc9ae6f43aa80403054a2461`, GitHub consumer job
`130991125949` passed all 1,674 library tests (eight ignored), including all
eighteen custody tests, and all thirty-three boundary tests. The boundary
results explicitly include all twelve managed CLI scenarios and the corrected
proof-before-network case. Other selected cloud/registry integration targets
also passed; the two protected Supabase profiles remained ignored. This is
diagnostic execution, not a full Buildkite readiness receipt. The follow-up
test layout removes only the duplicate seed-helper inclusion (thirty-two
boundary tests) to resolve clippy while retaining every product assertion.
Native validation hit the public metadata reader's rate limit before obtaining
readiness. A fresh complete verdict on the final exact candidate remains
required; neither GitHub success nor a rate-limit failure supplies it.
`tests/release_dependency_policy.rs`, Cargo manifests and source-CI configuration
remain intact; an unpublished source overlay cannot establish release-policy or
publication readiness. No local fixture claims hosted managed Auth, production
entitlement policy, real PostgreSQL, deployment or full remote governance parity.

The legacy protected CLI profiles in `tests/supabase_service.rs` and
`tests/propodus_live.rs` fail explicitly with `MANAGED_CLI_FIXTURE_UNAVAILABLE`
before inspecting fixture credentials or dispatching any CLI/network request.
Their real-service assertions remain intact for a future managed fixture; no
ignored profile is represented as passing. The explicit legacy PropodusClient
API test retains its own service boundary. Account completion response loss
resumes the idempotent confirmation using the same body and bearer; the binary
fixture asserts that one-time token exchange is never replayed.

## Native Laya research reconciliation (#1614)

The historical candidate failed dependency denial on Tokio 1.52.3
(`RUSTSEC-2026-0386`) and lacked a current-source Interfaces review. The
reconciled source retains accepted v0.138.0's Tokio 1.53.3 root lockfile and
sole-Laya kernel ownership. Native task/workspace admission, genuine current
spec reviews, material checkpoints and exact-candidate validation must establish
the new candidate's custody; neither an old result nor copied metadata does so.

An upstream advance invalidates the old publication base even when a previous
candidate has successful source-CI receipts. Current integration preserves the
accepted test-only plugin selection and its material validation contract. Old
receipts retain their original base/head bindings; reconciliation onto the new
accepted base requires native custody, checkpoints and fresh exact-candidate
publication checks. `PUBLICATION_MATERIAL_BASE_CHANGED` is not a test waiver.

The original frozen fitting/evaluation corpus definitions replace unsupported
placeholder families. Their partitions remain disjoint; the evaluation allows
zero false-ready adverse controls and cannot be reused for tuning.
`core::laya_readiness::fitting_cases::frozen_corpora_name_executable_and_disjoint_contract_families`
executes the eight declared bounded Rust witnesses and checks partition
separation. It generates no fitting/evaluation inputs, runs no model and
produces no evaluation scores. The explicit bounded-process signal-module path
supports compilation of shared source by the separate native worker workspace.

The new regression is part of the library unit suite. The actual source paths
select the existing required `supabase-consumer` check, whose second command
runs unfiltered `cargo test --locked --features supabase-cloud --lib` with its
listed integration targets. Execution requires the complete command record and
the named test result. The narrower `selective-core` recipe selected by these
paths runs `entrypoint_correctness`, not this regression. A green job with
`not_applicable`, an ignored test, or compilation alone cannot establish that
the bounded witnesses ran.

The independent `assets/tools/laya-native` workspace remains unqualified.
Root-library CI does not compile, audit or qualify that separate workspace;
its preserved dependency set, including zip 1.3.1, requires its own remediation
and verification before activation. The recorded fitted-worker diagnostic
approved six of twenty adverse controls and rejected four of twelve ready
controls. Those failures remain evidence, the held-out evaluation remains
unscored, and no threshold, numerical tolerance, coefficient bound, model
profile or policy requirement is relaxed. Native compilation, dependency
checks, real backend/protocol execution and a separately justified successful
qualification must precede any activation or readiness claim.

## Historical full-proof completion (#1610)

Required regressions exercise both native and provider history production,
complete proof-plan and material bindings, archive durability, and failure to
persist. A partial, failed, stale or decision-blocked result must not be archived
as successful full proof. Recording history must leave the candidate SHA intact.

`validation_history_archive_tests` exercises the actual pre-validation binding
and canonical append transaction, then removes temporary execution evidence and
looks up the full record by SHA. Native and provider-shaped complete-suite
fixtures must preserve the original task, epoch, plan and results without
changing Git HEAD or tracked governance. These fixtures test archival mechanics;
their observations are not evidence of an external provider run.

Completion tests must show that an original owner can complete a merged task,
or repair its done-but-unverified state, from recorded candidate proof without
launching current validation or borrowing another task's workspace. Adversarial
cases cover wrong task, commit or repository, corrupt evidence, intent edit and
revert, changed lease generation, incomplete task history, unproven integration,
dependency failures and revision races. Failed attempts must preserve task,
proof and journal state. Subsequent QA inspection must preserve this historical
meaning after unrelated work and epoch changes. Publication continues requiring
fresh complete proof and cannot consume this historical completion permission.

The inline Rust regressions are part of the library unit suite selected by the
complete source profile. Their presence and source review are not execution
evidence; the exact candidate still requires configured full validation.

## Immutable push-range ancestry

Ordinary push-range validation uses the previous branch tip as its `base` and
requires that tip to be an ancestor of the exact new push head. The separately
captured accepted target remains part of validation; a protected-master push
can have the accepted target equal to the new head. PR validation retains its
base-ancestry and exact merge-base requirements. See
`src/decapod/core/release_metadata.rs::validation_change_base` and
`tests/release_metadata_policy.rs` for the ordinary-range regressions.

Governed `refs/heads/agent/*/*` pushes use a distinct normalized context.
`.github/actions/ci-context/action.yml` clears `push.before` for this route and
captures the accepted branch declared by the committed `repo.base_branch`.
`assets/tools/ci_event_context.rs::resolve_workbranch` and
`verify_normalized_workbranch` require that captured accepted tip to be an
ancestor of the exact candidate, verify its checkout and complete unmodified
Git history, and recheck the captured ref. The branch namespace selects this
context; it does not itself grant rewrite or publication authority.

Rebasing an existing governed branch requires explicit maintainer authorization,
a retained original tip, and an exact old-head lease for the remote update.
Preserve authentic task custody and the active journal, record fresh native
material checkpoints for every rewritten material commit, and establish fresh
full publication proof against the accepted target. Earlier receipts and
historical completion records retain their original candidate/base bindings;
they do not prove the rebased tree. Divergent accepted targets, missing history,
and stale or fabricated checkpoint evidence still fail validation.

## Version smoke regression

The unchanged Nix workflow (`.github/workflows/nix-flake.yml`) smoke-checks the
built binary with `system version`. The CLI regression
`system_version_does_not_open_or_repair_project_storage` runs that command with
invalid project configuration and datastore modes 0700, 0775 and 0777; metadata
reporting must succeed without creating state or changing permissions. Stateful
commands must continue rejecting unsafe storage. In `tests/trajectory_cli.rs`,
`local_command_announces_a_detected_version_transition_once` checks that version
reporting leaves the old release counter untouched, then a normal project
command emits the pending migration notice exactly once. Full validation must
execute the source tests; these contracts alone do not prove Nix packaging or
post-merge reproducibility.

## Post-merge Gatling test-budget recovery (#1596)

The `Tests (Full)` job in GitHub Actions run `42744631477` ended with a
Bazel `//:gatling` TIMEOUT after 300.1 seconds, while executing the complete
50-test CLI integration suite with `--test-threads=4`. This is evidence of an
insufficient aggregate CI deadline, not of a specific assertion failure or a
successful run. The previous five-minute `moderate` budget did not establish
whether the remaining tests pass.

`.config/build/decapod.BUILD.bzl` now gives only `tests/gatling.rs` the
existing Bazel `long` class (a bounded 900-second ceiling), already used for
two other integration-heavy targets. The test inventory, assertions,
per-process deadlines, four-thread setting, nonzero exit behavior and required
CI checks are unchanged. A hang or failed assertion must still fail CI.

The concurrent `Deploy Docs` failure in run `42744631501` was GitHub Pages
remaining `deployment_queued` before an API deployment error. A later
unchanged-source deployment succeeded, so no speculative Pages workflow or
permission changes belong in this repair. Treat a new failure as independent
evidence requiring its own diagnosis.

Acceptance requires an exact-head, v0.136.1-governed candidate with material
checkpoint proof and the configured required CI graph; a declaration of a
larger timeout does not by itself prove the full test suite passes. The
post-merge `Tests (Full)` run is the decisive 900-second regression signal.

## macOS bounded-reader recovery (#1571)

The accepted v0.133.2 before-fix regression
`core::validation_epoch::input::tests::bounded_epoch_preserves_ordinary_epoch_digest`
failed with `ENOTDIR`. Descriptor-relative macOS enumeration must make the same
bounded and ordinary digests agree while retaining symlink/FIFO rejection and
all input bounds. The macOS iterator regressions also require independent cursors,
no pathname substitution, safe ownership after dropping the original handle, and
correct non-following file/directory/link classification. These are local input
safety checks, not evidence of cloud, hosted CI or publication readiness.

## Claim boundary delivery checks

The required CLI target runs the two `claim_boundaries` regressions with
independent caller environments and a shared worktree datastore. Their results
do not replace standalone validation: the tracked agent entrypoints and managed
Dockerfile must also match the evaluating release. A release refresh must use
the CLI and be committed, so CI does not repair those inputs during proof.

## Real recovery regression

The authorized private Rust fixture reproduced #1473 with two completed tasks:
full scoped validation passed, but fresh per-execution evidence identities made
the second snapshot hash differ. Unit tests in `verify_tests.rs` compare repeated
report identities while rejecting material/policy/verdict drift. The separate
opt-in pending-batch test in `tests/support/live_recovery.rs` preserves the real
failed attempt and retries without deleting evidence, pruning or reopening.
It requires per-task verification and final unscoped publication readiness;
compilation or an ignored test remains insufficient acceptance.

## Documented agent bootstrap execution

The always-selected `cli-contracts` proof executes the inline
`documented_bootstrap` module in `tests/cli_contract_enforcement.rs`, which
interprets the generated
`AGENTS.md` initialization commands through real CLI subprocesses. Human
provisioning creates a committed local-backend project with container workspaces
disabled. The fixture sets `DECAPOD_CLAIM_AUTORUN=false` because claiming
otherwise attempts an optional container launch independently of that project
setting. This disables no claim lease, orientation, or proof gate. It asserts
that container image storage was not initialized and explicitly closes its
temporary directory so cleanup failures fail the test. Installation, remote
backends and Docker execution are outside this fixture's scope. The agent acquires its own session, initializes context, creates
and claims a task, creates a governed workspace, enters the returned path, and
orients before any implementation file is written.

The fixture removes inherited Decapod and Git environment overrides and CI mode
flags rather than setting Git-gate bypasses. Orientation must fail with the
worktree-specific diagnostic from the main checkout both before and after
workspace creation. Successful orientation must retain the created task's intent
and governed-plan binding in the actual worktree. Implementation then changes
only that worktree, with the main checkout's HEAD and tracked work unchanged.
The module is part of the target source itself, so Bazel cannot omit an
undeclared support file. Unlike the selective CLI matrix, this proof also runs
on test-only changes. This runtime regression complements the four-entrypoint text-order checks in
`src/decapod/core/assets.rs`; neither substitutes for exact-candidate validation.

## Native readiness receipt custody

`workspace::ensure_required_governance_artifacts_in_pr` requires current plan,
claims and trajectory material but not the legacy tracked
`sections.validation` preparation receipt. Requiring that output before native
proof is circular, and writing it into the candidate after proof would stale
the proof itself. `decapod govern project-check` retains the explicit legacy
preparation writer; inventory continues to report its actual integrity and
freshness rather than laundering absent or stale receipts into success.
Malformed configuration and missing required governance still fail.

Native execution success is published as one atomic completion directory only
after its receipt and verdict have been written and synced in private staging.
The runtime writes its compact historical proof before that publication point.
The outer full-readiness boundary then appends that
proof to the canonical history; failure of that append revokes returned
publication readiness while preserving the execution evidence. Failed execution, stale candidate/target
or blocked decision produces no success receipt; receipt or verdict creation,
partial-write or sync failure leaves no published success bundle and revokes
returned readiness. Pending staging files are never completion evidence. No checkout or
Git metadata is changed by receipt production. Regression coverage in
`tests/consumer_readiness.rs` uses actual native source execution to assert
binding, candidate immutability and absence on failed/stale proof;
`tests/unit/core/workspace_publication_tests.rs` preserves material gates and
legacy integrity diagnostics. Runtime unit regressions inject creation,
partial-write and sync failures for each completion artifact and assert absence
of a successful verdict/receipt pair, plus preservation of an existing bundle. These new regressions await the official
configured validation of this candidate; their presence is not a passing result.

## Validation Overview

Decapod validates the exact candidate against declared intent, accepted policy,
a bounded required proof profile and verified execution custody. The agent owns
repair and revalidation; human review retains merge and release authority.
Buildkite evidence is required when that provider is configured. GitHub mirror
results, prepared prerequisites and replay fixtures do not independently grant
publication readiness.

Optional Laya assessment follows successful deterministic proof and fresh material
bindings. Disabled mode executes no inference. Required mode retains declared
obligations in the bounded `validation-results.v3` projection and requires a
valid affirmative response under the pinned manifest. Failed proof, oversized
inputs, missing runtime, stale bindings, rejection and abstention cannot become
publication permission. Four captured projection hashes remain compatibility
vectors; they establish serialized input preservation, not model accuracy.

Canonical proof encodings, event identities and historical Git commits are not
rewritten. Removed research profiles have no current acceptance obligation and
no historical failed result is relabeled as success. External model and hosted
service profiles are executed only under their explicit fixture requirements;
unavailable or ignored profiles are not passed evidence.

Publication reviews bind current Architecture, Interfaces and Security content
to the current code fingerprint. `updated` requires a semantic change from the
accepted base; otherwise an honest `unchanged-with-reason` review is required.
Full validation and checkpoint inspection remain separate checks.

## Native sequence verification

`tests/native_proof_execution.rs` exercises real compiled Rust commands sharing
scratch artifacts, later-step nonzero exit, cumulative output limits, one original
deadline, cancellation and tool substitution between steps. It also checks that
unbound successors, shell executables and more than 64 commands fail before
execution. Earlier subprocess output remains available when a later preparation
fails. These tests are focused adapter checks, not proof that every required
platform, workflow or release archive executed. Full publication still requires
the prescribed exact-candidate `decapod validate` result and all required evidence.
On macOS, canonical `TMPDIR=/private/tmp` keeps fixture paths within the strict
owned-snapshot contract. The short process-group cleanup fixture warms first-use
Mach-O loading separately; its original 100 ms descendant deadline is unchanged.

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

The validation worker subprocess (`__project-check-worker`) reports through
anonymous pipes. To prevent pipe deadlock when worker output exceeds the
system pipe capacity (#1526), the parent drains stdout and stderr on
dedicated threads while polling for child exit, so the worker never blocks
on a full pipe while the parent waits for termination. Reader threads are
joined after child exit for complete output, and joined on timeout so no
reader is left running.

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

The `group_broker` Bazel target likewise runs exclusively relative to sibling
targets. CI recorded its final two burst clients reaching the production
120-second request deadline under concurrent target load, after earlier clients
completed sequentially. Scheduling isolation preserves both 20-client burst
tests, durable-result assertions, uncertain-outcome/replay checks and all
production deadlines. This configuration change requires fresh complete core
suite execution; an isolated historical pass alone does not establish readiness.

Source-owned shell, checkout, setup, release, and dependency-policy regressions
must exercise the actual authored helper or workflow body. Tool setup verifies
pinned official bytes and exact version output before adding tools to PATH.
Regular-file release staging preserves executable bytes. Tests do not publish
real packages merely to establish routing or archive policy.

## Validation Rules System

Decapod enforces governance through a unified, rule-driven validation system that
extends the Constitution and OVERRIDE.md authority model. Rules translate
applicable principles into mechanically checkable requirements.

### Rule Registry and Precedence

The rule registry resolves an effective rule set from three tiers:

1. **Built-in rules** (Constitution authority): Embedded default rules supplied by
   Decapod covering scope, custody, evidence, and workunit governance.
2. **Project overrides** (OVERRIDE.md authority): Project-specific rule
   customizations in `.decapod/OVERRIDE.md` under the `

## VALIDATION_RULES Overrides
   section. Overrides can tighten severity, add constraints, or configure
   evaluator parameters within declared override eligibility.
3. **Task constraints** (Governed plan authority): Task-specific constraints from
   the governed plan (forbidden paths, file-touch budgets) automatically become
   rules with TaskConstraints authority.

Precedence: Kernel invariants (NonOverridable) > Task constraints > Project
overrides > Built-in defaults. TightenOnly rules can only increase severity or
add constraints.

### Built-in Validation Rules

| Rule ID | Authority | Severity | Evaluator | Description |
|---------|-----------|----------|-----------|-------------|
| `decapod.scope.authorized_paths` | Constitution | Error | scope.validator | All changed files must be within authorized scope defined in governed plan |
| `decapod.scope.protected_paths` | Constitution | Critical | gatekeeper | Protected paths in config.toml and governed plan must not be modified |
| `decapod.custody.task_ownership` | KernelInvariant | Critical | workspace | All work must be in claimed task workspace with valid ownership |
| `decapod.custody.workspace_isolation` | KernelInvariant | Critical | workspace | Changes isolated to task workspace; root repo must not be modified |
| `decapod.evidence.required_and_fresh` | Constitution | Error | evidence.validator | Required evidence artifacts must exist and be fresh (max age configurable) |
| `decapod.evidence.proof_backed_completion` | KernelInvariant | Critical | workunit | Task completion requires passing proof-plan gates with valid evidence |
| `decapod.scope.file_touch_budget` | Constitution | Error | scope.validator | Changed file count must not exceed declared file-touch budget |

### Rule Evaluation and Verdicts

Each rule evaluates to one of three verdicts:

- **PASS**: Required check executed successfully and established the specified condition.
- **FAIL**: Evaluated condition was violated.
- **UNVERIFIABLE**: Condition could not be established due to missing evidence, inputs,
  or evaluator capability. UNVERIFIABLE never counts as PASS.

FAIL and mandatory UNVERIFIABLE verdicts block governed completion and publication
transitions. Every blocking result includes a structured remediation contract with:
- Stable error code
- Rule identifier and applicable authority
- Actual violation and expected condition
- Affected paths or governance entities
- Required remediation steps
- Whether autonomous remediation is permitted
- Whether human authorization is required
- Evidence needed for revalidation

### Override Integration

Project-specific rules are declared in `.decapod/OVERRIDE.md` under a dedicated

## VALIDATION_RULES Overrides section. Each rule override uses a fenced block
with the rule ID as heading:

```markdown
### decapod.scope.authorized_paths
````markdown
severity: error
applicability:
  required_changed_paths: ["src/**"]
  forbidden_changed_paths: ["**/secrets/**"]
evidence_max_age_seconds: 43200
````
```

Overrides are validated for:
- Unknown rule IDs (fail closed)
- Duplicate definitions (fail closed)
- Malformed definitions (fail closed)
- Unauthorized weakening of kernel invariants (rejected)
- Override eligibility violations (TightenOnly, NonOverridable enforced)

### Applicability and Context Awareness

Rules declare applicability conditions (required/forbidden paths, task types, Git
refs, CI/local-only). The registry only evaluates applicable rules. Custody rules
(task_ownership, workspace_isolation) only apply to governed agent workspaces
with active task claims. They do not apply to:
- Detached/post-merge validation contexts
- Consumer repository validation
- Release/post-merge validation contexts
- Fixture/test validation contexts

### Remediation and Recovery

Every blocking result provides actionable remediation:
- **Autonomous**: Agent can repair (e.g., revert out-of-scope changes)
- **Authorization required**: Human approval needed (e.g., protected paths, scope expansion)
- **Evidence production**: Agent must produce fresh evidence (e.g., re-run validation)

Agents must follow the remediation contract and revalidate. Repeated evaluation of
unchanged inputs must not invent progress. Kernel invariants cannot be waived by
candidate-controlled policy changes.

### Integration with Validation Pipeline

The validation rules gate runs as part of `decapod validate` after override
authority resolution and before project specs validation. It evaluates all
applicable rules against the current candidate state, evidence artifacts, and
governance context. Results are included in the validation report with
policy fingerprint and validation epoch binding.

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

## Intent Reconciliation Acceptance Boundary

Repeated hosted HTTP504 failures fetching pinned Bazel release archives are
repaired by `.config/build/downloader.cfg`: try Bazel's official mirror, then
the same official GitHub release URL with an explicit download query, retaining
the original URL as a fallback. `rules_cc`, `bazel_features`, `rules_shell`,
`protobuf` and `rules_java` mirror archives and `rules_rust` and `supply-chain`
query-route archives were verified against upstream pins. Version selection, integrity
checks, compiler requirements, source execution and the complete test graph
remain unchanged; fresh exact-candidate validation must exercise the repair.

The accepted v0.133.0 source inventory differs from the last reviewed profile
only by the package version in `Cargo.toml` and `Cargo.lock`. The explicit
required-profile review declaration is reconciled to that complete inventory;
the finite workflow graph, adapters, provider selection and proof requirements
remain unchanged. This review pin is configuration, not execution evidence.

<!-- decapod:requirement REQ-INTENT-TRACEABILITY -->
The read-only reconciliation report must detect missing/orphaned/duplicate
requirement references, missing proof-spec mappings, stale code fingerprints,
and unresolved human questions. A matching manifest or reference is not
semantic proof; `intent.reconcile` cannot satisfy validation obligations,
approve spec changes, or authorize publication. Unit tests must exercise
these negative cases and preserve existing proof and publication gates.

## Immutable Impact Negative Acceptance

<!-- decapod:requirement REQ-INTENT-CHANGE-IMPACT -->
Unit and RPC tests must preserve existing empty-params compatibility while
proving exact-commit ancestry, deterministic NUL-safe add/modify/delete/rename
classification, authored-spec versus generated-projection distinctions, and
bounded uncertainty on malformed, non-UTF8 or excessive paths. A code change
without reviewed authored specs remains observable and explicitly unmapped.
Human-intent questions, requirement markers, stable hashes and changed-source
inventory are advisory: none may satisfy a proof gate or change
`semantic_alignment: not_proven`.

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

- Repository signal fingerprint: `841d552cd5b5e885d212e00cd6cf25edeb0fb9c66b82405eb4142fbee8fdabc8`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (38 files), `docs/` (1 files), `src/` (228 files), `tests/` (230 files)
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

Generic configured Rust prerequisites must execute through the bounded physical
adapter with locked inputs and recorded outcomes. A failure stops before Laya
assessment. This repository has no mandatory external Rust worker recipe.
Optional inference, numerical qualification and hosted authorization need their
own actual evidence; source tests do not establish them.

## Immutable CI and version-only release command proof (#1397)

`ci_event_context_policy` preserves its existing accepted-carrier tests, including
regular committed configuration custody. Its original local run exposed the
legacy carrier's unavailable `tomllib` on the host interpreter (52 passed, one
failed). That failure is preserved, not marked passed by the experimental Rust
parser's diagnostic run. The carrier migration is not part of this candidate;
complete hosted checks must execute under the existing declared CI environment.
`ci_checkout_policy` retains real private-checkout/containment and rustup-dispatch
coverage; owned macOS fixture paths use canonical filesystem identities rather
than weakening containment assertions.

Material-spec report notes distinguish `outcome=passed` with
`proof=authored_change` or `proof=version_only_exact_projections` from explicit
local `outcome=skipped` reasons. These notes report the executed gate branch;
they do not promote a project-check result into publication readiness. The
whole-command regression inspects this JSON record and the non-ready scope rather
than guessing proof execution from an aggregate green count or absent failure text.

## Released native acceptance boundary

The opt-in observer `tests/acceptance/native_readiness.py` invokes an externally
installed released CLI on agent-prepared governed native branches. Unlike the
runtime fixture, it cannot replace prerequisites or accepted-target transport.
It requires a successful unchanged-candidate external receipt/verdict pair,
actual failed native execution without success publication, and rejection of a
subsequently dirty candidate. It is not a provider or publication authority and
does not change plan acceptance or configured proof. Its failed-native and dirty
controls are distinct from runtime in-flight target-race tests. See
`docs/native-readiness-acceptance.md` for prerequisites and the outstanding real
execution requirement; authored acceptance code alone does not close #1474.

## File-claim reconciliation with native acceptance

The file-scope candidate preserves the accepted native-readiness and live-recovery
acceptance suites from PR 1499 alongside the bounded claim regressions. Explicit
path scopes still require ordinary category, dependency, lease and trust gates;
they neither replace native execution evidence nor establish isolated-worktree
custody. The 0.123.7 release changes the discovered build inventory only through
the accepted Cargo package version. The reviewed inventory declaration is measured
after this merge, before spec refresh; required proof checks are unchanged.

The earlier 0.123.4 source-CI run failed release-bound projection drift. Regenerating
the entrypoints, managed Dockerfile and manifest with official 0.123.7 repairs
those generated inputs but is not a passing test result. This merged candidate
requires fresh exact-head execution evidence. Trust administration remains outside
this PR and unresolved in issue 1480.

## Historical recovery regression contract

The unit history assessor and strict verifier share the same per-commit logic.
Negative fixtures cover multiple missing commits, late tip checkpoints, changed
parent evidence, malformed governance, unavailable refs and self-asserted
exception declarations. The real `workbranch_governance` CLI fixture creates
two actors through separate sessions and tasks in actual linked worktrees; it
reproduces missing history and `SPEC_REVIEW_REQUIRED`, follows the documented
recovery/review commands, and verifies the fresh publication material while
preserving original commits/custody. It denies foreign-checkout mutations and
resolution of an unarchived observation. Passing this gate is not full native,
Buildkite, hosted security or cloud-profile proof. Full exact-candidate validation
remains mandatory, and unsupported exception/maintenance capabilities stay absent.

### Workbranch fixture admission

`tests/workbranch_governance.rs` exercises custody through the actual actor-bound CLI. A manual Git worktree does not become authoring-admissible merely because `agent.init` observes it: `workspace ensure` must verify and bind the checkout first. A session release deliberately clears task assignments, so resumption requires a fresh task claim and authoring admission. Negative fixtures assert that observation preserves inherited governance and that foreign or nonexistent branch-task bindings cannot be admitted. These preconditions preserve the existing custody contract; passing fixtures do not replace independent exact-candidate publication proof.

The existing Laya post-proof judgment remains distinct from deterministic source evidence. This governance recovery change introduces no worker-runtime migration or new model-proof contract.

## Datastore revision 2 proof profiles

`tests/datastore_efficiency.rs` checks nonempty history preservation, additive
uniqueness without renumbering, unsafe-definition rejection, rollback, repeated
initialization and 64 competing event writers. Its ignored measurement case is
explicitly invoked for physical byte/WAL/tail evidence; it is not a timing
assertion in normal CI. Migration and event unit suites supplement these checks.
Disposable native PostgreSQL baseline evidence does not qualify hosted Supabase,
64 exclusive claimants or complete client/backend parity. Publication still
requires the full exact-candidate profile. The v0.134.0 proof-source inventory differs from the previously reviewed v0.133.2 inventory only in accepted Cargo package versions; workflow, action, helper and executor definitions are unchanged.

## Current assessment verification boundary

Laya is the sole post-validation model contract. Deterministic source, migration,
state-commit golden vectors and custody checks remain required. Retired model
research artifacts are removed rather than reclassified as Laya evidence.
Private runtime registration version 2 rejects older bindings explicitly and
allows re-registration without changing model or proof hashes.

## Reviewed journal repair, validation exceptions, and replay stability (#1581–#1583)

A task journal may contain multiple independently identified, uncheckpointed
domain-event suffixes. A repair request can name all reviewed task IDs in one
invocation, with a bounded human-authored reason. Before any acceptance is
written, the implementation must confirm that each selected ID has a
checkpointed predecessor and an uncheckpointed domain-event range, that no
selected ID repeats, and that no unselected task has an uncheckpointed suffix or
divergent live projection. One canonical database transaction records an
acceptance event and a new transition checkpoint for each selected task.
Failure anywhere in preflight or write must preserve the prior event count
and all prior checkpoint heads. No historical event is deleted or synthesized.

Exceptional repair and optional local-validation bypass require distinct
permissions in the **main repository** configuration. Roles and actor labels
are exact and default-denied. The authorization reader must reject an
unresolvable root, missing policy, symlinks, non-regular policy files,
oversized content, invalid TOML and unauthorized actors. A policy digest
binds the authorizing configuration to the audit event. Local actor labels
are cooperative claims and must not be represented as authenticated people.
Ordinary validation remains mandatory. An explicitly permitted bypass records
its reason, reports `bypassed` and leaves both proof readiness and publication
readiness false; it cannot mint accepted verification or native proof.

Verification self-execution must accept the canonical identity of the
currently running Decapod executable, including Nix's `.decapod-wrapped`,
but reject arbitrary executables even when named `decapod-*`.
Readiness replay normalization may mask per-run `request_id` and
`binding.evidence_digest` for each recognized readiness receipt, including
Laya. It must retain material input revisions, model and governance policy
bindings, mode, decision and blockers in the digest. The default Laya mode is
disabled; required mode needs explicit reviewed configuration and cannot
be disabled in violation of accepted policy. Two identical executions must
hash identically; changed material decisions must not.

Regression coverage exercises batch atomicity, unrelated-gap rejection,
wrong-actor denial, Nix self-identity and lookalikes, CLI exception
requirements, disabled Laya defaults, and replay identity stability.
These test definitions are falsifiable requirements; successful execution
is established only by CI receipts for the exact candidate commit.

## Release alignment and projection fixture scope

PR #1589 integrates accepted v0.134.2 journal/RBAC changes and preserves the active workstream. Entrypoints, managed image pin and spec attestations are regenerated by the installed published release. The production-projector replay test supplies a clearly synthetic artifact digest rather than submitting a large debug test binary to the registered-runtime verifier. Its four previously captured input hashes remain the compatibility oracle. Real model/runtime registration retains the existing 512 MiB bound and custody checks; replay vectors do not establish executable registration or model inference.

## CI fixture execution and review dispositions

Lifecycle tests execute their temporary shell runtime only after a bounded, side-effect-free listing proves the fixture inode has no open writers. The probe retries Linux `ETXTBSY` only, for at most two seconds; other failures remain fatal. A Linux regression holds an actual write descriptor, verifies the kernel refusal, then releases it and requires successful execution. Production container mutation and error handling are unchanged. Publication reviews classify SECURITY.md as unchanged relative to the accepted PR base when its semantic content is identical, regardless of accepted upstream additions; current code fingerprints still bind that explicit review.

The socket lifecycle fixture likewise permits a bounded `AddrInUse` interval after closing its own listeners: a parallel fork may retain the descriptor until exec. Its live-peer refusal and replacement-owner assertions still run first, and stale cleanup must succeed within two seconds. Production stale-socket removal continues refusing live peers; no retry policy is added to production operations.

Supabase HTTP fixtures explicitly restore blocking mode on accepted connections before applying the existing five-second read timeout. This is necessary when the platform inherits the nonblocking listener flag. A delayed-header regression verifies that the fixture waits for a valid request instead of failing immediately with `WouldBlock`; production HTTP transport is unchanged.

The delayed-header regression uses the approved Tokio networking API with a five-second overall deadline, while the fixture server deliberately uses blocking reads under its existing read timeout. No lint-policy exception or production networking change is required.

## Local relational compaction proof

`tests/datastore_relational_compaction.rs` proves historical index removal
preserves nonempty multi-table row digests and uniqueness, is idempotent,
rejects redefined historical indexes, atomically rolls back when a required
UNIQUE replacement is missing, and stays compact after subsystem
reinitialization. Any row/hash change, index recreation, partial mutation,
or incorrect upgrade-success claim falsifies completion. Full independent
candidate validation and CI remain required before publishing or merging.

## Aptitude initialization preserves existing prompt history

The existing `plugins_aptitude_tests` target in `tests/plugins/aptitude.rs`
exercises the public `initialize_aptitude_db` entrypoint against disposable
SQLite stores. Repeated initialization must preserve every persisted prompt
field, including stable IDs, disabled defaults, user-adjusted priorities,
usage counters, timestamps, custom prompts and historical duplicate defaults.
Equality of the complete ordered row snapshots is required; unchanged row count
alone cannot establish preservation.

When a default is absent, initialization must restore exactly its logical
`(context, prompt_text)` identity and authored priority, even if a custom prompt
already occupies that context. The restored row receives a fresh physical ID,
starts active with zero usage, and remains unchanged on the next initialization.
Every previously present row must remain identical. The two regressions are
included in the existing required `selective-plugins` source-CI target;
their presence is a test obligation, not an executed passing result. They add
no runtime seeding behavior, concurrency or hosted-storage qualification,
performance measurement, or publication-readiness claim.

Changes under `tests/plugins/` select that same complete plugin recipe, including
`//:plugins_aptitude_tests`, even when production source is unchanged.
`tests/source_ci_profile_policy.rs` checks the test-only selection, required job
inventory, identical production/test recipes, and preservation of unrelated-path
and release-only exclusions. A green `not_applicable` result cannot establish
that the prompt-history regressions executed.

## Datastore production qualification (v0.137.0 review)

`docs/datastore-v2/production-readiness-v0.137.0.md` records source findings
and open acceptance requirements, not newly executed proof. Qualification must
bind complete effective SQLite and native PostgreSQL catalogs to their reviewed
logical contract. Historical inventory provenance remains distinct from the
current package version, and legacy migration receipts cannot acquire invented
implementation attestations during an upgrade.

Acceptance requires actual released-client format compatibility, interrupted
migration recovery, revision and claim-generation fencing, mandatory atomic
mutation/event pairing on every retained route, and uncertain-commit retry
reconciliation. Bounded pagination and journal replay must preserve complete
history, canonical bytes and a coherent snapshot beyond the current aggregate
limits. The shared scenario corpus must exercise governance transitions and
proof bindings through both SQLite and the published native RPC client.

Existing compaction fixtures, compatibility-service mocks and earlier proof
ledger entries retain their stated scope. They cannot establish native cloud
deployment, hosted capacity, backup restoration, or this candidate's publication
readiness. Unavailable checks remain unavailable. The review records both the earlier unadmitted publication and the current
governed reconstruction. No missing validation receipt is claimed as passed,
and this documentation cannot weaken the configured runtime proof profile.

## Agent documentation index consistency

The Documentation Index in `docs/agent/api-index.md` is the canonical agent
document list, including the API index itself. The Markdown document links in
`docs/agent/README.md` and the agent section of `docs/agent/llms.txt` must name the
same files exactly once. The human book retains its separate section; a matching
basename under another directory does not satisfy an agent document link.

`tests/contract_conformance.rs` compares both indexes with the canonical table and
requires every indexed document to resolve through `assets::get_embedded_doc`.
The required `contracts` source-CI profile executes `//:contract_conformance`,
including this invariant. Its compile-time inputs are the three indexes; the
test calls the public embedded-document lookup for corpus membership. The
existing specification-refresh tests retain their independent coverage.
The Bazel test inputs include the inspected indexes, and the constitution
compression inputs include `repository-readiness.md`, so sandboxed builds can
embed that contract. This regression verifies documentation discovery and
packaging. It does not replace exact-candidate governance, runtime or publication
proof, and adding the regression does not establish that it has executed.

The real-libtest inventory fixture in `tests/integration_sharding_policy.rs`
copies its executable in a child process and waits for that process to finish
before invoking the supervisor. Parallel fixture processes must not inherit an
open writable descriptor for the relocated binary. The inventory still executes
the real copied libtest binary and checks its discovered test names; its separate
exit-126 regression continues to require immediate failure with captured
diagnostics and no retry.

The v0.138.1 rebase preserves the accepted historical-completion and Laya
research contracts while replaying this documentation and fixture repair onto
the current accepted target. The original task journal, earlier checkpoints and
archived pre-rebase tip retain their original provenance. Native APIs must
refresh release-bound projections, review the changed Bazel source inventory,
and record a distinct checkpoint for the replayed material. Earlier successful
v0.137.4 execution remains historical evidence; the rebased commit requires
fresh complete publication proof and execution of the documentation-index and
real-executable regressions against the captured accepted target.

## Onboarding integration proof boundaries

The refined setup is verified as part of the complete source PR profile. Real
PTY cases cover recommended setup, edit/cancel, terminal restoration, preview,
redirected output and cloud review cancellation without a service connection.
Managed-session CLI cases separately cover authoritative access and recovery;
neither fixture suite establishes a hosted purchase or paid customer journey.

The reviewed source declaration for the 0.139.0 reconciliation measures all 105
build/workflow/helper inputs against accepted master19acd21. Every input matches
that accepted tree; only the package version in Cargo.toml and Cargo.lock changed
from the preceding accepted cloud mergec87e4f49. Renewing the inventory digest
retains the same source adapter, accepted executor, required graph and verdict
policy. A matching inventory is preparation, and full exact-candidate validation
remains mandatory after every source or target change.

## Validation proof lifecycle

Issue #1629 separates proof admission from proof consumption. A fresh governed docs task must reach its first reviewed source command through full `decapod validate`, without supplying an earlier receipt or an undeclared workunit. Terminal publication rules consume the live runtime's verified complete suite before readiness receipts or historical completion are written. Full candidate/accepted-target/material/governance bindings remain required. Explicit workunit contracts retain journal, proof-artifact, source, current epoch, living-spec and capsule lineage checks; an absent projection cannot erase a durable declaration. Regression coverage must observe real command execution, reject failed commands without readiness/receipts, and retain unknown, forged, stale and mismatched evidence failures. Component fixtures may substitute only accepted-target transport, not source checks or execution observations.

Review regressions cover expiry during terminal evaluation and native receipt staging, a distinct mandatory project workunit obligation without a declaration, and a declared current workunit reaching the actual rule evaluator with a generated capsule, bound living spec, current execution journal and retained proof bytes. Explicit epoch mismatches, corrupted capsule/source and deleted manifests remain failures.

The fresh governed docs regression has its actual `#[test]` entrypoint in the already required `tests/entrypoint_correctness.rs` target, calling `tests/support/fresh_docs_readiness.rs` with helper-only `tests/common/consumer_readiness_fixture.rs`. Real CLI commands create and claim the docs task, ensure its managed workspace, approve its plan, refresh/review specs, checkpoint and commit. The public runtime's prerequisite callback runs the real CLI with explicit `ci_local` scope and requires observed success, nonempty passed checks, zero failures, no publication readiness and an unchanged clean candidate. The same runtime invocation then executes the complete real source suite and terminal rules; only accepted-target transport is substituted with a fresh read of the actual local bare Git upstream. This is CLI-admission plus complete-runtime coverage, not an end-to-end CLI HTTPS transport test. The unchanged selective-CLI recipe executes the same entrypoint, CLI-contract and contract-enforcement targets. All 16 preexisting consumer API cases remain in `consumer_readiness.rs` under CLI contracts; helper support contains no test cases. The genuine fresh docs case runs once in the entrypoint target, preventing the measured 65-second case from overloading CLI contracts. A green `not_applicable` selection is not execution evidence.

The fixture starts without a prior prerequisite receipt or ignored policy cache and verifies clean committed candidates throughout. The fresh docs fixture covers the positive first proof once. The existing real-source failure/committed-repair regression separately requires an actual failed assertion, distinct candidate and evidence identities, no failed-attempt readiness, ready-completion verdict or history artifacts, and fresh complete proof after repair. This avoids a duplicate governed preparation and admission cycle within the unchanged CLI target budget. Canonical scaffold regressions ensure an absent runtime cache causes no source healing or manifest rewrite, while a missing tracked living spec still triggers repair. Existing policy validation continues rejecting malformed or unsupported policy contracts. CLI fixture calls have finite 30-second limits, additionally capped by the runtime's original remaining budget during admission; the accepted aggregate CI budget remains unchanged.

The actual accepted 0.139.0 release at `19acd21f` changes only the Decapod package versions in inventoried `Cargo.toml` and `Cargo.lock` relative to its cloud-client parent. Its 105-file source inventory is reviewed as `sha256:c8786efce425085ccb62996e1d586dd8dabd491adea74af4829e638ec26d6174`; the pre-extraction candidate inherits those source definitions exactly. Updating the declaration records that measured accepted release delta; it grants no execution or publication evidence and changes no controller, executor, workflow, helper or source-selection contract.

Shared fixture extraction adds only declared Rust source inputs for the existing Bazel consumer and entrypoint targets. At placement commit `cff3a3c3`, the reviewed 105-file inventory hashes to `sha256:50c5532ad4dca740a8216688204d2c0446459da2782a66de9377aacb30d360c8`; `.config/build/decapod.BUILD.bzl` is the sole changed definition relative to then-accepted `19acd21f`. The accepted executor, three selected target names, commands, serial test threads, finite budgets, selectors, workflows and compiled carriers remain unchanged. Before/after named-case inventories preserve every existing case and the single fresh docs case; actual configured CI must establish execution after this placement change.

The accepted target subsequently advanced through onboarding PR #1636 and the actual 0.139.1 release, ending at `bc0a71971bce8ee036a24562b7fc4a9b771f2f40`. Reconciliation retains incoming onboarding source, tests and whole-product specifications, the existing validation lifecycle work and every prior native checkpoint. All 140 carried baseline obligations are unchanged; the incoming current journal declares no additional claim or unresolved decision. Source inventory changes since the placement commit are exactly the accepted `Cargo.toml`/`Cargo.lock` package-version bytes. The actual 105-file candidate inventory is reviewed as `sha256:31fa30d0bbc53f76f4f09cbca80101965b71dc1c00c337b54c53607ab3df4a40`; its only definition difference from the accepted target is the already reviewed additive Rust-input wiring. In that reconciliation, independent controller, compiled carrier, required graph, selectors and finite budgets remained unchanged.

Standalone source validation compares the repository after native `validate --refresh-specs` and rejects release-bound drift. The actual 0.139.1 evaluator exposed 0.139.0 entrypoint, managed-image and manifest pins after the accepted release advanced; those projections must be regenerated by an independently released 0.139.1 controller. Registry installation through the supported native session-update path is environment bootstrap, never candidate proof. The project auto-update preference is restored after scoped bootstrap, and projection-only preparation remains nonpublication evidence. The source inventory remains the same reviewed 105-file `sha256:31fa30d0bbc53f76f4f09cbca80101965b71dc1c00c337b54c53607ab3df4a40` set.

Actual pre-refresh candidate `c626fa46` confirmed the unchanged selected target bounds: CLI contracts passed in 281.6 seconds and contract enforcement in 274.2 seconds; the entrypoint target completed in 207.4 seconds with only the prior-release template mismatch failing. Its genuine fresh docs proof passed in 75.2473 seconds. These source-CI diagnostics confirm placement and retain the failure honestly; the regenerated candidate still requires its own complete configured execution and independent full native validation.

## Historical maintenance proof and fresh readiness

The audit binds native run43133190463 to b984a386 on c5de35a8 and its later
merge9b5f3de1. Artifact bytes and the terminal publisher's verified outcome are
the evidence; an earlier green prerequisite check or connector response alone
is insufficient. Run43111223693 and its authenticated report separately account
for the six original redundant refs. Current operations history has its own
complete-bundle import and strict integrity attestation.

The prepared follow-up3638c7e8 had six regressions execute in Buildkite8258,
but full publication was cancelled when accepted master advanced to c87e4f49.
Those checks cannot establish readiness for the reconciled audit. The final
source and tests must match the accepted implementation, the public record must
match authenticated artifact hashes and Git identities, and the exact audit
candidate must pass fresh full native validation before PR publication.

Remaining retirement still depends on private recovery authentication, current
leases and a reviewed merged-state executable finalizer. This documentation
does not discharge those conditions or either research draft's proof contract.
