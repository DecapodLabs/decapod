# Intent

## Cloud control-plane, Dactyl 0.18.0 adoption, and Decapod Local/Cloud unification (#1809, #1808, #1769, #1667, #1777, #1778, #1590, #1033, #1034, #1038, #1040, #1066, #1158, #1369, #1376, #1377, #1380)

Decapod adopts published registry crate `dactyl-db = "=0.18.0"` (SHA-256
`6c26eaf28272a50517d7fe300b56940ac03627477b48579293b231c33de905b4`, preserving
`quinn-proto >= 0.11.19` and synchronized `Cargo.lock` + Bazel `MODULE.bazel.lock`)
without path or Git overrides, and unifies all user- and agent-facing CLI,
capabilities, embedded docs, and human book documentation around **Decapod Local**
(`local`, free, offline, no account required) and **Decapod Cloud** (`cloud`,
account required):

- **Decapod Local/Cloud branding & docs (#1808, #1769)**: Retires the legacy
  Vercel preview URL default so unconfigured explicit `neon`/`supabase`
  datastores fail closed with `DECAPOD_CLOUD_API_URL` guidance. Introduces
  canonical `DECAPOD_CLOUD_*` env vars while retaining `DECAPOD_PROPODUS_API_URL`
  as a backwards-compatible alias emitting structured `decapod.deprecation.v1`
  notices, recognizes `decapod.cloud.store` (`v1`) envelopes and `Decapod-Cloud-*`
  diagnostic headers, and adds a responsive **Decapod Local** vs **Decapod Cloud**
  offerings section on `docs/book/src/index.md` linking to
  `https://decapodlabs.github.io/offerings.html` without public pricing or
  checkout routes.
- **Mixed-worktree routing & inline cloud parity (#1777, #1034, #1158)**: CLI
  and inline todo helpers resolve backend selection (`local` vs `cloud`) and
  canonical repository identity strictly from the invoking worktree rather than
  falling back to the root checkout's SQLite store in either direction
  (local-root/cloud-worktree or cloud-root/local-worktree), while honoring
  `#1776` accepted-cloud fail-closed enforcement against unaccepted local downgrades.
- **Device-bound DPoP custody & container signer broker (#1778, #1033, #1040, #1066)**:
  Dedicated per-origin/per-repository Ed25519 device keys are persisted outside
  Git (OS Keychain where available, otherwise mode-0600 `O_NOFOLLOW` external
  private file), wired into `dactyl-db 0.18.0` `RequestProofSigner` (RFC 9449
  `DPoP`) and `AccountOperation::{DeviceChallenge, DeviceEnroll, DeviceStepUp, DeviceRevoke}`,
  plus `decapod cloud logout|revoke` and a bounded host-side Unix-socket
  container request-signer broker (`[workspace.credentials]`) so container
  workspaces never receive private keys or raw bearer/refresh secrets.
- **Repository access tiers & owner repair (#1590)**: Local `db.recover`
  requires merged canonical `owner_repair_enabled = true` and RBAC `owner`
  authority; cloud repair queries `propodus_repository_access_v1` (`Read` +
  audited `Write`) via `CloudConnector::control_request` and executes
  `owner_repair_v1` atomically via `DactylBridge::atomic` with zero local
  SQLite fallback.
- **Hosted lease fencing & 5-state proof acceptance (#1809, #1038)**: Removes
  the `RESOURCE_CUSTODY_PROFILE_UNSUPPORTED` placeholder in favor of hosted
  exclusive lease fencing/renewal/release and wires `propodus_proof_acceptance_v1`
  / `propodus_proof_acceptance_get_v1` (`captured -> independently_verified ->
  currently_admissible -> published -> historically_accepted`) with safe
  `outcome_unknown` reconciliation and distinct `403`/`413`/`429` error types.
  Offline/loopback and local PG17 + PostgREST runs prove client wire and native
  database contracts without claiming hosted production acceptance or unrun Nix
  builds on hosts where `nix` is absent.

## Accepted release12 continuity

The accepted target advanced again to `7bd0a915` (0.148.12), which changes only the root package versions and changelog after9c. The prior9c merge remains a real parent with its own checkpoint; this release integration gets a new native checkpoint. All105 verification-source inputs were independently remeasured: Cargo.toml/Cargo.lock version bytes changed and103 remained identical. Required helpers, graph, commands and toolchain remain unchanged. Official session acquire installed a private0.148.12 controller (SHA256 `1fb9e592076c937f0be049f2329b287e22f2e6a346b8f351ec863bfc92a6d7d3`) and restored the temporary config opt-in; old/shared binaries remain intact. Native projection refresh does not transfer earlier Actions success or establish current admission.

## Current accepted-target integration

The active currentness task integrates accepted master `9c7e6cff` through a real two-parent merge after the external acceptance of PR1791. Its strict JSON source, test compile inputs and authored contracts are retained alongside this task's current proof obligations. Earlier352-based source results remain historical; the combined candidate requires new execution against9c. The native active journal stays with this task; Git preserves both parents and all accepted history. No merge admission is inferred from the external ref update.

## Accepted interoperability workstream context

## Unambiguous client requirements

Interop discovery must not silently erase a caller's required semantics.
Malformed JSON and duplicate decoded keys are refused before RPC startup on
both stdin and flag-parameter routes. Ordinary unique-key requests continue.
The existing unique-key decoder is reused from release-manifest validation;
focused cross-language parsing controls make no new conformance-receipt,
independent-executor, authority or publication claim.

This follow-up starts from accepted release `352d8929` (0.148.11), after
PR #1787 was merged by another actor while the parser extension was prepared.
The earlier unaccepted source and checkpoints remain in their original Git
history. This new governed task reauthors only the reviewed duplicate-key
correction. All 105 verification-source inputs were independently measured;
only root package versions in Cargo.toml/Cargo.lock changed from 0.148.10.
The remaining 103, workflow graph and declared toolchain are unchanged.
Source-inventory reconciliation is not execution or publication proof.

The first Actions evaluation built the candidate as 0.148.11 and passed its
191 project checks, then correctly refused release-projection drift: the
managed Docker header and manifest still described controller 0.148.10.
The official session updater installed a private 0.148.11 controller; native
refresh/project-check now owns those regenerated release surfaces. The
configured proof graph is unchanged. Host-only container and shared-task
proof blockers remain separate from portable source-CI execution and prevent
any claim of native admission. Metadata repair is not a passing runtime test.

## Checkpoint history recovery

This fresh governed workstream preserves PR #1788 and immutable source
`d53835ad2ee573996edcc31ef1ce2ef577a9a7cc`. Exact native inspection relative
to current accepted `352d8929` found one missing checkpoint, after the agent
committed a release merge before its asynchronous checkpoint completed. Eight
earlier authored commits passed that same inspection. The native assessment
of the older original baseline is retained separately with its complete
findings. Neither assessment is relabelled as a passing history.

After the present-time recovery archive was committed, the native resolver
accepted only its current archive/reauthoring disposition. The original gap
remains historical and unproven. Official 0.148.11 refreshes the current managed
Docker pin and manifest release metadata; its installation used the supported
session updater in a private location. Shared-host prerequisites are not
publication proof, and fresh applicable Actions execution is still required.

The source branch and failures remain preserved. This task reauthors only
reviewed workunit code, meaningful execution tests, source inventory and
semantic prose from the accepted target, with a present-time native history
recovery observation. It imports no prior governance, custody, success or
publication authority. A late checkpoint cannot repair the old commit; fresh
current checkpoints and independent execution must establish this candidate.

## Accepted release source review

This currentness workstream starts from accepted `352d8929` (0.148.11).
The preserved original branch integrated that release through a real
two-parent merge. All 105 verification-source inputs were
independently measured, including declared symlink target bytes and their
separately inventoried regular target. Only the root package versions in
Cargo.toml/Cargo.lock changed from 0.148.10; the remaining 103, required
workflow/executor graph and declared toolchain are unchanged. Reconciliation
retains the full configured proof policy and does not establish execution.

## Observed proof remains current only within its execution contract

A genuine prior success must remain inspectable as history while a newer full
not-ready judgment withdraws current workunit verification. Safe repair must
remain available: fresh complete proof for the exact current task and renewed
custody can restore VERIFIED. This correction uses the existing validation
history and workunit journal; it adds no alternative proof producer. The CLI
regression executes real required Python checks and an explicitly declared
external failure control. It proves bounded currentness and recovery, without
claiming complete external-input closure, forge publication admission, or an
independent agent/host.

## Actual-boundary workunit proof controls (#1660, #1735)

The combined candidate incorporates accepted release `34f837d4` (0.148.10).
All 105 verification-source inputs were independently remeasured: only the
root package versions in Cargo.toml and Cargo.lock changed from 0.148.9;
the remaining 103 inputs, workflows, required graph and toolchain are intact.
The reviewed inventory is reconciled accordingly. A scoped 0.148.10 controller,
installed through the official session updater, owns current projection refresh
and validation. Its actual binary digest is retained separately from candidate
evidence. Controller installation and release integration are not test proof.

The ordinary recovery path must retain the workunit's genuine task-bound
context capsule and policy lineage. The source-file counterexample prepares
that capsule through the public query and attach-state commands before
requesting proof, so missing context cannot substitute for the producer test.
Full validation uses its current runtime observation while the workunit is
CLAIMED; it does not require a prior VERIFIED projection. A generic terminal
rule refusal is not evidence of a circular proof dependency.

An agent-declared passing result and a real source file's digest must remain
diagnostic declarations. The existing canonical full-validation producer owns
VERIFIED admission. The campaign's actual-binary control reaches an admitted
task worktree and current execution checkpoint without gate-skip variables;
recording a source-file pass succeeds, while VERIFIED is refused without the
canonical producer record. The journal retains the declaration and CLAIMED
status. This confirms the bounded guard, not the issue's entire proof matrix.
Fresh trusted proof, split-store recovery, replay and publication remain
separate acceptance obligations.

## Autonomous safe config recovery and privilege custody (#1771, #1776)

The cloud boundary suite treats embedded schema discovery as file-only recovery,
including CLI and RPC reads. `tests/common/cloud_managed_scenarios.rs` compares
the complete project and private machine file inventories before and after each
read, and requires zero HTTP requests. Unsupported operational `store.query`
still exercises the cloud startup guard and must refuse local SQLite fallback;
schema availability does not authorize operational persistence.

First adoption on a verified accepted Git tree that predates repository policy
uses the compiled global approval defaults for ordinary operations. The resolver
confirms absence in that exact accepted tree, never an older ref or candidate
fallback. Candidate RBAC, shared-claim and maintenance grants remain denied.
Failure to read or verify accepted policy remains an error. Test fixtures must
establish their accepted baseline before executing candidate-branch operations.

PR #1779 previously used an explicit user exception for draft creation before
full validation on 2026-11-17. That historical exception does not authorize
another workstream, establish current admission or grant merge authority.

Agents must be able to discover the embedded configuration contract even when
their proposal is malformed, repair it in governed isolation, and resume
ordinary work. They cannot use schema validity, a repinned candidate policy,
or a changed base selector to acquire new exceptional authority. This change
reuses existing parser, branch-safe maintenance policy and publication owners.
It preserves accepted-policy rotation and revocation without repetitive human
confirmation. Real-host activation (#880), unified diagnostic (#1071), and
real-forge current admission (#1763) remain separate executable acceptance
obligations; parser/RBAC regression results cannot stand in for those results.

The proof-source inventory is reconciled against its last reviewed source
`edbfc00d`: only the package version in `Cargo.toml` and `Cargo.lock` advanced
from 0.148.7 to 0.148.9. The candidate preserves every workflow, command,
coverage obligation and configured execution provider while acknowledging
those release metadata bytes. Reconciliation does not establish test success.

## Truthful conformance discovery (#871, #1735)

Compatibility discovery must preserve required semantics at the parser boundary.
Discovery also reports the existing session requirements for context bundle
export/import, mentor obligations and assurance evaluation. Automatic session
acquisition does not make an operation session-free. Every RPC descriptor is
checked against the existing entry guards; read-only database inspection retains
its earlier bounded session-free route. No enforcement permission is changed.

The official 0.148.10 counterexample refused a valid unsupported extension but
accepted its trailing-comma flag form after silently replacing the parameters
with an empty object. Flag parameters now fail on invalid JSON before startup;
omitted parameters still mean an empty object. Ordinary supported negotiation
and valid unsupported-extension refusal remain available. This is an input
interpretation correction, not authority or executed-conformance attestation.

The combined candidate incorporates accepted release `34f837d4` (0.148.10).
All 105 verification-source inputs were independently remeasured: only the
root package versions in Cargo.toml and Cargo.lock changed from 0.148.9;
the remaining 103 inputs, workflows, required graph and toolchain are intact.
The reviewed inventory is reconciled accordingly. A scoped 0.148.10 controller,
installed through the official session updater, owns current projection refresh
and validation. Its actual binary digest is retained separately from candidate
evidence. Controller installation and release integration are not test proof.

The current RPC surface must also advertise `intent.reconcile`, `state.resolve`
and `state.diff`. The real entry boundary rejects unregistered operation names
with `unknown_op` before project/session startup. Registration and metadata
come from one declaration in the existing interop catalog; registration grants
no authority and preserves every registered operation's own gates. This bounds
undiscoverable additions without claiming an authenticated conformance producer.

The source-inventory review compared all 105 discovered inputs against the last
accepted review `edbfc00d`. Only the root package version in Cargo.toml and
Cargo.lock changed, from 0.148.7 to 0.148.9. The reviewed digest acknowledges
those release inputs; workflows, helpers, toolchains and required coverage are
unchanged. This review is not evidence that any required check executed.

Harnesses may discover implemented operations and their source test coverage
without an execution claim. The built-in interoperability manifest reports
`not_run` even where a registered conformance test and source location exist.
`core::interop::validate_manifest` rejects declared `verified` and `failed`
results with `unobserved_conformance`: the current manifest format supplies no
authenticated producer receipt. Ordinary negotiation remains available.
This closes the static-label counterexample only. Current binary-bound observed
conformance, independent consumers and the rest of the campaign remain required
before #871 or #1735 can be considered complete.

## Complete scoped source currency (#1753)

The current follow-up also repairs the source-version policy's currency boundary.
Canonical source headers explicitly permitted by config must not be forced into
a version-only rewrite by another gate. The exact canonical repository and real
source lineage remain mandatory; generated manifest/Docker provenance and all
execution checks remain current. No user edits of managed entrypoints are used.

## Post-integration proof boundary

The implementation was accepted externally in merge 41e7e152 with the exact
c35eb608 Git tree, then released as 0.148.7 at 9d1aed27. That forge transition
does not turn the stale prior native validation into admission. The current
proposal must validate its own head against the newly accepted base;
runtime changes are limited to the scoped currency correction above; required
checks remain enforced. Strict live
acceptance 07 additionally repeats the complete sequence on evaluating release
0.148.7; its receipts retain their own exact controller and fixture subjects. The release-only Cargo.toml/Cargo.lock version changes require a
reviewed source-inventory update; no workflow, command or coverage is removed. See docs/publication-admission-acceptance.md for exact identities.

## Current dependency policy (#1755)

A current required dependency failure reopens delivery even after historical
readiness passed. Decapod replaces the unmaintained `chrono` dependency with
`time`, without an advisory exception. Fresh source and live-forge proof must
cover this candidate before delivery issues are closed. The reviewed source
inventory changes only for Cargo.toml, Cargo.lock and MODULE.bazel.lock; accepted
commands, workflow adapters, requiredness and coverage remain intact.

During development of Decapod itself, an explicitly configured internal policy
can separate entrypoint release-version churn from the source package version.
Its scope is the exact canonical Decapod repository and source lineage; valid
canonical template bytes and fingerprints remain mandatory. It does not renew
historical execution evidence or weaken exact-artifact publication proof.

## Measured strict live acceptance

The current real GitHub acceptance uses the repaired timestamp dependency and
requires actual source-policy rejection, excluding a timeout as proof. Its
immutable receipts, exact source/head/base identities, quarantine and stale-target
controls are recorded in docs/publication-admission-acceptance.md. The final
source delivery independently requires fresh full exact-candidate validation.
Historical controller and fixture successes do not authorize a later artifact.

## Evidence-bound PR admission (#1747, #1748)

An agent earns PR admission only through complete live proof for the actual
candidate, accepted target and required policy. `PublicationReadiness` carries
that existing proof through the publisher; a saved historical success cannot
authorize a different present artifact. A target advance or changed source,
governance, execution tools or task custody requires fresh applicable proof.
PR admission is an observation for exact head/base identities, not authorization
to merge later. Failed post-creation admission preserves a separate rejection
and verifies that the matching PR is quarantined as a draft or already closed.

## Do What Must Be Done (#1690–#1692)

Maximize useful autonomous engineering per human intervention. Agents must
investigate discoverable information, make defensible engineering decisions and
verify uncertainty before delegating cognitive work to the human. The existing
orientation/mentor reflection protocol now accepts typed escalation proposals,
observes source predicates and policy/approval state, and challenges inadequate
proposals within its lifetime three-round budget. Human intent, explicit
authorization and exceptional decisions retain authority. The kernel neither
reasons as an LLM nor directly questions the human. Semantic exhaustion and
completed engineering work remain unproven by deterministic reflection receipts.

## Required test dependency coverage (#1672, PR #1684)

Test-only repairs must select the existing core or CLI lane that executes them.
`assets/tools/source_ci_profile.rs` includes the runtime and CLI recipe inputs,
shared consumer fixtures and Bazel declarations in that selection. An absent
selection is not successful test execution. This delivery retains the accepted
finite check IDs and complete recipes. Independently bounded shards, compilation
reuse and the five-minute timing requirement remain open under #1672; this
coverage prerequisite does not claim them. Exact-candidate validation remains
required after integrating accepted v0.145.3 source.

## Concurrent custody and diagnostic scope (#1469, #1661)

Independent tasks can author concurrently in distinct admitted worktrees. A task
lease has one live checkout binding; transfer, retirement and renewal preserve
fencing. Explicit shared-resource reservations govern integration and release
operations rather than excluding unrelated source work. Non-Git projects can
run bounded prerequisite diagnostics without being admitted for authoring or
claiming publication readiness. This repair retains the accepted master policy
and Decapod 0.143.1 release contract; it does not complete the broader ten-worker
or autonomous-runtime acceptance mandate.

## Evidence-grounded reflection before consequential action

Decapod helps agents reconsider a proposed action through optional bounded
reflection in orientation and mentor. It challenges a conflict with the current
plan or a literal ADR match, then recognizes an inspectable scope correction or
separately resolved governing gap. The ordinary orientation path remains simple.
Reflection is a reasoning aid: it establishes neither human authorization nor
semantic correctness, executed proof or completion. The kernel remains local,
daemonless and deterministic, with no model or new memory service.
See `core/mentor/reflection.rs` and `docs/architecture/bounded-reflection.md`.

## Published SDK adoption (#1638)

The client pins the normally published registry `dactyl-db =0.16.1` release
without changing local/cloud composition, session custody, role authority or
transport selection. Its patched `quinn-proto >=0.11.19` requirement must hold
for every resolved lock entry, including inactive dependencies. This adoption
does not enable HTTP/3 or establish hosted service acceptance.

## Governed cloud client and private custody (#1628)

The human runs `decapod init` once per repository and then talks to the coding
agent normally. The agent owns every subsequent Decapod invocation, including
sharing a browser sign-in link, bounded continuation, private session reuse,
refresh and explicit safe recovery. The user completes provider/account actions
without operating the CLI again. Pending sign-in is an honest incomplete
connection, not a reason to lose the user's task or create a duplicate task.

The agent carries the engineering request through Decapod's intent, task,
workspace, execution-history, validation and publication interfaces. It owns
sequencing and supported recovery, and returns an inspectable proof reference
bound to the checked source. The user supplies the outcome and required human
decisions, without manually orchestrating the command lifecycle. Decapod's
governance and configured validation remain the authority for admissibility
and readiness; an agent's completion message is not evidence.

Decapod's local-first governance kernel remains the system interface for agent
work. Optional cloud selection changes the operational task backend without
moving authorization policy into the agent or turning credentials into Git
state. `repo.backend = local|cloud` is the only project-level composition
choice. Dactyl owns discovery, physical connector selection, authentication
transport, account-completion transport, and cloud data I/O; Decapod owns
machine-private session custody and the bounded human-facing continuation.

The client must reuse valid sessions, serialize refresh, preserve interrupted
one-time outcomes, and expose pending/denied/action-required outcomes honestly.
Only service-confirmed completion plus an authenticated repository read permits
the init-ready message. This establishes current read access, not future write
or privileged authority. Local mode requires no cloud account. The implemented
task slice does not add full remote governance parity, pricing policy, hosted
PostgreSQL acceptance, or a release exception for an unpublished dependency.
Source: `core/cloud_client.rs`, `core/cloud_session.rs`, `core/todo.rs`, and
`lib.rs`. Fixture evidence and protected real-service evidence stay distinct.

## Current bounded-reader repair (#1571)

Restore ordinary local validation liveness on macOS by enumerating pinned
filesystem handles directly. The change must preserve digest equivalence,
streaming bounds, no-follow identity checks, and genuine failure results. It adds
no authorization exception, publication waiver, new datastore or cloud behavior.

## Rust consumer source-spelling coverage

## Product Outcome

Decapod keeps AI coding work tied to the human request, the repository's rules,
and evidence of what was actually checked. Its product outcome is work that
people can inspect, agents can continue, and the project can accept through
explicit governance gates.

Nix packaging proof must exercise the built Decapod binary independently of
checkout-local governance state. Shared CI checkout permissions must not require
loosening runtime storage security to smoke-check a package.

The harness chooses what to attempt and the agent performs the work. Decapod
resolves canonical state, decides whether the attempt is admissible, and judges
whether the resulting claim is proven. It does not choose the next action. A
passing verifier is not, by itself, an independently constrained verifier.
Each isolation axis needs its own usable evidence reference. A native-trace
label, a self-report, or a harness name does not prove that axis, and evidence
for one axis does not prove another. The verifier must also have reviewed the
same input digest as the completion evidence. `not_proven` stays distinct from
`failed`, and `unknown` stays distinct from `proven`. A capability is `stale`
only when a usable evidence reference is bound to a different governing
identity. An unknown judgment is not automatically a state blocker.

Concurrent local agents and worktrees must retain their own caller identity in
results, task ownership and audit evidence while sharing canonical storage.
The foreground broker's optional reusable connection must make bounded progress,
end with its owning foreground invocation, and hold transaction locks only for
actual database work. Migration and restore retain exclusive file-lifecycle
protection. An uncertain acknowledgement must never silently repeat a mutation.
Long repository paths must route through a real confined endpoint on Linux and
macOS; unavailable transport is an explicit error. Required broker acceptance
checks returned and persisted ownership, physical opens, lifecycle failures and
normal CI execution. This work does not merge, release, or close #1476.

The current follow-up repairs a demonstrated public Buildkite artifact-origin
compatibility defect in the released v0.121.0 controller. Accept the documented
exact HTTPS artifact host alongside the legacy bucket, preserving producer,
path, checksum, output and deadline checks. This independently qualified
controller prerequisite retains accepted master's existing source-bound CI API
attestation policy and every required source/native check. The original stricter
derived-release branch and its failed collection remain preserved until an
independently supported release can resume them; this fix does not close the
broader #1070/#1398/#779 or four-platform packaging requirements.

The maintainer delegated the functional safe-abstention decision after seeing two
meaningful ready controls approved and the third validly abstaining. Functional
delivery requires genuine approval on each tested backend and every adverse or
protocol control; a ready control may safely abstain, with its frozen expected-
accept mismatch preserved explicitly. This is no claim of perfect oracle agreement
or domain accuracy. Abstention still blocks readiness for that work. No production
threshold, model, prompt, numerical tolerance or deterministic gate changes.

The record must preserve what was asked, how the agent understood it, what
scope was accepted, what changed, and what remains unproven. That record stays
with the project when a conversation, process, model, or agent harness changes.
It connects human intent to agent work, tool execution, produced artifacts, and
the evidence and decisions needed for delivery.

## Human and agent experience

The human installs and initializes Decapod in the project:

```bash
cargo binstall decapod && decapod init
```

They then give work to their coding agent, review its understanding and results,
and decide questions that require human judgment. The agent owns operational
use of Decapod: orientation, Constitution retrieval, task coordination,
workspace setup, specification authoring, evidence collection, validation,
and supported recovery.

The agent also takes on the development work needed to deliver the request:
understanding the project, planning, configuring build and test tools and CI,
implementing changes, repairing failures, and preparing work for review within
the agreed scope and authority. Decapod steers that lifecycle through project
rules, durable state, checks, and actionable feedback. Validation is one phase
of the work.

The human should not need to operate the governance CLI for each coding task.
Unclear intent, conflicting requirements, missing permission, and unavailable
proof must become clear questions or blockers rather than hidden assumptions.

When an agent changes machines, Git history alone does not establish local task
ownership. Explicit task-scoped transfer preserves the original identity, intent,
revision and relevant source history after the source yields and seals custody.
The receiver accepts a private artifact through a trusted operator handoff and
must regenerate machine-specific evidence. This bounded local mechanism covers
standalone tasks; connected/shared work and global exclusivity across disconnected
stores require further authority rather than an invented replacement claim.

## User and Actor Contract

- The human supplies intent, constraints, authorization, and judgment
- The agent interprets the request and repository, plans and performs the
  work, maintains specifications, and records evidence through Decapod
- Decapod maintains governance and coordination state, resolves supported
  project authority, checks invariants, and gates supported transitions
- The repository preserves source, specifications, accepted history, and
  durable records needed for later review and continuation

An agent-authored specification exposes the agent's understanding. Its presence
does not establish that the interpretation is correct. Likewise, a passing
check supports the behavior that check covers; it does not prove all possible
properties of the product.

## Scope

### Intent and context before implementation

Agents consult Decapod before committing to a design when intent, boundaries,
interfaces, or proof requirements are uncertain. The embedded Constitution,
project overrides, orientation, and scoped context help the agent identify
the next useful question or action. Resolved authority and context sources
remain inspectable.

### Bounded, coordinated work

Tasks, claims, sessions, and isolated workspaces establish who owns the work
and where it may run. Concurrent agents must not silently acquire one another's
assignments or overwrite the main checkout. Existing issue trackers may remain
the organizational record; Decapod coordinates accepted repository execution.

Completed work must remain tied to the task that was actually verified.
Changes to task intent or custody cannot inherit a prior passing result; the
kernel exposes stale evidence and requires supported revalidation before that
work can again authorize completion or dependent execution.

### Durable understanding and continuity

Living specifications describe the product. Plans and trajectories describe
the current work. Claims state conditions that evidence can support or falsify.
Knowledge and context records preserve useful project information across
sessions. These records must retain unresolved assumptions and obligations so
the next agent can continue without inventing a clean history.

Checkpoint creation must preserve that history at the mutation boundary. A reused
identifier cannot replace earlier evidence with revised work. Identical retries
remain safe; changed work gets a new checkpoint and independent validation.

Interrupted execution must distinguish recoverable state from current permission
to continue. A journal can restore the exact bytes and custody that it recorded;
it cannot turn missing history into observed history, or refresh old proof merely
by replaying it. Checkpoints of pre-journal state are explicitly migration
baselines. Resume compares current source, intent, plan, and applicable authority
with the recorded binding and fails closed on drift. This continuity is an
on-demand kernel operation, not a scheduler or background retry service.

### Evidence and accepted work

Agents define proof expectations, run the required checks, preserve results,
and repair recoverable failures. Decapod binds supported evidence to the task,
repository state, policy, and validation epoch. Governed publication remains
blocked while required checks, evidence, or approvals are unsatisfied.

Completion after integration is a separate decision from publishing new work.
An agent may finish its original task using durable full validation evidence
for the exact candidate commit, provided the task's intent and custody remain
continuous and that candidate's integration is independently established.
Advancing the accepted branch does not erase that historical execution.
Historical completion must not require a new pending PR or another agent's
workspace, and it must never authorize publication of a different candidate.
Missing historical proof remains missing; a green hosting-provider badge or
the legacy tracked prerequisite receipt cannot reconstruct it.

Obligation completion must remain true when inspected from a fresh session:
current transitive dependencies, proof outcomes, proof freshness, and a nonempty
state-commit root determine readiness. Cached status is advisory. Verification
must expose missing or corrupt evidence as a failed outcome. Publication checks
explicit completion attempts and their dependencies without requiring unrelated
unattempted backlog to be complete (`core::obligation`, `core::validate`).

### Local operation with explicit integrations

Release automation may create an ordinary branch at the accepted target before
authoring a change. That event must retain its verified identity without becoming
a candidate, a release exemption, or publication proof. Source changes on that
branch still require the normal immutable-range checks.

## Acceptance Criteria

The implementation and its tests must establish that:

1. Project intent, local authority, and the agent's interpretation are available
   for review before publication
2. Required session, claim, workspace, and boundary checks reject invalid
   transitions with actionable diagnostics
3. Durable state and required lifecycle evidence commit together; stale
   updates and ownership conflicts remain visible
4. A later invocation can recover current work, its evidence, and unresolved
   obligations from supported repository records
5. Required proof cannot be replaced by a completion claim, generated
   fingerprint, provider probability, or unrelated successful check
6. Validation distinguishes failure, missing evidence, partial proof, and
   readiness for the exact candidate and accepted target
7. Recovery preserves user work and historical evidence, stays bounded, and
   escalates conditions that the supported interface cannot resolve

For container execution, recovery includes retaining failed or interrupted
workspaces and successful local-only work whose commits were not returned.
Failed commands cannot trigger publication. Successful PR requests use the
same governed publication proof as other workspaces, and host foldback cannot
replace divergent commits. Automatic cleanup protects dirty work, tracked work
hidden by index flags, and added or changed refs omitted by branch foldback.
Ignored successful build outputs remain intentionally disposable. A reported
retention reason is recovery information, not evidence that the command passed.

The concrete proof surfaces are defined in [VALIDATION.md](VALIDATION.md).
These expectations specify behavior; this document is not a test receipt.

## Constraints

Decapod does not supply a model's intelligence, write the implementation on
the agent's behalf, or replace the coding harness and human review. It governs
operations routed through its supported interfaces; it cannot prevent an
agent from issuing unrelated shell, Git, or service commands outside them.

Deterministic records and checks do not certify arbitrary natural-language
meaning. Local session custody does not authenticate an external model
provider. Local fixtures do not establish hosted tenancy, deployment, or
live-container behavior. No fixed full-validation latency or service-availability
SLO is asserted here.

Requests that change consequential scope or contradict the accepted contract
require a decision. A recoverable check failure requires more work. Neither
condition is completion.

### External engine tooling

Decapod's proof boundary does not operate Dagger engines. Agent skills and
project CI may own that tooling outside the kernel, while native receipt
custody, receipt-independent prerequisites and required Buildkite execution
remain governed by their existing contracts. Retired Dagger configuration must
fail explicitly rather than silently discarding a required check.

## Continuous Intent Traceability

<!-- decapod:requirement REQ-INTENT-TRACEABILITY -->
The agent must preserve the human-originated change intent while proposing
updates to living specifications and implementation. Explicit requirement
identities may be attached to authored specs across changes. The read-only
reconciliation report exposes missing or orphaned references, unresolved
questions, and outdated source/spec snapshots without declaring semantic
alignment or granting publication authority.

## Immutable Change Impact

<!-- decapod:requirement REQ-INTENT-CHANGE-IMPACT -->
The agent must be able to inspect what implementation and authored living-spec
material changed between an exact accepted Git base commit and the current HEAD
commit, without permitting source observations to rewrite human intent. An
unmapped code change remains explicitly unmapped, incomplete or unsupported
inputs remain unproven, and the harness owns any proposed code/spec repair.
The kernel retains independent review, validation and publication authority.

<!-- decapod:declared-capabilities:start -->

## Declared Capability Surfaces

These declarations select authoring and proof obligations; they do not attest implemented runtime guarantees.

- `agent-helper`
- `authentication`
- `authorization`
- `background-processing`
- `control-plane`
- `event-driven`
- `external-integrations`
- `governance-kernel`
- `persistent-state`
- `scheduled-jobs`

<!-- decapod:declared-capabilities:end -->

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `f58cbb80772dadab9ba0e8dd0550bcad6db1339555a651708eaec36fa6294b98`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (38 files), `docs/` (1 files), `src/` (242 files), `tests/` (252 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->

## Bounded native storage inventory (#1630)

Measure Decapod, Dactyl and Propodus dogfooding stores before selecting a compact
representation. Native CLI/RPC inspection exposes exact counts, explicit bounded
samples and redacted size/reuse observations. Retain all stored history and the
accepted managed-cloud behavior. This PR supplies the measurement prerequisite;
field contracts, representation, migration, retention and full domain parity
remain coordinated work requiring measured budgets and further proof.

## Laya readiness scope

The sole model evaluator is optional Laya assessment after deterministic proof.
The kernel remains daemonless and local-first; model installation and inference
are not required for ordinary disabled-mode operation. No retired scientific
corpus, calibration threshold or old runtime implementation is current policy.

## Optional team publication gate

Teams opt into `[laya] mode = "required"` with the reviewed model manifest and
private runtime registration. Current deterministic proof remains mandatory;
only an affirmative Laya decision may permit an already-authorized PR step.
Rejection, abstention and missing/stale evidence retain repair/revalidation
instructions. Assessment never grants merge or release authority.

## Supplemental Rust worker verification

### Diagnostic collection and unresolved acceptance (#1445)

Native diagnostics must be executable before independent parity and readiness evaluation are available. Missing reference/holdout inputs are publication acceptance gaps, not dependencies of a device tensor probe or model forward pass. The agent must produce public, reproducible Rust verification inputs and auditable agent-curated repository cases; these labels are not human-reviewed ground truth. Source/native diagnostic success alone must not trigger feature publication or a completion claim.

## Validation Rules System

Decapod enforces governance through a unified, rule-driven validation system that
extends the Constitution and OVERRIDE.md authority model. Rules translate
applicable principles into mechanically checkable requirements.

The intent is to make governance mechanically enforceable while preserving the
existing authority hierarchy. Rules translate constitutional principles into
mechanically checkable requirements with structured remediation contracts.

Key intent:
- **Mechanical enforcement**: Governance violations are detected and reported
  automatically, not reliant on human review of natural language.
- **Structured remediation**: Every blocking rule provides actionable repair
  instructions distinguishing autonomous repair, human authorization, and
  evidence production.
- **Anti-bypass**: Kernel invariants cannot be waived by candidate-controlled
  policy changes. Override eligibility (NonOverridable, TightenOnly) is enforced.
- **Context awareness**: Rules declare applicability conditions; custody rules
  only apply in governed agent workspaces with active task claims.
- **Structured remediation**: Every blocking result provides actionable repair
  instructions with stable error codes.
- **Three-verdict system**: PASS/FAIL/UNVERIFIABLE with UNVERIFIABLE never
  treated as PASS.

The system extends the existing Constitution/OVERRIDE.md authority model:
Constitution (built-in defaults) → OVERRIDE.md (project overrides) → Task
constraints (governed plan) → Resolved effective rule set → Validation.

Seven built-in rules cover the existing governance responsibilities:
scope enforcement, protected paths, task ownership, workspace isolation,
evidence freshness, proof-backed completion, and file-touch budgets.

Overrides are declared in OVERRIDE.md under a dedicated VALIDATION_RULES section
with TOML-formatted fenced blocks. Override eligibility (NonOverridable,
TightenOnly, ConfigurableOnly, FullyOverridable) is enforced per-rule.

The validation rules gate runs as part of `decapod validate` after override
authority resolution and before project specs validation. Results include
policy fingerprint and validation epoch binding.

The system is accessible via `decapod validate`, RPC (`validation.rules.list`,
`validation.rules.evaluate`, `validation.rules.get`), and OVERRIDE.md.

## Validation Rules System Implementation Status

The validation rules system has been implemented with the following components:

- **Rule Registry**: Manages built-in rules, project overrides, and task constraints with proper precedence
- **Seven Built-in Rules**: Covering scope enforcement, protected paths, task ownership, workspace isolation, evidence freshness, proof-backed completion, and file-touch budgets
- **Actual Evaluators**: Connected to existing Decapod evaluators (scope validator, gatekeeper, workspace, evidence validator, workunit validator)
- **Three-Verdict System**: PASS/FAIL/UNVERIFIABLE with UNVERIFIABLE never treated as PASS
- **Structured Remediation**: Every blocking rule provides actionable repair instructions with stable error codes
- **Override Integration**: OVERRIDE.md support with TOML-formatted fenced blocks and override eligibility enforcement
- **Context Awareness**: Rules declare applicability conditions; custody rules only apply in governed agent workspaces

### Implementation Quality

- All validation rules unit tests pass (8/8)
- Clippy clean with `-D warnings`
- Cargo fmt clean
- Actual evaluators connected (no placeholder Pass returns)
- Override eligibility enforced (NonOverridable, TightenOnly, ConfigurableOnly, FullyOverridable)
- Anti-bypass protections: kernel invariants non-waivable, candidate-controlled policy changes rejected

The validation rules gate runs as part of `decapod validate` after override authority resolution and before project specs validation. Results include policy fingerprint and validation epoch binding.

## Immutable CI context and release projection continuity

Readiness must use the candidate's committed target configuration and immutable
provider change range. A genuine version-only release may refresh exact generated
projections without inventing an authored requirement change; every other source
change retains the authored-spec obligation. Product implementation and new regression coverage remain Rust-native;
the accepted CI carrier is unchanged. Repeated header refresh must preserve project-authored
Dockerfile bytes and must not introduce whitespace that makes the independent
projection check disagree with the actual validator. This focused #1397 delivery
contributes to #1052/#1069/#1070; it does not by itself prove their full lifecycle,
platform, hosted-custody or derived-release acceptance scope.

## Independent task execution and kernel bootstrap

Independent tasks must acquire custody before their worktrees exist, including
basic-trust agents modifying overlapping logical source files. Task identity is
not title similarity, a category, or a source directory. `core/todo.rs` keeps
claims independent while `core/authoring.rs` admits the actual actor's observed
checkout against a current lease. A named mutable resource still requires its
own exclusive custody. Source overlap requires integration reconciliation and
combined validation; it never proves completion by itself.

A correctable prompt-evaluator invocation error must not become a policy denial
or an allowance. The complete original prompt still needs successful evaluation.
Recovery and publication retain trust, exact approval, fencing and independent
proof requirements. Unsupported cloud profiles remain explicit and cannot fall
back to local authority. See `docs/architecture/concurrency-custody.md` for the
implemented boundaries and remaining hosted/native integration limits.

## Governed recovery without retrospective proof

Agents must be able to continue legitimate work when earlier immutable commits
lack their own checkpoints. The supported outcome is a new, properly admitted
workstream with a present-time gap observation and fresh proof, while the
original source history remains unproven. This change adds no trust elevation,
human-approval assertion, database maintenance privilege or publication waiver.

## Datastore efficiency mission

Preserve the minimum durable operational authority and evidence required for
local-first governed work. The first storage increment removes a structurally
redundant event index while preserving exact historical records and uniqueness.
Its benchmark reports disk allocation, fixture-only reclamation, WAL and latency
separately. It does not establish full cloud parity, hosted capacity, new retention
permissions or the overall 30% storage goal. Subsequent changes require independent
measurements and compatibility proof, as sequenced in `docs/datastore-v2/architecture.md`.

## Laya-only source cleanup

Remove retired model implementation, compatibility fields, fixtures and stale
documentation from the current tree. Preserve canonical proof/event bytes and
historical Git commits. Resolve obsolete carried obligations through the
supported governance CLI; historical resolution identifiers remain auditable
until their accepted merge allows normal workstream compaction.

## Relational index compaction intent (v0.136.4)

Preserve all Decapod domain rows, stable IDs, proof bindings, journal history,
and local isolation while eliminating nonunique indexes duplicated by existing
UNIQUE btrees' leftmost-key coverage. Do not delete rows, rewrite events, alter
WAL settings, run VACUUM, weaken integrity, or silently fall back from cloud.
This is a bounded local SQLite optimization. Full semantic parity with
Propodus Supabase remains a separately gated migration (Propodus #71).

## Refined terminal onboarding and managed cloud setup

Human setup follows Project, Preferences and Review, with recommended defaults,
focused customization and an explicit apply/edit/cancel decision. Interactive
setup creates no project, Git, governance or cloud sign-in state before approval.
Existing settings and full proof definitions survive refresh, with private
backups before changed config replacement. Local operation needs no account.

Cloud setup uses the accepted Dactyl-managed session flow after apply. The service
owns pricing, account requirements, assignments and repository access; the TUI
does not infer authority from a local roster. A pending or denied handoff is not
setup success. Managed-project selection and the protected paid customer journey
remain separate work and are not claimed by this presentation change.

## Explicit managed project routing

Managed cloud commands must distinguish the local Git checkout from a service-issued project reference. `cloud projects` lists authenticated project names; `cloud project use` selects an exact name or a previously shown entry; `cloud project create` explicitly materializes a project through the existing idempotent account contract. Selection does not grant access. Existing session custody and local operation remain independent of this account-routing cache.

The resolved native project also supplies the repository field of newly created tasks. Before the first possible write dispatch, the existing machine-private todo operation journal atomically binds the operation to a digest of its resolved service/project route. Preflight login and initial project selection retain the same operation ID; subsequent selection changes cannot replay an uncertain or successful write into another project. Older unknown records remain bound to their original canonical repository semantics. The real PostgreSQL consumer exposed the former Git-label write mismatch; that failed candidate supplies no successful native proof.

## PR 1666: accepted v0.143.2 source integration

Reduce anonymous GitHub discovery consumption while preserving exact-source proof, immediate denial behavior and the original invocation deadline. Quota hints govern waiting, not acceptance; this proposal does not establish current provider execution.

## PR1666 current-master integration

Incorporate accepted master 9b900254, including custody, protected-path/workunit fixes and the 0.144.0 source release. Preserve the quota-pacing feature and its original PR history, then repair hosted CI failures one at a time. No local test execution or independent full readiness is claimed.

## PR1663 integrated storage behavior

Integrate PR1663 on accepted master5e23ad68 and0.144.1 while preserving storage admission, lossless history and format obligations. Keep accepted custody, quota and runtime-owner changes. Historical benchmark claims remain historical; only fresh hosted execution can establish this candidate's checks.

## PR 1662: accepted v0.143.2 source integration

Expose only project-opted local database maintenance capabilities to the agent. Repository policy and exact operation authority govern diagnostics and safe maintenance; a project flag does not confer recovery, raw SQL, cloud privilege or task/checkout ownership.

## PR1662 current-master preparation

Preserve prepared PR1662 capability-scoped database maintenance before integrating accepted master3afa9df9 and the0.144.2 source release. Default-deny accepted policy, owner-only repair, bounded safe operations and non-destructive lock custody remain obligations. Hosted CI must establish the exact integrated candidate; no local tests are run.

## Independent collector closure (#1573)

Merged1666 supplies bounded anonymous GitHub quota pacing. Preserve required
provider/source/repository identities, explicit access denial and the invocation
deadline while reconciling actual complete collection evidence. Official0.144.3
completed the complete1658 provider contract without identity substitution. This
follow-up records the actual observation and separately validates its own task
and current candidate; a prior task's proof never completes this task by itself.
No new credential path, provider fallback, receipt authority or local tests.

## Required maintenance evidence completion (#1568)

The local-only capability engine merged in1662. Its standalone
`tests/sqlite_maintenance.rs` target was not explicitly part of the existing
required source recipe. Register those actual storage/CLI/RPC controls in the
required CLI contract target without substituting an adapter mock or changing
provider authority. Native SQLite absence must fail these required cases.
Cloud policy/delegation remains separately enforced by Propodus; local opt-in
and forged principal environment never establish authenticated cloud authority.
Hosted execution and exact-candidate readiness remain pending.

## Published SDK convergence (#1667)

Adopt the current published `dactyl-db =0.16.1` registry artifact, preserving
SQLite/Neon/default Supabase features and application-owned governance decisions.
The official sparse index reports non-yanked checksum
`1b41dde2e8be3fc986f7f5a7eb4e5bfb9af49d4c39dabe98fd74a0ccb4ad7ac6`;
the downloaded crate matches it. The 0.16.1 patch has identical `src/*.rs` bytes
to 0.16.0 and repairs upstream test compatibility. Version 0.15 adds bounded
resource accounting and scans; 0.16 adds logical connectors/session transport.
These additions do not confer application authority or independently prove a
hosted deployment. Exact candidate compatibility remains a hosted verification
obligation, with no local test execution.

## Diagnostic overrides cannot authorize readiness (#1659)

Git, tooling and fingerprint gate-skip variables support diagnostic fixtures;
their presence must never authorize full readiness, publication or verified
completion. The shared runtime must reject a full claim before prerequisites or
provider selection, and CLI/RPC/publication paths must preserve that boundary.
Existing CI-local and project-prerequisite diagnostics retain their explicit
partial scope and `publication_ready=false` behavior.

Use the existing judgment, validation-history and receipt owners. Keep historical
observations intact and require a fresh applicable validation after removing an
override. Real child-process regressions isolate environment changes and must
verify both rejected full claims and truthful diagnostics. No local test result,
current hosted success or completed repair is claimed by this plan.

## Observed workunit proof convergence (#1660)

The existing `record_proof_result` pass row is a declaration: it initializes both
proof epochs to None, and `ensure_verified_ready` currently checks only pass rows.
Durable execution binding and referenced-byte hashes preserve custody/integrity
but do not authenticate who observed a successful check. Whole publication has
additional gates, so misleading VERIFIED state is not by itself a proven forge
merge bypass. Preserve that distinction in real binary adversarial controls.

Converge VERIFIED with the existing canonical `validation_history` producer.
A current canonical observation must bind this exact task, workunit contract,
source, accepted target, policy and tool closure. Add only bounded references and
read-only applicability inputs to existing proof records, not a second datastore
or evaluator. Runtime terminal judgment must consume actual completed checks;
opaque additional gates remain unproven. Legacy declarations retain history and
require fresh observed proof, with explicit safe downgrade/recovery. No new
cryptographic trust root or OS-isolation claim is inferred from shared UID access.
No local tests run; supported real CLI/runtime and hosted proof remain required.

## Mixed-store recovery (#1657)

Provide explicit, versioned and idempotent recovery for a nonempty legacy TODO
store beside valid canonical task-journal state. Reuse datastore lifecycle,
maintenance authority and journal verification. Preserve original files, task
identities, chronology, custody/proof bindings and unknown outcomes. Diagnose
equivalence, divergence and corruption independently; do not implicitly merge
contradictory state, delete legacy input, reconstruct missing proof, or replay an
unknown mutation. Ordinary operation resumes only after an inspectable safe
recovery result. The empty-stub path is a bounded first repair; general nonempty reconciliation
remains unimplemented. Hosted regressions must exercise real store and recovery boundaries.

## Storage convergence follow-through (#1630)

Audit the current merged compact journal/event/component and bounded inspection
implementation before adding representation changes. Preserve canonical logical
hashes, custody, snapshot-consistent continuation, historical oversized values
and explicit physical-maintenance authority. Use supported native read-only
inspection for the Decapod, Dactyl and Propodus dogfood stores. Publish aggregate
measurements with binary/schema/codec identity and sampled/complete coverage;
these observations are not before/after savings or hosted PostgreSQL proof.

Hosted tests must cover any repaired inspection or reconstruction boundary.
Existing historical fixture benchmarks retain their original release/environment
scope. Cross-backend, scale, migration and deployment requirements remain open
until supported executable evidence establishes them. No live store repair or
physical compaction, policy weakening, local CI or payload publication is part of
the measurement pass.

## Accepted local verification requirements (#1658)

Decapod must preserve the repository-local verification contract already accepted
on the target. A candidate may add required checks, but replacing a command,
arguments, measured toolchain, environment, timeout or coverage declaration must
not become authoritative merely because the candidate recomputes a review digest.
`repository_profile` owns this preservation check; `validation_runtime` must apply
it before selecting an execution provider. A previously accepted strict profile
continues to require its existing external assurance.

The dedicated change restores the preserved repair against v0.144.3 and adds
real-process consumer regressions for repinning, additive checks and stale source.
Initial adoption remains supported. Requirement replacement needs a separately
accepted target-policy transition; this work introduces no candidate-side override
or new authority store. Hosted tests and exact-candidate validation remain pending
until their actual observations are collected; no local test result is claimed.

## Versioned mixed TODO-store recovery (#1657)

Add a supported explicit review/reconciliation path that can run despite the
mixed-store migration guard. Preserve canonical journal, both source histories,
ownership, proof bindings and unknown operation outcomes. Retire legacy input
only when the kernel independently proves that its authority is already present
in canonical state. Never replay a guessed task mutation or manufacture missing
history. Corrupt, changed, unsupported or contradictory inputs remain blocked.

Use existing physical coordination, task-journal verification, bounded evidence
and authoritative project policy. Local cooperative role labels remain distinct
from authenticated cloud authority and OS isolation. Recovery must be versioned,
inspectable and safely resumable after interruption. This work does not grant
historical exceptions, cloud maintenance rights or verified task completion.
Hosted checks provide execution evidence; no local tests or Buildkite runs are
part of authoring, and full native publication readiness remains unproven.

## Nix publication after squash releases (#1626)

Preserve Decapod's governed engineering and independent publication boundaries
while repairing Nix packaging eligibility. Both existing Nix gates must recognize
protected-master squash/linear release commits by their own release subject and
ordinary two-parent release merges by the second parent's release subject.
Unknown/root/octopus shapes and non-release subjects must not publish. Keep the
native GitHub-hosted ownership, protected ref/event restrictions, credential-free
selector, locked packaging proof and separate Cachix publisher unchanged.

Use actual Git histories to test the workflow scripts through Rust policy tests.
Hosted-only verification is required; no local tests, Buildkite queries or
automatic merging. This change does not claim that a historical missing Nix cache
entry has been populated: actual native packaging and authenticated publication
remain separate observations after acceptance.

## Canonical local-target proof (#1069)

Extend the existing repository-local verifier so an explicitly accepted local contract can read the canonical repository's accepted Git branch without a forge. The normal CLI must execute its reviewed checks through the sealed host and preserve exact source, accepted base, integration, tools, policy, and task custody. Candidate-only target opt-in and strict-profile or cloud fallback must fail closed. Local shared-user authority and separate remote publication remain explicit limits; the complete ten-worker A–H demonstration is still unproven. Hosted regression execution, rather than local test runs, will establish this slice's evidence.

## Decapod Labs media and documentation stewardship (#1707)

The independent Decapod Labs website owns the canonical visual identity and
original website marketing-media renderer. The Decapod kernel repository carries
only reviewed, generated copies of the approved mascot (SVG/PNG), animated
README GIF, product MP4 and poster. It must not add a marketing rendering
workflow to the kernel's source-CI workflow inventory.

The kernel's mdBook documentation is a separate technical product surface,
published at `https://decapodlabs.github.io/decapod/`. Its landing page uses
the approved Decapod Labs wordmark, official kawaii mascot, coral/cream visual
palette and the existing product video, with a direct link to the company
homepage. The technical manuals remain technical and preserve precise
definitions of scope, authority, proof and unsupported guarantees.

The landing page must not claim an unconditional sandbox, semantic correctness
oracle or hosted-cloud acceptance. No docs theme change, image, video or
successful media build satisfies Decapod's native validation/readiness gates.

## Documentation identity across the technical manual (PR #1741)

The documentation shell must present the canonical Decapod Labs identity on
all mdBook pages, in addition to the existing introduction content. The
persistent sidebar brand area uses the website-owned `assets/logo-a.png`
wordmark (dark Decapod text with strong-orange Labs text) beside the official
crab mascot when the light theme is selected. Dark mdBook themes substitute
the website-owned `assets/logo-c.png` wordmark for contrast, retaining the
same mascot. The landing-page company link uses `logo-c.png` and remains
accessible by its Decapod Labs name. The docs favicon remains the reviewed
mascot icon.

The public asset paths belong to `DecapodLabs/decapodlabs.github.io` and are
referenced by the technical book without duplicating or regenerating brand
artwork inside the kernel. Both palette variants must remain visually legible
at narrow viewport widths. A missing remote image must not prevent access to
the manual, its links, search, or code examples. Branding changes do not modify
authority, readiness, proof guarantees, or the scope of documented commands.

Acceptance requires inspecting the generated sidebar and landing page in both
light and dark palettes, confirming the correct asset references and accessible
names, and independently checking the publicly served Pages result. Passing a
source build alone does not establish that the public deployment occurred.

## Rust-native public documentation shell redesign

The public documentation remains generated with the reviewed mdBook v0.4.40
Rust binary and the canonical Markdown in `docs/book/src/`. Replace the default
mdBook visual shell through its supported `docs/book/theme/index.hbs` template
and project CSS without introducing an Astro, Node, npm, or external application
build system. Preserve the existing chapter filenames, document links,
search implementation, theme selection, print output and keyboard navigation.

Create a distinct Decapod Labs interface with a full-height navigational
chapter rail, branded sticky top navigation, roomy editorial reading surface,
contextual quick-reference rail at wide viewports, and a contrasting company
footer. The official mascot appears in the sidebar and reading rail. The
approved `logo-a.png` serves light navigation, `logo-c.png` serves dark
navigation, and `logo-b.png` serves the dark footer. Warm cream, the light
coral `#F19C76`, vivid orange `#F05B37`, restrained plum, generous rounded
surfaces and tactile card treatment form the visual identity. Maintain proper
focus indicators, legible code and tables, narrow-screen sidebar behavior,
contrast and reduced-motion preferences. The original product film remains
on the landing page without introducing autoplay with audible sound.

The browser presentation is explanatory documentation for humans and external
readers; the installed Rust binary retains authority over agent-facing,
versioned operational instructions. Theme assets and layout never authorize
changes to scope, governance policy, completion or evidence. Success requires
an actual mdBook build, structural checks on the rendered navigation and links,
responsive review and independent observation of the published Pages site. A
source build alone does not establish publication.

The report's manifest and managed Docker pin must be generated by the actual
0.148.7 evaluating binary. An earlier CI-local pass with 0.148.6 projections was
not full readiness. Only supported CLI regeneration repairs that metadata;
the current candidate still requires complete fresh proof before publication.
