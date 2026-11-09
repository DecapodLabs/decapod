# Governance liveness and historical evidence recovery

Audit baseline: accepted master `15b69ce43fc56440d498d52d8726f89d499615fe`
(0.133.0). GitHub search `repo:DecapodLabs/decapod is:issue author:worldofgeese
created:>=2026-09-01` returned exactly **36 issues**, all currently closed.
Their bodies, comments, closing events, referenced PR metadata and merged Git
changes were inspected. A closing comment is distinguished from an automatic
closing event; incidental references from other repositories are excluded.
“Implemented” below means the route is present in current source, not that every
historical environment or hosted acceptance scenario was rerun.

PR [#1566](https://github.com/DecapodLabs/decapod/pull/1566) merged at
`a2208585578b2cc857413011981f766885f9882a` on 2026-11-09. Its final branch
contained later maintainer-directed changes after the earlier `24d5aee9`
candidate. The PR discussion explicitly retained failed proof and missing
per-commit checkpoints; merge is an acceptance event, not retroactive proof.
Official 0.133.0 was installed through the supported session update path.
A new actor acquired task custody, created a managed worktree from current
master and completed task-bound orientation without a maintenance exception.

## Issue and implementation ledger

| Report | Closure and subsequent changes | Current disposition and limits |
| --- | --- | --- |
| [#1290](https://github.com/DecapodLabs/decapod/issues/1290) — [BUG] workspace prune never removes the worktree workspace ensure creates | closing event: [#1292](https://github.com/DecapodLabs/decapod/pull/1292) (`f0f0c79cada6`) | Current-worktree preservation is intentional; explicit skipped/current_workspace reporting is implemented. Not a promise that every original pruning failure was the same cause. |
| [#1294](https://github.com/DecapodLabs/decapod/issues/1294) — decapod validate times out (VALIDATE_TIMEOUT_OR_LOCK) after decapod init --force --proof --all in an existing workspace | closing comment/reference: [#1301](https://github.com/DecapodLabs/decapod/pull/1301) (`2e1177103f81`) | Bounded gate work/diagnostics implemented; later independent pipe deadlock #1526 demonstrates that timing cleanup was not a complete liveness solution. |
| [#1295](https://github.com/DecapodLabs/decapod/issues/1295) — decapod workspace ensure --container: storage error 'no such table: risk_zones' on decapod v0.99.7 | closing comment/reference: [#1301](https://github.com/DecapodLabs/decapod/pull/1301) (`2e1177103f81`) | Canonical schema preparation exists before workspace lookup. Fresh 0.133.0 task/claim/workspace bootstrap succeeded during this audit. |
| [#1296](https://github.com/DecapodLabs/decapod/issues/1296) — decapod validate --refresh-specs produces non-deterministic spec_input_hash/content_hash across identical-version evaluator runs | closing comment/reference: [#1301](https://github.com/DecapodLabs/decapod/pull/1301) (`2e1177103f81`) | Stable path/config normalization exists. Current generator and epoch binding remain distinct from hosted cross-platform proof. |
| [#1297](https://github.com/DecapodLabs/decapod/issues/1297) — Validator runtime sits at 120-second timeout boundary | closing comment/reference: [#1301](https://github.com/DecapodLabs/decapod/pull/1301) (`2e1177103f81`) | Slow scans and repeated primitives work addressed; later pipe deadlock is a different waiting cycle, not malformed reporter input. |
| [#1298](https://github.com/DecapodLabs/decapod/issues/1298) — TODO list and infer orientation use inconsistent task schema | closing comment/reference: [#1301](https://github.com/DecapodLabs/decapod/pull/1301) (`2e1177103f81`) | Todo/orientation resolve canonical owning-store state; fresh task-bound orientation succeeded in this workstream. |
| [#1299](https://github.com/DecapodLabs/decapod/issues/1299) — workspace status hangs during stale-workspace scan | closing comment/reference: [#1301](https://github.com/DecapodLabs/decapod/pull/1301) (`2e1177103f81`) | workspace status is observational; it must not launch the validator or remove another agent’s workspace. |
| [#1300](https://github.com/DecapodLabs/decapod/issues/1300) — Fresh workspace cannot resolve control-plane database | closing comment/reference: [#1301](https://github.com/DecapodLabs/decapod/pull/1301) (`2e1177103f81`) | Owning-repository store resolution exists. A workspace-local missing projection is not lack of user intent. |
| [#1303](https://github.com/DecapodLabs/decapod/issues/1303) — Specs generator appends duplicate `## Codebase Attestation` blocks with orphan `:end` sentinels instead of replacing the sentinel-delimited region | closing event: [#1305](https://github.com/DecapodLabs/decapod/pull/1305) (`c99a168bfa68`), [#1309](https://github.com/DecapodLabs/decapod/pull/1309) (`1bf494429737`) | Reporter observed recurrence after #1305; #1309 preserves authored boundaries. Convergence and loss prevention need separate assertions. |
| [#1304](https://github.com/DecapodLabs/decapod/issues/1304) — repomap_determinism walks gitignored build trees, making the 120s validate bound reachable on a first validate after a normal dependency install (0.99.10) | closing event: [#1305](https://github.com/DecapodLabs/decapod/pull/1305) (`c99a168bfa68`) | Tracked/non-ignored discovery excludes dependency trees. Fallback and source-proof discovery have separate bounds. |
| [#1307](https://github.com/DecapodLabs/decapod/issues/1307) — decapod data broker: divergence detection (verify) has no repair primitive, pushes agents toward raw sqlite3 access | closing event: [#1309](https://github.com/DecapodLabs/decapod/pull/1309) (`1bf494429737`) | Explicit preview/apply preserves a pending audit event and records unknown/abandoned rather than inventing success. This is the useful non-waiving recovery precedent. |
| [#1308](https://github.com/DecapodLabs/decapod/issues/1308) — decapod validate --refresh-specs silently regenerated a governed spec section, destroying an authored bullet the same run's contract requires agents to add | closing comment/reference: [#1309](https://github.com/DecapodLabs/decapod/pull/1309) (`1bf494429737`) | Generated-region ownership and failure-before-overwrite checks exist. Authored prose is still the agent’s responsibility. |
| [#1311](https://github.com/DecapodLabs/decapod/issues/1311) — specs.refresh fails via Capability adapter with SQLite 'malformed' error on a store that broker verify and validate both report clean (0.99.12) | closing comment/reference: [#1315](https://github.com/DecapodLabs/decapod/pull/1315) (`41aa1fe0149b`) | Explicit specs.refresh avoids unrelated database lifecycle work. Historical corruption causation was never proven by a clean broker scan. |
| [#1312](https://github.com/DecapodLabs/decapod/issues/1312) — govern trajectory init hard-resets trajectory.json; no way to hold two concurrent runs' evidence (unlike plan.json's accumulation) | closing event: [#1315](https://github.com/DecapodLabs/decapod/pull/1315) (`41aa1fe0149b`), [#1317](https://github.com/DecapodLabs/decapod/pull/1317) (`7681d699b859`), [#1334](https://github.com/DecapodLabs/decapod/pull/1334) (`84cc5dab1336`) | Archives were later deliberately reverted (#1334). Current Git-owned document, guarded replacement and explicit PR boundaries supersede the old archive contract. |
| [#1313](https://github.com/DecapodLabs/decapod/issues/1313) — Reproducible SQLite index corruption on shared decapod.db under concurrent container/host access; REINDEX fails, dump+reload recovers cleanly | closing comment/reference: [#1315](https://github.com/DecapodLabs/decapod/pull/1315) (`41aa1fe0149b`), [#1317](https://github.com/DecapodLabs/decapod/pull/1317) (`7681d699b859`), [#1321](https://github.com/DecapodLabs/decapod/pull/1321) (`1eb30e8ffcf9`) | Dactyl-backed verify/backup/recover and bounded lifecycle coordination exist. Arbitrary external writers and unsupported mount locking remain outside their guarantee. |
| [#1314](https://github.com/DecapodLabs/decapod/issues/1314) — No redaction/normalization for absolute local paths embedded in validation.json / trajectory.json | closing comment/reference: [#1315](https://github.com/DecapodLabs/decapod/pull/1315) (`41aa1fe0149b`) | New persisted paths are normalized/redacted. This does not retroactively scrub already-published historical artifacts. |
| [#1323](https://github.com/DecapodLabs/decapod/issues/1323) — Policy gate denies federation.rebuild/supersede unconditionally; approval_categories = [] does not satisfy it | closing event: [#1328](https://github.com/DecapodLabs/decapod/pull/1328) (`329d586bae91`) | Operation identity now matches configured approval policy; empty category policy does not bypass independent trust/zone checks. |
| [#1360](https://github.com/DecapodLabs/decapod/issues/1360) — Container workspace validate corrupts host decapod.db (SQLite WAL over virtiofs) | closing event: [#1364](https://github.com/DecapodLabs/decapod/pull/1364) (`814595db931f`) | Explicit container launcher uses a verified private snapshot; automatic shared-store runs have a distinct contract. Real VM filesystem lock propagation is not inferred from fixture success. |
| [#1361](https://github.com/DecapodLabs/decapod/issues/1361) — [BUG] Per-target approvals never satisfy todo archive (and other gates): check_approval always gets target_path=None | closing event: [#1362](https://github.com/DecapodLabs/decapod/pull/1362) (`3b32e376a081`) | Target/scope/expiry/revocation matching exists. General action-wide compatibility remains; bounded handoff authorization uses the stricter exact-target path. |
| [#1431](https://github.com/DecapodLabs/decapod/issues/1431) — [BUG] Repositories other than Decapod cannot pass local readiness: only decapod.pr-readiness.v1 is accepted | closing event: [#1434](https://github.com/DecapodLabs/decapod/pull/1434) (`2f96c08061e4`) | Consumer repository-local readiness exists. It is bounded local proof, not complete hosted CI/release coverage. |
| [#1470](https://github.com/DecapodLabs/decapod/issues/1470) — Source inventory lists both Justfile and justfile on a case-insensitive file system, so repo.pr-readiness.v1 stops at 'missing bound input: justfile' | closing comment/reference: [#1481](https://github.com/DecapodLabs/decapod/pull/1481) (`46b22c8f7429`), [#1520](https://github.com/DecapodLabs/decapod/pull/1520) (`caf732e04866`) | Actual spelling is preserved; issue comments document released APFS acceptance. Distinct files/hardlinks must remain distinct where supported. |
| [#1473](https://github.com/DecapodLabs/decapod/issues/1473) — PROOF_HOOK_FAILED deadlock: qa verify recover and regen require the validation pass that the hook prevents | closing event: [#1482](https://github.com/DecapodLabs/decapod/pull/1482) (`9740b2fee6f9`), [#1497](https://github.com/DecapodLabs/decapod/pull/1497) (`189bb4e6692a`), [#1509](https://github.com/DecapodLabs/decapod/pull/1509) (`3f7620bf3585`), [#1524](https://github.com/DecapodLabs/decapod/pull/1524) (`47b86d3f538c`) | Scoped fresh verification capture and atomic recovery exist; #1509 fixed artifact workspace resolution and #1524 fixed transient replay identity. They preserve old completion history. |
| [#1474](https://github.com/DecapodLabs/decapod/issues/1474) — Native validation can never pass publication readiness: required validation receipt is never written | closing event: [#1483](https://github.com/DecapodLabs/decapod/pull/1483) (`0bf43e4c65fc`), [#1499](https://github.com/DecapodLabs/decapod/pull/1499) (`0a900ba3117c`), [#1511](https://github.com/DecapodLabs/decapod/pull/1511) (`24f97dd9d38f`) | Native verdict/receipt publication is paired outside frozen candidate. The issue closed through diagnostics work while comments still explicitly lacked some live failing/stale controls; closure alone is not acceptance proof. |
| [#1475](https://github.com/DecapodLabs/decapod/issues/1475) — Feature request: Dagger as a validation proof provider (local dagger check or the GitHub check run) | closing comment/reference: [#1484](https://github.com/DecapodLabs/decapod/pull/1484) (`7b6081305f5a`), [#1507](https://github.com/DecapodLabs/decapod/pull/1507) (`705a47d80778`) | Withdrawn architectural direction: adapter introduced by #1484 was removed by #1507. Not a currently supported proof provider. |
| [#1476](https://github.com/DecapodLabs/decapod/issues/1476) — 0.121.2: caller identity changes during concurrent TODO operations | closing comment/reference: [#1485](https://github.com/DecapodLabs/decapod/pull/1485) (`41b2f983b706`), [#1486](https://github.com/DecapodLabs/decapod/pull/1486) (`2ea3ae5c8b78`), [#1515](https://github.com/DecapodLabs/decapod/pull/1515) (`958e8fd18048`), [#1517](https://github.com/DecapodLabs/decapod/pull/1517) (`a45bdc81541f`) | Per-request caller context and required socket traversal exist. Comments identify actual released eight-caller observations; unmerged #1503 is not credited as a merged fix. |
| [#1477](https://github.com/DecapodLabs/decapod/issues/1477) — 0.121.2: todo handoff uses unknown instead of the caller; recovery guidance disagrees | closing comment/reference: [#1486](https://github.com/DecapodLabs/decapod/pull/1486) (`2ea3ae5c8b78`), [#1566](https://github.com/DecapodLabs/decapod/pull/1566) (`a2208585578b`) | Caller/owner/recipient separation and exact single-use handoff exist; #1566 strengthens generation/owner fencing and current CLI approver attribution. |
| [#1478](https://github.com/DecapodLabs/decapod/issues/1478) — 0.121.2: prompt gate blocks quoted read-only HTTPS diagnostics | closing comment/reference: [#1487](https://github.com/DecapodLabs/decapod/pull/1487) (`1b1258bd9cc3`), [#1566](https://github.com/DecapodLabs/decapod/pull/1566) (`a2208585578b`) | Bounded read-only HTTPS is recognized without exempting arbitrary quoted history. #1566 separately classifies unevaluated invalid input; real policy denial stays denied. |
| [#1479](https://github.com/DecapodLabs/decapod/issues/1479) — 0.121.2: mandatory initialization orders orientation before its required worktree | closing event: [#1488](https://github.com/DecapodLabs/decapod/pull/1488) (`aab2ab879449`), [#1498](https://github.com/DecapodLabs/decapod/pull/1498) (`969630770eec`), [#1566](https://github.com/DecapodLabs/decapod/pull/1566) (`a2208585578b`) | Text ordering fix gained a real CLI bootstrap regression (#1498). #1566 then fixed the distinct task-acquisition/destination-admission cycle. |
| [#1480](https://github.com/DecapodLabs/decapod/issues/1480) — No CLI path to raise agent trust; basic trust blocks shared claims on disjoint files | closing event: [#1500](https://github.com/DecapodLabs/decapod/pull/1500) (`99e07b3060d4`), [#1513](https://github.com/DecapodLabs/decapod/pull/1513) (`7e2a46d5b916`), [#1522](https://github.com/DecapodLabs/decapod/pull/1522) (`558d2ac11280`), [#1566](https://github.com/DecapodLabs/decapod/pull/1566) (`a2208585578b`) | Original file-scope/shared opt-in fix still serialized overlaps by design. #1566 changes independent-task semantics: source overlap is coordination metadata; task and observed checkout custody remain exclusive. |
| [#1526](https://github.com/DecapodLabs/decapod/issues/1526) — validate times out when worker JSON fills stdout pipe before parent reads it | closing event: [#1532](https://github.com/DecapodLabs/decapod/pull/1532) (`f31bf15256ba`) | Concurrent bounded draining/reaping exists. A timeout remains distinct from an evaluated failed policy gate. |
| [#1527](https://github.com/DecapodLabs/decapod/issues/1527) — qa verify recover --stale runs validation instead of listing stale items | closing event: [#1535](https://github.com/DecapodLabs/decapod/pull/1535) (`e9b7a0b22a26`) | Stale-only listing short-circuits before validation. Observation does not acquire a new proof requirement. |
| [#1531](https://github.com/DecapodLabs/decapod/issues/1531) — repo.pr-readiness.v1: no reviewed way to declare a CI-only workflow in source_coverage | closing event: [#1542](https://github.com/DecapodLabs/decapod/pull/1542) (`2ae482f9eae6`) | Reviewed ci_only rows disclose limitations instead of false local coverage. They never become successful hosted checks. |
| [#1533](https://github.com/DecapodLabs/decapod/issues/1533) — Version checks get HTTP 403 from crates.io, so capabilities always reports is_latest: true | closing event: [#1539](https://github.com/DecapodLabs/decapod/pull/1539) (`3408295551ea`) | Identifying registry requests and opt-in installation exist. This audit used session acquire with an explicit opt-in in a disposable project to install official 0.133.0; no root config was edited. |
| [#1534](https://github.com/DecapodLabs/decapod/issues/1534) — Task journal: historical gaps make journal verify and checkpoint fail, with no supported repair | closing event: [#1540](https://github.com/DecapodLabs/decapod/pull/1540) (`c6b7919d823f`) | Explicit current-projection gap checkpoint exists. It preserves domain events but subsequent journal completeness must not be advertised as proof of the missing transitions; review authority/propagation separately. |
| [#1536](https://github.com/DecapodLabs/decapod/issues/1536) — Nix flake invocation cannot load its SQLite runtime dependency | closing event: [#1543](https://github.com/DecapodLabs/decapod/pull/1543) (`d66d7a9524d0`) | Nix package closes over SQLite runtime. Guix/Nix host execution remains distinct from native macOS fixture proof. |
| [#1544](https://github.com/DecapodLabs/decapod/issues/1544) — Name the incompatible executable in native ELF validation errors | closing event: [#1551](https://github.com/DecapodLabs/decapod/pull/1551) (`84ad84ac8e0d`) | Native tool diagnostics carry check/executable/format identity. Rejection of a foreign executable remains correct. |

## Dependency model

| Transition | Authoritative preconditions / producer | Failure, authorized next transition, and evidence | Concurrency / authority boundary |
| --- | --- | --- | --- |
| Establish session | Current actor and local session; `session acquire` | Missing identity is not another actor. Local operation does not require a cloud login. | Caller context remains request-scoped; a local label is not Supabase authority. |
| Create / claim task | Canonical store, actor, dependency state, CAS owner/generation; `todo add`, `todo claim` | Acquisition is custody-pending, not filesystem admission. Claim conflicts never emit successful ownership events. | Same-task incompatible claims have one winner; unrelated source overlap does not lock the repo. |
| Admit workspace | Current actor/task lease plus observed destination checkout; `workspace ensure` | Provision and verify destination before authoring; no requirement for an already-existing destination at claim time. | A writable checkout is exclusive; symlink/branch/lease changes invalidate admission. |
| Handoff / release | Owner, generation, receiver, applicable exact bounded approval; `todo handoff`, `todo yield` | Cross-owner recovery needs real authority; no owner impersonation. | CAS advances fencing; another agent cannot release or steal the current owner’s custody. |
| Complete task | Dependencies, current custody and substantive task-bound verification; `todo done --validated` | Capture fresh proof or explicitly recover completed-task verification. Do not rewrite old completion as a new observed event. | State, proof and event transition atomically; changed material invalidates stale capture. |
| Review plan/spec | Current local plan and actual code/spec hashes; `govern plan review-spec` | Missing review is recoverable before publication. An unresolved human decision is a separate state; `resolve-spec-review` records intent provenance, not independently authenticated human identity. | Code/spec edits stale the review; refreshing generated regions alone need not. |
| Record trajectory | Current workstream, scope and preserved prior journal; `govern trajectory record` | Record current observations, failed checks and uncertainty. Do not overwrite unarchived prior work. | Each workspace has its own journal; accepted Git retains earlier workstreams. |
| Checkpoint | Staged material, current journal, new cumulative checkpoint; `govern artifacts checkpoint` | A checkpoint can cover work now. It cannot be inserted into an immutable earlier commit. | Locked document RMW preserves checkpoint identity and prior records. |
| Inspect checkpoint history | Actual immutable base/head and per-commit source records; `verify-checkpoints --format json` | Enumerate exact commits/trees/obligations. Missing historical checkpoint is permanently unproven in that history; unreadable input is unavailable, not a proven gap. | Capture ref IDs once; inspection never mutates admission or the source branch. |
| Recover work from a historical gap | Fresh claimed workspace at accepted base plus identified source and reason; `record-history-recovery` | Record a present-time observation, review/reauthor source, checkpoint it, then resolve the active recovery obligation with current references. The original gap remains unproven and archived. | Established actor owns the new checkout; original task, branch, commits and evidence stay intact. |
| Validate / publish | Exact candidate/target, current governance/spec review and complete independent proof; `decapod validate` | Fresh technical evidence can clear current failures. Unsupported authority and unavailable provider access remain blocked, not green or silently skipped. | Accepted executor/controller retains final authority; target, candidate or policy changes invalidate readiness. |
| Diagnose / maintain database | Current backend, canonical accepted policy and exact capability; existing Dactyl operations, proposed #1568 admission | Live contention permits bounded observation/wait/retry; corruption or replacement requires a distinct higher-risk path. | Descriptor lifetime releases OS locks; no lock-file deletion, process killing, custody transfer, or cloud privilege inferred from local policy. |

There is **no single root cause** for all 36 reports. Several share a design
pattern: a remediation path was required to pass the invariant it was supposed
to establish, or an observational command inherited mutation/validation
prerequisites. Other causes are ordinary I/O deadlocks, missing parameter
binding, filesystem behavior, authority/proof conflation, and incomplete
acceptance coverage. Fixing error text alone does not repair those transitions.

## Minimal deadlocks and current recovery

- **Approval identity (#1323/#1361):** approve action/target A; mutation checks
  action/target B or `None`; no grant can satisfy the actual check. The fix
  carries the same operation/target/scope into admission and the transaction.
- **Bootstrap (#1479/#1480 and #1566):** orientation or claim requires an admitted
  worktree; provisioning requires claim. The corrected path establishes actor,
  claims task custody, provisions/verifies destination, then admits authoring.
- **Verification (#1473):** proof hook rejects old completion; recovery first
  runs unscoped validation containing that hook. Scoped fresh evidence capture
  and atomic finalization break the dependency without replaying completion.
- **Receipt (#1474):** publication requires the receipt produced only after
  publication readiness. Native validation now writes the external receipt and
  verdict together after actual execution; project-check is not that receipt.
- **Task journal (#1534):** verify finds an uncheckpointed suffix; checkpoint
  calls the same suffix check; restore accepts only known predecessors. Explicit
  gap acceptance records current projection plus affected event range. It does
  not reconstruct the earlier sequence or independently authenticate a human.
- **Authored Git history (#1566):** create a material commit without its own
  checkpoint, then add one at the tip. The old immutable commit still lacks
  the required evidence. Official 0.133.0 reproduces this at
  `21694285cca2b6259f9ea3c09e5440029c02f397` in the original PR range. A tip
  checkpoint, fake receipt, successful merge or changed branch name cannot
  prove that earlier checkpoint existed. This PR supplies the fresh-workstream
  recovery route below, with explicit custody of the historical gap.
- **Spec review (#1566):** publication rejects missing current reviews. The
  supported `review-spec` transition must remain reachable before validation,
  bind actual code/spec bytes, and clear the review obligation without clearing
  checkpoint, proof, trust, or other publication obligations.

## Focused repair and supported sequence

The checkpoint verifier and JSON assessor share one per-commit implementation.
The assessor distinguishes proven, unproven history, invalid history and
unavailable inputs, and reports every observed per-commit failure rather than
only the first. A history assessment always states `publication_ready=false`: it
does not execute the other required publication gates.

```sh
# Observe; nonzero remains nonzero for unproven/invalid history.
decapod govern artifacts verify-checkpoints --base-branch ACCEPTED_SHA \
  --head-ref FAILED_SOURCE_SHA --target-base master --format json

# Preserve the source branch and its task. Create a new task/workspace normally.
decapod todo add 'Reauthor reviewed work with fresh evidence' --scope src,tests
decapod todo claim --id NEW_TASK_ID
decapod workspace ensure --branch agent/ACTOR/NEW_TASK_ID-recovery
# Enter the returned worktree at the accepted target.
decapod govern artifacts record-history-recovery \
  --source-ref FAILED_SOURCE_SHA --reason 'Reviewed recovery rationale'
```

The record binds source commit/tree/governance blob, destination change/base,
current actor, reason and exact assessment. Authority is only current task
custody; `historical_proof=not_proven` and `exception_authorized=false` are
explicit. Repetition is idempotent only for identical context. Conflicting
reasons/actors or an already-advanced destination do not reset evidence.

Review and reauthor selected source in the new worktree. Do not import the old
governance record, generated fingerprints or invalid commits as ancestors.
Review affected specs through `govern plan review-spec`, refresh supported
projections, stage material, checkpoint and commit the current observation and
work. The existing `resolve-obligation` command may then record the disposition
of the active recovery work with references to that archived observation and
the source. It cannot erase an uncommitted recovery observation; the CLI first
requires it to exist in Git. Add another current checkpoint before committing
that resolution. Run full independent validation on the exact new candidate.

This recovers the **work**, not proof of the old history. Existing strict
checkpoint admission remains unchanged: resolving a current obligation never
waives the original missing checkpoint. No authenticated-human exception path
is invented. A local trust label or a decision-reference string does not
establish that authority. Future exception-based publication needs an explicit
verifiable local maintainer contract or the separately implemented connected
authority in Propodus #88. Such exceptions must remain distinguishable from
proof; they are not enabled in this PR. The independently scoped local authority
and truthful exception judgment prerequisite is [#1569](https://github.com/DecapodLabs/decapod/issues/1569).

## Database maintenance capability analysis (#1568)

[Decapod #1568](https://github.com/DecapodLabs/decapod/issues/1568) is a separate
implementation scope from immutable Git recovery. Existing `storage_lock.rs`
provides bounded shared connection / exclusive lifecycle locks, typed timeout
and upgrade refusal, and descriptor-owned release. Existing database maintenance
uses Dactyl integrity checks, online backup and explicit logical recovery.
Those mechanisms must be reused rather than replaced.

The proposed local-only gate should resolve accepted canonical project config,
default disabled, bind its policy identity/hash, and recheck exact capability
and datastore/custody before execution. A worktree’s changed config cannot
authorize itself. A small fixed capability vocabulary covers inspection,
integrity verification, lock diagnosis, safe idempotent bounded retry and
backup. Existing ordinary operations must retain their semantics. Recovery,
restore, migration, replacement, raw SQL, forced lock deletion and another
actor’s custody must not become enabled by this flag. Releasing an OS lock is
only descriptor lifetime; `todo yield` releases only the actor’s actual task
lease under its separate domain contract.

This could reduce diagnostic/maintenance admission deadlocks around #1307,
#1313 and #1360, and distinguish a current live owner from unavailable owner
knowledge. It cannot prove missing checkpoints, satisfy spec review, fabricate
validation receipts, repair unsupported VM lock propagation, or grant a cloud
role. Unrestricted access to canonical policy or the host OS is outside the
agent-level enforcement boundary.

Cloud maintenance remains denied/unimplemented until project opt-in **and**
verified repo-admin or scoped delegated-lead authority are server-enforced by
[Propodus #88](https://github.com/DecapodLabs/propodus/issues/88). Paid entitlement,
repository write access and local agent labels are not that authority. The
private issue was inspected; no private SQL, credentials or deployment material
is copied here. No Propodus or Dactyl repository changes are part of this PR.

Defer #1568 implementation and its disabled/opt-in/self-escalation/contention/
retry/idempotency/audit tests to its dedicated PR. Combining it here would add a
second independent authorization change and defeat the one-focused-repair
constraint. No new maintenance capability is claimed implemented or tested.

## Proof ledger

- Official released 0.133.0 installed through session acquisition; real current
  task claim, managed workspace creation and task-bound orientation succeeded.
- Focused governance-document regression suite: 72 passed before the final
  immutable-reader refinement; final verification is recorded with the candidate.
- Real two-actor CLI recovery fixture: passed; it preserves original source
  commit/tree and custody, denies foreign-checkout mutation and premature gap
  resolution, and proves current checkpoint history without proving the old
  source. The extended fixture also reproduced `SPEC_REVIEW_REQUIRED`,
  resolved it through actual `review-spec`, and passed publication-material
  verification. These checks do not establish hosted publication readiness.
- The broad local unit run reported 1,604 passed, 41 failed and 8 ignored.
  Failures include noncanonical macOS temporary paths, bounded epoch directory
  enumeration and platform recovery/worker fixtures. The epoch regression was
  independently reproduced on clean accepted master, even with canonical
  `TMPDIR=/private/tmp`, and is tracked separately in [#1571](https://github.com/DecapodLabs/decapod/issues/1571).
  Its `/dev/fd` directory enumeration fails with `ENOTDIR`; this repair does not
  change that reader or claim these failures are green.
- Formatting, final Clippy/unit/integration reruns and independent exact-candidate
  provider results are separate obligations. No result here implies hosted
  Supabase, unsupported maintenance, or full publication proof.

## Reviewed release inventory reconciliation

The independent installed 0.133.0 controller compared a clean accepted-master
snapshot and this worktree: both discover the same 119 inputs, with digest
`sha256:d8c65ab6901a57c4df55f3914dd0eeaecbf7338177233831854ade1304e53361`.
The inherited declaration still described 0.132.0. Comparing every inventoried
path with the independently captured 0.132.0 inventory identifies only the
already-accepted Cargo.toml/Cargo.lock release-version changes. The declaration
was deliberately reconciled after that complete review. No executor, source-CI
helper, workflow, required check or capability authorization was changed.

## CI and fixture reconciliation after `5557b015`

The original recovery implementation and historical references are preserved.
Accepted master `15b69ce43fc56440d498d52d8726f89d499615fe`, tested from a clean
source snapshot with a separate Cargo target directory, reproduces the seven
original workbranch failures (2 passed, 7 failed). The original candidate added
a passing recovery fixture (3 passed, 7 failed). After correcting only fixture
setup and expectations, all ten tests pass. No production custody or actor
resolution code changed.

The fixtures now use the actor-bound CLI for `workspace ensure`, rather than
calling a library operation while its ambient caller identity is absent.
`agent.init` and status are observations in an unadmitted manual worktree; they
must preserve inherited governance until explicit checkout admission. Foreign
and nonexistent branch-task contexts are rejected by `workspace ensure`, not
by expecting a read-only observation to mutate or acquire custody. Finally,
`session release` intentionally clears task assignments. The supported resume
sequence is a fresh task claim followed by authoring admission, not reuse of
the old binding. The negative assertions and actual prerequisite-receipt checks
remain in `tests/workbranch_governance.rs`.

The previous 41 unit failures divide into these observed causes:

| Cause | Failures | Disposition |
| --- | ---: | --- |
| Bounded epoch directory enumeration | 21 | Twelve execution-checkpoint, three bounded-input and six workunit tests share the macOS descriptor-enumeration failure, including assertions masked by the earlier input error; retained in #1571. |
| Noncanonical temporary paths | 15 | Snapshot/host-adoption fixtures supply an alias where the production boundary requires a canonical root. Rerun with `TMPDIR=/private/tmp`; do not relax canonical identity. |
| Permission/recovery diagnostics | 2 | Host-specific fs-permission recovery expectations; unrelated production paths are unchanged. |
| Vega cache identity/permissions | 2 | Temporary-path/cache fixture constraints; unchanged by history recovery. |
| Independently reviewed toolchain inventory | 1 | Supplemental-worker fixture fails before execution; no successful worker proof is inferred. |

The macOS epoch reader is deliberately deferred. Its shared `RepoSignalIo`
contract returns `fs::ReadDir`; a descriptor-owned Darwin enumeration repair
needs an independently tested iterator/ownership change. Replacing it with an
unverified pathname or disabling the bounded reader is not an admissible
minimal fix to this recovery PR.

### External workflow registration

All five configured source workflows match the existing `agent/**` push. Native
GitHub runs for exact head `5557b01553c083ee95085d24959fc4553e508a39` completed:
Deploy Docs, Constitution Sync, Documentation Sync and Decapod Validate passed;
CI failed, including HTTP 504 while fetching pinned `rules_rust`. Those native
results do not satisfy the configured Buildkite contract.

The active Buildkite repository webhook received this exact push/ref at
2026-11-09T05:45:11.988Z and returned HTTP 200. A complete later exact-head check
registry still exposes only build 5952, the passed push execution for
`.github/workflows/decapod-validate.yml`. The other four workflows have no
discoverable Buildkite bindings. Thus source trigger conditions and GitHub
event delivery are demonstrated; downstream importer selection/registration
and any unregistered build associations require private service inspection.

Public metadata identifies the existing GitHub Actions trigger settings at
`/decapodlabs/decapod/settings/triggers/01a0bcf1-5df5-7ebf-a2f3-936b1aae82eb`.
The supported Buildkite CLI reports no stored credentials for `decapodlabs` in
this environment. An authorized service administrator must inspect the existing
trigger against the five already-reviewed workflows; this PR changes neither
service identity nor required workflow/check definitions. Evidence is recorded
in #1573. HTTP 403 remains an external quota condition, not a provider-client
engineering change or a passing result.

### Publication boundary

`AGENTS.md` rule 7 and the installed `decapod validate` command contract expressly
hold creation of every PR, including drafts, until full exact-candidate
validation succeeds. The supported `workspace publish` path enforces this
readiness boundary before PR creation. Neither a draft label, a successful
push-only check, native GitHub success nor a prerequisite receipt waives it.

### Reconciled local results

Both full unit suites used `TMPDIR=/private/tmp` and `--test-threads=1`; the
accepted checkout remained clean and used a separate Cargo target directory to
avoid artifact reuse between source snapshots. Accepted master reports
**1,613 passed, 26 failed, 8 ignored**; this candidate reports **1,620 passed,
25 failed, 8 ignored**. Every remaining candidate failure is also present on
accepted master. The sole baseline-only failure is the shared-directory mode
fixture already corrected in `5557b015` to verify the mode actually established
by the host. No new unit failure was introduced.

Compared with the earlier noncanonical run, all fifteen canonical-root failures
and the supplemental toolchain-inventory failure now pass. The remaining
twenty-five failures are twenty-one bounded epoch reader cases, two permission/
recovery cases, and two Vega cache cases. These remain failed; no platform
validation or proof boundary was relaxed.

Final focused commands and outcomes for the fixture reconciliation:

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`: passed.
- `cargo test --locked --lib core::governance_document::tests:: -- --test-threads=1`: 72 passed.
- `cargo test --locked --test workbranch_governance -- --test-threads=1`: 10 passed, including actual CLI recovery/spec review.
- `cargo test --locked --test kernel_bootstrap --test workspace_interlock -- --test-threads=1`: two passed.
- `cargo test --locked --test contract_conformance --test spec_conformance --test constitution_claims --test substrate_integrity -- --test-threads=1`: 43 passed across the four suites.

Rust test invocations above used the same canonical temporary directory. They
are local regression evidence, not hosted publication authority. All historical
proof distinctions, datastore boundaries and required CI identities remain
unchanged.
