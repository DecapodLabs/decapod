# Vega readiness: functional delivery and historical contract diagnosis

## Accepted scope revision, 2026-10-20

The maintainer explicitly replaces the implementation-added 59-acceptance /
5%-risk / 95%-confidence / 50%-coverage research publication prerequisite with
functional integration acceptance. Required Vega is active after complete
mandatory validation: a current valid affirmative verdict can satisfy the extra
condition; rejection, abstention, malformed/unavailable inference and stale state
block readiness. Deterministic authority and human merge authority are unchanged.
There is no inactive/advisory substitute, forced verdict or model replacement.

The old statistical criterion remains **not passed**, with its original studies,
failures and limits preserved below. Domain-level readiness reliability and
production-calibrated accuracy remain unestablished, not a current delivery
qualification claim. No adapter, threshold, calibration or numerical tolerance
changes are made by this revision.

The twelve fixed cases in `tests/fixtures/vega-functional-regressions.json` were
frozen before their first model execution (SHA256
`97b4667e3cb3a74b41cfcc0e1ab713dcd046895aa0c8d813f6856b0c9ff68094`).
They cover three substantive existing Rust gate/tokenizer/calibration contracts,
matching unmet requirements, uncertainty, contradictions, untrusted instructions,
declared-but-unexecuted proof and failed/stale deterministic proof. Contract
checks actually execute before their observations enter the shared production
projection. Expected outcomes are outside model input. Real-model outcomes are
required on Metal and CPU, with positive acceptance as well as negative behavior.
This is bounded functional regression coverage, not independent population labels.
No fitting or tuning on these cases is authorized or performed.

The prior twelve production protocol requests all produced actual singleton
false, abstain=false/unbound=false; neither response validation nor the final
predicate inverted an affirmative answer. Their baseline disclosed constructed
nonexecution and an unassessed trajectory; it was not a meaningful verified-ready
positive. New cases test actual executed bounded obligations without hiding their
fixture provenance in public reports. No original result is relabelled.

The [completion checklist](vega-completion-checklist.md) records the implementation
and required exact-candidate evidence. The diagnosis below records the previous
contract and results; its original research-blocking instructions are superseded
by this explicit decision, while all observations and accuracy limitations remain.

## Preserved pre-revision diagnosis

The Rust integration executes the pinned Vega Boolean contract, but the current
profile has not demonstrated the readiness judgment required by issue #1445.
No option inversion or wrong calibration route was found. The failed experiment
also does **not** measure the production input path. Repeating it, increasing its
sample count or raising a confidence cutoff is not a justified product repair.

The maintainer requires the complete working integration in one PR and rejected
the earlier inactive-stage proposal. No inactive/advisory replacement is being
implemented. The current repair connects verification to the shared production
path, separates exposed diagnostics from final admission, and inspects supported
Vega adaptation before proposing training. Model/calibration pins, numerical
tolerances, deterministic authority and readiness risk/coverage criteria remain
unchanged. A passing original-scope PR is still conditional on actual evidence.

## What the recorded results actually test

