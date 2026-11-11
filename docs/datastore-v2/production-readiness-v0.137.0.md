# v0.137.0 datastore production-readiness review

Status: source review and proposed repair requirements. The repairs below are
not implemented by this document. This is not a production certification,
a completed migration, or a passing validation receipt.

## Decision

Decapod v0.137.0 has substantive local integrity protections, but the reviewed
release does not establish a production-qualified, equivalent SQLite and native
Supabase governance model. Enterprise production sign-off should remain pending
until the concrete gaps and proof requirements below are resolved.

“Acceptable to Capital One or OpenAI” is not an independently verifiable
engineering standard: this review has no access to either company's internal
acceptance process. The actionable standard here is explicit invariants,
authenticated cloud authority, safe upgrades and recovery, bounded resource
use under a declared workload, and reproducible evidence tied to released
software.

The product contract remains one logical governance model. Solo local operation
requires no external account. Cloud authority is authenticated and enforced by
Propodus. SQLite and PostgreSQL may use different physical layouts, but revision,
custody, evidence and completion semantics need an explicit equivalence contract.
Local user, organization and repository context must be representable without
turning locally asserted identity into authenticated cloud authority.

## Evidence and scope

| Component | Reviewed identity |
| --- | --- |
| Decapod release | v0.137.0, 82388e0c57197c0b0622cb023210f6f908d78abe |
| Propodus source | 177b7fd95ff996a4215a9e56439c79b6dba8408c |
| Dactyl source and published version | 1fa52a232e1d34a27be28d158298805ffc175ffc, dactyl-db 0.11.1 |
| Publication base | Decapod master 57a05bb25e8e87d5857d4b63ceca2907a1048cdd |

