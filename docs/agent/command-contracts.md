# Command Contracts

This document defines the normative operational contracts for the Decapod CLI.

## `decapod eval`
- **Intent:** Evaluate an untrusted agent prompt before any repository or tool action

## `decapod activate`
- **Intent:** Activate local control plane state and run startup migrations

## `decapod init`
- **Intent:** Bootstrap system and manage lifecycle

## `decapod setup`
- **Intent:** Configure repository (hooks, settings)

## `decapod session`
- **Intent:** Session token management (required for agent operation)

## `decapod cloud`
- **Intent:** Optional Decapod Cloud credential and project commands

## `decapod constitution`
- **Intent:** Embedded Constitution Graph queries and lookups

### `decapod constitution get` & `decapod constitution overlay`

- **4-Layer Precedence Order (`#877`):** `1. binary_embedded` -> `2. organization_overlay` (explicit local path under `[context.organization_overlay]` or verified imported cache `.decapod/managed/organization-overlay.json`) -> `3. repository_project_override` (`.decapod/OVERRIDE.md`) -> `4. task_context_projection` (strictly non-authoritative projection).
- **Overridability & Fail-Closed Enforcement:** Organization directives specify `overridable`, `extend_only` (rejects weakening via `ORG_OVERLAY_EXTEND_ONLY_WEAKENING_REJECTED`), or `non_overridable` (rejects any `.decapod/OVERRIDE.md` override via `ORG_OVERLAY_NON_OVERRIDABLE_CONFLICT`).
- **Verification & Offline Use:** `decapod constitution overlay status [--format text|json]` inspects trust (`verified_signature`, `verified_digest`, `local_configured`, `tampered`) and freshness (`fresh`, `offline_cached`, `stale`). `verify` fails closed on invalid, tampered, stale, or conflicting overlays. `import <PATH>` verifies and caches bundles locally for disconnected offline execution; `clear` removes the imported cache.
- **No Secret Leakage:** `organization_private` directives retain hash provenance in context capsules while redacting private policy text from portable context bundles (`ORG_OVERLAY_PRIVATE_LEAK_REJECTED`).
- **RPC:** `constitution.get` (`resolved: true`) and `constitution.overlay.status` (read-only).

## `decapod docs`
- **Intent:** Access agent-facing methodology documentation (restricted to docs/agent/)
- **Restriction:** Only handles documents under `docs/agent/`.

## `decapod todo`
- **Intent:** Track tasks and work items
- **Preconditions:** Agent must have an active session.
- **State Transition:** Managed via `todo.db`.

## `decapod obligation`
- **Intent:** Governance-native obligation graph

## Historical validation and task completion

- **Inspect:** `decapod qa verify commit <full-candidate-sha>` reads the canonical
  local full-validation history without session, migration or workspace mutation.
  Historical inspection never grants publication readiness.
- **Complete:** `decapod todo done <task-id> --validated --commit <full-candidate-sha>`
  completes the original task from its own recorded full proof after verifiable
  integration. The original task contract and supported ownership lineage must
  remain intact. The normal session, dependency and atomic revision checks apply.
- **Repair:** The same command can repair a done-but-unverified task without
  reopening it or changing its completion timestamp. It does not require another
  pending PR, a different task's admitted workspace or a fresh validation child.
- **Evidence:** Full proof is stored outside the validated candidate and indexed
  by its complete immutable SHA. `.decapod/governance.json` retains its legacy
  prerequisite validation section; that section alone is insufficient.
- **Limits:** Missing historical full proof cannot be reconstructed from a green
  PR badge. A new candidate or task change requires fresh proof. Historical
  completion permission never satisfies `workspace publish`. Cloud-backed
  historical lookup/completion is unsupported; ordinary cloud validation is
  unchanged.
- **Integration boundary:** This version observes public GitHub accepted branches
  over HTTPS. Fetch missing accepted Git objects through the normal governed VCS
  path before retrying. Private-repository integration is unsupported. Ordinary
  merge/fast-forward ancestry is sufficient; squash/rebase requires an exact
  complete candidate tree in the bounded accepted history. New lease generations
  and additional task-specific proof gates cannot inherit the old result.

