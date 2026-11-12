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
Contract hashing reuses the plan's existing progress reset on a clone: routine
approval and measured phase/gate progress do not spend more rounds. Changes to
governing definitions still invalidate convergence. Execution/proof gates check
their own state independently.

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

Required Buildkite [library execution on `dc7bf2fa`](https://buildkite.com/decapodlabs/decapod/builds/8602#01a1cfd6-5d0d-4abb-853c-b4ed9164f022)
passed 1709 library tests (8 ignored), including all 16 reflection cases and the
canonical knowledge-lifecycle case. These are deterministic protocol tests, not
an evaluation of a language-model agent.

| Decision | Baseline contract (source audit) | Executed deterministic assertions |
| --- | --- | --- |
| Routine sufficiently scoped action | No proposed-action comparison | Round 1 proceeds without a question; 0/1 false-positive challenges |
| Caching proposal touches protected mutation path | Constraint not compared with a proposal | Binding question; execution check rejects it before action |
| Adapter-only revision citing the contract | No continuation or resolution link | Round 2 proceeds, links the resolved gap and leaves completion unproven |
| Persuasive paraphrase retains conflict | No bounded feedback exchange | Terminal blocked in round 2 |
| Incomplete but narrower proposals | No reflection round budget | Terminal blocked by round 3 |
| Superseded/inferred decision tagged accepted | Lifecycle can be lost in projection | Supersession/withdrawal/dispute and inferred/imported classifications preserved |
| Six ordinary phase transitions | No reflection continuity | Convergence stays at round 1; a governing definition edit invalidates it |

This establishes one declared-scope flaw caught before execution (1/1), not
semantic proof that caching is safe or a claim of improved model intelligence.
The baseline column describes source contracts, not a controlled model
comparison. The existing required entrypoint suite includes `infer_precision.rs`
for the real CLI/RPC continuation and legacy response compatibility.

Observed protocol-call samples under the concurrent CI library suite:

| Fixture | Call time | Compact outcome bytes | Governance JSON growth |
| --- | ---: | ---: | ---: |
| Safe | 75.213 ms | 575 | 1490 bytes |
| Initial conflict | 58.845 ms | 1213 | — |
| Corrected response | 43.966 ms | 575 | 4017 bytes total across two rounds |

The two-round compact journal measured 2843 bytes. The safe fixture observed
zero main-database-file growth. These samples exclude fixture initialization and
CLI startup; they are not portable latency guarantees. A historical installed
orientation call returned 1820 ASCII JSON bytes excluding newline in 2.50
seconds wall (1.52 user, 0.83 system), including CLI startup/session work. That
single host observation is a different environment/scope and must not be used
to claim a speedup. The CLI fixture also records paired legacy/reflection
timings and payloads in its test log.

Persistence uses the existing plan only: no new database table, row stream,
index, duplicate source body or transcript. The journal has at most three
receipts plus a terminal stop, capped at 64 KiB compact JSON. Formatting adds
overhead, Git retains historical versions, and each accepted round rewrites the
existing governance document. Rejected stale requests and exact retries do not
write it. This is bounded per-workstream storage, not total datastore size or a
measurement of all session/WAL/cache activity.

The initial required CLI run reached round-2 `proceed`, then exposed a fixture
assertion expecting an empty `blocked_by` array. The standard successful RPC
envelope omits empty blockers; the fixture now accepts that omission or an empty
array, while rejecting nonempty or malformed blockers. This changes no runtime
permission. Earlier failed full validations remain historical evidence.
Current readiness always requires a fresh complete `decapod validate` receipt
for the exact candidate and accepted target, rather than this document.

The work incorporates accepted target
`fc1e2b94f6909633bf6dd7044fd7a54a05e85a4b`; the installed supported
controller reports 0.140.0. Release projections were refreshed through supported
interfaces. No PR may be opened before the full exact-candidate gate succeeds.

Limits: explicit plan constraints and literal ADR matches cannot infer arbitrary
semantic conflicts from code. Proposed paths are declarations, not an OS write
sandbox; existing scope and validation gates inspect actual changes. Local
actor labels retain the cooperative trust boundary. Replay/interruption cases
use the existing atomic writer; no power-loss simulation or model evaluation is
claimed.
