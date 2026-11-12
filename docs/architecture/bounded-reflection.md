# Bounded reflection before execution

## Source audit and implementation boundary

Audit base: `19acd21f7ff50656323d037915273f03cdedc04e` (origin/master).
The human supplies intent; the agent owns inference and execution; the kernel
checks supported transitions. Reflection does not establish understanding,
authenticate human approval, execute tests, or establish publication readiness.

| Existing surface | Observed implementation | Composition / gap |
| --- | --- | --- |
| `lib.rs::run_infer_orientation`, `rpc.rs::OrientationPacket` | Task/plan pointers, proof hooks, keyword decision gates; no mentor call or continuation. Task title overwrites explicit intent. | Add optional reflection input/output; preserve legacy calls. |
| `mentor.rs::detect_contradictions` | Literal ADR/path matching, score-based retrieval, capped lists; no semantic conflict solver. ADR enumeration was unsorted. | Reuse potential-conflict detector; expose source identity. Heuristics stay advisory. |
| `plan_governance.rs::{patch_plan,ensure_execute_ready}` | Locked atomic plan writes, contract edits reset acceptance; scope and human-input gates. | Keep compact bounded receipts in the plan; enforce unresolved binding challenges at existing execution boundary. |
| `state.rs::{resolve_state,resolve_decisions,resolve_failed_paths}` | Composes custody, plans, proof freshness, failed paths and publication. Federation decision tags can override lifecycle. | Repair lifecycle projection; never treat inferred knowledge as human approval. |
| `plugins/federation.rs::{supersede_node,transition_node_status,read_node_full}` | Typed nodes, confidence, source edges, validity intervals, supersession/dispute/deprecation, canonical events and rebuild. | Reuse lifecycle; no copied knowledge store, generic graph or embedding layer. Deprecation is withdrawal from active use, not erasure. |
| `plugins/decide.rs::record_decision`, `interview.rs::next_question` | Curated questions/answers; recorded actor/rationale do not authenticate human authority or prove behavior. | No second interview engine; unresolved human questions remain separate gates. |
| `core/trajectory`, `governance_document.rs` | Evidence loops, sole tracked governance document, bounded lock and atomic publication; historical journals preserved in Git. | Store hashes, scope and dispositions, not conversations; existing custody checks and lock serialize submissions. |
| `core/obligation.rs::derive_obligation_status`, `validation_epoch`, `validate`, `workspace` | Derived proof/dependency readiness, source-bound freshness, exact candidate publication gate. | Reflection never sets obligation status, proof results, plan acceptance or readiness. |

Mentor retrieval mixes ranked recommendations and potential obligations; its
score is not an authority classification. State projection also contains legacy
best-effort reads and placeholder timestamps. Reflection must not promote these
views into a new permission engine. Source citations establish byte identity,
not that an agent's interpretation is correct. Read-only orientation without
reflection input must remain inexpensive and stateless.

## Chosen slice

Extend `infer orientation` and `mentor.obligations`, retaining their envelopes.
CLI reflection requires a matching explicit outer task ID, preventing its
legacy decision gates and the continuation from using different governing intent.
An explicit reflection request names the claimed task, proposed operation and
literal repository paths. One question comes from existing plan constraints or
mentor potential contradictions. Human questions and pending human spec reviews
retain priority. Existing plan constraints are binding; retrieved ADR matches
are advisory. No retrieved instruction can change the plan or approval policy.

Use one bounded journal in the current plan, the existing governance lock and
task execution-custody capture. Store at most three compact receipts with input,
context and progress digests, proposed paths and disposition. A continuation
compares its expected token under the lock. Identical submission IDs replay
without writes; changed reuse and stale tokens fail without consuming a round.
Meaningful progress requires a removed conflict or strictly narrower scope;
new prose or arbitrary citations cannot clear a challenge. Changed sources
invalidate previous conclusions without resetting the lifetime budget.

`proceed` means only that this reflection has no current challenge. `reflect`
returns one bounded question. `human_decision` preserves an existing authority
gate. `blocked` is terminal for stagnation or exhaustion, or reports a current
custody/policy failure. Three rounds include the initial submission. A third
unresolved reflective round terminates; repeating a non-progressing response
terminates sooner. Terminal journals cannot be reset by ordinary plan edits or
reinitialization. A new accepted workstream is the explicit recovery boundary.

The first correction demonstrated by this slice is a scope revision: an agent
proposes caching by changing a protected mutation path and its adapter, then
retains only the adapter in its declared scope after inspecting the governing
constraint. The kernel can establish that the conflicting declared path was
removed. It cannot establish that a future implementation preserves visibility;
the original test/proof gates still own that claim. This is a deterministic
protocol evaluation, not a language-model intelligence evaluation.