## `decapod validate`
- **Intent:** Prove complete current-workbranch readiness before pull request creation
- **Subject:** The exact current work branch, its accepted target tip, source and governance material, reviewed CI/release definitions, toolchain and applicable complete check matrix.
- **Outcome:** Exit 0 only after all required evidence passes. Missing, empty, stale, running, skipped-required or failed proof returns nonzero. Any relevant branch or target change invalidates earlier success.
- **Publication loop:** Work branch pushes and iteration are allowed before preflight. Hold PR creation (including a draft) until intent is converged, the branch is mergeable into the accepted target, and full validation succeeds against the exact current candidate and accepted target. Follow the validation report's actionable instructions, repair reported root causes on the work branch, and rerun exact-candidate validation until it succeeds; never bypass a reported gate or fabricate evidence. `workspace publish` combines branch push and PR creation, so it remains held at this readiness boundary; earlier standalone branch pushes are allowed. RPC `validate.run` enforces the same validation contract.
- **CI context:** Inside GitHub Actions (`GITHUB_ACTIONS=true`) or Buildkite (`BUILDKITE=true`), public CLI/alias/RPC validation executes the real bounded local gates and ignores remote preflight configuration. It reports `scope=ci_local` and `publication_ready=false`, never writes a full-readiness receipt or waits for its own suite, and rejects snapshot transport arguments. Provider environment flags alone cannot authorize publication: the publisher independently invokes the complete workbranch readiness boundary.
- **Prerequisites:** Once source and required governance material are ready, checkpoint and commit the candidate, then run plain `decapod validate`. Validation owns configured build prerequisites and tests; do not run a separate manual build or shadow preflight. `decapod govern project-check` runs bounded project/governance prerequisites for bootstrap and finite native/CI recipe execution. Its partial success never grants publication readiness. Complete workbranch validation runs outside CI and aggregates actual source checks without recursion.
- **Providers:** `proof.validation_buildkite_preflights = false` selects native execution; true selects remote execution on the current Decapod workspace branch. The `decapod.pr-readiness.v1` profile retains the complete finite source-CI contract on either route. Consumer `repo.pr-readiness.v1` instead proves explicitly reviewed required local commands with `scope=repository_local_readiness`; it cannot select Buildkite or attest hosted CI, release or deployment execution. Explicitly accepted `canonical_local_target = true` on the canonical local branch selects `scope=repository_local_git_readiness` with forge `publication_ready=false`; candidate-only activation and cloud/strict-profile fallback are rejected. Missing capabilities and unknown or unreconciled contracts fail explicitly.
- **Automatic provider execution:** With Buildkite validation enabled outside CI, `validate` pushes or reuses the exact current workspace branch commit, waits for every required imported Buildkite workflow, collects authenticated execution artifacts once, and rechecks the candidate, accepted target and provider attempt identities before returning success. The required project checks execute on Buildkite; this route neither runs a duplicate local prerequisite suite nor requires a fresh local validation receipt before submission. It uses already configured official Git access, repository check URLs only to locate Buildkite runs, and the reviewed Buildkite reader; missing tools, access denial, timeout, cancellation, failed jobs or incomplete custody return nonzero. It never installs credentials or falls back after an authorization failure.
- **Host handoff:** `validate --snapshot-prepare` prepares private metadata for the existing workspace branch and returns nonzero while proof is pending. An authorized host may push only the same exact candidate commit to that branch. `validate --snapshot-resume <HANDOFF> --snapshot-adopt <H>` independently verifies the unchanged source/tree/target and the exact remote branch tip; it cannot accept a different commit or a temporary branch. If a host object API changes commit identity, first checkpoint and review that real source identity before preparing a new handoff. Never edit a handoff to assert success. `validate --snapshot-resume <HANDOFF>` independently verifies Buildkite identities, raw artifact digests and execution evidence. No per-run branch or synthetic commit is created, and validation cleanup never deletes the workspace branch. Existing GitHub workflows may run independently; their results and artifacts neither satisfy nor block this Buildkite profile. This is not a compatibility claim for a separate `bk preflight` command.
- **Boundaries:** Validation may perform the authorized non-forced push of its existing workspace branch. It does not open a PR, create a release/tag, upload a package/image or deploy. Remote CI submission needs existing authorized access and remains subject to external-action policy. The core source-PR profile requires actual compile, test and build execution. Release-only packaging is outside this profile and has no packaging-assurance claim; publish/deploy dispositions describe simulation only, never actual delivery.
- **Todo-scoped `/tmp` cleanup:** Once validation passes and PR creation completes (`decapod workspace publish` or post-validation PR creation), flush ONLY the current todo's `/tmp` build and scratch directories (such as `/tmp/decapod_validate/<task-id>` and any todo-scoped `CARGO_TARGET_DIR` under `/tmp`). Never perform wildcard or root-level deletion of `/tmp`, never delete another concurrent agent's or sibling todo's `/tmp` path, and never remove shared `/tmp/decapod` controller sockets/state or active sibling directories.
- **Recovery:** Do not ask again before an already-authorized configured validation push or a supported, safely replayable retry to the same repository and workspace branch. Recheck the exact source and accepted target; stop on explicit cancellation, access or authorization denial, or a change that requires new authority. Follow the validation report's actionable recovery instructions, preserve blockers requiring human input, repair root causes or missing proof, and rerun against the exact candidate. For supported remote source-CI contracts, validation uses the accepted target’s independently verified executor to build and test the candidate, even when candidate helper bytes change. The controller keeps the required graph and final verdict; candidate output cannot authorize itself. `CONTROLLER_CONTRACT_UNSUPPORTED` identifies an unsupported carrier, protocol or accepted executor. Rerunning the same controller, repinning configuration or running a manual build cannot resolve that blocker; use an independently supported controller for this exact contract, and preserve the blocker if none is available. No automatic controller replacement is performed. Do not mark unexecuted checks green, weaken the required profile to pass, or replace unavailable provider evidence with a local receipt.

## Living-spec refresh surfaces

- **RPC:** `decapod rpc --op specs.refresh` refreshes supported spec fingerprints,
  attestations, overlays, generated boundaries and the manifest.
- **Validation flag:** `decapod validate --refresh-specs` requests the same
  supported refresh while running validation. Validation remains authoritative;
  this flag does not make unrelated gates pass.
