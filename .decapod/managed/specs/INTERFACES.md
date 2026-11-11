# Interfaces

## Agent delegation and cloud authorization interface

Decapod is invoked by each contributor's coding agent in the repository.
Local `DECAPOD_AGENT_ID` labels and `[governance.rbac]` describe cooperative
identity and exceptional local grants only. Cloud `todo` calls use a
managed-Auth bearer, canonical repository binding, and backend-owned checks.
Client role labels, task-assignee strings and `actor_roles` cannot substitute
for that bearer or select a cloud privilege. The same agent command interface
serves owners, maintainers, contributors and contractors; no contractor-specific
human command is added.

Propodus alone resolves the effective `read`, `write`, `maintainer`, or
`owner` tier and enforces expiring direct user grants (including explicit
revocation over inherited group/team/org grants). Cloud claim/release/complete
must bind the acting authenticated principal independently from the
caller-supplied agent label. The protected two-principal service proof checks
both ordinary lifecycle concurrency and label impersonation. HTTP fixtures
show client composition only; local mode never falls back from a failed cloud
authorization. Merge readiness remains governed by the independently
configured publication and proof gates.

## Version reporting

`decapod system version` reports the installed binary version and existing
update guidance without reading configuration, acquiring a session, initializing
a datastore or migrating project state. It works in uninitialized, damaged and
unsafe-permission checkouts. Stateful commands retain storage permission
rejection and the first normal command still reports a pending migration.

## Publication projection

`interop.project` validation reports expose `proof_judgment` alongside
`publication_ready`. Eligibility comes from `project_report_publication`.
A report that only sets `publication_ready` or `passes` stays ineligible.
A fresh pass with a non-empty head and a receipt digest can be `proven`.
Stale rules or verifier identity, a failed status, missing canonical state,
and a corrupt status stay distinct and are not publishable.

`state commit verify` requires the scope-record bytes and an expected root.
Omitting the root is an error, not a successful verification.

`state.resolve` remains the canonical snapshot. `judge_stored_verification`
re-hashes the files named by a verification row and binds that digest to the
snapshot. The row's status field is not accepted as proof. Governed-transition
decisions are appended to the `traces` stream that `decapod flight-recorder`
already renders. When the transition is not admissible and proven, that payload
includes `blocker_class` and `required_capability`. Those fields describe the
blocker. They are not an instruction to the harness.

`judge_completion_with_verifier` is the completion judgment plus an optional
list of isolation axes. An empty list preserves `judge_completion` and ignores
the envelope. `judge_verifier_integrity` checks each required axis against that
axis's evidence reference and checks `canonical_input_hash` against the
completion `inputs_hash`. `attest_capability` keeps observed support only when
the attestation carries a usable evidence reference and a current identity.
`admit_operation_surface` answers whether an operation class may write the
given paths. None of these functions is a new RPC, and none selects work. A
missing blocker class means the cause is unclassified. It does not mean
`state`.

## Claim scope acceptance

`todo add --scope 'paths:["src/a.rs"]'` declares the files an exclusive claim
coordinates. Distinct callers can claim disjoint files in the same canonical
repository; logical source overlap is coordination metadata rather than an authoring lock. Task custody and observed writable-checkout custody remain exclusive. The real CLI
regressions check both claim acknowledgements and subsequent `todo get` data.
Shared-claim acknowledgement preserves the existing primary assignment and
adds only secondary membership. Enabling the option solely in a worktree
must not affect the canonical datastore policy.

## Recovery replay comparison

`qa verify recover` and per-task verification use the same normalized validation
report. Fresh execution paths and receipt IDs do not constitute material drift;
changed scope, policy, artifact content, epoch, target or verdict still does.
Persisted output hashes remain SHA-256 values. Baselines captured with the old
comparison can require one explicit supported recovery to refresh them. No
command flag bypasses proof, and atomic batch finalization is unchanged.

## Exact consumer source spelling

The `agent.init` reconciliation result must contain only `Justfile` when that
is the sole tracked spelling, even when APFS resolves `justfile` to the same
inode. The Rust CLI regression checks `discovered_sources` against the Git
index and retains `proof_status=not_run` and `merge_ready=false`: initialization
does not certify native readiness. No CLI flags or response schemas change.

## Optional basic-trust secondary claims

`[governance].allow_basic_shared_claims` is false by default. When enabled in
the main repository config, local shared claims by basic agents can join an
existing different primary owner. The operation records the config fingerprint
and asserted labels atomically with the ownership event. It does not grant
verified trust, authenticate an identity, authorize primary claims or waive
other gates. This option does not apply to cloud-backed claims.

## Historical verification recovery acceptance

## Local broker caller protocol

The internal Unix-socket broker request in
`src/decapod/core/group_broker.rs` uses protocol version 3. Alongside
request ID, arguments and an absolute queue/execution deadline, each request carries a canonical absolute working
directory and an explicitly allowlisted caller context. Agent identity, session,
task, correlation, causation, idempotency and claim-autorun inputs replace—not
merge with—the elected leader's corresponding environment. The child executable
is resolved independently of either working directory.

This internal protocol is not a new public task ownership or authorization API.
The ordinary CLI/session/policy checks still execute; request fields cannot grant
trust or approval. Missing or unrecognized caller fields fail validation rather
than inheriting the leader identity. The existing restricted local socket alone
transports the ephemeral session password; durable dedupe stores no credential.
Structured replay binding includes arguments, actor, working directory and
non-secret context, but excludes password rotation. Protocol mismatch fails
closed; no persistent daemon or alternate IPC transport is introduced.

The regression in `tests/group_broker.rs` requires two concurrent claim callers
to traverse the same elected leader and checks both returned and stored owners.
Unit coverage in `tests/unit/core/group_broker_tests.rs` checks replay separation,
rejected arbitrary environment keys and removal of absent leader credentials.
Current full publication proof remains required; these declarations are not a
substitute for the controller's authenticated test result.

## Native readiness completion artifacts

## Interface Overview

Humans supply intent, review results and authorize merge/release. Agents perform
work and invoke Decapod through finite CLI/RPC operations at planning, custody,
validation and publication boundaries. Repository state preserves continuity.
The public Buildkite reader admits only its source-bound artifact routes and
verified producer/content identities; diagnostics do not create execution proof.

