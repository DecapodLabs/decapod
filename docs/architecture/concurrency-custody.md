# Task concurrency, checkout custody and recovery

Decapod owns the transition rules. Dactyl executes datastore operations.
Propodus owns private identity, repository authorization, entitlement and
transactional enforcement. Git retains authored code, specifications and proof
artifacts. Local agent trust is never Supabase user authorization.

## Call graph and corrected boundaries

Local task requests enter `todo::add_task` / `claim_task_with_lease`, then
`with_task_journal`, `DbBroker::with_transaction` and the existing Dactyl-backed
connection. State, revision/lease generation, domain events and task journal
changes share the transaction. Physical SQLite locks and WAL coordination remain.

Previously, fuzzy title similarity could consolidate a new request into somebody
else's active task. Category expertise could assign its owner without a claim.
`detect_overlaps` and a second category-owner check then rejected independent
claims. Handoff independently repeated overlap exclusion and transferred category
ownership. These paths confused task accountability with source coordination.

Tasks now keep their own identity. Categories suggest routing without assigning
custody. Category, directory and logical file overlap are advisory integration
risks, including overlaps between tasks held by the same actor. Exclusive task
ownership, dependency proof and lease fencing still govern acquisition. An
explicit `resource:<name>` scope reserves that named mutable resource through
the same task transaction; it is not a directory, module, repository or
organization mutex. Its ownership does not grant deployment/publication approval.
The limited cloud task/event profile cannot reserve resources and rejects such
claims explicitly.

`workspace::ensure_workspace` selects an explicitly requested destination task
before inspecting an unrelated root checkout branch. It provisions the worktree
from the accepted base only after acquisition. `todo::admit_authoring` observes
the registered Git checkout itself, then atomically binds its host-local identity
to the actual actor, task and current lease generation. The identity includes
canonical Git directories, filesystem device/inode and creation time. It is not
a global cloud identity. No API accepts a caller's `verified=true` assertion.

The existing canonical datastore gains a version-one `authoring_bindings` table,
keyed by observed checkout identity, with a matching `authoring.bind` event in the existing workspace lifecycle
stream in its transaction. Admission checks task journal authority without
creating an uncheckpointed task-event suffix. No second database or lock service is introduced. A live
binding survives a branch switch and excludes another task, including another
task held by the same actor. Only durably inactive custody can retire a binding;
renewed/transferred generations require explicit fresh admission. Observations
are repeated inside the admission transaction and at governed CLI/RPC authoring
entrypoints. They are cooperative admission checks, not an operating-system
sandbox against an unrestricted local process or an external Git edit racing
after the final observation.

Secondary task membership does not imply the primary lease or access to its
checkout. An explicit secondary task workspace request returns
`WORKSPACE_SECONDARY_CUSTODY`, with the supported independent-task route.
Independent tasks can be claimed with the same logical source footprint at basic
trust, then provisioned normally. No primary-owner impersonation is needed.

## Handoff and approval

An already-authorized caller retains its actual identity. A cross-owner recovery
requires a bounded one-use approval of a versioned target containing the task,
current owner, generation and receiver. Its fingerprint uses an unambiguous
serialized tuple. A changed snapshot or failed owner/generation CAS cannot
consume approval or emit a successful handoff. Successful transfer consumes the
exact approval and advances fencing in the transaction. Ordinary owner handoff
retains its existing policy. Diagnostic policy evaluation is usable before a
source workspace exists; neither evaluation nor human conversation grants trust.

The policy CLI authorizes its executing caller before mutation. `--actor` labels
the approving decision and cannot substitute an operator identity for a basic
caller. Reserved local cooperative operator roles remain local policy, never
cloud administrator roles. Local policy CLI and filesystem-binding APIs reject
unsupported cloud profiles instead of making local shadow authority.

## Evaluator recovery

