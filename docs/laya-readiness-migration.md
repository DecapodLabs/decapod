# Laya readiness migration and provenance

Decapod connects human intent to governed agent work and verified delivery.
The maintainer selected Laya to reduce the cost of its optional, required
post-validation model judgment. This changes the evaluator; deterministic proof,
task custody, exact-state checks, agent publication and human merge authority
remain Decapod responsibilities. Model output alone never authorizes delivery.

## What the earlier releases actually shipped

| Implementation | Release | Relevant behavior |
| --- | --- | --- |
| [#1343](https://github.com/DecapodLabs/decapod/pull/1343) | [#1345, v0.103.0](https://github.com/DecapodLabs/decapod/pull/1345) | Optional hosted Jev provider; advisory `trajectory_satisfies_intent` after assurance reconstruction. It could not supply proof or approval. |
| [#1346](https://github.com/DecapodLabs/decapod/pull/1346) | [#1347, v0.104.0](https://github.com/DecapodLabs/decapod/pull/1347) | Persisted advisory observations in trajectory/history. |
| [#1351](https://github.com/DecapodLabs/decapod/pull/1351) | [#1352, v0.104.2](https://github.com/DecapodLabs/decapod/pull/1352) | Jev selection during init and private credential persistence. |
| [#1460](https://github.com/DecapodLabs/decapod/pull/1460) | [#1461, v0.120.0](https://github.com/DecapodLabs/decapod/pull/1461) | Replaced active Jev with Rust-native Vega, external pinned assets, Metal/CPU execution, a production `validation-results.v3` projection and active required judgment when selected. Completed #1445. |

#1345 is a release PR, rather than a separate implementation issue. Its diff
updates the changelog and Cargo release metadata. `CHANGELOG.md` records this
sequence; a release entry is not independent evidence of numerical accuracy.
The historical implementation PR timelines do not identify additional original
implementation issues to close as part of this migration.

[#1400](https://github.com/DecapodLabs/decapod/issues/1400) concerns broader
validation latency. Merged #1403 and #1404 reduced discovery, transfer and
collection costs; they did not establish a five-minute complete validation run.
Laya inference time must be reported separately from source compilation,
Buildkite queue/execution, model acquisition, integrity hashing and collection.
Replacing the evaluator does not complete every open lifecycle, platform or
release-assurance issue.

The creator's [speed recommendation](https://x.com/raberhalex/status/2114082360367182113)
and the maintainer's [Laya selection](https://x.com/raberhalex/status/2114117592021717050)
motivate this change. Neither establishes code-readiness accuracy. This is a
replacement in the accepted architecture, not a Git rollback to v0.103.0 or a
restoration of the retired hosted Jev endpoint and credentials.

## Qualification outcome on 2026-10-25

**The migration is not qualified for publication.** Native execution and
configuration are implemented and have focused passing tests, but the pinned
profiles did not satisfy the required readiness behavior. This branch is
preserved development work, not an accepted replacement for released Vega.
Full final-candidate `decapod validate` has not passed, and no migration PR has
been published. No installed controller, accepted policy or released profile
was replaced by these experiments.

The first study had confounded scope and proof descriptions. Those original
inputs/results remain diagnostic history. The corrected production projection
uses actual executed Rust gate assertions, the executing binary's digest and
consistent workflow identity; replay limitations are recorded outside the model
state. It retains unmet obligations, unresolved requirements and manipulation
text. Rubric v2 uses the documented binary false/true alternatives explicitly.
Null alternatives in the earlier request were documented valid SDK input,
rather than an invalid API invocation. No threshold was changed.

All four corrected requests fit their tokenizer limits. Their reported
true probabilities were:

| Pinned profile | Ready | Unmet requirement | Unresolved question | Manipulation |
| --- | ---: | ---: | ---: | ---: |
| English | 0.6639 | 0.6757 | 0.7000 | 0.6760 |
| Multilingual | 0.6466 | 0.8365 | 0.8631 | 0.7421 |
| Typed decisions | 0.4937 | 0.4907 | 0.4973 | 0.4890 |

All reported act probability 1. The first two profiles accepted every request;
the typed profile rejected every request, including ready work. These exposed
development cases are not a final holdout, and a scalar threshold does not
separate their required dispositions. Upstream public-intent benchmarks and
specialist typed workflows do not qualify a code-readiness judge.

One bounded local Rust adaptation captured the multilingual model's frozen
pre-affine features on actual Metal. Only its final 768 scorer coefficients
were fitted; encoder, Transformer head, action-head weights and original bias
were frozen. The 32 fitting rows comprise four agent-authored contract families
with actual Rust witnesses, **not 32 independent originals or human labels**.
Four distinct evaluation families and their acceptance were committed before
fitting in `7232f7eb639dffaeaa35e32f5df40f4aaa1a52fb`. They remain unscored.

The initial optimizer penalized normalized changes, permitting unscaled
coefficients as large as 386,606.72. Its artifact failed the existing 10,000
safety bound and still had six fitting-score errors. That failure is preserved.
The corrected scalar Newton optimizer applies the stated L2 prior directly to
runtime coefficients, retaining lambda 0.001 and a 5,000-iteration maximum.
Finite-difference gradient, known-system solve and low-variance coefficient
bound regressions passed before rerunning the fixed fitting inputs. It
converged in five iterations; largest coefficient magnitude was 1.5782235.
Loss fell from 1.20087080599 to 0.59737159492, but **10 of 32 fitting-score
classifications still disagreed**: six adverse rows scored affirmative and four
ready rows scored negative. This is a fitting diagnostic, not execution of the
fitted profile through the complete worker or readiness predicate.

A subsequent diagnostic executed those **same fitting inputs** with the fixed
artifact on actual Metal, including the unchanged action head, calibration and
production response predicate. It approved eight of twelve ready controls,
rejected four ready controls, and **approved six of twenty adverse controls**.
No malformed-response, abstention or downstream policy interpretation concealed
these six accepts. They concern unmet requirements (three families), contradictory
requirements, declaration-only proof and injected approval instructions. This
is a direct functional failure, not the superseded statistical research gate.
The diagnostic stayed in process: it did not install the artifact, establish
IPC custody, run the held-out evaluation or authorize publication.

The structurally valid but unqualified readout has SHA-256
`c3123413d4f88f9d9e64f3f1e8d1924b659210e34d2b2612b831156fa3abd74c`.
It remains private, was not installed or activated, and is not the baseline
manifest. No evaluation inputs entered fitting, scaling, stopping or the
diagnostic report. Fitting stops here; these results do not justify threshold
changes, a relaxed coefficient limit or tuning against the untouched cases.

The precise unmet condition is useful, safe model judgment on the actual
production representation. The next capability route is a compatible
readiness-specific upstream profile or a separately scoped adaptation study,
followed by fresh qualification. The tested fit does not establish whether a
different fitting objective would suffice or additional head capacity is
necessary. Neither route has been qualified here. This is not a request for
unknown private datasets, a claim that all Laya models are incapable, or
permission to ship an inactive or advisory substitute. Existing required
deterministic gates stay intact.

## Runtime and artifact boundaries

`assets/tools/laya-native` is a separate Rust workspace. The maintained
[`laya-onnx` SDK](https://github.com/NandhaKishorM/laya/tree/7a77439b0923d6efe00052d50a7f787840bfce98/laya-rust)
supplies the CPU inference, tokenizer, typed sequence construction and upstream
calibration implementation. Candle supplies native Metal and optional CUDA
execution of the same checkpoint. The SDK itself has no Metal provider.
There is no Python runtime, local Python export or text-generation substitution.

`core::laya_assets::baseline` pins the multilingual profile, source revision,
SafeTensors, encoder configuration, tokenizer, agent configuration and ONNX
graphs individually. The original model bundle is
`convaiinnovations/laya@7b928d828b7b0e022f929d9bd2e44165aa270148`;
the split ONNX graphs are the explicitly separate community conversion
`desplega/laya-onnx@511bd780d5ab8592bee0a6298fc8c8ab386a268f`.
The conversion's producer manifest records its source checkpoint and tools.
It is not described as an upstream-published ONNX release.

Model weights stay outside Git and the published Decapod crate. Explicit setup
verifies byte counts and SHA-256, copies safe cache artifacts into a separately
verified staging installation and atomically publishes a complete immutable
version. Unsafe hardlinks remain rejected. Runtime registration is private and
independent for each maintainer; the team shares a manifest digest. No active
validation downloads or silently selects a different checkpoint.

## Evidence and limits

The independently published English and multilingual quickstart fixtures are
copied from the pinned SDK source. Tests compare token IDs, option markers,
raw option/action logits and calibrated SDK answers. Raw tolerances were
declared before comparison: `1e-3 + 1e-4*abs(reference)` for option logits and
`1e-2 + 1e-4*abs(reference)` for action logits. Rounded SDK answers use `1e-4`.
These are new Laya comparisons; existing Vega tolerances and failed studies
are unchanged. Shared libraries, fixed short examples and public producer
outputs limit independence and coverage; this is not all-input equivalence.

The latest unchanged-tolerance multilingual four-question comparison passed on
both native backends. Metal took 220.327 ms synchronized compute after 187.627
ms load, with maximum option error `1.1563301e-5` and action error
`0.00061035156`. Native Candle CPU took 12,106.195 ms compute after 438.758 ms
load, with option error `5.2452087e-6` and action error `0.0007324219`. The
maintained ONNX SDK CPU path separately matched the published rounded answers
and exact token/marker rows. No numerical tolerance was widened.

A separate Metal comparison also passed the pinned SDK's published
`laya-rust/tests/golden/multilingual/case_long_exact_limit.json`
(SHA-256 `8c4eb0fb6d6e87f5e7efc27cded4e475e72def9184cd8b5b516d356af704ec9a`).
Its 760 state tokens produced 2,376 input tokens across three questions without
truncation. Choice probabilities/selection, expected score, Boolean probability
and action probabilities matched published rounded answers within the unchanged
`1e-4` tolerance; synchronized computation took 2,829.141 ms. This producer
fixture supplies calibrated answers, not raw intermediate tensors. It extends
the measured Metal input range to this example, not all 1,024-token inputs,
CPU long-context parity or readiness semantics.

The latest unsupported-CUDA-to-fresh-CPU IPC regression passed with actual
ONNX inference: 11,916.512 ms worker wall time, 1,607.677 ms load, 744.381 ms
inference, 8,763.573 ms integrity hashing and 3,593,699,328 bytes process peak
host RSS. This measures the component retry, not NVIDIA execution, isolated GPU
allocation or an accepted readiness verdict. The sandbox initially exposed no
Metal device; a missing inventory made the pinned Candle constructor panic.
An explicit inventory guard now returns unavailable instead, with an executed
regression. Actual Metal numerical execution was rerun outside that restriction.

Focused verification also passed eight kernel Laya tests (three external
diagnostic/export tests ignored), 35 init/configuration tests (one ignored),
18 worker process/protocol/math/device tests, four corrected-optimizer math
tests, and four external frontend/fallback/SDK tests. Those commands are focused
checks, not the prescribed final publication validation. Timings are individual
observations, not repeated-sample guarantees or complete validation latency.

The executed fitting inputs retain their original generator digest
`7385bd750a28c7118739630404dd7a4447eb704bed41467b66771b144087d4cb`.
Its exact source snapshot is preserved privately. Subsequent rustfmt-only
changes to the generator and fitter are not represented as a new fitting run,
new labels or a new evaluation. Failed artifacts and original source identities
remain bound in the private diagnostic records.

The first production-shaped readiness development run used the same
`validation-results.v3` builder and real IPC response interpretation on CPU and
Metal. Rubric v1 approved the ready control **and all three adverse controls**
(missing obligation, unresolved requirements and injected approval text).
Those failures remain diagnostic history. Numerical agreement did not establish
readiness capability. Rubric changes require new functional execution; the
exposed cases are development regressions, not an untouched final study.

Laya's trained `act`/`escalate` output and binary false/true ordering are
interpreted conservatively at their reported four-decimal resolution. A
rounding interval touching a decision boundary abstains. No readiness-domain
confidence claim or fitted risk bound is inferred from those probabilities.
Domain-level readiness reliability remains unestablished. The superseded
Vega statistical prerequisite and its failed results remain historical; this
migration neither reinstates it nor claims it passed.

Every state token and explicit obligation must fit the selected checkpoint's
1,024-token total limit, including the question and options. Oversized inputs
abstain without truncation or selecting the most confident document window.
Unsupported lengths, malformed responses, missing inference, stale evidence,
changed configuration or runtime/model drift cannot authorize publication.