Optional Laya input uses `validation-results.v3` through the shared production
projector. Intent, proof hooks, unresolved questions, contradictions, stop
conditions, phase gates and actual observations remain distinct. The complete
input must fit its existing 32 KiB and declared model bounds; inputs are never
selectively clipped to obtain an approval. Private replay validates deterministic
evidence before using that same builder but establishes no live custody.

`readiness_assessment::permits_decision` retains failed-proof dominance and
requires an affirmative, nonabstaining decision in required mode. Live callers
also establish freshness, accepted policy, runtime identity and existing human
authority. `ValidationReport` contains only the optional `laya_readiness` model
field; inactive evaluator aliases and compatibility DTOs are not public API.

## Reviewed native sequence interface

`native_proof::Invocation.following` is an ordered vector of explicit native
programs, arguments, working directories and environments, not a deserializable
command authority. It is omitted from serialization when empty, preserving
existing single-command invocation digests. The complete check is bounded to
64 commands. Every successor undergoes the same executable, argument, environment,
platform and recursion checks as the first command. Artifacts are verified only
after all commands succeed in their shared owned scratch directory.
`CheckDiagnostic.steps` retains each attempted step's exit/termination, duration,
output hashes and preparation/spawn failure. A changed later tool preserves
earlier execution diagnostics while making the check fail. These observations
remain execution evidence; they neither select the required graph nor override
missing platform or policy obligations.

`toolchain_digest_bounded` accepts the caller's remaining time and cancellation
while retaining the original 15-second/1-GiB measurement ceiling. Repository
proof seals retain the originating deadline and cancellation token; expired or
cancelled seals cannot authorize another measurement. The CLI subtracts native
discovery time before entering the readiness runtime. Declared invocation limits
remain stable across final re-selection; execution still uses the lesser of
that limit and the original remaining budget.

## Human and agent entrypoints

The separate scientific worker validates its own ephemeral nonce format locally (sha256: plus64lowercase hexadecimal digits). It does not call or expose the main crate's private model-installation digest helper. Candidate4cdcd25d native compilation rejected that cross-crate call with E0603 before any new test or model execution; repair preserves the abstraction boundary and leaves all receipt/acceptance checks intact. A canonical nonce detects malformed bookkeeping, not authenticated global ownership.

### CI-attested execution declaration

`proof.ci_attested.workflows` maps each supported workflow's native step keys to
the compiled required checks. Its optional `pipeline_trigger` declaration names
the finite `buildkite-gha-0.102.7` contract, exact workflow name and, when present,
the supported authored concurrency expression. The configured executor is trusted
to supply one keyless importer; metadata verifies its terminal success and actual
job UUID, not the plugin implementation. Derived concurrency gates and the reserved
bootstrap evidence identity are distinct from required logical validation checks.
Collection reads only build, complete steps and complete jobs metadata. It rejects
failed, retried, missing, duplicate or unexpected jobs and mismatched source/run
identity. Optional configured workflows are collected only when the compiled
catalog requires them. Exact accepted-target policy equality remains mandatory.

The activation PR uses an explicitly approved one-time direct-check and exact-CI
bootstrap review. It cannot report released-controller publication readiness for
its own new policy. Subsequent changes use the accepted policy and released
controller through ordinary validation.

Human setup is `cargo binstall decapod && decapod init` in the project, followed
by a task given to the person's coding agent. The README keeps this path short.
Operational commands belong in the agent contract and agent documentation.

The human-facing README is a brief product overview: its existing visual layout
and section order make capabilities easy to scan, while the introduction
explains agent-owned engineering work. It distinguishes parallel owned tasks
from continuing one pull request in another tool. Validation stays in prose as
an agent responsibility; its optional remote execution applies to that phase,
not to Decapod as a whole. The README does not add operating instructions or
historical proof reports to the single-command setup path.

`CONTRIBUTING.md` gives human contributors the review expectations and project
layout. The agent contract and linked technical references carry the operational
workflow. Contribution guidance assigns build orchestration, verification and
repair to the agent, without requiring a human to expand credential permissions
or run a manual prerequisite suite.

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

Project authority comes from `.decapod/config.toml` and `.decapod/OVERRIDE.md`.
Each registered override directive owns a fenced source body. Resolution
preserves nested Markdown and examples, removes the outer wrapper, and exposes
`directive_id`, `source`, `source_hash`, `body_hash`, `byte_count`, and
`precedence`. Duplicate exact directives, unclosed wrappers, and non-empty
unknown Decapod-namespaced directives invalidate the whole overlay. Empty
retired generated directives remain upgrade-compatible. Partial application
must not conceal ambiguous authority.

## Validation Rules System Interface

Decapod exposes the validation rules system through CLI, RPC, and configuration
interfaces.

### Configuration Interface