`eval --stdin --format json` distinguishes evaluated `allow` / `block` from
`invalid_input` / `error`. Empty, missing or conflicting input produces a typed
non-success diagnostic with `evaluated=false` and an instruction to evaluate the
complete original prompt. Correcting input does not require human approval.
Unavailable input/evaluation must recover first. Only a successful full-prompt
`allow` admits action; a genuine denial still requires human review.

## Cloud compatibility and evidence boundaries

The consumer remains on released `dactyl-db` 0.11.1 and its `/query` and `/batch`
compatibility route. Conditional task updates, version fencing, zero-row conflict
interpretation, state/event atomic batches and unknown-write reconciliation
remain at their existing boundaries. No automatic write retry was added.
Propodus's restricted profiles execute server-owned SQL and authenticate scope;
a client repository string or checkout path cannot grant access.

Propodus's separate native `dactyl_query_v1` / `dactyl_batch_v1` profile has real
PostgreSQL/PostgREST conformance infrastructure. Its test identities model the
managed Auth tables; this does not prove hosted Supabase, real GitHub OAuth,
native Decapod routing or a published native Dactyl integration. Remote validated
completion, cloud checkout binding, resource reservation and lease handoff remain
unsupported. Enabling them requires an explicit Decapod-defined versioned
contract, Dactyl transport support where necessary, private Propodus enforcement
and fresh authorization/transaction proofs in that repository. No private SQL or
provider interface was changed by this repair.

The maintenance branch was bootstrapped with a one-time user-authorized standard
Git worktree exception. It has no fabricated Decapod task admission. Candidate
fixture tests are engineering evidence only. Publication still requires an
independent accepted controller and the complete exact-candidate gate.

## Engineering proof and limits for this repair

Inspected public Decapod base: `24164460c936cb42d7cf302acac80e74cfa8ef71`.
Released Dactyl consumer dependency: `dactyl-db` 0.11.1; inspected Dactyl main
`1fa52a232e1d34a27be28d158298805ffc175ffc`. Native transport exists separately
at candidate `c37dcebe0d8487cd7fbd79248a38b8aae1694b39`; that is not proof of a
published compatible artifact. Inspected Propodus accepted main:
`0bf9e4280137796967de4ef1d8587e96807cce7c`. The tested backend source at
`405cd6835e07f6ccf79f33ce32ab70e37a72e3ea` has no backend-source diff against
that accepted main. No cross-repository implementation was changed.

The local engineering run passed 196 focused unit cases (one ignored opt-in real Docker
lifecycle case) and 97 CLI/contract cases across 11 suites, with strict Clippy and format
checks. The fresh-repository fixture creates managed worktrees using the actual
candidate CLI without skipped Git gates or a maintenance exception. It exercises
basic-trust independent overlapping tasks, checkout exclusion, invalid identities,
fencing, source-branch preservation and overlapping edits. Its clean textual
merge first fails an executable combined invariant, then passes after explicit
reconciliation. These fixture outcomes do not certify this source PR.

The private disposable PostgreSQL/PostgREST foundation and native RPC suites
passed under the race detector: authenticated sharing, one same-task claim
winner, failed-transition event suppression, rollback, repository isolation,
permission/entitlement/session denials and restricted API privileges. Additional
isolated fixture probes passed distinct-task concurrency with overlapping source
metadata on both compatibility HTTP and native RPC, across two repository scopes.
The real released-Dactyl consumer also passed against PostgreSQL, with independent
persisted state/event assertions and simulated-provider onboarding/session reuse.
The added private probes were executed as a test overlay without modifying the
existing Propodus worktree; they are engineering evidence, not a merged private
regression suite. Logs, fixture identities and database state remain outside Git.

These results establish neither hosted Supabase security nor managed OAuth,
native Decapod consumer integration, actual container authoring, production
resource deployment or publication readiness. Those capabilities retain their
separate proof requirements. The installed governance controller rejected
trajectory recording from the authorized external maintenance worktree with
`SCOPE_VIOLATION`; no task admission, approval, checkpoint or readiness receipt
was fabricated to hide that limit.