## External mechanism review

The supplied questioning reference asks one question at a time, probes plausible
errors and checks actual performance rather than accepting descriptions. The
transfer here is focused falsifiability; curriculum and learner scores do not
belong in the kernel.

The supplied persistent-knowledge reference was inspected at
`2c8320d78206097f809862617d81a7e09efc01cc`. Relevant mechanisms include
`src/core/think/gather.ts` (bounded multi-source retrieval, scope propagation,
explicit degradation), `facts/withdrawal.ts` (source/visibility/subject-bound
withdrawal and transactional invalidation), `facts/supersede-resolve.ts`
(reject self/dangling/cyclic supersession), `trust/provenance-drift.ts`
(preserve writer provenance and lower-only repair), `think/index.ts`
(gaps/synthesis and optional persistence), `enrich/thin.ts` (bounded evidence
and abstention), `enrichment-service.ts` (quarantine inferred entities), and
`eval/retrieval-quality/harness.ts` (explicit retrieval quality measurement).
Useful lessons are source identity, lifecycle before retrieval, bounded context,
and honest degradation. Synthesis, background enrichment, automatic extraction,
entity accumulation and personal-assistant architecture are excluded.

## Verification and costs

The 15 protocol cases passed on `32fdc534` in diagnostic run
`43215838324`, job `131175931908`. The library suite's only failure
was lifecycle-fixture setup: canonical federation policy rejected unregistered
basic-trust writers. The isolated fixture now registers verified writers; its
corrected lifecycle assertions still need execution. These diagnostics are
source-repair evidence, not complete Buildkite publication acceptance.

| Decision | Baseline contract (source audit) | Observed deterministic protocol assertions |
| --- | --- | --- |
| Routine sufficiently scoped action | Orientation has no proposed-action comparison | Round 1 proceeds without a question; 0/1 false-positive challenges |
| Caching proposal touches protected mutation path | Constraint not compared with a proposal | Binding question; existing execution boundary rejects it before action |
| Adapter-only revision citing the contract | No continuation or resolution link | Round 2 proceeds and links the resolved gap; completion remains unproven |
| Persuasive paraphrase retains conflict | No bounded feedback exchange | Terminal blocked in round 2 |
| Incomplete but narrower proposals | No reflection round budget | Terminal blocked by round 3 |
| Superseded/inferred decision tagged accepted | Lifecycle can be lost in projection | New lifecycle assertions pending repaired fixture execution |

This establishes one declared-scope flaw caught before execution, not a semantic
proof that caching is safe or that an agent has become more intelligent.
The baseline column describes actual source contracts, not a controlled model
comparison. `infer_precision.rs` now joins the existing required entrypoint
suite so the CLI/RPC exchange is exercised by configured validation.

A historical installed-release orientation call returned 1820 ASCII JSON bytes
excluding newline in 2.50 seconds wall (1.52 user, 0.83 system), including CLI
startup/session work. This was a single host observation. Current safe and
corrected fixtures emit numeric protocol timings and sizes to stderr; the CLI
fixture records paired baseline/reflection payloads and timings on one fixture.
These new measurements await the next configured execution and are not
comparable to the earlier host sample without accounting for environment and
scope. The safe protocol fixture's datastore-size equality assertion passed;
that establishes zero main-database-file growth for that fixture.

Persistence uses the existing plan only: no new database table, row stream,
index, duplicate source body or transcript. The journal has at most three
receipts plus a terminal stop, capped at 64 KiB compact JSON. Normal governance
formatting adds overhead and Git retains historical versions. Each accepted
round rewrites the existing governance document; rejected stale requests and
exact retries do not. This is bounded per-workstream storage, not a total
repository-size estimate. Measured governance/journal byte counts will be
reported from configured execution rather than inferred from this ceiling.

The work branch now incorporates accepted target
`f1a6c7a0f8c54bfbb1153e2385c72aafaa8c7ad2`. The installed supported
controller reports 0.139.1 and release projections were reconciled through
supported interfaces/accepted source. Earlier projection/quota failures and
private snapshot identifiers remain in the governed trajectory. Fresh plain
validation must establish exact-candidate readiness before any PR is opened.

Limits: explicit plan constraints and literal ADR matches cannot infer arbitrary
semantic conflicts from code. Proposed paths are declarations, not an OS write
sandbox; existing scope and validation gates inspect actual changes. Local
actor labels retain the cooperative trust boundary. Replay/interruption cases
use the existing atomic writer; no power-loss simulation or model-agent
evaluation is claimed.