- **Preconditions:** Use an active session and the claimed, isolated workspace.
  Review and author the material living-spec prose before refreshing it.
- **Authored scope:** Living specs describe the configured project from purpose
  to architecture to implementation depth. Lead with the whole product, then
  explain components and source-grounded behavior. A narrow task updates the
  relevant part without replacing project-wide identity. Cite repository paths
  and symbols; distinguish implemented behavior, plans, assumptions, and gaps.
  Templates and capability declarations are obligations, not runtime evidence.
- **Boundary:** Refresh does not rewrite authored intent or architecture and
  cannot substitute for the required material spec change. It does not approve
  a plan or mark a todo complete.
- **Recovery:** There is no top-level `decapod specs.refresh` command. Use one of
  the two invocations above, inspect the refreshed files, and rerun validation.

## `decapod govern`
- **Intent:** Governance: policy, health, proofs, audits

### `decapod govern plan` contract revisions

- Plan acceptance applies to the recorded execution contract. A real change to
  title, intent, task bindings, proof hooks, questions, stop conditions,
  contradictions, deferrals, scope constraints, or phase/gate definitions returns
  the plan to `DRAFT` and clears all phase and gate progress, including timestamps.
  Phase ordering can make later evidence depend on earlier definitions, so a
  revision conservatively requires replay of every declared phase.
- Exact no-op updates preserve acceptance and progress. Spec-review records are
  retained, including unresolved human decisions; their independent content-bound
  review rules still apply.
- After a material edit, review the revised contract and use the separate
  `decapod govern plan approve` transition. Editing and accepting a changed
  contract in the same low-level patch is rejected without writing the document.
  An explicit state transition remains a declared approval, not authenticated
  proof of a human identity or semantic correctness.
- `enter-phase` and `exit-phase` require an approved or executing plan. Completing
  phases cannot promote a draft or annotating plan to `DONE`. Reapproval does not
  restore progress erased by a contract revision; satisfy the current gates and
  replay the phases in declaration order.
- Execution and publication reject `DONE` plans with incomplete phases and
  completed phases with unsatisfied gates. These structural checks supplement
  exact-candidate validation and do not substitute for actual proof execution.

### `decapod govern trajectory record`

- **Preconditions:** Use an active session and initialize the run with `decapod govern trajectory init --run-id <run-id> --original-intent "..." --derived-intent "..."`.
- **Inputs:** Repeat `--loop-json '<object>'` for typed loop attempts and `--check "name=status"` for named check results. Each loop input is one JSON object, not an array.
- **Schema:** The tables and required fields below are generated by `decapod docs build` from [trajectory.schema.json](../../assets/schemas/trajectory.schema.json), `$defs.loop` and `$defs.check`. They are embedded in `decapod docs ingest` and `decapod docs show command-contracts --source embedded`.
- **Required loop fields:** `intent_id`, `trajectory_id`, `loop_id`, `loop_type`, `attempt`, `trigger`, `grader_result`, `mutation_proposal`, `status`.

| Input | Canonical values |
| --- | --- |
| `grader_result` | `pass`, `fail`, `skipped` |
| `loop_type` | `agent`, `verification`, `event`, `improvement` |
| `mutation_proposal` | `none`, `prompt`, `tool`, `config`, `rubric`, `policy` |
| `status` | `open`, `retrying`, `passed`, `failed`, `blocked` |
| `trigger` | `human`, `webhook`, `cron`, `agent`, `grader`, `engine` |
| `--check status` | `passed`, `failed`, `partial`, `unavailable` |