The publication base includes [PR #1606][rbac-pr] and the v0.137.1 release
metadata update. Findings remain pinned to the reviewed v0.137.0 source.
The review acknowledges
authenticated principal/delegation enforcement in Propodus migration 007; it
does not relabel agent-label impersonation as an unresolved implementation
defect. Merged source and fixture descriptions do not prove hosted deployment.

The findings below come from source inspection. No new Rust test suite,
PostgreSQL integration suite, hosted Supabase workload, power-loss experiment,
backup restoration, or production corruption reproduction ran in this review.
Existing [proof-ledger entries][proof-ledger] retain their original scope and
uncertainties; their reported passes are not new executions for v0.137.0.

## Existing safeguards to preserve

- Local task changes, domain events and checkpoints share a transaction
  ([todo transaction boundary][todo-tx]). Completion checks status/revision,
  dependencies and proof binding before transactional persistence
  ([completion][todo-completion]).
- Local handoff compares assignee and lease generation and consumes approval
  within the custody transaction ([handoff][todo-handoff]).
- Event append serializes sequence allocation, rejects conflicting reused
  identities, and accepts identical retries; stream/sequence uniqueness has a
  database constraint ([event implementation][events]).
- The compaction implementation uses transactional changes, guarded definitions,
  logical copy comparison and foreign-key checks. Its compressed envelope checks
  length and SHA-256 and bounds decompression ([compaction][compaction],
  [codec][codec]).
- Native Propodus RPC has tenant isolation and constrained operations. Migration
  007 binds custody to authenticated principals and implements delegated access
  ([delegated authority][delegation]). Unsupported local/cloud capabilities fail
  explicitly rather than silently switching to local authority.

These protections are meaningful. The remaining findings identify their
boundaries and the evidence needed for a broader claim.

## Findings

### F1. Native cloud transport and governance equivalence are incomplete

Decapod constructs Dactyl's existing Supabase service route
([dactyl.rs][dc-route]). Dactyl 0.11.1 documents that route as the service
/query and /batch endpoints ([route contract][dactyl-route]).
Propodus identifies native Supabase RPC as its canonical architecture and its
integration as an integration candidate ([backend status][propodus-readme]).
A compatibility-service test therefore cannot establish released native
Supabase consumer support.

The release explicitly rejects cloud execution custody, journal operations,
handoff, verified checkout admission, shared resource reservation and validated
completion ([todo][todo], [authoring][authoring],
[resource reservation][cloud-resource]). Those are central governance semantics,
not merely table-layout differences.

Repair requirement: publish native RPC support through Dactyl, integrate the
released dependency in Decapod, and version a shared capability/transition
contract. Carry user/org/repo context consistently, with authentication confined
to the cloud authority boundary. Complete or explicitly scope out unsupported
capabilities before admitting work that requires them. Host checkout facts must
remain independently verified; a client-supplied path is not proof.

### F2. Cloud mutations lack fencing against an earlier claim

Claim, release and completion increment a version but do not compare the
caller's expected revision or claim generation. Release/completion predicates
match task ID, active status and agent label ([cloud mutations][cloud-mutations],
[approved backend statements][profile]).

A source-derived counterexample is a delayed completion from claim A arriving
after the same principal and agent label release A and acquire claim B.
The old request can match B's current state. Authenticated principals and
transaction serialization do not distinguish these two claim lifetimes.
This scenario was not executed against a deployment during the review.

Repair requirement: use the revision/generation from the original admitted
claim in the server-side mutation predicate. Re-reading the newest generation
immediately before submission would not fence the old operation. Bind durable
operation identity, request fingerprint, principal and recorded outcome to the
state/event transaction. Task creation already has operation-ID/fingerprint
reconciliation ([existing add reconciliation][cloud-add]); the requirement is
to qualify every supported transition and uncertain-commit retry.

### F3. The compatibility service does not require every mutation's audit event

The Go service's pair validator checks events that are present, but skips
non-event operations and does not require a following event for each mutation
([validateEventPairs][pair-validation]). Its /query route accepts a single
approved operation, and the database executor calls that validator before
execution ([HTTP preparation][service-http], [database execution][service-db]).

Consequently, the reviewed compatibility path permits an approved state
mutation without its required event. This is a source-derived finding, not a
deployed exploitation result. The native RPC path already rejects unbatched
writes and requires the following event ([native validation][native-pair]).
The defect must not be generalized to that native path.

Repair requirement: enforce the complete mutation/event pairing rule at every
retained write entry point, including /query. A missing event, wrong event,
stale zero-row mutation or failed event insert must not produce a successful
transition. If the compatibility path is retired, prove it is unavailable to
the shipped client/deployment rather than leaving an alternate write route.

### F4. Cloud listing has an ordinary repository-growth ceiling

The cloud list query reads all tasks ordered by updated_at and ID, without a
cursor or page limit ([list implementation][cloud-list]). Both reviewed backend
paths reject more than 1,000 result rows and enforce response-byte bounds
([service bounds][service-bounds], [native bounds][native-bounds]).
Completed tasks count because the query has no status filter.

Repair requirement: version bounded pagination across the client and approved
backend operations. Prefer an immutable ordering key, define membership and
snapshot semantics under concurrent changes, bind cursors to authorized
repository scope, and preserve response bounds. Merely raising the limit moves
the same failure to a larger repository.

### F5. Local journal verification and recovery have aggregate lifetime limits

The journal defines 10,000 tasks, 100,000 records, 32 MiB aggregate data and
8 MiB per checkpoint record as distinct limits ([constants][journal-limits]).
Global capture counts all tasks, including archived tasks
([global capture][journal-capture]). Global verification loads every checkpoint,
and checkpoint loading rejects excess aggregate history
([checkpoint loading][journal-load], [verification][journal-verify]).
Append also rejects prospective growth beyond a selected task's history budget
([append][journal-append]).

These limits govern different paths. An ordinary selected-task mutation does
not count every repository task, so this review does not claim that every
operation stops at 10,001 tasks. Nevertheless, valid individual histories can
collectively exceed global verification/recovery limits. A supported lifecycle
beyond the limits was not established.

Each mutation captures a full projection and reads the selected history
([record_changes][journal-changes]). Storage and latency amplification require
measurement; no measured performance failure is claimed here.

Repair requirement: make verification/recovery use bounded pages and coherent
snapshots, or implement a separately reviewed, verifiable history lifecycle.
Retain historical event bytes, canonical digests, predecessor chains and failure
atomicity. Removing evidence or simply lifting allocation guards is not an
acceptable capacity repair.

### F6. Migration provenance and compressed-value compatibility are incomplete

The applied-migration ledger has no implementation digest. Invalid JSON becomes
an empty/default ledger; migration completion and JSON receipt persistence occur
separately ([entry format][migration-entry], [ledger load][migration-load],
[receipt boundary][migration-receipt]). This demonstrates weak history
qualification and reliance on idempotence after receipt loss; it does not
demonstrate observed data loss.

Repair requirement: validate ledger structure, versions, identity, ordering and
new definition bindings before mutation. Fail explicitly on corruption and
incompatible history. Define interrupted-commit/receipt recovery. Preserve
legacy records as legacy/unattested: computing today's digest cannot attest
which historical implementation ran. A descriptor digest must not be described
as a complete transitive Rust implementation digest.

The compactor writes BLOB envelopes into summary columns declared TEXT
([summary rewrite][summary-rewrite], [column definitions][summary-schema]).
The new reader decodes them; v0.136.4 read those columns directly as strings
([new reader][new-reader], [previous reader][old-reader]). SQLite affinity
permitting this representation does not establish older-client compatibility.
The reviewed migration planner does not establish a universal minimum
reader/writer boundary ([planner][migration-planner]).

Repair requirement: prove either lossless legacy-readable storage or an actual
incompatible-client boundary before access. A new marker ignored by old binaries
cannot itself protect the format. Test real released old/new readers and writers
and preserve summary identities and exact logical bytes.

### F7. Schema qualification and domain authority remain provisional

The inventory identifies release 0.136.4 and an earlier pinned source while
Cargo declares 0.137.0. The test asserts equality between those release fields
([inventory][inventory], [package][cargo], [test][contract-test]). The assertion
is inconsistent in the exact release source. This is a deterministic source
finding, not a claim about the cause of a particular CI failure.

The test compares column names/order and skips absent optional tables. It does
not compare effective types, nullability, defaults, keys, checks, indexes or the
native PostgreSQL catalog. The inventory itself labels those proofs provisional.

The local task DDL leaves several status/priority/boolean/revision/generation
domains and the parent relationship unenforced in SQL ([task DDL][task-schema]).
Rust supplies some additional checks, including journal rejection of negative
revisions/generations. An unrestricted integer accepted by --one-shot is a
concrete input gap ([argument][one-shot]). This is not a claim that every
application invariant is absent.

Tags and dependencies also have parallel representations. Tag writes update
both forms; dependency readiness reads the authored list while edge
synchronization skips unresolved targets ([tags][task-tags],
[dependency handling][task-dependencies]). Convergence must specify canonical
versus derived authority and preserve unresolved intent.

Repair requirement: distinguish historical inventory provenance from current
release qualification, compare complete effective schemas, classify intentional
backend differences, and introduce forward constraints after checking historical
rows. Do not relabel old measurements as new evidence, silently coerce invalid
history, or delete unresolved dependencies to make a constraint pass.

## Repair order and acceptance

The smallest useful sequence is to establish authoritative schema/migration
contracts, close mutation/custody gaps across released transports, and qualify
capacity and recovery against those same semantics. Native cloud deployment
cannot be certified from a basic task CRUD subset.

| ID | Owner / finding | Falsifiable acceptance requirement |
| --- | --- | --- |
| P1 | Decapod / F6–F7 | Corrupt, duplicate, incompatible or mismatched migration receipts fail before datastore, ledger, version-counter or catalog changes; valid legacy provenance remains explicitly unattested. |
| P2 | Decapod / F6 | Actual released old/new binaries demonstrate exact legacy-readable values and safe writes, or explicit incompatible-client rejection before access; interrupted upgrades preserve rows, evidence and truthful receipts. |
| P3 | Decapod + Propodus / F7 | Complete initialized SQLite and native PostgreSQL catalogs, including lazy tables, match reviewed types, defaults, nullability, keys, checks and indexes with intentional differences recorded. |
| P4 | Propodus + Decapod / F2–F3 | Claim A → release → claim B → delayed A mutation changes neither B nor successful event history. Unpaired, mismatched and failed audit operations roll back state on every retained route. |
| P5 | Propodus + Decapod / F2 | A committed request with a lost response reconciles to the original durable outcome; identical retry adds no effects, changed-payload reuse conflicts, and unresolved receipts are retained. |
| P6 | Dactyl + Decapod + Propodus / F1 | The published client executes the native RPC path through actual PostgREST/PostgreSQL; capability mismatch fails before write. Shared scenarios prove dependencies, custody, handoff, revision/proof binding and completion semantics. |
| P7 | All cloud components / F4 | Traverse 2,001+ tasks completely under declared concurrent-update semantics; enforce byte/page bounds and reject cursor scope changes across tenants. |
| P8 | Decapod / F5 | Verify and recover valid histories beyond 10,000 tasks, 100,000 checkpoint records and 32 MiB with bounded memory. Corruption in early/middle/final pages fails, and unsuccessful recovery leaves state unchanged. |
| P9 | Decapod / F5 | Preserve canonical digests and every historical event byte; concurrent mutation cannot yield a successful mixed-snapshot verification. Report long-history mutation and verification latency. |
| P10 | Decapod + Propodus / F7 | Invalid domain/relationship writes fail at the declared boundary; migration reports incompatible historical rows without silent coercion. Authority tests cover tags and unresolved dependencies. |
| P11 | Release/deployment owners | Bind full configured validation and migration, backup/restore and workload evidence to the actual candidate/client/backend versions. Record the supported workload and recovery objectives before sign-off. |

All requirements above remain open in this documentation PR. The acceptance
matrix is a repair specification; it does not create test results or substitute
for implementation.

## Execution and publication record

The released Decapod binary allowed the incoming request, and a local session
was acquired. agent.init then failed during migration with “Operation not
permitted (os error 1).” A separate diagnostic confirmed that this execution
environment denies AF_UNIX socket creation. Decapod's owner discovery propagates
that error before its physical datastore fallback ([owner connection][owner]).
The supported execution escalation was rejected because sandbox escalation is
disabled.

The initial draft was published from an ordinary Git worktree without a
Decapod-admitted task or successful validation receipt. Its required governance
and generated-spec checks failed, and that original evidence remains inspectable
at commit `2b40eaa70314d29933a19f5f38a635c43ba36929`.

The subsequent repair acquired Decapod task `cicd_01m74qgbr134hv26` and
created a Decapod-admitted worktree from immutable accepted master
`57a05bb25e8e87d5857d4b63ceca2907a1048cdd`. This document was
re-authored in that worktree. Publication readiness must be established by a
real staged checkpoint, exact-candidate validation, and independently observed
provider results; neither the earlier draft nor this textual record proves them.

This change remains documentation-only. Runtime gates, configuration, migration
code and database behavior are unchanged. The runtime repairs and their proofs
require separate admitted work and cannot be inferred from approval of this
review. Review of this document does not confer production readiness.

[rbac-pr]: https://github.com/DecapodLabs/decapod/pull/1606
[proof-ledger]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/docs/datastore-v2/proof-ledger.json
[todo-tx]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/todo.rs#L52-L99
[todo-completion]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/todo.rs#L3595-L3705
[todo-handoff]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/todo.rs#L5583-L5621
[events]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/events.rs#L692-L899
[compaction]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/storage_compaction.rs
[codec]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/storage_codec.rs
[delegation]: https://github.com/DecapodLabs/propodus/blob/177b7fd95ff996a4215a9e56439c79b6dba8408c/internal/backend/schema/007_delegated_agent_authority.sql
[dc-route]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/dactyl.rs#L313-L345
[dactyl-route]: https://github.com/DecapodLabs/dactyl/blob/1fa52a232e1d34a27be28d158298805ffc175ffc/src/lib.rs#L82-L94
[propodus-readme]: https://github.com/DecapodLabs/propodus/blob/177b7fd95ff996a4215a9e56439c79b6dba8408c/README.md#L7-L22
[todo]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/todo.rs
[authoring]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/authoring.rs#L21-L29
[cloud-resource]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/dactyl_todo.rs#L345-L352
[cloud-mutations]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/dactyl_todo.rs#L345-L472
[profile]: https://github.com/DecapodLabs/propodus/blob/177b7fd95ff996a4215a9e56439c79b6dba8408c/internal/backend/profile.go#L43-L48
[cloud-add]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/dactyl_todo.rs#L59-L135
[pair-validation]: https://github.com/DecapodLabs/propodus/blob/177b7fd95ff996a4215a9e56439c79b6dba8408c/internal/backend/profile.go#L137-L172
[service-http]: https://github.com/DecapodLabs/propodus/blob/177b7fd95ff996a4215a9e56439c79b6dba8408c/internal/backend/http.go#L114-L134
[service-db]: https://github.com/DecapodLabs/propodus/blob/177b7fd95ff996a4215a9e56439c79b6dba8408c/internal/backend/database.go#L128-L224
[native-pair]: https://github.com/DecapodLabs/propodus/blob/177b7fd95ff996a4215a9e56439c79b6dba8408c/internal/backend/schema/003_rpc.sql#L109-L121
[cloud-list]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/dactyl_todo.rs#L51-L55
[service-bounds]: https://github.com/DecapodLabs/propodus/blob/177b7fd95ff996a4215a9e56439c79b6dba8408c/internal/backend/http.go#L177-L212
[native-bounds]: https://github.com/DecapodLabs/propodus/blob/177b7fd95ff996a4215a9e56439c79b6dba8408c/internal/backend/schema/003_rpc.sql#L154-L158
[journal-limits]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/task_journal.rs#L24-L27
[journal-capture]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/task_journal.rs#L301-L329
[journal-load]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/task_journal.rs#L541-L581
[journal-verify]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/task_journal.rs#L938-L944
[journal-append]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/task_journal.rs#L737-L802
[journal-changes]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/task_journal.rs#L815-L827
[migration-entry]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/migration.rs#L361-L399
[migration-load]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/migration.rs#L1064-L1079
[migration-receipt]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/migration.rs#L783-L788
[migration-planner]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/migration.rs#L751-L764
[summary-rewrite]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/storage_compaction.rs#L169-L185
[summary-schema]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/schemas.rs#L728-L729
[new-reader]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/plugins/lcm.rs#L449-L451
[old-reader]: https://github.com/DecapodLabs/decapod/blob/v0.136.4/src/decapod/plugins/lcm.rs#L423-L460
[inventory]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/docs/datastore-v2/logical_contract_v1.json#L1-L28
[cargo]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/Cargo.toml#L4
[contract-test]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/tests/datastore_contract.rs#L7-L105
[task-schema]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/schemas.rs#L438-L468
[one-shot]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/todo.rs#L499-L501
[task-tags]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/todo.rs#L1871-L1889
[task-dependencies]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/todo.rs#L2878-L2998
[owner]: https://github.com/DecapodLabs/decapod/blob/82388e0c57197c0b0622cb023210f6f908d78abe/src/decapod/core/storage_owner.rs#L759-L795