The validation rules system is configured through `.decapod/OVERRIDE.md` under a
dedicated `

## VALIDATION_RULES Overrides section. Each rule override uses a
fenced block with the rule ID as heading:

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

Supported override fields:
- `severity`: `advisory`, `warning`, `error`, `critical`
- `applicability`: `required_changed_paths`, `forbidden_changed_paths`, `task_types`, `ref_patterns`, `ci_only`, `local_only`
- `evidence_max_age_seconds`: Maximum age of evidence in seconds (0 = no limit)
- `requirements`: `required_evidence`, `required_evaluators`, `required_context`
- `remediation`: Override the remediation contract

Override eligibility is enforced per-rule:
- `NonOverridable`: Cannot be overridden (kernel invariants)
- `TightenOnly`: Can only increase severity or add constraints
- `ConfigurableOnly`: Can configure parameters but not disable
- `FullyOverridable`: Full override permitted

Invalid overrides (unknown rule IDs, duplicate definitions, malformed content,
unauthorized weakening of kernel invariants) cause validation to fail closed.

### RPC Interface

The validation rules system exposes the following RPC operations:

#### `validation.rules.list`
List all registered validation rules with their effective configuration.

**Request:**
```json
{"op": "validation.rules.list", "params": {}}
```

**Response:**
```json
{
  "success": true,
  "result": {
    "rules": [
      {
        "id": "decapod.scope.authorized_paths",
        "description": "All changed files must be within authorized scope...",
        "authority": "constitution",
        "severity": "error",
        "evaluator": "decapod.scope.validator",
        "effective_severity": "error",
        "override_eligibility": "tighten_only",
        "applicability": {...},
        "requirements": {...},
        "remediation": {...},
        "provenance": {...}
      }
    ]
  }
}
```

#### `validation.rules.evaluate`
Evaluate all applicable rules against the current context.

**Request:**
```json
{
  "op": "validation.rules.evaluate",
  "params": {
    "context": {
      "changed_paths": ["src/main.rs", "tests/test.rs"],
      "task_type": "feature",
      "git_ref": "refs/heads/feature/xyz",
      "is_ci": false,
      "evidence_artifacts": {
        "governed_plan": "sha256:...",
        "workspace_status": "sha256:..."
      },
      "validation_epoch": {...},
      "policy_fingerprint": "sha256:..."
    }
  }
}
```

**Response:**
```json
{
  "success": true,
  "result": {
    "rules_report": {
      "results": [
        {
          "rule_id": "decapod.scope.authorized_paths",
          "verdict": "pass",
          "severity": "error",
          "evaluated_at": "2026-11-05T18:23:31Z",
          "evidence_refs": ["governed_plan", "workspace_status"],
          "message": "Rule passed",
          "remediation": null,
          "policy_fingerprint": "sha256:...",
          "validation_epoch": {...}
        }
      ],
      "passed": 7,
      "failed": 0,
      "unverifiable": 0,
      "overall_verdict": "pass",
      "has_blocking_failures": false,
      "has_unverifiable_mandatory": false
    }
  }
}
```

#### `validation.rules.get`
Get a specific rule by ID with its effective configuration.

**Request:**
```json
{"op": "validation.rules.get", "params": {"rule_id": "decapod.scope.authorized_paths"}}
```

**Response:**
```json
{
  "success": true,
  "result": {
    "rule": {...},
    "override_values": null,
    "effective_severity": "error",
    "effective_applicability": {...}
  }
}
```

### CLI Interface

The validation rules are evaluated as part of `decapod validate`:

```bash
decapod validate --format json
```

The validation output includes a `rules_report` section in the JSON output:

```json
{
  "status": "failed",
  "rules_report": {
    "results": [...],
    "passed": 6,
    "failed": 1,
    "unverifiable": 0,
    "overall_verdict": "fail",
    "has_blocking_failures": true,
    "has_unverifiable_mandatory": false
  }
}
```

Verbose output (`-v`) includes per-rule details with remediation contracts.

### OVERRIDE.md Integration

Project-specific rule overrides are declared in `.decapod/OVERRIDE.md` under a
dedicated `

## VALIDATION_RULES Overrides section. Each rule override uses a
fenced block with the rule ID as heading:

```markdown

## VALIDATION_RULES Overrides

### decapod.scope.authorized_paths
````markdown
severity: error
applicability:
  required_changed_paths: ["src/**"]
  forbidden_changed_paths: ["**/secrets/**"]
evidence_max_age_seconds: 43200
````
```

The OVERRIDE.md parser validates:
- Known rule IDs only (unknown IDs fail closed)
- No duplicate rule definitions
- Well-formed TOML within fenced blocks
- Override eligibility respected (NonOverridable, TightenOnly enforced)

### Applicability and Context Awareness

Rules declare applicability conditions. The registry only evaluates applicable
rules based on:
- Changed paths (required/forbidden patterns)
- Task type (feature, bugfix, refactor, docs, test)
- Git ref patterns
- CI vs local execution context

Custody rules (`task_ownership`, `workspace_isolation`) only apply to governed
agent workspaces with active task claims. They do not apply to:
- Detached/post-merge validation contexts
- Consumer repository validation
- Release/post-merge validation contexts
- Fixture/test validation contexts

### Remediation Contract

Every blocking rule result (FAIL or mandatory UNVERIFIABLE) includes a structured
remediation contract:

```json
{
  "error_code": "SCOPE_VIOLATION",
  "violation_description": "Changed files exceed authorized scope",
  "expected_condition": "All changed files within declared scope",
  "observed_condition": "Files outside authorized scope modified",
  "affected_paths": ["src/secret.rs"],
  "required_steps": [
    "Revert changes to files outside authorized scope",
    "Request human approval for scope expansion"
  ],
  "autonomous_remediation_permitted": true,
  "human_authorization_required": false,
  "evidence_for_revalidation": ["governed_plan"],
  "related_rules": ["decapod.scope.file_touch_budget"]
}
```

Agents must follow the remediation contract and revalidate. Kernel invariants
cannot be waived by candidate-controlled policy changes.

## Current-evidence recovery for historical completion claims

`plugins::verify::VerifyCommand::Recover` accepts repeated `--artifact TODO_ID=FILE`
inputs when an unverified completed task lacks a usable file baseline. Inputs
must name tasks in the recovery batch and readable current files; no implicit
AGENTS.md substitute or historical CI receipt is imported by this batch path.
`prepare_recovery_captures` collects file hashes and the active validation epoch
before validation, rejecting missing per-task inputs and additional proof
obligations that cannot be established by the validation-only capture contract.
Relative artifact paths use the same task-aware lookup during capture and replay:
the invoking checkout takes precedence, then the owning task workspace is
consulted. Replay still requires the stored content hash to match; a missing or
changed artifact is not accepted as proof.

`refresh_batch_baselines_with_captures` requires a genuinely passing current
snapshot, checks the original task and proof projection under the write
transaction, and rechecks captured file bytes and epoch before inserting pending
proof. It preserves completed task status, timestamp and revision, and records
previous proof fields plus the explicit fresh-current-material provenance in the
append-only recovery event. `finalize_recovered_batch` remains the sole batch
promotion boundary after final validation and every task's artifact checks.
A pending or failed capture is not verified completion; a newer task, baseline,
file or epoch invalidates the attempt. Existing proof plans containing additional
obligations are not downgraded by supplying new files.

The required regression surface in `tests/unit/plugins/verify_completion_tests.rs`
includes multiple completed tasks without proof rows, missing/foreign artifact
inputs, extra proof obligations, failed snapshots, changed files, concurrent task
revision changes, pending final failure, retained completion history and atomic
successful promotion. These are authored tests until exact-candidate validation
executes them; source review alone is not passing proof.

