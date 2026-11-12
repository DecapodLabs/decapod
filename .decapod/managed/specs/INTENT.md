# Intent

## One human setup, agent-owned connection

The human runs `decapod init` once per repository and then chats with their agent.
The agent owns every later Decapod command, cloud connection, browser handoff,
resume and refresh. It asks the human only to complete an actual browser sign-in
or account action. `src/decapod/core/cloud_session.rs` keeps pending and managed
sessions outside Git, keyed by backend origin and canonical repository. Successful
sign-in is usable only after the backend confirms repository access. Local mode
and explicit Neon compatibility remain supported; no cloud failure creates local
replacement task state. Hosted OAuth, account entitlement and deployment require
separate service evidence.

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

- Repository signal fingerprint: `aed633e1d2011448d4152c0bfd2c9f7d223ed048eee9578e191b3a397be951cd`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (38 files), `docs/` (1 files), `src/` (226 files), `tests/` (224 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->

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

## Cloud repository purchase approval

Cloud init carries repository identity into generated context, collects the
requested GitHub roster and presents the hosted purchase-approval handoff.
The preserved commercial contract is USD15 per named user per repository
monthly, with payment approval owned by a verified repository owner/admin.
The non-secret config records requested intent; it cannot establish a paid
seat, a cloud credential or datastore authorization. Local use remains
available without login or purchase.

This recovery preserves the previously authored CLI contribution from
`26a499d9f0789a5ed00605d23cfc57f5392ae8fc` on the current accepted source.
Hosted billing deployment, sandbox purchase acceptance and released native
client credential integration remain separate rollout requirements. A printed
approval link alone cannot establish their completion.
