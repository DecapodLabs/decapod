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

The adversarial corpus, complete exchange, measured costs and validation status
are recorded below when observed. Authored tests are not passing evidence until
the configured validation executes them. No baseline or timing is invented.

Baseline observation with installed 0.139.0 on this worktree:
`infer orientation --task-id feat_01m77nmmjwd12v30 --format json` returned an
1820-byte ASCII JSON object (excluding trailing newline) in 2.50 seconds wall
time, 1.52 seconds user and 0.83 seconds system. This is one invocation including
normal CLI startup/session work, not a controlled latency benchmark.
Its response has no reflection field or continuation.

| Decision | Baseline orientation | New deterministic contract (test assertions, execution pending) |
| --- | --- | --- |
| Ordinary sufficiently scoped task | No reflective session | One optional round, no question; legacy call remains unchanged |
| Protected mutation path proposed for caching | Constraint not compared with a proposal | One binding question before execution |
| Adapter-only revision citing the contract | No continuation or resolution link | Round 2 clears the removed path conflict and records its source |
| Persuasive paraphrase retaining the conflict | No stagnation protocol | Terminal blocked in round 2 |
| Incomplete but narrowing proposals | No round budget | Terminal blocked by round 3 |
| Superseded decision with accepted tag | Can project as accepted | Canonical lifecycle wins; inferred/imported labels remain proposals |

New reflection persistence is confined to the existing plan. There are no new
database rows, indexes, duplicate source bodies or conversation transcripts.
The journal has at most three receipts plus a terminal stop, capped at 64 KiB
compact JSON; excess relevant gaps or size fail rather than dropping authority.
Normal governance serialization adds formatting overhead and Git retains
authored historical versions. Each successful round rewrites the existing
governance document; retries and rejected stale submissions do not. This is a
bounded per-workstream cost, not an estimate of total repository datastore size.
Existing CLI/session infrastructure has its own costs. The tests print actual
outcome sizes, plan-file growth and protocol latency, and assert unchanged
datastore file size. Until those tests run, no resulting latency/storage delta
or detection-rate claim is established.

The first full validation stopped on a stale reviewed-source inventory before
executing candidate builds/tests. The reviewed-source change since
`2f01156c` is solely the accepted package version bump in Cargo manifest/lock;
the scoped declaration repair retains the complete existing proof contract.

Limits: the first slice recognizes explicit plan constraints and the existing
literal ADR heuristic. It cannot infer arbitrary semantic architecture conflicts
from code or prose. Proposed paths are declarations, not an OS write sandbox;
existing scope/validation gates check actual changes. Local actor labels and
declared approvals retain their existing cooperative trust boundary. Repeated
invocations exercise replay/interruption recovery through the existing atomic
writer; this corpus does not simulate power loss or evaluate a model agent.