- **Exact JSON values:** Loop enum strings are case-sensitive. In particular, `grader_result` uses `pass` / `fail`, while loop `status` uses `passed` / `failed`. Arbitrary trigger descriptions are not enum values.
- **Check aliases:** The check parser additionally accepts `pass` as `passed` and `fail` as `failed`, ignoring status case and surrounding whitespace. Check names must be non-empty. `no_checks_run` is a computed proof status, not an accepted check input.
- **Identity and attempts:** Non-empty `intent_id`, `trajectory_id`, and `loop_id` are required. `trajectory_id` must equal `--run-id`; `intent_id` must match the initialized intent (by default `intent:<run-id>`). Attempts for each `loop_id` start at 1 and remain contiguous, without duplicates.
- **Verification evidence:** A verification loop with `grader_result: "pass"` requires at least one `proof_refs` entry, and every entry must be non-empty; `grader_result: "fail"` requires non-empty `feedback`. Feedback on every loop is limited to 2048 bytes. A passing enum value is not evidence by itself.
- **Optional loop fields:** `parent_loop_id`, `tool_calls`, `observations`, `feedback`, `proof_refs`, and `custody_event_id`. Event and improvement loops record evidence; a mutation proposal does not apply a change.
- **Example:** See [the minimal trajectory record](payload-examples.md#trajectory-record) for an initialized run and an unverified inspection.


## `decapod data`
- **Intent:** Data: archives, knowledge, context, schemas

### Governed Local Database Maintenance (`decapod data db ...`)

- **Read-only inventory (ungated):** `decapod data database inspect` / RPC `data.database.inspect` remains non-initializing and read-only.
- **Governed capabilities (default-deny):** `decapod data db capabilities`, `decapod data db lock {diagnose,retry,release-own}`, `decapod data db maintain {inspect,verify,backup,recover}`, and `decapod data db audit` (plus `data.database.{capabilities,maintain.inspect,verify,backup,recover,lock.diagnose,lock.retry,lock.release_own,audit}`) require an authenticated local `DECAPOD_AGENT_ID` + `DECAPOD_SESSION_PASSWORD` inside the agent's claimed `.decapod/workspaces/*` worktree and canonical main-repository `[governance.db_maintenance] enabled = true`.
- **Boundaries:** `db.backup` writes only to non-existing paths under `.decapod/backups/` (`<= 512 MiB`) and never implies `db.recover`; `db.recover` requires repository `owner` repair authority (`#1590`); `backend = "cloud"` fails closed (`CLOUD_PRINCIPAL_UNVERIFIED`); raw SQL, `sqlite3`, vacuum, and restore are unmodeled (`CAPABILITY_UNSUPPORTED`).

### Broker Audit Verification & Orphan Recovery (`decapod data broker ...`)

- **Writer incarnation provenance:** New `pending` broker audit events record optional OS writer provenance (`writer_pid`, `writer_start_id`, `writer_boot_id`, `writer_host_id`, and Linux `writer_pid_ns`).
- **Liveness classification:** `decapod data broker verify` and `decapod data broker repair` establish `writer_liveness = "stopped"` only when provenance proves either (a) same host/boot/PID-namespace with an absent PID or a different observed `writer_start_id` (PID reuse), or (b) a different prior boot on the same verified host. Event age alone and bare PID checks without start-identity/boot correlation never establish writer termination.
- **Fail-closed unknown handling:** Active writers (`writer_liveness = "active"`) are always refused. Legacy provenance-free, cross-host, or unverifiable pending events report `writer_liveness = "unknown"` (`human_review_required = true`) in `verify` and `repair` preview, and `decapod data broker repair --apply` refuses unless `--acknowledge-unknown-writer` is explicitly provided after human review.

## `decapod auto`
- **Intent:** Automation: scheduled and event-driven

### `decapod auto container run`

- **Delegated cgroup parent (`#1820`):** Accepts an optional `--cgroup-parent <path-or-slice>` flag (or `DECAPOD_CONTAINER_CGROUP_PARENT` environment variable; CLI flag takes precedence) and passes `--cgroup-parent` to the container runtime while keeping `--cpus`, `--memory`, and `--pids-limit` enforced. Cgroups are never disabled.
- **Fail-closed validation:** Only canonical absolute cgroup hierarchy paths (e.g. `/delegated/decapod`, without the `/sys/fs/cgroup` mount prefix, trailing slashes, empty segments, `.` or `..` traversal, whitespace, control characters, or metacharacters) and systemd slice names (`*.slice`) are accepted. Invalid values fail closed with `container_invalid_cgroup_parent` before touching the container runtime.
- **Rootless Podman `cgroupfs` auto-detection:** When neither `--cgroup-parent` nor `DECAPOD_CONTAINER_CGROUP_PARENT` is set and local `podman info` reports rootless `cgroupfs` on cgroup v2 (`serviceIsRemote: false`), Decapod inspects `/proc/self/cgroup` and selects the deepest non-root writable ancestor under `/sys/fs/cgroup` whose `cgroup.subtree_control` enables `cpu`, `memory`, and `pids` without traversing symlinks.
- **Diagnostics:** OCI/cgroupfs permission errors (`crun`/`runc` `cannot set limits without cgroups`, `/sys/fs/cgroup` permission denied) return `container_cgroup_permission_denied` with delegated cgroup parent remediation, distinct from `container_workspace_permission_denied`.

## `decapod qa`
- **Intent:** Quality assurance: verification and checks

## `decapod decide`
- **Intent:** Architecture decision prompting

## `decapod workspace`
- **Intent:** Agent workspace management
- **Preconditions:** Task must be claimed.
- **State Transition:** Creates git worktrees/containers.
- **Publication:** Publication is a governed transition and remains blocked while required validation or evidence is unsatisfied.
- **Scratch cleanup:** When PR creation succeeds (`workspace publish`), flush ONLY that todo's `/tmp` build and scratch directories (such as `/tmp/decapod_validate/<task-id>` and any todo-scoped `CARGO_TARGET_DIR` under `/tmp`). Never perform wildcard or root-level deletion of `/tmp`, never delete another concurrent agent's or sibling todo's `/tmp` path, and never remove shared `/tmp/decapod` controller sockets/state or active sibling directories.

### Workspace diagnosis

`decapod workspace diagnose [--json] [--offline] [--pr <number>] [--operation <name>]`
reads checkout and custody state, accepted policy, candidate config and
override proposals, artifact freshness, dirty-file groups, and historical
proof. Optional GitHub readback observes the exact PR head and required forge
checks. `--offline` skips remote reads. Credentials appear as capability
status. Diagnosis does not acquire sessions or repair files.

JSON uses `decapod.workspace-diagnostic.v1`. Exit 0 means the selected local
operation has no observed blocker. Exit 1 means blockers remain, 2 means
candidate config is invalid, and 3 means the default publication diagnosis
lacks verified remote evidence. Use the ordered next actions to recover.
Live validation and readback remain owned by `workspace publish`.

## `decapod rpc`
- **Intent:** Decapod-specific structured RPC interface for agents

## `decapod release`
- **Intent:** Release lifecycle checks and guards

## `decapod capabilities`
- **Intent:** Show Decapod capabilities (for agent discovery)

### `decapod capabilities contracts`

- **Intent:** Deterministic, read-only inspection of composed service capability contracts (`Decapod -> Dactyl -> Propodus`) for schema, operation, auth scope, revision, policy-copy, prestate, shared-invariant, and two-step fault/severed-ack drift before governed admission.
- **Usage:** `decapod capabilities contracts [--fixture] [--contract <PATH>] [--dir <PATH>] [--format text|json]` (also via RPC `interop.capability_contracts`).
- **Offline isolation:** Never requires network calls or Cloud credentials; unrelated local offline repositories remain unaffected and usable (`local_offline_usable=true`, Cloud capability marked `unsupported`).
- **Validation gate:** When `.decapod/contracts/capability_contract.json` exists, `decapod validate` checks it against accepted host policy and fails closed on drift, dropped negative cases, or self-certification (`SELF_VERIFICATION_REJECTED`).

### `decapod capabilities parity`

- **Intent:** Deterministic, read-only inspection of the versioned Local vs Cloud logical operation and evidence parity matrix (`decapod.cloud-parity-matrix.v1`), including explicit enumeration of unsupported Cloud operations and placement opacity boundaries.
- **Usage:** `decapod capabilities parity [--dir <PATH>] [--format text|json]` (also via RPC `interop.cloud_parity`).
- **No blanket parity / no local SQLite fallback:** Reports supported logical operations separately from Cloud-unsupported operations (`todo.lease.renew`, `todo.claim.shared`, `todo.edit`, `todo.comment`, `todo.handoff`, `todo.archive`, `todo.transfer.export_import`, `todo.journal.reconcile_legacy`, `qa.verify.commit`, `data.database.maintenance`) and never creates or touches `.decapod/data/decapod.db`.

## `decapod infer`
- **Intent:** Inference governance: shape context before model, validate after

## `decapod trace`
- **Intent:** Local trace management

## `decapod system`
- **Intent:** System: capabilities, version, doctor, migrations

### Read-Only System & Migration Inspection (`decapod system ...`)

- **Non-initializing diagnostics (`decapod system migrations`, `decapod system doctor check`, `decapod system version`, `decapod system capabilities`):** Run without requiring `DECAPOD_SESSION_PASSWORD`, without creating `.decapod/data/`, without modifying `version_counter.json`, `applied.json`, or `catalog.json`, and without consuming a pending migration notice.
- **Migration inspection (`decapod system migrations --format {json,text}`):** Resolves `workspace_root`, `governance_root`, and `decapod_root` (following the main repository `.decapod` from governed worktrees) and exposes `version_counter`, `applied`, compiled/on-disk `catalog`, `pending_migrations`, `action_required`, `reason_codes`, and actionable `instructions`.

## `decapod context`
- **Intent:** Context: infer, lcm, internalize, preflight, impact

## `decapod integration`
- **Intent:** Preview, reversibly remove, or restore repository integration without deleting project state

### Reversible Repository Integration Removal (`decapod integration ...`)

- **Read-only preview (`decapod integration preview`):** Makes zero filesystem, Git, SQLite, session, or network mutations. Classifies `generated`, `edited`, `authored`, `unknown`, and `active` resources separately using ownership and digest evidence. A file is never classified as `generated` solely by filename.
- **Bounded apply (`decapod integration remove --authorize`):** Requires explicit `--authorize`. Removes only positively owned, unchanged generated entrypoints, hooks, CI workflows, `.gitignore` rules, and untouched scaffolds while preserving edited/shared/unknown entries, authored instructions/overrides/specs, `.decapod/data/`, `.decapod/governance.json`, `*.bak` backups, global config, and credentials. Refuses uncertain ownership, symlink replacement, and active-custody conflicts (active todo leases, authoring bindings, registered/dirty worktrees, storage locks) with non-zero exit. Writes `.decapod/integration-restore-manifest.json` atomically before removal so interrupted or repeated invocations remain idempotent.
- **Restore (`decapod integration restore`):** Restores removed generated files and `.gitignore` entries byte-for-byte from the restoration manifest while refusing to overwrite conflicting newly authored files or symlinks.
- **Authority boundaries:** Permanent deletion of `.decapod/data` (`--purge-state`) and protected governance removal (`--remove-protected-governance`) are refused by integration removal and remain separate explicit authority boundaries.

# RPC Operations (Auto-generated)

### Operation: `AgentInit`
### Operation: `WorkspaceStatus`
### Operation: `WorkspaceEnsure`
### Operation: `WorkspacePublish`
### Operation: `ContextResolve`
### Operation: `ContextCapsuleQuery`
### Operation: `ContextBundleExport`
### Operation: `ContextBundleImport`
### Operation: `ContextBindings`
### Operation: `ConstitutionGet`
### Operation: `ConstitutionOverlayStatus`
### Operation: `ConstitutionLinksQuery`
### Operation: `ConstitutionLinksNavigate`
### Operation: `SpecsRefresh`
### Operation: `PolicyGrantExplain`
### Operation: `IntentReconcile`
### Operation: `ConstitutionMigrate`
### Operation: `AgentRegistryQuery`
### Operation: `SchemaGet`
### Operation: `StorageInventory`
### Operation: `DatabaseCapabilities`
### Operation: `DatabaseMaintainInspect`
### Operation: `DatabaseVerify`
### Operation: `DatabaseBackup`
### Operation: `DatabaseRecover`
### Operation: `DatabaseLockDiagnose`
### Operation: `DatabaseLockRetry`
### Operation: `DatabaseLockReleaseOwn`
### Operation: `DatabaseAudit`
### Operation: `StoreUpsert`
### Operation: `StoreQuery`
### Operation: `ValidateRun`
### Operation: `ScaffoldNextQuestion`
### Operation: `ScaffoldApplyAnswer`
### Operation: `ScaffoldGenerateArtifacts`
### Operation: `StandardsResolve`
### Operation: `MentorObligations`
### Operation: `AssuranceEvaluate`
### Operation: `InteropManifest`
### Operation: `InteropNegotiate`
### Operation: `InteropCapabilityContracts`
### Operation: `InteropCloudParity`
### Operation: `StateResolve`
### Operation: `StateDiff`

# Runtime Checkpoints and Recovery

Runtime journals use the existing canonical datastore. They do not replace Git
as the authority for authored governance or grant publication readiness.
Workunit mutations require an initialized local runtime (`decapod init`). A
missing or partial policy store fails before any workunit journal append.

- `decapod todo journal checkpoint` records the selected store's current task
  state as an explicit baseline or current checkpoint
- `decapod todo journal verify` inspects integrity without repairing state
- `decapod todo journal restore --expected-current-digest <digest>` verifies
  the complete chain and the observed live-state precondition before restoring
  an absent or known predecessor projection; divergent authority is preserved
- `decapod todo transfer export --id <id> --to <receiver> --output <private-file>`
  seals a previously yielded standalone local task and exports at most 4 MiB
  of task-scoped provenance; only the actor that yielded it can export
- `decapod todo transfer recover --id <id> --transfer-id <id> --output <new-file>`
  republishes the identical durable artifact after interrupted publication
- `decapod todo transfer import --input <private-file> --expected-digest <sha256:digest> --accept-source-transfer`
  requires operator-authenticated source handoff, matching GitHub origin and
  receiver, fetched source commit, and absent destination identity/history.
  Imported proof is historical; claim, orientation and local proof are required.
  Connected/shared tasks, replay and conflicting authority fail closed. Source
  sealing is local cooperative enforcement, not global exclusivity across clones.
- `decapod govern workunit checkpoint --task-id <id> --operation-id <id>`
  records a checkpoint, with optional `--expected-generation <n>`
- `decapod govern workunit journal-status --task-id <id>` inspects the journal,
  JSON projection, `proof_status` (`proven`, `not_proven`, `stale`, `absent`),
  and split-store audit records; `recover --task-id <id>` restores the exact
  recorded manifest without refreshing verification
- `decapod govern workunit resume --task-id <id> --operation-id <id>` checks
  current execution authority before continuing, with the same optional
  generation precondition
- `decapod govern workunit converge [--task-id <id>] [--dry-run]` inspects or
  archives a separately initialized workspace-local datastore as `not_proven`
  audit history (`WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED`) without promoting
  split-store records to `VERIFIED`, converging the workspace onto the canonical
  atomic store

- `decapod govern trajectory checkpoint --run-id <id> --checkpoint-id <id>`
  records bounded source, intent, plan, trajectory, task custody, and policy
  bindings; repeat `--approval-id <id>` for explicit required approval references
- `decapod govern trajectory verify-resume --run-id <id> --checkpoint-id <id>`
  rechecks those bindings and current approval validity without mutating them

An approval reference never issues a grant. Resume requires live task custody
and current dependency readiness. Checkpoint-only commits may descend from the
captured commit; branch changes, rewinds, changed source, or changed authority
require review and a fresh checkpoint. Existing execution checkpoints cannot
silently drop required approval references.

Reuse an operation ID only for an identical request. A failed file publication
may follow a successful durable journal append; inspect status and reconcile
that operation rather than submitting a changed request with the old ID. Legacy
baseline records describe observed current state, not reconstructed prior
history. Neither successful replay nor a readable manifest proves current
source, task, approval, or validation freshness. Full `decapod validate` and the
governed publisher retain their existing independent proof requirements.

# Governance Artifact Ownership

`.decapod/governance.json` is the sole tracked governance document. Its logical
plan, trajectory, validation, and claims sections share normalized
intent and epoch records. Health Engine operational claims remain separate in
`.decapod/data/decapod.db`; workunit runtime manifests live in `.decapod/data/workunits/`.
Do not hand-edit either control-plane authority.

Use `decapod govern artifacts migrate` for lossless legacy adoption and
`begin-pr --id <change> --base-branch <branch>` for an explicit new-PR boundary.
Normal `workspace ensure`, `agent.init`, and validation initialize a proven fresh
worktree branch from the embedded governance template and current claimed task.
The plan starts DRAFT and the trajectory has no checks or completion proof; only
actual successful validation writes a receipt. Protected refs and read-only
status/inventory never trigger initialization. Repeats, renames, and rebases keep
the active change identity and journal. Dirty inherited or ambiguous context
fails closed rather than being overwritten.
The boundary requires discarded state to be recoverable from Git, keeps explicit
unresolved obligations, and leaves accepted completed claims in Git history.
Use `claim` for actual current-PR claims and `resolve-obligation` with proof
references to retire a carried obligation explicitly. Status and
`verify-checkpoints` are read-only and never migrate state.

Populate in order: stable PR identity → intent/scope plan → actual current claims
and planned checks → work/evidence trajectory → validation → staged-material
checkpoint per commit. Persist meaningful boundaries as work progresses; do not
wait until the final response or repeatedly rewrite unchanged content.

Before each authored commit, stage code/spec changes, run
`decapod govern artifacts checkpoint --id <unique-id> --summary "..." --proof-ref <evidence>`,
then stage governance.json and commit. Publication checks exact material and
cumulative checkpoint preservation across the entire commit graph, including merges.
Checkpoint IDs are immutable within the active PR. Repeating an ID succeeds
without a write only when the summary, normalized proof-reference set, and
current material digest match its original record. Different code, governed
inputs, summary, or proof references return `GOVERNANCE_CHECKPOINT_ID_CONFLICT`
without replacing history. Record changed work with a new ID. This check runs
under the existing governance-document lock; independent history verification
still rejects erased or rewritten records in authored commits.

Historical inspection is available without task acquisition:
`verify-checkpoints --base-branch <accepted> --head-ref <source> --target-base <target> --format json`
reports exact commits, material obligations and recovery dispositions. A missing
historical checkpoint remains unproven; a new tip checkpoint cannot prove it.
The report always says `publication_ready=false` because it is not full validation.
To recover work, preserve the source branch, acquire a new task and a fresh
workspace from the accepted target, then use
`record-history-recovery --source-ref <immutable-source> --reason <rationale>`.
This records present-time gap custody, not an authorized exception. Review and
reauthor selected source, review specs, checkpoint and commit the observation.
Only after that archive exists may `resolve-obligation` record the current
recovery disposition with evidence references. Preserve the old unproven
history; never fabricate its checkpoints or treat a decision label as authority.
When a Git merge (`MERGE_HEAD` present) leaves `.decapod/governance.json`
conflicted or with a trajectory/custody hash mismatch, run
`decapod govern artifacts reconcile-merge` before committing the merge. It
reads immutable commit states (`HEAD`, `MERGE_HEAD`, `merge-base --all`),
refuses non-merge states or conflicting unaccepted PR authority, preserves
complete active journal history and failed/missing proof states, and recomputes
canonical trajectory hashes without fabricating passing proof.
`inventory --claims-note "..."` records a current-PR checkpoint, not a fake claim
or a policy-history append. `inventory --compact` performs lossless legacy
normalization. Neither surface repairs malformed evidence by overwriting it.

## `todo handoff`: caller and ownership recovery

The acting caller is captured from the active `DECAPOD_AGENT_ID` request before
broker execution. `--from` is an optional expected-current-owner precondition;
it never selects the actor or supplies that owner's trust. `--to` selects only
the receiver. Ordinary owner handoff can omit `--from`. A missing caller is a
`HANDOFF_CALLER_MISSING` error rather than an assumed `unknown` owner.

`govern policy eval --command todo.handoff --path <task-id>` reports the same
handoff zone as mutation: verified trust and explicit approval by default,
subject to the existing operator policy. Output also states the stronger
cross-owner recovery conditions. Dependency and lease gates remain. Logical source overlap is advisory; named shared-resource reservations still exclude.

Only the AI agent/harness interfaces with and executes Decapod after setup;
the only Decapod command a user executes is `decapod init`. Humans provide
intent and approval through the agent/harness, not through operator CLI sessions.
When a recorded owner is unavailable or incorrect, the authorized agent/harness:

1. Inspects `todo get --id <task-id>` and preserves its owner and history.
2. Evaluates `govern policy eval --command todo.handoff --path <task-id>` under
   its established request identity and presents the exact source owner,
   intended receiver and reason to the human for a decision.
3. After receiving the required explicit human approval, executes the diagnostic's
   exact approval command with a bounded lifetime, recording the approving
   decision separately from the executing caller. It must not manufacture approval.
   A command-wide or different-task approval cannot authorize recovery.
4. Under the same established authorized caller identity, executes
   `todo handoff --id <task-id> --from <recorded-owner> --to <intended-receiver>
   --summary <reason>` and reports the audited result.

The actual caller must already satisfy the existing trust and ownership policy.
An authorized harness may use its already established local operator role; a
verified agent may recover under its own identity with the same exact approval.
Neither may select a stronger role, promote its own trust, or impersonate the
old owner merely to pass a denied gate. Missing authority remains a blocker to
resolve through the authorized harness, not a request for a human CLI command.
The reserved `operator`, `cli` and `decapod`
roles retain the existing local cooperative policy semantics; environment labels
are not OS authentication, credentials, or proof against a hostile local process.
No agent trust row is promoted by recovery.

Cross-owner recovery requires a live approval bound to task, current owner, lease
generation and receiver, with at most 24 hours remaining, even when a
broader operator policy suppresses ordinary handoff prompts. Successful recovery
consumes its exact approval records in the same transaction as the ownership
change. Failed preconditions preserve both custody and approval. The event and
result record the actual actor, previous owner, receiver, recovery flag and
consumed approval IDs. Repeating a transfer requires fresh authority; a stale
`--from` fails rather than silently transferring changed custody. This operation
never marks task work verified or waives proof gates.

Recovery is discoverable in the failed invocation itself, not only in this
reference: handoff validation errors retain their original reason and append
`HANDOFF_RECOVERY:` with a `decapod.todo.handoff-recovery.v1` JSON diagnostic;
non-success handoff JSON results include that diagnostic in `recovery`. It names
the task, actual caller, expected owner and receiver, and supplies bounded
`inspect_argv` and `evaluate_argv` arrays to the invoking agent/harness. Its
`next_action` requires inspection before retry (reconciliation can fail after a
committed transfer), established caller authority, and human decision through
conversation when required. It never returns a self-authorizing approval step.

## Explicit claim file scopes

Agents can create a todo with `--scope 'paths:["src/a.rs","tests/a.rs"]'`
and `--dir <repository-directory>`. This declares the intended literal file or
directory footprint; it is coordination metadata, not proof of actual edits or
an OS write sandbox. The existing task journal and transfer carry the scope.
Use 1–256 bounded ASCII relative paths; traversal, absolute paths, glob syntax and
existing symlink traversal are rejected. Disjoint explicit footprints avoid the
old repository-root path conflict. Equal paths and parent/child paths report integration risk; they do not exclude independently owned tasks.
Explicit footprint comparison conservatively folds ASCII case on every platform
to prevent case-only aliases on case-insensitive filesystems. Unicode paths are
unsupported; retain a conservative directory claim instead. Case-only distinct
Linux files may therefore report an advisory alias risk.
A legacy or malformed scope retains a conservative directory footprint for diagnostics. Existing
broad claims must be resolved rather than silently narrowed by another caller.
Categories remain routing metadata. Dependency, lease, approval and explicit resource-custody gates remain independent.
File scopes do not create verified isolated-worktree custody or waive shared
resource exclusion. Work-claim projections and fleet overlap diagnostics use
the same footprint. Different explicit scopes are not fuzzy-merged as duplicate
todos even when their titles are similar. This is not a trust-grant API: shared
claims retain their existing verified-trust requirement. Humans provide intent
through the agent; the only human-executed Decapod command remains init.

Checkout authoring admission (`workspace.authoring.bind` / `workspace.authoring.release`)
maintains a 1-to-1 invariant between live registered Git worktrees and active task
leases (`authoring_bindings(worktree_id)` primary key plus unique `task_id` index):
one checkout binds at most one active task (`AUTHORING_CHECKOUT_OCCUPIED`), and one
active task lease binds at most one live worktree (`AUTHORING_TASK_ALREADY_BOUND`),
while deleted/unregistered worktrees are replaced atomically on retry (`replaced_missing_worktree`).
Same-owner `todo renew` advances an already-admitted binding's `lease_generation`
in-transaction (`authoring.renew`), whereas `yield`, `release`, `reclaim`, `handoff`,
`done`, `archive`, and `workspace prune` retire bindings and emit `authoring.release`.
Explicit `--scope 'resource:<name>'` reservations do not claim repository source
paths; they exclude conflicting resource claims (`predicate=resource_reservation`)
and gate shared mutations (`release lineage-sync` on `resource:release`; `workspace publish`
on `resource:integration` / `resource:integration:<base_branch>`) with `SHARED_RESOURCE_OCCUPIED`
while leaving isolated worktree authoring and local validation unblocked.

`infer orientation`, branch governance seeding (`trajectory.repo_scope` and custody
`intent.constraints`), and `govern trajectory {init,record}` parse explicit `paths:[...]`
JSON arrays through `fleet_coord::declared_paths` rather than splitting on commas.
Already-generated fragmented projections in `.decapod/governance.json` can be repaired
with `decapod govern trajectory reconcile-scope [--task-id <id>] [--run-id <id>]`,
which requires the prior journal to be archived in Git (`SCOPE_RECONCILIATION_ARCHIVE_REQUIRED`),
rejects ambiguous task or scope matches (`SCOPE_RECONCILIATION_AMBIGUOUS`), preserves
original intent, custody events, checks, and checkpoints, resets plan approval to `DRAFT`,
and is idempotent once canonical.

## `decapod todo journal reconcile-legacy`

Review the canonical main repository's quiesced local legacy TODO input before
normal startup. Only empty/no-schema input and a byte-identical canonical copy
are supported; other layouts remain blocked. The returned review digest binds
source/canonical bytes, Unix identities/modes, policy and actor. Apply using
`--expected-review sha256:<digest>` under an active local session and the explicit
canonical `todo.legacy.reconcile` grant. `--prepare-only` requires that token and
persists an unresolved operation while preserving the source. Resume with the
same apply command after interruption; never edit its receipt to assert success.
There is no arbitrary source/destination or task-import surface. Responses keep
`publication_ready=false` and `completion_proven=false`, including committed
replays. See `docs/architecture/legacy-todo-recovery.md` for bounds and limits.