The candidate-binary regression in `tests/verify_deadlock_regression.rs` also
invokes the real recovery CLI for two completed tasks lacking proof rows. With
explicit current artifacts, an independently unapproved plan still fails actual
child validation; recovery must preserve task status, completion timestamp and
revision without creating evidence or pruning either task. This failure-path
coverage does not establish successful full-native CLI recovery or publication
readiness. The Tokio lock repair changes only the runtime package version and
checksum, not this interface or its evidence-admission obligations.

Recovery guidance is an agent/harness error contract, not a human command
checklist. Epoch drift and aggregate missing-proof errors direct the agent to
non-reopening batch recovery with explicitly selected task-bound current files.
A failed scoped child validation emits `VALIDATION_CHILD_FAILED` with only a
fixed allowlist of blocker labels from at most 1 MiB per output stream, including
`NEEDS_PLAN_APPROVAL`; this retains the actual unrelated blocker even when a
separate ordinary validation encounters aggregate missing proof first.
Unknown failures provide the supported `decapod validate --format json`
diagnostic and original-command retry route. Raw child messages, paths, URLs,
environment values and credentials are never forwarded by this diagnostic.
Labels are diagnostic observations, never acceptance or proof. Initial/final
recovery failures preserve historical/unverified or pending states respectively.
Humans may initialize Decapod; execution and recovery instructions target agents.

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

