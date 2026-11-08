# Governed transitions

Decapod is not an agent workflow engine. It does not need to decide the next
action.

```text
harness:   decides what to attempt
agent:     performs the work
Decapod:   resolves canonical state
           checks admissibility
           constrains scope
           records provenance
           validates resulting state
           evaluates proof
           controls completion and publication eligibility
```

Context windows and agent sessions are ephemeral. Governed intent, state,
evidence, and proof are durable. A later agent resumes from repository state.
Notes written by the previous agent are not authority.

## Canonical state hierarchy

1. Repository identity from `state::canonical_snapshot`: the same revision and
   provenance `state.resolve` uses. `identity_from_snapshot` reads that head,
   governance or validation-receipt hash, and Decapod version. It does not
   compute a second head.
2. Decapod coordination records: task revision, lease, claim scope, and
   dependency edges in the local datastore.
3. Evidence bound to that identity: verification artifacts with
   `proof_plan_results[].output_hash`, validation receipt hashes, rule
   fingerprint, and verifier identity.
4. Projections: `state` is not stored again. `project_validation_report` and
   `governed_transition` derive a judgment from the records above.
5. Non-authority: task comments, `last_verified_notes`, progress prose,
   `passes: true`, and `publication_ready: true`.

If a higher source is missing or contradictory, the judgment is `unknown`.
Decapod does not pick whichever copy is convenient.

## Transition model

`core/governed_transition.rs` distinguishes:

| Phase or judgment | Meaning |
| --- | --- |
| `requested` | An actor asked to attempt work. |
| `admissible` | The attempt is inside the authorized identity, scope, budget, lease, and claims. |
| `rejected` | The attempt conflicts with those constraints. |
| `attempted` / `applied` | Work was started or files changed. This is not proof. |
| `validated` | A validator ran. The result still has to be judged. |
| `proven` | Evidence matches the current head, rules, and verifier and the observation passed. |
| `failed` | Evidence shows the claim is false. |
| `not_proven` | The actor declared success and the evidence does not establish it. |
| `stale` | Evidence or authorization was for a different governing identity. |
| `unknown` | The record is missing, incomplete, or corrupt. |

`not_proven` is not `failed`. `unknown` is not `proven`. `stale` is not fresh.

Admission uses the caller's paths, the governed forbidden paths and file-touch
budget, prerequisite proof, lease expiry, and other actors' exclusive claims.
It does not select the next task.

## Proof and publication

Completion is `proven` only when an `EvidenceRecord` has intact digests, an
observed pass, and the same `StateIdentity` as the current head, rules
fingerprint, and verifier.

`substantive_task_proof` accepts a verification artifact only when
`proof_plan_results` contains a gate and an output digest. A bound
`passes: true` object is `unproven`. `effective_verification_status` returns
`unproven` in that case, and `work_claim::from_todo` stays blocked.

`judge_stored_verification` is stricter. It loads `state::canonical_snapshot`
and re-hashes every claimed file. The stored digest must equal that
reconstruction bound to the snapshot's head, rules fingerprint, and verifier.
A SQLite row that only asserts `pass` or a digest is `not_proven`, `stale`, or
`unknown`. On a resolvable Git checkout, an exclusive completion uses this
judgment, so the row alone cannot clear the proof gate.

`record_transition` appends the decision to the existing `traces` event stream.
The flight recorder renders that stream. The event is provenance, not a second
canonical state.

## Verifier integrity

A passing observation and an independently constrained verifier are different
claims. `VerifierEnvelope` records the subject, the canonical input hash, the
harness identity and version, the verifier identity, and one observation for
each isolation axis: parent turns, project instructions, user instructions,
filesystem scope, tool scope, and network scope. Each observation is
`proven_excluded`, `observed_inherited`, or `unknown`.

`effective_isolation` keeps that observation only when the evidence source is a
native trace or an external observation. A self-report, a harness name, or an
absent source becomes `unknown`, including a child statement that it did not
see the parent context. Axes are independent. Proven exclusion of parent turns
does not exclude project instructions, user instructions, tools, files, or
network.

`judge_verifier_integrity` binds the envelope to the current `StateIdentity`.
An incomplete identity is `unknown`. A different identity is `stale`. An empty
requirement is `proven` for that bound envelope, because no isolation was
required. Observed inheritance on a required axis is `not_proven`, not
`failed`. Any remaining unknown required axis is `unknown`. The judgment is
`proven` only when every required axis is `proven_excluded` under native or
external evidence.

