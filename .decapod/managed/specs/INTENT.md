# Intent

## Product Outcome

Decapod keeps AI coding work tied to the human request, the repository's rules,
and evidence of what was actually checked. Its product outcome is work that
people can inspect, agents can continue, and the project can accept through
explicit governance gates.

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

Obligation completion must remain true when inspected from a fresh session:
current transitive dependencies, proof outcomes, proof freshness, and a nonempty
state-commit root determine readiness. Cached status is advisory. Verification
must expose missing or corrupt evidence as a failed outcome. Publication checks
explicit completion attempts and their dependencies without requiring unrelated
unattempted backlog to be complete (`core::obligation`, `core::validate`).

### Local operation with explicit integrations

Decapod runs on demand without a daemon and uses local repository state by
default. Cloud storage, container execution, remote validation, and advisory
decision providers have separate access and proof requirements. They extend
particular boundaries of the product without owning its intent or governance.

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

- Repository signal fingerprint: `20cb0a82971ddc723a3d45c985bf1ce38591b45ac1481c29bbd1541386d08c64`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (22 files), `docs/` (1 files), `src/` (188 files), `tests/` (193 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->