Captured task verification artifacts include `task_id` and `task_revision`.
A passing legacy record without this binding, a malformed binding, or a different
task/revision projects `validation_status: stale`; a done task then projects
`status: blocked`. Dependency admission and completion reject that evidence.
Proof references remain historical references rather than renewed permission.
`qa diagnose` recommends `decapod qa verify regen <id>` for stale binding;
`qa verify --stale` includes revision drift as well as age. The `--stale` flag
is global and applies to every `qa verify` subcommand, including
`qa verify recover --stale`: it lists stale done tasks (with
`last_verified_at` and `verification_policy_days`) and returns without
running any child validation (#1527). Explicit recapture
runs real validation before binding new evidence. Batch recovery first leaves
its refreshed evidence `CLAIMED` and requires per-task and aggregate proof.

`STALE_TASK_VERIFICATION` reports a task or verification baseline changed while
verification was running. The stale result is discarded without replacing the
newer evidence. Inspect the current task and rerun verification or recapture.
Strict revision binding also invalidates proof after custody changes such as
release or owner edits; comments alone do not change task revision.

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

The explicit local transfer commands preserve an original standalone task:
`todo transfer export --id <id> --to <receiver> --output <absolute-private-file>`
requires an actual yield by the source actor and seals source mutation.
`transfer recover --id <id> --transfer-id <id> --output <new-private-file>`
recovers identical bytes after interrupted publication. Recovery selects the
immutable export event by task and transfer identity instead of recapturing the
entire subject journal. The canonical event read enforces its existing 8 MiB
record bound before payload materialization, and recovery revalidates the
repository, exporter, task, transfer, envelope and artifact digest bindings.
Successful exports at the 4,095/4,096-event boundary remain recoverable after
the export/checkpoint append and after later comments. This does not relax the
4 MiB artifact or 4,096-event historical-bundle limits.
`transfer import --input <absolute-private-file> --expected-digest <sha256:digest>
--accept-source-transfer` requires a trusted source handoff, the same GitHub
origin and intended receiver, fetched source commit and absent local authority.
The `decapod.task-transfer.v1` artifact is bounded to 4 MiB and 4,096 subject
events. Sessions and unrelated tasks are excluded. Export supports only yielded
tasks without shared owners, parent/dependency edges or incoming relationships.
Import returns `execution_ready: false`; it neither claims the task nor imports
current-machine proof. `TODO_PROJECTION_MISSING` remains blocking until the real
import, claim and applicable execution gates succeed.
The source seal assumes all cooperating writers use a supporting controller.
An older executable, privileged database writer or disconnected copy can ignore
that fence; the artifact does not establish an online exclusive-ownership lease.

Cleanup requires ownership evidence. Task-like names, old timestamps, and an
empty inventory from a different container engine do not establish permission
to delete. Managed invocation receipts bind the exact path, directory generation,
lease, and creating runtime; uncertain resources are preserved for review.
Workspace/control subprocesses have a 15-second deadline and image builds a
600-second deadline, with bounded output and owned-child termination/reaping.
A timeout does not prove that a task, branch, or container is absent.

`container run` reports `workspace_cleanup` in successful summaries: `removed`,
`preserved_by_request`, `preserved_unsynced_branch`, `preserved_unexpected_head`,
`preserved_hidden_index`, `preserved_unreturned_refs`, or
`preserved_dirty_workspace`. Failed executions retain their failure status and
workspace path; runtime termination retains files after invocation-owned runtime
cleanup. `--keep-worktree` adds an unconditional retention request. Git inspection
errors report `container_workspace_status_unavailable`; finalization failures
report `container_workspace_finalization_failed` with the underlying diagnostic
and retained path. Foldback rejects divergent host history without forcing it.

`--push` publishes the returned branch only after successful execution. `--pr`
uses the common governed workspace publisher on the clone, including full
readiness and verified draft-PR creation/reuse; its result appears as
`publication` in the summary. Combining `--push --pr` does not cause a separate
unvalidated push. Publication precedes final foldback because the publisher may
commit checkpointed material. Publication failure retains the clone; repair the
reported gate, then run `decapod validate` and `decapod workspace publish` there.
A failed command never invokes either publication action.

`--local-only` cannot combine with `--push` or `--pr`; the shared boundary rejects
that conflict before starting a runtime or creating a clone. Omit the publication
flags to retain local work, or omit `--local-only` to return the actual candidate.
An explicit `--pr-base` differing from the configured governed publication target
returns `container_publication_base_mismatch`. Align the intended configuration,
review the accepted target and revalidate from the retained workspace rather
than silently publishing against another base.

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
An existing checkpoint ID is immutable within the active PR. The checkpoint
mutation returns `GOVERNANCE_CHECKPOINT_ID_CONFLICT` without saving if its summary,
normalized proof references, or current material digest differ. An exact retry
returns the current document without a write, including after committing the
original checkpoint. Changed material requires a new ID; historical verification
remains an independent publication gate.
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
store. Local physical connection factories hold shared lifecycle guards;
maintenance takes exclusive access, and contention reports `STORAGE_LOCK_TIMEOUT`.
An attempted shared-to-exclusive upgrade reports `STORAGE_LOCK_UPGRADE_BLOCKED`.
Locks are descriptor-owned and
released by the operating system, never deleted based on age. This coordinates
cooperating Decapod processes, not arbitrary external SQLite clients.

State and its canonical event commit together at the local transaction boundary.
Event replay with the same identity and content is idempotent; divergent content
fails with `EVENT_ID_CONFLICT`. Canonical streams and projections live in the
datastore. Legacy JSONL and split databases are migration inputs, not parallel
runtime authority. Migration is lossless, bounded, and idempotent; conflicting or
malformed evidence blocks rather than being silently discarded.

Runtime continuity is exposed through the existing domain commands. Task journal
commands require the local backend; cloud selection fails closed without a local
fallback. Workunit mutations require an initialized local runtime; missing or
partial policy tables fail before journal mutation. Verification returns a nonzero
exit status for projection conflict:

- `todo journal checkpoint` captures an explicit current-state baseline;
  `todo journal verify` checks the journal without restoring state;
  `todo journal restore --expected-current-digest <digest>` requires the observed
  live projection precondition and refuses divergent authority
- `todo journal checkpoint --accept-historical-gap <task-id> --accept-historical-gap <other-id> --reason "<reviewed rationale>"` accepts multiple reviewed historical uncheckpointed suffixes (#1534, #1581). Every selected task must have a checkpoint head and a real suffix. Duplicate IDs, unrelated gaps and unrelated divergent projections block the operation before mutation. One broker transaction records a `task.journal.gap_accepted` event and a new transition checkpoint for every accepted task; on failure nothing commits. Historical events remain intact; the reviewed reason and `todo.journal.repair` permission in the main repository's `[governance.rbac]` are mandatory.
- `validate --bypass-validation --bypass-reason "<reviewed exception>"` requires a separate `validate.bypass` role, records `validation.bypassed` with `proof_ready=false` and `publication_ready=false`, and never satisfies QA replay, CI, or publication. Ordinary `validate` still executes mandatory checks. `[laya] mode = "disabled"` is the default, with required mode acting only after deterministic proof.
- `govern workunit checkpoint --task-id <id> --operation-id <id>` records a
  versioned checkpoint; optional `--expected-generation` fences a known head
- `govern workunit journal-status --task-id <id>` reports integrity and projection
  consistency; `recover` restores exact journaled manifest bytes
- `govern workunit resume --task-id <id> --operation-id <id>` verifies current
  execution authority before recording continuation; it does not run work or
  reissue proof, and accepts the same generation precondition as checkpoint
- `govern trajectory checkpoint --run-id <id> --checkpoint-id <id>` captures
  current source, task custody and policy, with repeated `--approval-id` references
- `govern trajectory verify-resume --run-id <id> --checkpoint-id <id>` checks
  continuation without changing the recorded checkpoint or granting approvals

Operation identifiers bind immutable requests. Identical replay is idempotent;
a changed request must use a new identifier and a valid predecessor. Missing,
unsupported, corrupt or contradictory history is a typed blocker. A migration
baseline states that earlier history is incomplete. Neither projection recovery
nor a baseline grants a lease, approval, successful check, or current proof.

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

Readiness reports expose `agent_guidance` for the validation phase: agent
ownership, the plain validation entry command, the next action, and the
conditions that require human input. Reconciliation repair tasks expose `owner`
and `resume_command`. These fields steer the agent; they do not change the
readiness verdict, grant authorization, or replace execution evidence. Missing
build/test configuration and unsupported adapters remain explicit blockers.

For supported remote execution, validation captures an accepted executor R
independently from candidate source H. Versioned preparation and execution
custody bind R’s revision, tree and profile closure separately from H’s actual
execution revision and tree. The required graph and final verdict remain in the
installed controller. Authenticated artifact-only workers inherit the measured
R executor through their verified preparation dependency.

`CONTROLLER_CONTRACT_UNSUPPORTED` distinguishes a reviewed inventory from a
carrier, protocol or accepted executor the running controller cannot admit. Its
`next_action` is `obtain_supported_controller`, and readiness remains false.
Repeating validation with that controller, changing the declaration or building
manually cannot repair this incompatibility. The current runtime does not
bootstrap a replacement controller or accept a candidate's own readiness claim.

`proof.validation_buildkite_preflights=false` selects native execution; `true`
selects the optional Buildkite adapter for the same required profile. Unknown
profiles, unavailable adapters, and unreconciled source contracts fail explicitly.
The remote route pushes or reuses the exact existing workspace branch, executes
its project checks remotely, and verifies authenticated build, attempt, artifact,
and final source/target identities. It does not run duplicate local prerequisites,
create a temporary branch, or treat GitHub Actions results as Buildkite proof.
Configured validation does not ask the human for a separate local approval to
push that checked work branch. The bounded validation push authorization does
not grant permission for unrelated Git writes, merges, releases or deployments.
Existing authorized provider access is required; denial does not authorize
credential installation or fallback.

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

The CI context action exposes a required boolean `no_change` disposition. It is
true only after an explicit created push with an exact zero before value names
an ordinary branch whose head equals the captured accepted target. Missing or
malformed event data, unavailable base history, dirty or hidden source, and
unrelated heads fail verification. Consumers must not infer this disposition
from a release-looking branch name or equal caller-supplied IDs. Constitution
schema validation runs for both dispositions; only real ranges use candidate
custody. Full validation and measured receipt APIs still reject equal endpoints.

## Obligation and HealthEngine verification

`obligation verify --id` prints its structured current derivation and returns a
nonzero exit when the obligation is unmet. `obligation validate-graph` prints the
whole graph result and exits nonzero for any unmet node, cycle, or missing
endpoint. `dependency_errors` exposes dangling edges; `total_edges` counts raw
stored edges. Malformed requirement arrays are errors, never empty proof lists.
`get` and `list` retain the stored status field for compatibility; that projection
cannot authorize readiness.

`obligation complete --id --commit` records a completion attempt using a nonempty
root and prints current readiness. It does not assert `Met`; missing IDs or empty
roots fail. Full validation rechecks recorded roots and legacy `met` projections,
including their transitive evidence, while unattempted open obligations remain
backlog. A public whole-graph check intentionally asks about every node.

Health claim references resolve an exact ID first, then a unique subject alias.
Duplicate subjects return `HEALTH_CLAIM_AMBIGUOUS`; a storage failure is not
reported as a missing claim. `govern health get` may refresh the advisory cache,
but obligation verification does not depend on that refresh. These behaviors are
implemented in `core/obligation.rs` and `plugins/health.rs`.

## Outbound Dependencies

Physical SQL execution remains behind Dactyl. Optional cloud storage uses the
selected Propodus/Supabase contract with server-derived authorization and no
fallback to local authority. Git supplies repository identities and versioned
artifacts. External executors supply source-bound proof only under the configured
provider policy; an unavailable provider remains an explicit blocker.

Laya model assets are immutable, pinned external artifacts. Explicit setup
verifies the private cache and optionally registers a native runtime and CPU
companion. Merely enabling `--laya-decisions` does not download assets or prove
that inference works. Required Laya policy and installation bindings are checked
again before final readiness. Runtime-registration version 2 requires explicit
re-registration of older local bindings, preserving files on refusal.

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

## Local storage-owner transport (opt-in)

`DECAPOD_STORAGE_OWNER=1` enables the foreground owner for an elected group
broker. Default routing remains unchanged while this transition is validated.
The owner uses a confined local Unix socket derived from the canonical datastore,
with a versioned, bounded-frame protocol. Long paths use a hashed private UID or
explicit shared GID directory under /tmp; the same endpoint serves all worktrees. This is an internal cooperative-local
interface, not a new remote database service or authentication boundary.

An Open frame binds the canonical database path, access mode, and immutable
caller metadata to one lease. Session passwords are excluded. SQL parameters
retain their original types, including integer versus real and blob values.
The owner serializes leases, not individual statements, so BEGIN through
COMMIT/ROLLBACK cannot interleave across callers. Read-only leases are enforced
by the server and cannot use Read messages to disable query-only protection.
Temporary objects are removed at lease release, and connection configuration,
attachment, connection-local result counters and unsupported controls are rejected.

No operation is replayed after an uncertain transport outcome. Failure before
any owner connection exists may use the existing local path; accepted but
unanswered protocol traffic does not authorize local fallback. Closing or
losing a lease rolls back unfinished work. Normal foreground shutdown sends a
drain signal while retaining its lifetime pipe, stops new leases, and allows an
established lease up to five seconds to finish. It then reaps or kills/reaps the
supervised worker. Actual pipe EOF terminates the worker immediately, including
during SQL. The endpoint is removed after worker exit.

The #1476 regression retains the original two-agent legacy case and adds eight
owner-path agents in distinct Git worktrees. It must assert each caller's returned
and persisted assignee and traversal through one elected leader with one physical
owner opening. Additional fixtures cover twenty concurrent mutations, eight-way
exclusive-claim collision, disconnect after execution with another queued request,
unknown outcomes without replay, long paths, and parent loss during SQL with
rollback recovery. Socket-probe skips cannot pass owner acceptance. These tests
and required configured validation remain evidence obligations, not assertions
of completed platform qualification.

Group-broker protocol 3 persists an UNKNOWN receipt before command launch.
Same-ID retrieval is bound to the original caller and never relaunches a started
request; missing acknowledgement requires inspection, not a new request ID.
A command exit failure does not prove that no partial mutation committed.
Broker audit envelope 1.1.0 adds caller_agent_id while retaining actor/actor_id
as subsystem labels. request_id uses the originating broker request when present;
event_id stays unique. Historical 1.0 events deserialize with an empty caller
field rather than inventing prior attribution. Policy continues to run in each
isolated caller subprocess, with task-specific checks using the actual claimant.

## Intent Reconciliation RPC

<!-- decapod:requirement REQ-INTENT-TRACEABILITY -->
`decapod rpc --op intent.reconcile --params '{}'` returns an advisory,
versioned reconciliation report. It exposes the governed-plan intent digest,
unresolved questions, current code fingerprint and last spec refresh identity,
canonical authored spec hashes, explicit `REQ-*` references, and observable
gaps. It accepts an empty parameter object, requires no session or worktree
for inspection, and does not mutate spec or governance records. Its
`semantic_alignment: not_proven` field is deliberately never a success proof.

## Immutable Intent Change-Impact RPC

<!-- decapod:requirement REQ-INTENT-CHANGE-IMPACT -->
`intent.reconcile` accepts an optional `base_commit` string containing an
exact 40/64-hex Git commit ID. When present, report schema 1.1.0 adds
`change_impact` with immutable base/head identities, sorted changed files
and rename predecessors, authored-material versus generated-only spec deltas,
REQ identifiers explicitly cited in changed authored specs, unmapped
implementation paths, completeness, advisory review/evidence flags and
structured findings. Uncommitted bytes are excluded from this Git comparison.
Invalid symbolic, absent or non-ancestor bases return a validation error.
The original empty-parameter report stays version 1.0.0 without an impact
field. Nothing in this operation authorizes a mutation, approval or completion.

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `3e796be7a4649dabf00062558b430819cbd5fbb189f976e21b82241d500e9785`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (23 files), `docs/` (1 files), `src/` (224 files), `tests/` (219 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->

## Plan contract revision boundary

`govern plan update`, `phase-add`, `phase-update`, `add-entry-gate`,
`add-exit-gate`, and `update-gate` revoke acceptance and all phase/gate progress
when their proposed contract actually differs. Repeating identical values leaves
acceptance and progress intact. Review the revised contract and explicitly
`govern plan approve`, then satisfy and replay its phases; `enter-phase` and
`exit-phase` return `NEEDS_PLAN_APPROVAL` for draft or annotating plans.

`plan_governance::patch_plan` rejects a combined contract change and accepted
state with `PLAN_CHANGED_REQUIRES_APPROVAL`, preserving the previous document.
Final proposed state, rather than the pre-patch phase list, determines whether
`DONE` is structurally valid. `validate_plan_progress` enforces incomplete-phase
and unsatisfied-completed-gate rejection for execution and publication as well.
Spec-review decisions are preserved by these editors and retain their separate
review and resolution rules. Explicit approval still records declared authority;
it does not authenticate the approver or independently judge natural language.

The public plan CLI regressions live in the existing branch-enforced
`tests/cli_contract_enforcement.rs::plan_governed_execution` module. Their
standalone predecessor was not selected by the source-PR graph; keeping all
three cases in this executed target makes CLI routing, reapproval, and phase
replay behavior part of the normal required proof without changing its adapter.

## Consumer readiness and repeatable initialization

`proof.required_profile.id = "repo.pr-readiness.v1"` is the supported local consumer
profile. `proof.repository_contract` declares a reviewed command digest,
`source_coverage` rows and per-command `executions`. Every discovered workflow has
exactly one explicit mapping to real required commands; all configured commands
execute. Absolute installed programs, declared runtime/tool files, exact environment,
working directory and timeout are configuration-bound. Unknown IDs, stale pins,
empty/optional commands and missing or duplicate coverage fail closed. Init exposes
both discovered digests for review without updating them or implying proof success.

A `source_coverage` row may instead declare a reviewed CI-only limitation (#1531):
`{ source = ".github/workflows/ci-only.yml", ci_only = true, reason = "..." }` (`commands`
may be omitted or empty). Such a row pins the limitation through `reviewed_commands_digest`,
requires a nonempty `reason` and no local commands (a `reason` without `ci_only`, or
`ci_only` with mapped commands, fails closed as a false coverage claim), and is reported in
`validation-results.v3` as a `NonApplicable` limitation stage for that workflow —
never counted as a passed or skipped check. When every discovered workflow is
`ci_only`, no row maps commands by design: the required local commands still execute
normally as the repository's local build/test contract and the usual
every-command-mapped invariant does not apply. Rows without the new fields keep their
pre-#1531 serialized identity and digests.

`init` and `init with` share option-provenance-aware preservation: omitted defaults
retain authored configuration even under `--force`; explicit arguments still change
it. The generated GitHub validation workflow fetches complete history and supplies
immutable `CHANGE_EVENT`, `CHANGE_BASE` and `CHANGE_HEAD` to CI-local validation.

## Local CPU companion and model-free startup protocol

`setup laya --runtime <path> --cpu-runtime <path>` registers explicit local native
executable pins. Startup probes and fallback use the existing Laya protocol and
bounded process adapter. A successful probe is not model inference evidence.

## Supplemental Rust worker verification

Generic Rust proof recipes remain physical prerequisites, not model policy.
Configured manifests, features, arguments, dependency locks and timeouts are
bounded and source-bound. Worker outcomes cannot authorize publication, and
this repository declares no required worker recipe.

## Material-spec gate execution records

The validation JSON report's existing `notes` array records material-spec
pass/skip disposition. A valid immutable release emits
`material_specs: outcome=passed proof=version_only_exact_projections`;
a local early exit instead identifies its explicit skipped reason. Required
CI context errors remain failures, never these local skip dispositions.

## Handoff caller and recovery contract

The `todo handoff` interface captures the actual request caller before broker
execution. `--from` asserts the expected recorded owner and `--to` selects the
receiver; neither changes caller identity or trust. The result and task event
preserve actor, source owner, receiver and recovery authority separately.
`plugins::policy::eval_effective_policy` reports the enforced handoff zone,
including verified trust and approval, from the same canonical risk-zone data.

`core::todo::handoff_task_as` supports an explicit in-process request caller and
shares the broker's existing cooperative operator trust semantics for handoff.
It never promotes stored agent trust. Cross-owner recovery additionally requires
an explicit matching source owner and live exact task-bound approval. Successful
recovery consumes the approval records atomically with custody mutation and
records their IDs; failed preconditions retain both state and authority. General,
wrong-target, expired and consumed approvals cannot authorize that recovery.
The reserved local operator identity is not operating-system authentication.

The scoped tests in `tests/plugins/todo.rs` and `tests/plugins/policy.rs` cover
caller/source separation, CLI default caller, operator recovery, policy parity
and rejection boundaries. These tests are authored obligations until full
candidate validation executes; no test or PR-readiness success is asserted here.

The `eval_effective_policy` helper shares `is_read_only_operation` with broker
mutation enforcement and return LOW with no mutation requirements or HITL gate
before reading mutation policy or initializing a datastore. In particular,
`todo.get`, `todo.list`, `todo.status`, and `policy.list` cannot seed TODO state
or inherit mutation-zone trust and approval requirements, including for paths
whose write-risk classification is critical. The read-only side-effect regression
in `tests/plugins/policy.rs` remains authored and unexecuted pending validation.
This helper-level guarantee does not make the entire CLI side-effect-free:
preexisting policy CLI initialization can still initialize policy storage.

The first executed plugin suite on `fe0b7b77` passed policy tests and 24 of
27 TODO tests, but three TODO fixtures failed before exercising their intended
assertions: missing subprocess Git author identity, identical source/recipient
input, and a legacy CLI fixture omitting actual caller identity. The fixtures
now supply explicit caller context and deterministic local test Git identity,
and separately check source-owner and recipient spoof rejection. These repairs
remain unverified until the exact new candidate executes; no runtime policy
requirements were relaxed to accommodate the fixtures.

### Agent/harness execution boundary

Users execute only `decapod init`. All other Decapod interfaces, including
handoff evaluation, approval recording and recovery execution, are invoked by
the AI agent/harness. A human provides intent and explicit approval through
that agent/harness, never a required human operator CLI session. The harness
preserves the actual established caller independently of approval attribution,
expected owner and recipient, and reports the resulting audit. Missing caller
authority remains a blocker; the execution boundary grants no role elevation.

The invoking agent/harness discovers handoff recovery in actual failure output:
`ValidationError` preserves the denial reason and appends `HANDOFF_RECOVERY:`
plus JSON schema `decapod.todo.handoff-recovery.v1`; non-success JSON responses
carry the same object as `recovery`. Fields identify task, actual actor, expected
owner and recipient. `inspect_argv` and `evaluate_argv` provide bounded read and
policy-evaluation actions; `next_action` requires established authority and
conversational human approval when necessary, never self-authorization. It
warns that reconciliation can fail after custody commits, requiring inspection
before retry. Diagnostic construction itself does not read or write state.
Regression assertions cover both early validation errors and JSON precondition
failures without creating storage; these new assertions await exact CI execution.

Other typed failures, including post-transfer reconciliation I/O, emit the same
versioned recovery context to stderr on a best-effort basis before returning the
original typed error unchanged. Failure to write diagnostic output cannot mask
the original error. The context uses a bounded generic reason for these failures
and does not duplicate their potentially sensitive raw cause into recovery JSON;
the ordinary error retains its original cause. Unit regressions cover preserved
I/O classification and diagnostic broken-pipe behavior.

### Integration with request-scoped broker identity

The v0.123.1 accepted broker carries each request's established caller and working
directory into its subprocess. Handoff reads that request identity independently
of expected owner and receiver; merging the broker fix does not promote caller
trust or remove exact recovery approval. Native-proof interfaces and their agent-directed recovery remain intact
alongside this handoff contract.
The earlier fixture-repair candidate `67b166d9` passed both push and PR GitHub
plugin suites; this integrated candidate still requires fresh complete proof.

The concurrent caller regression uses distinct seeded `features` and `docs`
categories while retaining independent caller identities, directories, scopes,
leader traversal and response/persisted-owner assertions. Normal category
validation remains enforced: caller names do not create categories. Buildkite
build 2931 exposed the invalid-category fixture setup before either claim;
the corrected fixture requires fresh official execution evidence.

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

## Regenerated handoff guidance

`docs_cli::todo_handoff_contract` is the canonical generator body for the existing
agent-only caller and ownership recovery contract. `build_docs` emits it between
governance artifacts and explicit claim scopes, preventing Docs Sync from deleting
the tracked handoff section. The focused `trajectory_docs` regression regenerates
command contracts and checks exact inclusion, single emission, required approval
and unchanged trust boundaries. This repair changes documentation generation, not
handoff authorization or task custody. Its runtime regression still requires
execution through the configured validation workflow.

### Native validation recovery diagnostics

Each executed native check may produce `native-N.diagnostic.json` alongside its
private stdout, stderr, and step records in the reported external evidence
directory. The versioned diagnostic identifies the complete check key, elapsed
milliseconds, termination, check-level error and the three related filenames.
This metadata is an agent recovery aid, not an imported receipt or readiness
authority. The independent evidence, candidate and target binding and all
required gates remain necessary; incomplete or failed execution cannot publish a
successful completion bundle.

## Concurrency and recoverable evaluation

Task claims expose advisory source overlaps and `custody_pending` authoring
state. `workspace ensure` verifies a claimed task's observed checkout before
admitting authoring. Secondary ownership has an explicit independent-task
recovery diagnostic, rather than borrowing the primary lease. `resource:<name>`
is an explicit local mutable-resource reservation; the cloud task/event profile
returns `RESOURCE_CUSTODY_PROFILE_UNSUPPORTED` for that capability.

Cross-owner handoff approvals bind task, current owner, lease generation and
receiver, expire within the supported 24-hour window and are consumed only by
successful transfer. Failed diagnostics supply current generation and the exact
policy evaluation target without fabricating approval. The policy CLI's
approving `--actor` is separate from its executing caller.

Prompt evaluation returns `invalid_input` or `error` with `evaluated=false` for
unevaluated requests and nonzero exit. Recovery requests complete-original-prompt
evaluation without human approval for invocation correction. `allow` and `block`
retain their policy meaning and exit semantics. Compatibility `/query` / `/batch`
transport remains unchanged; native RPC/hosted proof is not inferred from it.

## Checkpoint history inspection and recovery

`govern artifacts verify-checkpoints --format json` returns the versioned
`decapod.checkpoint-history.v1` assessment. `scope=checkpoint_history` and
`publication_ready=false` prevent confusing this gate with complete readiness.
`history_proven`, `inspection_complete`, exact base/head commits, verified
commits and per-commit findings expose proven, unproven-history, invalid-history
and unavailable outcomes. Each finding names its commit, tree, obligation,
recovery and material digest when knowable. An incomplete assessment is explicit.

`govern artifacts record-history-recovery --source-ref <ref> --reason <text>`
requires a fresh admitted workstream at its accepted base. It records current
actor/custody, immutable source identities and gap assessment, with
`historical_proof=not_proven` and `exception_authorized=false`. It neither copies
source nor changes its ownership/history. Identical calls are idempotent;
conflicting context is rejected. `resolve-obligation` requires the recovery
observation to be archived in Git before retiring its active obligation.
Publication still independently verifies every commit in the new range.

## Event storage compatibility

Event physical layout revision 2 adds no CLI/RPC fields and changes no proof
encoding. Historical DDL remains available to migration fixtures; current
initialization no longer creates `idx_events_stream_seq`. The full unique
`idx_events_stream_seq_unique` continues enforcing stream-local order identity.
`EVENT_INDEX_COMPACTION_UNSAFE` identifies altered prerequisite definitions.
Older stores missing the unique tree receive it additively; duplicate sequences
fail with the original typed storage error and rollback, without normalization.
Native PostgreSQL uses Propodus's own migrations; the SQLite index migration is
never sent through cloud RPC.

## Laya-only optional setup and initialization

`setup laya` supports cache-directory, runtime, CPU companion, offline and
update-check options. `init --laya-decisions[=true|false]` explicitly selects the
optional assessment policy; omission preserves it. Init preserves current Laya
settings and opaque user extension tables. Only `[laya]` has model-policy
semantics. No retired model configuration or receipt type is an API alias.

## Relational compaction compatibility

The `sqlite.compact_redundant_relational_indexes.v001` forward migration
changes only redundant local SQLite btree structures. No CLI/RPC, Dactyl
operation, task revision, event sequence, transaction semantics, or cloud
schema change. An unexpected historical index or missing unique enforcement
raises `RELATIONAL_INDEX_COMPACTION_UNSAFE` and rolls back all index removal.
Freed SQLite pages are reusable but are not auto-vacuumed.