The measured source is `9f017d31ce733f55e766e0068ca9c7f66b501417`.
Official controller v0.119.1 passed all 39 source-CI steps and 49 native tests;
three fixed 138/314/4096-token fresh-text Metal/CPU numerical comparisons passed.
The final full-validation report `vega-1191-validation-20261019T232350Z` failed
`vega-native-8`, with `publication_ready=false`. Its SHA256 is
`bfbceed66da9bc6b789c937bfb204f7c84d3ea5a6f9d29e1e204d51a989592e5`.
[The measured report summary is public](https://github.com/DecapodLabs/decapod/issues/1445#issuecomment-6281028106).
The detailed native revisions, tolerances, timings, lifetime-memory scope and
fallback limits remain in [the reference contract](vega-reference-contract.md).
These measured results do not validate later documentation commits.

`readiness_evaluation::state` constructs a benchmark payload containing intent,
the proposed patch, additional scenario text, oracle provenance and a test patch.
It bypasses `vega_readiness::projection`. Production uses `validation-results.v3`:
actual check observations/dispositions, declared intent and plan obligations,
unresolved facts and changed paths. Production contains no source diff, test
patch or raw logs. Exact material hashes bind custody outside the model prompt;
they do not reveal omitted code to the model.

Consequently the production obligation-preservation correction cannot change
these benchmark inputs. The repeated 23-conflict count neither tests that
correction nor establishes a production false-ready rate. Likewise, identical
production summaries can describe materially different untested implementations:
different custody hashes prevent stale proof, but cannot make an omitted
behavioral distinction observable to Vega. Broader branch/merge correctness is
not established by this bounded input.

All 29 accepted Metal inputs were reconstructed from the unchanged corpus and
the exact Rust builder's field order; all 29 SHA256 values matched the recorded
`state_sha256`. CPU accepted the same IDs with the same input hashes. Prompts
contained 1,213–3,541 total tokens, below 4,096; all extra conditions were present.
The production prompt refuses overflow rather than truncating. There is no
evidence that these visible conditions disappeared through clipping.

The complete typed outcomes, full-precision probabilities and input commitments
for **every accept** are preserved in
[exposed-v2-acceptance-audit.json](../tests/fixtures/vega-public-review-sources/exposed-v2-acceptance-audit.json).
The largest CPU/Metal difference in accepted yes-probability is
`0.00003403946227387156`; this agreement is backend consistency, not an
independent semantic oracle. Per-case raw hidden tensors/logits were temporary
and are not available for retrospective full recomputation of all 29 cases.
The three independent mathematical fixture comparisons must not be expanded
into a claim that every diagnostic row was independently reproduced upstream.

## Contract audit

| Concern | Finding |
|---|---|
| Trained task | This repository contains a generalist typed-decision base plus two gated task adapters. Decapod runs the **base**, adapters off; the repository name does not mean the public-intents adapter is running. Published typed-choice/other-domain results do not establish code-readiness skill. |
| Option order | Pinned upstream `option_texts` uses `[no, yes]`; Rust `boolean_prompt` preserves it. `Outcome.probabilities` is `[false, true]`. |
| Type and interpretation | The worker uses Boolean type index 2. `p[1]` is probability of the proposition, not an ordinal score, approval flag or probability of error. No score-to-Boolean conversion occurs. |
| Mapping | A top-label-consistent singleton `true` accepts; singleton `false` rejects; abstention, unbound/OOD, missing/ambiguous sets and malformed output cannot authorize publication. All 29 accepts actually contain singleton `true`, abstain=false and unbound=false. |
| Calibration | Adapters-off uses the pinned base `vega_config.json` calibration, not an adapter or teacher table. Base weights SHA256 `09c4c5fe203cf166d419dfaa72d75b26da00f47f66bafbfd750b2263ee1ca66b` also matches both adapters' declared base identity. |
| Threshold semantics | At alpha/risk `0.1`, Boolean conformal qhat is `0.5823749681631878`, confidence abstention threshold `0.5958565839690714`, unbound threshold `1.269094705581665`. These reproduce the upstream decision convention; they are not a readiness-domain 10% or 5% risk guarantee. |
| Prompt and layers | Template 0, query-first prefix below the configured boundary, fixed Boolean question and ordinary no/yes criteria are used. Upstream captures zero-based raw block outputs 13/19; the native worker captures those outputs before final normalization, not shifted `hidden_states` indices. Pooling order is state/question/last, options and state chunks. |
| Role qualification | A finite, correctly calibrated typed answer can still be semantically wrong or outside a validated task. `unbound=false` is not certification that code readiness is in-distribution. No published readiness qualification was found in these pinned artifacts. |

Primary contract sources are the pinned
[package common code](https://github.com/NandhaKishorM/vegaml/blob/21a17c90624aa85cec60b1092fa72cb8b720f71a/vegaml/_common.py),
[package API](https://github.com/NandhaKishorM/vegaml/blob/21a17c90624aa85cec60b1092fa72cb8b720f71a/vegaml/_api.py),
[checkpoint API](https://huggingface.co/nandakishorm/vega-08b-public-intents/blob/42ac8d4fa9efb61f96e53d80962cf28f6dfe667e/vega_api.py)
and [base configuration](https://huggingface.co/nandakishorm/vega-08b-public-intents/blob/42ac8d4fa9efb61f96e53d80962cf28f6dfe667e/vega_config.json).
Their SHA256 values are respectively `34f6fa580d30828bc4a4663fd0a6a870eee783d250a70a64cfdd1d683bb92cfc`,
`5301ffa59a9fe0b719c2aeda626b42001f972fb348bcb1e51192ffd690f06860`,
`ebfd2a586084306ce631068e36c168476522e69647f4ce580c56814bc229e81d` and
`12f4f73784d58532ef244288c425df24944a8bda9c8414aa7c0981dfd2d88494`.
Python was read as specification, never executed. Review of the contract and
scoped numerical results supports these findings, not universal runtime parity.

## Classification of all accepts

| Diagnostic variant | Accepts | What is observable and what the label supports |
|---|---:|---|
| Gold benchmark patch | 6 | Patch, issue and oracle description are visible. Agreement supports benchmark-oracle resolution only; no current proof or independently established complete-readiness label is supplied. |
| Unmodified base | 4 | Empty proposed patch and explicit unresolved F2P declaration are visible. Base code/current execution is absent. These known failing-obligation cases are outside the product's post-validation stage, regardless of the model's incorrect yes. |
| Missing measurement | 4 | Added peak-memory obligation and explicit absence of measurement are visible. Yes misses that constructed condition. This is a semantic regression, not an independently observed source-project readiness failure. |
| Changed requirements | 6 | Missing documentation/platform-test evidence is explicit, but the added “new API” and supported-platform scope are underspecified. A positive answer lacks support; the reject-versus-abstain label is not independently adjudicated. |
| Ambiguous performance | 5 | “Fast enough” has no workload or threshold. Yes misses explicit uncertainty within the scenario; it does not prove the actual source project is unready. |
| Contradictory evidence | 4 | Two unresolved reports are explicit, but are constructed rather than independently observed. Yes misses the scenario conflict; no production error rate follows. |

Thus 19 accepts miss visible constructed conditions and four are wrong-stage
cases. None establishes a sign/mapping bug. The six matching proxy positives
must not be called independently labelled production-ready branches. This
study cannot distinguish task incapability from every representation/oracle
limitation, but it does not justify the proposed readiness role.

Raising a confidence cutoff is not a demonstrated repair. Axum 1934's gold
case has yes-probability `0.70175346101484`; its changed-requirement and ambiguous
cases are *more* confident at `0.7050366954515972` and `0.7101406833792423`.
The wrong-stage Axum 1119 case is `0.7396260304384419`, above every matching gold
accept. Even excluding wrong-stage cases, a cutoff above all conflicting
semantic accepts retains only one of the six gold accepts, far below the
required positive coverage. This is diagnostic evidence against a simple
cutoff repair, not permission to fit another policy on exposed data.

## Final-evaluation admission decision

All v2 units are now **exposed diagnostic data**, never an untouched final
holdout again. Original source, labels, corpus digest, failures and output bytes
remain unchanged. The existing mandatory final check still fails; it has not
been removed or relabelled as passed.

The existing 356-record source audit yielded 82 structurally/timing-eligible
snapshots, not 82 readiness labels. The 24 previously inspected semantic cases
are reviewed in [final-admission-review.json](../tests/fixtures/vega-public-review-sources/final-admission-review.json).
Narrow static witnesses are retained, not promoted into complete readiness.
Later CI, unresolved source scope, review preferences, unsupported performance
claims and incomplete execution coverage remain explicit. **No new final units
are admitted or frozen by this audit.** Nine previously prepared pilot cases
also add `source_facts` absent from production; executing them would not test
the requested production projection. No additional scraping or model run was
performed to conceal this mismatch.

A valid future study must construct inputs through the production projection,
retain its tokenizer/overflow contract, and bind independently supported
requirements, candidate state, evidence and expected disposition. Adverse facts
must remain observable; verdicts and label rationale must not leak into input.
Known failed/stale/missing required proof belongs in deterministic policy tests.
Unresolved cases are excluded or explicitly abstaining, not positive by approval
or merge status. No claimed final readiness label may exceed its source scope.

Development, calibration and final evaluation must have disjoint related
source families/issue lineages; variants and backports cannot add independent
units. Freeze final units before any fitting. The existing one-sided 95%/5%
criterion requires at least 59 zero-error **accepted independent sampling
units**, not 59 variants or merely 59 total cases. Coverage and trustworthy,
representative labels remain separate requirements. Owner grouping limits
leakage but does not itself prove independence. No compatible recalibration is
justified by the available admissible evidence, and none was performed.

## Authorized continuation: repair production evaluation

The maintainer rejected the earlier inactive-stage proposal. It was never
implemented and is no longer the proposed delivery. The requirement remains a
working required Vega decision after deterministic validation, in one feature PR.

`vega_readiness::project` now supplies the same bounded typed projection to live
assessment and `EvaluationSnapshot::project`. Replay first invokes the existing
complete-suite evidence verifier; failed/missing/stale proof cannot dispatch the
model. This is verification of replay records, not authentication of their origin.
`Response::validated_outcome` shares live request/backend/token/numerical checks
with the native collector. No second full-patch approximation is used by the new
production-protocol diagnostics. New regressions are authored; results must be
recorded after prescribed exact-candidate validation.

The fixed development fixtures explicitly identify their observations as
constructed. They exercise retained obligations, contradictions, questions,
phase flags and declarations without a matching measurement. Actual model
responses to them are diagnostic capability probes, not independent labels,
calibration data or a final holdout. The unchanged numerical checks still run
independently. Required final evaluation now reports the source-admission gap
instead of rerunning exposed v2 and calling it untouched final evidence. Old
benchmark construction, hashes, measurements and failures remain preserved.

## Compatible upstream capability and adaptation audit

The selected base engine remains `09c4c5fe…`, adapters off, with its own calibration.
The published `public_intents` and `typed_decisions` adapters bind this base, but
their documented training tasks do not qualify code-readiness judgment. There is
no sign, option-order, calibration-route or hidden-layer correction established
by this audit. No adapter was silently enabled.

The pinned Apache-2.0 upstream already documents a supported adaptation route:
[rank-32 gated engine adapters](https://github.com/NandhaKishorM/vegaml/blob/21a17c90624aa85cec60b1092fa72cb8b720f71a/notebooks/vega_finetune.py).
Its approximately 0.84M fitted parameters affect seven engine projections plus
the routing gate; frozen-backbone features can be cached. Routing needs task
negatives, and temperature/conformal/abstention calibration must use a distinct
validation split. The inspected script SHA256 is
`7c2db72f4f2f5bdeab398304b4e64e3413e500b494eaecedf817f3017a51a352`.
This was source inspection only; no Python was executed. A Rust implementation
would need exact routing/base-hash binding and numerical qualification before
activation, not a different agent framework.

Current upstream `0eb2c28e4b08cfa8663897406430985501627d44` also supports a
[PCA/logistic task head](https://github.com/NandhaKishorM/vegaml/blob/0eb2c28e4b08cfa8663897406430985501627d44/vegaml/ttt.py)
over frozen features. It changes the readout rather than fixing Boolean mapping.
Its source requires task labels and cross-validation; a head is not automatically
servable merely because fitting returned coefficients. Source SHA256:
`1a6a3ae3a5c5eb7272823be34133a0fb008cb8d8e10108997eb836799049ecd8`.
The [calibration contract](https://github.com/NandhaKishorM/vegaml/blob/0eb2c28e4b08cfa8663897406430985501627d44/docs/calibration.md)
fits temperature and conformal threshold; temperature preserves argmax and
cannot repair systematically wrong answers. Source SHA256:
`fc880534c02abb1d67fd9704bc39cf03dad26ae39a35a401dd52565a6e4f6d95`.

The other public official model is
[vega-U at 4d205028…](https://huggingface.co/nandakishorm/vega-U/tree/4d2050287982ca7a7bb0c22fd19a389f5f94474e).
It has a different engine (`78f96be7c6a06abb873e573fcf9e43ef467010a30c72cd6cac32538cd7078db8`),
a learned port (`398ae79d5e3956a3ed809d500fbdf57982d303c719e464bb81ba12a5ee7d23bd`),
extraction statistics and reader-specific calibration. Its config SHA256 is
`000461315c798b6eca62323476360c14cf31b4e1adcba21ab8266fca41d3fcf8`.
It is not a compatible manifest-only upgrade of this pinned runtime and publishes
no readiness-domain qualification. Different generic benchmark scores do not
justify that port/engine/calibration expansion or remove the label dependency.

No fitting is scientifically justified yet: the audited 24 public source cases
still admit zero independently supported complete production readiness units.
Training on the exposed benchmark or on model-generated decisions would neither
repair label provenance nor create a new final evaluation. The smallest supported
adaptation, **if production-shaped development errors and admissible labels
justify it**, is a pinned gated engine adapter with separately fitted route
calibration; not a backbone training project. It still needs disjoint development,
calibration and untouched final units, including at least 59 independent accepted
zero-error units for the existing numerical risk bound plus coverage and sampling
justification. No paid compute, external data sharing, model replacement, fitting,
threshold change or inactive product gate has been introduced.

See [the completion checklist](vega-completion-checklist.md) for issue boundaries,
implemented behavior, regressions, exact prior evidence and remaining conditions.