`judge_completion_with_verifier` calls `judge_completion` first. An empty
requirement returns that judgment unchanged, so existing completion behavior
stays in place. A base judgment other than `proven` is also returned unchanged:
failed evidence stays `failed`, and a missing proof stays `not_proven`. A
`proven` base with a required axis and no envelope is `unknown`. A `proven`
base with an envelope is then judged by `judge_verifier_integrity`. A semantic
reviewer can supply evidence. It does not by itself satisfy an integrity
requirement.

## Runtime capability evidence

`CapabilityAttestation` is not the project capability registry in
`capabilities.rs`. It records one runtime capability id, the declared support
(`proven`, `unavailable`, `unknown`, or `stale`), the evidence source, the
governing identity it was bound to, and a limitation string.

`attest_capability` returns `unknown` for a self-report, a harness name, or an
absent source, even when the declaration says `proven`. An incomplete identity
is `unknown`. A complete attestation bound to a different identity is `stale`.
Otherwise the declared support is returned. `unknown` and `stale` are not
`proven`, so a policy that needs current support cannot treat them as
available. `advisory_harness_capability` ignores the harness string and returns
an unknown, harness-named attestation. Codex, Claude, Grok, and Antigravity are
identities, not evidence.

## Blocker classification

`classify_blocker` names why a transition is not yet safe to treat as proven.
`capability_gap` names a capability and a provider class for that blocker. The
trace records `blocker_class` and `required_capability`. Neither function
changes admission or selects the next action. There is no `capability.resolve`
operation.

An admissible `proven` transition records no blocker. A rejected or stale
admission whose reason contains `exclusive` or `lease` is `coordination`. A
reason that contains `forbidden`, `budget`, `prerequisite`, or `may not` is
`policy`. A stale admission, or a reason that contains `state`, is `state`.
The remaining judgment maps `stale` and `unknown` to `state`, `not_proven` and
`failed` to `verification`, and a non-admissible `proven` judgment to
`mechanical`. `inference` is available through `capability_gap` for a problem
that still needs semantic judgment. Its provider class is `frontier` and
`escalation_required` is true. Every other class names a runtime capability and
does not require escalation.

`admit_operation_surface` maps an operation class to a mutation surface.
`design.review` may write `docs/**` and `.decapod/**` and may not write
`src/**` or `Cargo.toml`; any other path stays `unknown`. `implementation` is
admissible at this surface and remains subject to `admit`. `publication` may
record evidence and may not mutate files. An unrecognized class stays
`unknown`. The harness's phase name is not the operation class.

Publication eligibility is `decide_publication` / `project_report_publication`.
The projection field `publication_ready` is that decision. The boolean in an
agent-written report is ignored. Stale, failed, unproven, and unknown reports
are not eligible. A second publication with a different receipt is rejected.
The same receipt replays.

## Freshness and state commit

A proof for S0 is not a proof for S1. Any difference in head, rules
fingerprint, or verifier makes `commit_freshness` return `stale`. An empty
component returns `unknown`.

`state_commit::verify` checks the canonical scope-record hash. It fails when
the bytes or the expected root are missing. `state_commit::freshness` answers
whether that commitment still matches the governing identity. A matching hash
of an old commit does not authorize later work.

These events invalidate dependent proof when they change the identity:

- governed file content, represented by head
- validation rule or policy fingerprint
- verifier implementation or version
- base or candidate revision included in that head

## Recovery and concurrency

`recover` reads `DurableWork` and re-judges its evidence. The `notes` field and
any prose argument are ignored. Missing durable state is `unknown`. A crash
after mutation and before evidence stays `applied` and `not_proven`. The same
operation id and digest replays through `apply_operation` and does not append
a second effect. A reused id with a different digest conflicts.

`resolve_caller` returns the canonical identity to each caller. One caller's
foreground note is not copied into another caller's resolution. An incomplete
canonical identity is `unknown` for every caller.

`admit` rejects an exclusive path overlap, an expired lease, a forbidden path,
a file-touch budget overrun, unproven prerequisites, and an authorization that
was minted for a previous identity.

## Rules

Builtin, project, and task rules still live in `validation_rules`. Forbidden
paths do not remove a rule from consideration. An Error or Critical rule fails
when a forbidden path is in the change. Task and project copies cannot replace
a `NonOverridable` rule body. Publication-phase rule
`decapod.publication.fresh_proof` runs only when the caller sets
`include_publication`. Validation of ordinary worktrees does not treat that
phase as already satisfied.

## Flight recorder

`transition_trace` / `trace_json` is the payload for op `govern.transition`.
It records the actor, prior state, rules, verifier, admission, scope, expected
and observed effects, evidence hash, outcome, reason, freshness, and, when a
blocker remains, `blocker_class` and `required_capability`. It does
not record notes or a passes flag. The flight recorder remains a renderer over
events. The trace is provenance. Canonical state stays in the repository and
the datastore.
