# Numerical reference contract and measured verification

The functional integration was delivered in merged [PR #1460](https://github.com/DecapodLabs/decapod/pull/1460),
candidate `04ed11ec`, and remains active through official v0.121.0. Subsequent
#1464 reran the required native qualification at `c77df1f0`: 60 native tests and
all eight scientific runs passed. The original safe-abstention mismatches and
unestablished domain accuracy remain explicit. Statements below that no feature
PR was ready describe their named historical failed checkpoints, not current
publication state. They are retained to preserve failure and research provenance.

## Current functional acceptance revision, 2026-10-20

The maintainer replaced the implementation-added 59-acceptance/5%-risk/
95%-confidence/50%-coverage research prerequisite with functional active Vega
integration acceptance. The old criterion is **not passed**. Its study, failures,
source-audit/admission ledgers and limitations remain preserved below as historical
research, not current instructions to block solely on missing research units.
No upstream calibration, model identity or numerical tolerance is changed.
Domain-level readiness reliability remains unestablished.

The reviewed eight-run mechanism now retains the three numerical phases, six
pairs of fixed functional cases on both backends, and a required strict aggregate.
Actual candidate Rust gate/tokenizer/calibration contract checks supply observations
to the shared production projection/request/response/final predicate. Cases and
expected outcomes are frozen before model execution in
`tests/fixtures/vega-functional-regressions.json`; labels stay outside the prompt.
Following the maintainer's delegated decision, each actual backend must approve
meaningful ready work; a different ready control may validly abstain while its
expected-accept mismatch remains reported. All-ready oracle agreement is a
separate reported result, not silently relabelled qualification. Always-abstaining
or rejecting models fail. Unmet obligations must reject and uncertainty,
contradiction/manipulation and failed/stale-proof controls must remain non-ready.
The reducer rechecks complete case/backend coverage, bindings, actual raw verdicts
and unchanged numerical exports. A reported match or successful collection cannot
authorize publication. No fitting or threshold tuning is performed.

Pre-revision `fef84896` passed all39 source steps,1463 library tests/3ignored,
53 native tests and the three fixed numerical Metal/CPU fixtures. Full report
042518Z (SHA25606be1331c630bce469e9250e111baf4f2bc8e6902716924d183037e56e51d2af)
failed the old admission condition. Synchronized Metal/CPU compute for138/314/4096
tokens was0.181/1.637s,0.381/3.604s,5.278/46.282s; whole verified requests were
13.470/14.910s,13.673/16.885s,18.595/59.487s. Peak process-lifetime RSS reached
5090705408B including prior/reference allocations, not isolated GPU/model memory.
Long Metal sampled hidden maxabs5.151912539247405e-5, pooled chunks3.1408493256002146e-5,
and combined-engine maxabs1.9880187955262363e-4Metal/3.723481778798027e-5CPU met the
unchanged criteria. Three fresh pure-text cases, shared generic infrastructure and
same-agent reference authorship do not establish all-mode or independent human
reference correctness. Actual unsupported-CUDA-to-CPU component fallback passed;
NVIDIA hardware and full Linux loader-companion routing remain separate limits.
New functional outcomes require the next prescribed exact-candidate validation.

### Actual functional run on 59eaf087

Official v0.119.1 passed Buildkite2311 all39source steps,57native tests and all
three numerical Metal/CPU cases. Report081406Z, SHA256
`d7fc06ddf9187b1e1ed6536bf4572ea38d6f5446846aadab9c6d16cce7305c76`,
failed its strict functional aggregate:22of24 expected case/backend outcomes match.
These are12fixed scenarios on two backends, not24independent sampling units.

| Case | Metal / CPU decision | Frozen expectation |
|---|---|---|
| gate-ready | accept / accept | accept |
| prompt-ready | accept / accept | accept |
| calibration-ready | abstain / abstain | accept; mismatch preserved |
| gate-unmet, prompt-unmet, calibration-unmet | reject / reject | reject |
| unresolved | abstain / abstain | non-ready |
| contradictory | reject / reject | non-ready |
| input-manipulation | abstain / abstain | non-ready |
| declaration-not-proof | reject / reject | reject |
| failed-proof, stale-proof | blocked before model / blocked before model | zero model calls |

The calibration-positive response is valid: yes0.57303135/0.57303569, prediction
set[false,true], abstain=true, unbound=false, no worker or validation error. The
shared required-mode predicate denies readiness, preserving the product contract.
No threshold, label, profile or output was changed to force agreement. This
demonstrates useful active approval and conservative blocking but does not pass
the frozen all-positive expectation or establish calibrated domain accuracy.

Current synchronized compute Metal/CPU for138/314/4096tokens was
0.168/1.369s,0.303/3.033s,4.294/39.078s; whole verified requests
11.224/12.439s,11.472/14.090s,15.369/50.203s. Peak process-lifetime RSS
5028216832B includes prior/reference allocations, not isolated GPU/model memory.
The 4096-token reference matched calibrated abstention/unbound on both backends.
Original sampling, same-author/reference and unsupported-mode limits still apply.

### Functional interpretation and current source evidence

After reviewing these actual results, the maintainer delegated the safe-abstention
decision. Functional delivery permits conservative uncertainty on a ready control
but never permits its publication. All original expected labels and mismatches
remain visible. Genuine positive approval is required on both actual backends;
every adverse/control case, raw response check and numerical comparison remains
mandatory. This is a transparent qualification revision, not a passing claim for
the earlier all-positive assertion, old risk criterion or domain accuracy. There
is no fitting or model/calibration/threshold/input/tolerance change.

`08b967b2` passed Buildkite2321 all 39 source steps and 1,464 library tests with
three ignored. The controller's first attempt stopped at public API HTTP403;
its retry rejected duplicate exact-workflow Buildkite builds after signed-push
redelivery. Both terminal reports remain failed and no new native result follows
from them. The qualification and documentation checkpoint requires fresh full
validation on this same branch, preserving the provider uniqueness boundary.

## Preserved earlier criteria and results

Research qualification and old recipe descriptions below are historical; numerical
equations, artifact pins and comparison tolerances remain current and unchanged.

**Current product decision:** [the contract diagnosis](vega-readiness-decision.md)
audits all 29 accepts from the completed 9f017d31 run. Its v2 benchmark payload
bypasses production projection and is now exposed diagnostic data, never an
untouched final holdout again. The numerical passes and original failed results
below remain evidence within their stated scope. No readiness qualification,
threshold change or passing publication claim follows. The maintainer rejected
inactive/advisory delivery. Shared-production projection and response-check
diagnostics now replace the second input builder for new protocol verification;
their constructed observations supply zero independent labels. The original
risk/coverage requirement remains blocked pending admissible final units, rather
than rerunning exposed v2 as a new holdout. See the
[completion checklist](vega-completion-checklist.md).

The criteria below were frozen before candidate outputs. The executed-results section identifies the later measured subset; historical failures are retained separately. A passing subset never establishes unsupported context lengths, modes, backend execution or readiness-domain calibration.

Source: NandhaKishorM/vegaml@21a17c90624aa85cec60b1092fa72cb8b720f71a, vegaml/_common.py (Apache-2.0), SHA256 34f6fa580d30828bc4a4663fd0a6a870eee783d250a70a64cfdd1d683bb92cfc.
Tokenizer: Qwen/Qwen3.5-0.8B@2fc06364715b967f1860aea9cf38778875588b17 tokenizer.json SHA256 5f9e4d4901a92b997e463c1f46055088b6cca5ca61a6522d1b9f64c4bb81cb42 (Apache-2.0).
Engine/config: nandakishorm/vega-08b-public-intents@42ac8d4fa9efb61f96e53d80962cf28f6dfe667e, config SHA256 12f4f73784d58532ef244288c425df24944a8bda9c8414aa7c0981dfd2d88494; weights 09c4c5fe203cf166d419dfaa72d75b26da00f47f66bafbfd750b2263ee1ca66b.

Independent implementation: scalar F64 engine/backbone equations and an independent byte-level BPE routine, authored from pinned upstream specifications. No candidate engine, decoder, calibration, prompt or backbone math calls generate expected outputs. Shared JSON/SafeTensors/SHA256/regex/Unicode infrastructure and weight artifacts limit independence. Author is the same agent: this is a separate implementation, not separate human review. Conditional engine checks use candidate pooled features as INPUTS only and do not prove those features. Separate combined checks use the scalar backbone's own computed pooled features.

Predeclared comparison criteria: token IDs/spans/selected block indices exact; scalar engine vector/readout abs error <= 0.001 + 0.0001*abs(reference). F32 projector reductions have up to 2048 terms (n*machine epsilon about 0.00025); the provisional tolerance allows reduction order and accumulated nonlinear steps, not arbitrary output replacement. Report max absolute/relative errors, not just pass. Calibration compared on identical supplied engine readouts: probability/temperature abs <= 1e-10; set/abstain/unbound exact including equality boundaries. Synthetic pooling <= 1e-5 + 5e-6*abs(reference); empty/invalid spans must fail. No tolerance change after inspection without a separately recorded rationale and rerun. The scalar-backbone criteria below additionally govern selected-block comparisons; no full-context parity claim follows from the measured short CPU subset.

Evaluation: group by repository/issue lineage, freeze held-out case bytes and digest before inference or any tuning. Preserve pinned upstream calibration. Zero false-ready and one-sided 95% upper bound <=5% among accepted independent cases requires at least 59 independent accepts with zero errors; coverage >=50% overall and among ready cases. Related variants are not independent samples. Agent-curated static and counterfactual cases establish bounded regressions, not production calibration. Report deterministic gate cases separately from model-judgment cases. No fitting on holdout, no claim that policy rejection demonstrates model understanding.

## Prospective source audit and assessment-input correction

`core::vega_readiness::projection` previously serialized the plan intent but
omitted its declared proof hooks, deferred questions, stop conditions and phase
gates. `validation-results.v3` preserves those obligations and other unresolved
plan facts under the same byte/token limits. Gate flags are declarations, not
execution proof; overflow abstains without truncation. This input correction
does not alter the frozen v2 evaluation, pinned upstream calibration or risk
criterion. Its preservation and overflow tests await prescribed validation.

`tests/fixtures/vega-public-review-sources/README.md` records the prospective public
source capture: 356 review snapshots from 274 repositories, with exact source
review and commit identities, requirements and observed CI contexts. The pinned
Rust audit checks complete histories, temporal binding, later requirements edits
and conflicting verdicts, excludes previous-corpus owner families and prevents
selected verdicts or future CI from entering prospective input. It runs as an
independent diagnostic in numerical collection and cannot replace any existing
scientific acceptance condition. Source-audit failure does not suppress native
model checks. New source records have not been used for inference or fitting;
source membership, structural eligibility and a public approval alone are not
semantic ground truth or independent human adjudication. The new heldout inputs
are not yet frozen for inference. All source-audit regression results are pending.

## Executed candidate 30015be0 (2026-10-19 UTC)

Official v0.119.1 collected [Buildkite 2261, all 39 source steps](https://buildkite.com/decapodlabs/decapod/builds/2261), including 1,459 passing library tests and 3 ignored tests. It then compiled the native worker on this Mac and executed **43 passing worker tests, none ignored**. Full validation `vega-1191-validation-20261019T182235Z` correctly failed the required strict scientific aggregate (`vega-native-8`). All six evaluation groups completed on both backends. No inference failed and no case was excluded by a dispatch deadline. Failed bootstrap, public-reader and earlier snapshot attempts remain preserved failures. The successful source retry used existing GitHub authorization to redeliver the original signed push, without a Buildkite login.

All three fixed **138/314/4,096-token** numerical fixtures passed on actual Apple M1 Max Metal and CPU. The 314-token fixture activates two memory slots; the 4,096-token fixture activates all 16. Six tokenizer fixtures, the exact 4,096-token boundary and two 4,097-token rejections matched the separate byte-BPE calculation without truncation. Selected raw blocks 13/19, pooling, the separate scalar-F64 engine, identical-readout calibration and combined independently computed backbone/engine comparisons passed unchanged tolerances. These are three fresh pure-text prompts and six backend comparisons, not six independent prompts, human review of the reference, empirical HF execution or all-input/streamed/cache/image equivalence.

At 4,096 tokens, 8,192 sampled Metal block scalars had maximum absolute error `5.151912539e-5` and normalized RMS `2.715368673e-5`; 16,384 pooled chunk scalars had maximum error `3.140849326e-5` and normalized RMS `1.233811494e-5`. Combined engine maximum errors were `0.000198801880` on Metal and `0.000037234818` on CPU. Both long-fixture decisions abstained and were unbound, matching the independent calculation; neither authorizes readiness. The scalar-F64 long calculation took 694.831 seconds. The real eight-F32 Metal probe was bit-exact with synchronized readback; 32 fresh-state recurrence comparisons also passed. The patched shader identity remains `3ba973f49a14c320b748c256a40ab08190ba9ae611319397d2279e4fdd4b9568`.

| Tokens | Backend | Synchronized backbone/pooling/readback/cleanup | Whole verified request |
| --- | --- | ---: | ---: |
| 138 | Metal | 181.345ms | 11.614s |
| 138 | CPU | 1.503s | 12.956s |
| 314 | Metal | 318.283ms | 11.745s |
| 314 | CPU | 3.165s | 14.547s |
| 4096 | Metal | 4.570s | 15.990s |
| 4096 | CPU | 41.264s | 52.677s |

Conditions: M1 Max with 24 GPU cores and 64 GiB memory, macOS 27.0.1, Rust 1.96.1, Xcode 27; F32 backbone/CPU engine and F64 calibration. These are uncontrolled one-shot observations, with installation verification/reverification included in request totals, not controlled cold/warm benchmarks. Maximum numeric-process lifetime RSS was 5,016,993,792 bytes, including earlier attempts and allocations; it is not isolated model allocation or total GPU memory. Actual unsupported CUDA returned `hardware_unavailable`, zero tokens and no outcome; the production classifier permitted enabled CPU fallback and rejected disabled fallback, followed by genuine fresh CPU inference. Failure of available Metal, NVIDIA hardware and full Linux loader-companion routing were not exercised.

The frozen v2 holdout still fails the declared criterion. **Per backend:** 138 judgment cases from 23 issue lineages; 102 actual outcomes, 36 unsupported-token cases, no inference failures or deadline exclusions, 52 abstentions and 29 acceptances, including **23 false-ready outcomes** against the proxy labels. Nine issue units had an acceptance and all nine had an erroneous acceptance, giving the reported one-sided 95% upper limit of 1.0 under the interval assumptions. Nonabstaining coverage was 36.23%; benchmark-resolved acceptance coverage was 26.09%. Required-abstention labels had 17 correct, 15 incorrectly definitive and 14 unobserved outcomes. These are complete collection dispositions, not complete supported-input coverage or production calibration. Source authorship, dependence across four repositories, full-patch representation and the 23-original sample remain limitations; no fitting or thresholds changed.

Across102executed requests per backend, nearest-rank p50/p95 whole-request latency was13.812/16.309sMetal and30.944/47.842sCPU. Synchronized compute p50/p95 was1.958/4.037sMetal and19.375/36.191sCPU. Maximum evaluation-process lifetime RSS was3,726,934,016BMetal and4,951,293,952BCPU. Related examples, fixed ordering, full per-request integrity hashing and concurrent read-only source investigation limit performance interpretation.

Current runtime SHA is `73f1c6f90cbc3e23e60810f9ddb3cca829ea5aa0de985fee5dee82f4aa566f5d`; compound model and v2 corpus pins remain `2b985edf9a5641fabe12b54f5f94bdff2ca888f19551593b8d9f87cd088cf9fc` and `09df504b0d50004499172c1cebf9fbf414ed91d72a703ebd0b0a634e3a7e3e88`. The precise model digest is carried by the private proof manifest; original private exports and failed validation are retained outside Git. Current public execution details are linked from [issue1445](https://github.com/DecapodLabs/decapod/issues/1445#issuecomment-6273527599). No feature PR readiness is established.

## Executed candidate d3ebad07 (2026-10-19 UTC)

Official v0.119.1 completed [Buildkite2231, all39steps](https://buildkite.com/decapodlabs/decapod/builds/2231) and all40native tests. The real M1 Max eight-F32 dispatch/synchronization/readback was bit-exact in4.606750ms (device creation excluded). All32fresh-state recurrence comparisons passed in197.947833ms, with unchanged maximum output/state errors3.515577587e-41/3.725290298e-9 and declared tolerances. These are primitive results, not new model parity.

The numerical collection preserved pinned public-download errors (curl56/28), with no complete model output or independent reference export. Most evaluation shards also failed setup. The last explicit-CPU shard executed22of23selected cases with0inference failures and1unsupported input,4acceptances all false-ready against proxy labels,11abstentions, and704008.059208ms collection time. Across those22requests total inference was22613.725708–45144.082792ms, synchronized backbone/pooling/readback/cleanup9384.522958–31901.306ms, and maximum process-lifetime RSS4905746432B. This includes previous process work and full per-request integrity hashing, without controlled warm/cold conditions or isolated GPU-memory measurement. Other shards are missing; these counts are not a complete holdout or production-risk estimate. No fitting, thresholds, corpus or labels changed. The earlier aa66 short/chunk model results below remain historical evidence and are not current-candidate readiness.

Report `/private/tmp/vega-1191-validation-20261019T072203Z.json` and private evidence `decapod-readiness-Em0dEG` retain this failure. Strict aggregation correctly returned nonzero, but its early missing-export error obscured available phase diagnostics in the final receipt. The reporting correction now writes a private failure receipt with independently attempted context-bound phase diagnostics and explicit scientific-reverification scope, without relaxing the final conjunction. New reporting/access-denial regressions require execution through fresh validation. A successful collection-only exit remains insufficient for scientific or PR acceptance.

## Executed candidate aa66c8e4 (2026-10-19 UTC)

Official v0.119.1 completed [required Buildkite2216, all39steps](https://buildkite.com/decapodlabs/decapod/builds/2216), [Actions39201940594](https://github.com/DecapodLabs/decapod/actions/runs/39201940594), native compilation and **30 passing native tests**, including the real Metal recurrence regression. Full validation then correctly failed held-out scientific acceptance. Private report `/private/tmp/vega-1191-validation-20261019T045602Z.json`, evidence directory `decapod-readiness-jgUdd2`, retains every phase and failure. No feature PR is ready.

The M1 Max executed32fresh-state recurrence repetitions per diagnostic against independently indexed F64 equations, comparing32,896output and16,384final-state scalars each time. All passed the predeclared1e-6+1e-5-relative bound: maximum output difference3.515577587e-41 and state difference3.725290298e-9. Short/chunk32-repetition wall times408.426958/171.094916ms include synchronization/readback. The standalone real eight-scalar Metal probe was bit-exact in229.022042ms; the later diagnostic probe took6.174666ms, excluding device creation. Source inspection plus improved real comparisons supports the narrow barrier repair, not equivalence for every kernel or model mode.

Both fixed model fixtures passed the independently implemented Rust tokenizer/backbone/pooling/engine/calibration comparisons on Metal and CPU without changed tolerances. Short138-token Metal sampled8192block scalars: maximum error4.344122569e-6, normalized RMS3.427706843e-6; combined independent engine error1.453727703e-5. The314-token fixture has209state tokens and two active memory slots: Metal chunk error1.011768277e-6, normalized RMS8.037759093e-7; combined engine1.046089242e-5. CPU combined errors remained8.429680478e-6/7.953352800e-6. Exact masks, selected blocks13/19, sampled positions, calibrated sets, abstention, unbound and settle checks passed. This is two fixed fresh pure-text prompts within the then512-token reference range, with same-agent authorship/shared parsers and weights, not independently human-reviewed or all-context equivalence.

Representative short Metal output in `[false,true]` order was `[0.7082103355241359,0.2917896644758642]`, singleton false, no abstention/unbound,12settle steps. Synchronized backbone/pooling/readback/cleanup took5461.428333ms, total20178.218541ms including12851.063751ms verification/reverification. CPU took1716.755834ms/15080.970833ms. Process lifetime peak RSS was2415198208B/4868046848B respectively, including prior work, not isolated allocation or total GPU memory. Additional314-token Metal compute took361.8825ms after earlier diagnostics; this is not a controlled cold/warm comparison. Actual unsupported CUDA execution returned hardware_unavailable with zero tokens/no outcome; the production classifier permitted enabled fallback and rejected disabled fallback, followed by fresh CPU model inference. This remains component fallback, not failure of available Metal or exercised Linux loader-companion custody.

Both heldouts completed their bounded collection. Automatic Metal:27outcomes,21token-limit errors,90deadline exclusions,18accepts including15false-ready against proxy labels,9abstentions. CPU:12outcomes, no inference errors,126deadline exclusions,5accepts including4false-ready,7abstentions. Accepted issue units5/1 were all erroneous; one-sided95% upper risk1.0. Required-abstention labels: Metal2correct/6incorrect definitive/38unobserved; CPU2/2/42. Model-judgment coverage is incomplete and nonaccepting. Collection wall times609152.854666/612409.243833ms crossed the600-second soft cutoff in an in-flight request; the entire1520609.707541ms collector stayed within its1800-second hard bound. These full-patch benchmark proxies are not the production bounded-validation-summary distribution or human readiness ground truth. Long inputs and input representation limit attribution of these outcomes to production calibration; the observed criterion failures remain failures.

## Coverage protocol subsequently executed on 30015be0

The worker permits a scalar reference resource range up to4096fresh pure-text tokens. It changes no equations, weights, calibrated outputs, tolerance, risk criterion or holdout membership. Before long-model observations, a third fixture was frozen: the same literal coverage sentence below repeated306times, followed by the first0..32literal ` x` suffixes whose **independent** tokenizer prompt has exactly4096tokens. Candidate tokenization and model outcomes do not select this fixture. Both model backends must match that independent token/span layout and activate all16memory slots. A separate one-token extension must be exactly4097and rejected without truncation by both tokenizers. Failure to construct the frozen fixture is reported, never replaced after seeing model results. These boundaries and long-model comparisons now have the measured30015execution above; the earlier authored resource-range change alone was not proof.

The reviewed recipe uses the existing eight-run limit: `--collect-scientific-numerics`, six `--collect-readiness-shard --index 0..5` runs, then **required** `--verify-scientific-collection`, each with the kernel-supplied `--proof-output`. Numeric collection attempts all three fixtures independently. Each shard selects ordinal modulo6 from the unchanged frozen judgment cases and attempts automatic Metal and explicit CPU under one absolute1600-second soft dispatch deadline covering preparation and both backends. The original1800-second command and7200-second overall kernel bounds remain, without per-case reset. No nested process groups or alternative preflight are introduced. Exhausted or unsupported cases remain explicit coverage failures; dispatching fewer cases cannot satisfy acceptance.

Collection phases return success only for collection/publication of bounded private receipts, marked collection_only/publication_ready=false and acceptance_pending. They do not claim passing scientific acceptance. The last mandatory command rechecks current-directory/context/runtime/model/corpus bindings, exclusive phase identities and private single-link files, rereads numeric exports and recomputes numerical comparisons and all grouped evaluation metrics. Missing/malformed/replayed/mixed receipts, altered labels, inconsistent outcomes or any unsatisfied numerical/risk/coverage/independent-label condition fail. The kernel retains the final verdict. These ephemeral receipts neither reconstruct task custody nor authenticate against a malicious worker/same-user forgery. Previous-run files are never imported.

One immutable hash-verified tokenizer snapshot per backend collection identifies genuinely over-limit states before model loading. It only rejects unsupported inputs; every supported state still runs complete before/after model verification and real inference. Token overflows are reported as unsupported_input_tokens, not model abstentions. Reducer, shard, private-file, identity and tokenizer-snapshot regressions are among the43worker tests executed on30015. The23lineages, dependence, same-agent proxy labels and known failed baseline outcomes still cannot establish the frozen production calibration criterion. No fitting or new calibration pin is introduced. The vendored license uses the official Apache standard text, correcting a copied project-specific appendix without changing shader bytes.

## Historical candidate 19bdaaf3 and pending Metal repair (2026-10-19)

Official v0.119.1 completed the full run recorded privately in `/private/tmp/vega-1191-validation-20261019T035404Z.json`, evidence directory `decapod-readiness-Ff4Roh`. All **28 worker tests passed**, including both independent memory-reference analytic tests. Required [Buildkite CI2211](https://buildkite.com/decapodlabs/decapod/builds/2211) passed all39steps. [Actions39196818895](https://github.com/DecapodLabs/decapod/actions/runs/39196818895) passed on its same-SHA failed-job retry; the first attempt's integration fixture launch failed with OS26 executable-file-busy before invoking its supervisor. Both attempts remain history. Full validation correctly failed the scientific collector, so source results do not authorize publication.

CPU comparisons passed on both fixed prompts. The138-token combined independent engine error was8.429680478e-6. The314-token fixture now independently computed209state rows and two active memory chunks: chunk maximum absolute error5.560140508e-7, normalized RMS6.214951953e-7; combined engine error7.953352800e-6. This verifies the reference-memory correction and this bounded CPU subset, not all input lengths or readiness calibration.

Metal dispatched the real model on both prompts. Short raw-block error was0.00134473118 and normalized RMS0.00102319962; the314-token raw error was0.002752724117 and normalized RMS0.001386391886. Component backbone tolerances passed, but combined independent engine errors **0.001178328630 and0.001727843707 failed** the unchanged mixed engine bound. Earlier short Metal success on76 is not substituted for these failures. The real eight-scalar probe was bit-exact in6.93375ms after synchronization, excluding device creation.

Both heldout phases still executed. Automatic Metal produced27outcomes,21token-limit errors and90deadline exclusions,18accepts including15false-ready against proxy labels, and9abstentions. Explicit CPU produced12outcomes, no inference errors and126deadline exclusions,5accepts including4false-ready and7abstentions. Accepted issue units were5and1 respectively, all erroneous; one-sided95% upper risk1.0. CPU overall nonabstaining coverage was5/138, positive acceptance1/23; required abstentions were2correct,2incorrect definitive and42unobserved. Neither partial run satisfies calibration. The corpus and runtime hashes, per-case timings and cumulative process memory remain in private receipts. No fitting, denominator changes, error-to-abstention conversion or holdout tuning occurred.

Source inspection identifies a synchronization hazard in pinned attention-rs f22e42c: a64-thread Metal recurrence group can span multiple SIMD groups, but its token loop lacked a final barrier between shared-key readers and the next token's writers. The separate worker now vendors only the exact metal-kernels0.2.6 subpackage and adds three end-token threadgroup barriers (ordinary, variable-length and GQA recurrence). All other upstream files, API, arithmetic and dispatch geometry remain unchanged. Base commit/tree, original and patched shader hashes and declared Apache-2.0 redistribution terms are recorded in `assets/tools/vega-native/vendor/metal-kernels/README.decapod.md`. The targeted Cargo resolver also normalized twelve existing Windows-only dependency edges without changing any package versions; no Windows execution is claimed. Main-crate and Bazel dependency locks are unchanged.

Before observing patched execution, `metal_recurrence_reference.rs` freezes32fresh-state real-Metal repetitions,257tokens,128key/value dimensions and a sparse dyadic fixture. An independently indexed F64 state equation supplies all expected output and final-state scalars, with absolute1e-6 plus relative1e-5 tolerance. No candidate CPU/model math supplies the oracle. The regression and probe receipts bind patched shader SHA256 `3ba973f49a14c320b748c256a40ab08190ba9ae611319397d2279e4fdd4b9568`. This source-level repair and its tests are **not yet executed**; functional causality and renewed model parity require fresh full validation. A passing primitive will not establish full backbone or readiness calibration. The512-token reference range and all prior model, calibration, corpus and comparison criteria remain unchanged.

## Historical candidate 76e87da2 and reference recovery (2026-10-19)

Official v0.119.1 validation completed source proof, native compilation and **26 passing worker tests**, then correctly failed the required scientific collector. Source evidence: [Buildkite CI2206](https://buildkite.com/decapodlabs/decapod/builds/2206) and [Actions39192398162](https://github.com/DecapodLabs/decapod/actions/runs/39192398162). Private full report: `/private/tmp/vega-1191-validation-20261019T031409Z.json`; evidence directory `decapod-readiness-uiU9Ek`. Failures remain preserved; these observations do not authorize publication.

The M1 Max executed the pinned model through Metal, with synchronized readback and consumed-instance reuse rejection. The138-token fixture passed the separate scalar-F64 tokenizer/backbone/conditional engine/combined engine/calibration comparisons on both Metal and CPU with unchanged tolerances. Metal sampled8192 raw block scalars: maximum absolute error9.847112463e-5, normalized RMS6.72976349e-5; conditional engine3.519953733e-6, combined independently computed engine4.048284910e-4. Metal probabilities were[0.7081824646882422,0.29181753531175775], singleton false, no abstention/unbound. This is one short numerical fixture, not a labelled readiness decision. Metal synchronized backbone/pooling/readback/cleanup4815.821ms, total16078.406ms including10661.075ms verification/reverification; CPU backbone1428.784ms and total12625.356ms. Process lifetime high-water RSS was2562490368B for Metal and4806197248B for CPU, including earlier work, not isolated allocation or total GPU memory. The real eight-element Metal probe was bit-exact in5.454875ms, excluding device creation. Single requests after probes are not controlled cold/warm latency benchmarks.

Actual unsupported CUDA execution on this Metal-only build returned hardware_unavailable. The production retry classifier permitted enabled fallback and rejected disabled fallback; fresh CPU model inference then succeeded with the same pinned artifacts. This exercises the component fallback path, not failure of working Metal or full kernel runtime-binding/ELF-loader companion custody and publication.

The unchanged additional fixture executed314tokens with209state tokens and two active memory chunks on both backends. Conditional independent F64 engine comparisons passed (Metal3.51327524e-6, CPU4.20507368e-6 maximum absolute errors). Its separate backbone comparison failed because the authored reference returned all-zero chunks and an inactive mask. Combined engine differences around0.30 used those incomplete reference inputs; they do not establish a production pooling defect. The narrow reference repair now verifies the exact engine config hash above and independently accumulates its own block19 rows by `floor(i*k/n)`, directly following pinned vegaml `_chunk_pool`, with128minimum tokens,128tokens/chunk and16slots. It calls no candidate pooling helper. Analytic arithmetic-series tests freeze128/129/209/384/512-token means, offset/multichannel behavior, inactive slots, malformed ranges and nonfinite/overflow rejection. The512-token reference cap, fixtures, model artifacts, calibration and tolerances stay unchanged. This repair and its tests require fresh execution before any nonempty-chunk parity claim.

Both heldout phases ran despite the numerical failure. Automatic Metal:32 model outcomes,26token-limit errors and80deadline exclusions from138judgment variants;18accepts included15false-ready against proxy labels, with14abstentions. Explicit CPU:14model outcomes,11token-limit errors and113deadline exclusions;7accepts included6false-ready and7abstentions. Accepted issue units were5/3 respectively, all with at least one false-ready; one-sided95% upper risk1.0. Metal nonabstaining coverage18/138 and resolved acceptance3/23; CPU7/138 and1/23. Required abstention labels: Metal4correct/6incorrect definitive/36unobserved; CPU2/3/41. Errors are input-coverage failures, not abstentions; the existing report groups token-limit errors under inference_failures. No input was truncated or removed, and no fitting or threshold changes occurred. These are incomplete evaluations of agent-authored proxy labels; observed long inputs1133–3636tokens exceed independent reference coverage. Neither broader backbone correctness nor production calibration is established. The frozen23-lineage count and label authority also remain insufficient independently of observed failures. No feature PR is ready.

## Public source research and reference independence

The exact Qwen snapshot lists config, tokenizer, weights and metadata but no intermediate tensor corpus. The pinned vegaml tree includes Python/MLX/ONNX implementations and tests; these were read as specifications and were not executed. The local Rust reference is an independently coded calculation by the same agent, not a separately reviewed reference.

A further Rust backbone candidate was found: `EricLBuehler/mistral.rs@28ed6daca455b4ae063896014d1ea698b912c539`, `mistralrs-core/src/vision_models/qwen3_5/text.rs`, SHA256 `4ed1ab37a0c2a19b873a5214cf432fc24cca1ba626280c822168d116e8de4070`. It contains hidden-state tap machinery. It has not been adapted, dependency-reviewed, built or executed as an independent oracle; shared Candle/kernel/source ancestry must be assessed. Thus no claim is made that an adequate full-backbone reference is unavailable, or that full-backbone parity passed.

`assets/tools/vega-native/reference_tokenizer.rs` implements byte mapping, ranked BPE, special-token matching, NFC and prompt segmentation separately from the candidate tokenizer/prompt builder. `reference_engine.rs` independently decodes F32/BF16 weights and calculates the boolean engine, memory, world projection, dynamics, thermal probabilities and abstention. The scalar backbone calculation below supplies independently computed block/pooling inputs for a separate combined comparison. Candidate `2f785c82` executed the limited CPU comparison and primitive tests described below; the reference has not received independent human review. Synthetic primitive tests and candidate CPU/Metal agreement alone do not establish backbone parity.

## Scalar backbone reference, before numerical observations

`reference_backbone.rs` implements the dense pure-text model through raw decoder blocks 13 and 19 from [Transformers v5.3.0 source](https://github.com/huggingface/transformers/blob/aad13b87ed59f2afcfaebc985f403301887a35fc/src/transformers/models/qwen3_5/modeling_qwen3_5.py), Apache-2.0, source SHA256 `0e7410f5251e5c324a5628c5340ee3551f7a6a505ce398569fcc3b5f3ac07f7a`. That Python source was read as a mathematical specification only, never executed. Model config SHA256 is `b90b86f35c8e6925ef74ee04d0e758f0a845c83a42089ad82bbaa948de9b4204`; full checkpoint SHA256 is `04b1c301231dd422b8860db31311ab2721511346a32cb1e079c4c4e5f1fe4696`, exactly 1,746,942,600 bytes. The calculation verifies those identities before decoding. It uses independently written scalar F64 projections, causal convolution, recurrent delta-state equations, centered/gated RMS normalization, interleaved query/gate extraction, causal grouped attention, text RoPE and pooling. At most four scoped token-row threads execute fixed-order reductions; it calls no candidate model or tensor kernels. It is not an empirical reproduction of the HF runtime.

The initial scope is one unpadded pure-text prompt of at most 512 tokens, fresh state, no images/adapters/cache or long-state/chunked equivalence. The diagnostic compares one prompt against each successful backend; backend repetitions are not independent prompt samples. Token IDs/spans, selected layers, sample positions and chunk masks must match exactly. Private raw samples cover sorted positions `{0, state.start, state.end-1, last}` at both blocks; normal production requests collect none. Before observing any outputs, the provisional component tolerance is `0.005 + 0.001*abs(reference)` and normalized RMS error at most `0.002` for sampled activations and real pooled vectors. These permit F32 versus F64 reduction/recurrence differences across 20 blocks (individual projection widths reach 6,144), not arbitrary formula divergence; they are provisional comparison criteria, not an analytic universal error bound. Maximum absolute/relative and normalized RMS errors and sample counts are retained. Existing conditional-engine and identical-readout calibration tolerances stay unchanged. Combined independently computed backbone/engine probabilities additionally require absolute error <= `0.005`, with exact prediction set, abstention, unbound and settle behavior. No change after observing results is permitted without recorded rationale and rerun.

Both native backend attempts run before this oracle calculation, so reference failure cannot suppress their execution/resource diagnostics. Each full private numeric backend export and the independent expected-value export retains the 1 MiB bound. The combined diagnostic receipt records their paths and SHA256 digests instead of duplicating arrays or raising the limit; these are retained diagnostic files, not authority for a worker to approve itself. The reviewed `--collect-native-verification --proof-output <new-path>` recipe additionally attempts the separate chunk and holdout phases without waiving any failed result. Reference projection/convolution, zero-state recurrence/decay, query-gate layout, RoPE, normalization/overflow and comparison fixtures passed in the 25-test run on `2f785c82`. Later collector/reuse tests remain unexecuted. Full-context equivalence, independently reviewed reference correctness, production fallback routing and readiness calibration remain unsatisfied.

Further public attempts: pinned Mistral's Qwen config/text/tap APIs are private; the exposed speculative attachment loads an unrelated draft model, so no generation API was substituted for tensors. [Ferrite at bdaddf5c76b9392ba64e77fb565288fe1fdec53c](https://github.com/AKMessi/ferrite/tree/bdaddf5c76b9392ba64e77fb565288fe1fdec53c) was inspected but rejected as an oracle because its README explicitly records unresolved SSM divergence; its pinned tree contains no reference tensor corpus or licence file. No Ferrite code was copied or executed.

## Frozen benchmark-derived evaluation construction

`evaluation_corpus.rs` reads the public MIT SWE-bench Multilingual Parquet artifact at dataset revision `846e647b9f33c0b51b739d005d13d85493c9af09`, 1,875,846 bytes, SHA256 `92abca7cb527b41a9f66d03a26ce441ff7319e3a49f985998fd56be4bb9b08b2`. The public methodology describes manually curated issue/PR pairs verified against fail-to-pass and pass-to-pass tests: <https://www.swebench.com/multilingual>. This is not a human-labelled Decapod-readiness dataset. Patches retain the original projects' licences.

The frozen development groups are astral-sh/ruff, nushell/nushell and sharkdp/bat (20 original tasks). Holdout groups are burntsushi/ripgrep, tokio-rs/tokio, tokio-rs/axum and uutils/coreutils (23 original tasks). Related tokio projects stay together. Every variant of an issue stays in its repository group. The generator checks unique issue IDs and duplicate base/patch tuples, sorts case IDs, and refuses changed counts. No model outputs or tuning informed this split. The dataset's original split is named test; this separately declared development/holdout split governs this experiment only.

The initial v1 construction (preserved in Git through `4a96c804`) produced gold-patch, unmodified-base, missing-obligation, changed-requirement and ambiguous-requirement judgment cases (215 total), plus failed/stale/missing proof policy cases (129 total, excluded from judgment coverage). Before any model outputs or fitting, v2 explicitly adds one contradictory-semantic-evidence case per original, labelled to abstain: 258 judgment cases and the same 129 policy cases. Existing cases, project split, source artifact, upstream calibration and numerical risk/tolerance thresholds remain unchanged. Expected abstentions now have separate accuracy/miss counts; confidently rejecting ambiguous evidence is not counted as a correct abstention. This addition covers an explicit issue requirement, not a response to model results. Gold labels mean only upstream benchmark-oracle resolution. Added requirements, conflicts and policy conditions are explicitly constructed scenarios, not invented claims about actual CI runs. Their labels and rationales are agent-authored adaptations, not independent human readiness adjudication. Requirements, pinned source commit, patch, test patch, F2P/P2P names, provenance and rationale remain inspectable. No fitting or upstream calibration replacement is performed.

The native recipe's `--diagnose-native` collects these inputs independently of model setup/inference and records a private corpus digest and path. For standalone reproduction the same worker supports `--prepare-evaluation --proof-output <new-private-path>`; it downloads only the exact public artifact, verifies size/hash and decodes it in Rust. Neither mode executes benchmark test scripts (including any Python) or claims to have rerun upstream tests. The original 1 MiB diagnostic receipt limit remains; the separate reviewable corpus is capped at 8 MiB and installed without clobbering an existing path. Evaluation metrics remain unmeasured until an evaluation actually executes. Related variants cannot inflate the risk denominator: even 23 independent gold accepts with zero errors would have a one-sided 95% binomial upper bound of about 12.2%, above the frozen 5% criterion.

## Bounded model evaluation and final acceptance

The separate worker now authors `--evaluate-readiness --split holdout --backend auto --proof-output <new-private-path>` (or `development` / explicit `cpu`). It constructs the same frozen inputs with size/hash verification and runs actual inference with fresh recurrent state for each judgment case inside the kernel-owned worker process. Expected labels, split and variant identifiers, and label rationales are omitted from model input. Requirements, proposed patch and explicitly scoped upstream benchmark evidence remain visible. No fitting or pin changes occur. One soft 600-second dispatch window stops new assessments; an in-flight synchronous assessment may cross that cutoff. The original hard kernel worker deadline includes all earlier work and remains the execution bound, with no per-case reset. There are no nested model process groups that could escape macOS worker termination. The required recipe runs this mode after independent probe/model/reference diagnostics, so missing final acceptance cannot suppress those earlier measurements.

The private report preserves each outcome, error, unsupported input and unexecuted case, selected backend, model/runtime/corpus hashes, synchronized inference stage timings and lifetime process high-water RSS including prior cases (not isolated allocation or total GPU memory). Coverage denominators include all frozen judgment cases; errors and deadline exclusions are not model abstentions. Policy-only cases are excluded from model judgment. Related variants count once per issue for accepted-unit risk, and any false-ready variant makes that accepted issue erroneous. The one-sided 95% exact binomial upper bound is reported with its independence/exchangeability assumption; clustering, selected repositories and possible training contamination remain limitations. Positive labels mean benchmark-oracle resolution only, not independently adjudicated general PR readiness. The current 23-unit holdout cannot satisfy the frozen 5% upper-risk criterion even with zero false-ready. Consequently this mode records measurements but returns nonzero while the numerical criteria or production label authority remain unsatisfied; it is not a way to obtain green from diagnostic success. Full backbone parity and production fallback routing remain separate unverified obligations. Evaluation arithmetic/control-flow fixtures passed on `2f785c82`; these synthetic tests did not execute held-out model judgments or establish their labels.

## Preserved execution failure and released recovery

Released v0.119.0 reached native snapshot preparation on source candidate `4de86774440fbb27983cb2ffb124c0522f8d0eea` after collecting source proof, then returned `READINESS_BLOCKED: snapshot does not support index conflicts, unreviewed symlinks or submodules`. `native_proof/snapshot.rs::parse_index` explicitly supports only the reviewed Rust toolchain alias; the existing golden-vector fixture `tests/fixtures/state_commit_repo/symlink_test -> target.txt` is rejected. No supported CLI/recipe field reviews another alias. The fixture and immutable vectors must remain intact. This controller enforcement gap prevents the first native compile/probe/model execution, not just parity/calibration. A separately accepted and released controller repair is required; a candidate controller must not authorize its own acceptance. All reference/corpus modules above remain authored and unexecuted until the prescribed flow can run them.

Official v0.119.1 was verified published on 2026-10-18, installed through cargo-binstall and traced to accepted master `7371ad25`, which contains merged #1458. The earlier v0.119.0 failure above is historical evidence, not a current unsupported-controller assertion. Existing scalar reference, frozen v2 corpus, tolerances and risk criteria are preserved unchanged. Native compilation and the measured subset below subsequently executed; installation alone did not establish those results, and later changes still require exact-candidate validation.

## Explicit native dependency compatibility repair

Full official v0.119.1 validation of `aa36acc5` passed source proof and native snapshot preparation, then compiled native dependencies for 262626 ms before failing: candle-vllm `2c2c5293` unconditionally calls three DFlash top-k/grouped-convolution functions available only with CUDA in attention `5b4d645e`. No worker test, device probe or model inference executed. The preserved report is `/private/tmp/vega-1191-validation-20261018T224908Z.json`.

The reviewed upstream successor [candle-vllm b1055eca63de76b548d0d4b0daa96ff8cd1d0596](https://github.com/EricLBuehler/candle-vllm/commit/b1055eca63de76b548d0d4b0daa96ff8cd1d0596) changes only its Cargo manifest; the chosen Qwen3.5 hidden-state API and model source are unchanged. Its compatible pins are Candle `23a6f388a82a71c88067ad2b0d64ac9d00f24aac` (only CUDA workspace/error changes from b60efcf) and attention `f22e42cf27dd31efd2d1b838bd9e4cbf85bd2305` (MIT). Attention adds genuine Metal DFlash functions/build fixes, Qwen4-only kernels and MoE/GGUF repairs; this dense non-speculative profile does not activate those model modes. The inspected GDN normalization follow-up repairs a compile expression in the new Qwen4 sigmoid fallback, not a claim of changed or verified Qwen3.5 parity. Runtime source identities and lock are explicitly updated before validation; model/tokenizer/engine/calibration pins, reference equations, frozen holdout and numerical tolerances are unchanged. This is not a model update during inference, vendored stub, generation API substitution, candidate controller or new controller prerequisite. Compilation and parity under the new runtime remain unverified until executed.

## Executed results on candidate 2f785c82 (2026-10-18)

Official controller v0.119.1 completed the source preflight, snapshot and native build. All 25 worker unit tests passed, including the independent primitive calculations, tokenizer BPE, finite/tolerance guards, grouped risk arithmetic, input/label separation, private export and lock/receipt provenance checks. The real Metal tensor probe on Apple M1 Max (24 GPU cores, macOS27.0.1, 64GiB RAM) dispatched, synchronized and read back all eight exact F32 results. First recorded probe time was201.541ms (excludes device creation); a subsequent recorded probe was4.274667ms. These are capability measurements, not model latency or controlled cold/warm benchmarks.

Pinned model installation and the separate CPU model path executed. For one138-token pure-text prompt, CPU inference used F32 backbone/engine and F64 calibration, raw blocks13/19 and four sampled positions0/46/78/137. The separately implemented F64 reference computed its own tokenizer inputs, backbone activations and pooled vectors before combined engine comparison. Its calculation took25186.521ms. No tolerances were changed after observing results.

| Compared quantity | Scalar count | Maximum absolute error | Normalized RMS error |
| --- | ---: | ---: | ---: |
| Raw sampled block activations | 8192 | 4.268281e-6 | 3.191319e-6 |
| State/question/last pooled features | 6144 | 1.762856e-6 | 1.530984e-6 |
| Boolean option pooled features | 2048 | 1.184254e-6 | 1.038775e-6 |
| Memory chunk vectors | 16384 | 0 | 0 |

The33-token state did not activate memory chunks: the chunk comparison covers zero-filled unused slots only, not real chunked memory equivalence. Largest relative component errors were0.0131732(raw),0.0322156(features) and0.00481806(options); small absolute errors near zero satisfy the predeclared mixed absolute/relative and normalized-RMS criteria. Six independent tokenizer/prompt fixtures matched exact IDs/spans; one oversized input was rejected. Conditional F64 engine maximum absolute error was3.348265e-6; the engine driven by independently computed backbone features differed by at most8.429680e-6. Identical-readout calibration passed its1e-10 criterion; the combined comparison passed its0.005 probability bound and exact set/abstention/unbound/settle checks. This remains same-agent separate mathematical implementation with shared parsing/hash/weight inputs, not independent human review, empirical HF execution, full-context equivalence or production calibration.

Representative CPU probabilities were `[0.708211532643791,0.2917884673562089]` in `[false,true]` order; prediction set `[false]`, abstain=false, unbound=false. The fixed diagnostic explicitly contains a missing requirement and is not an independently labelled readiness evaluation. CPU backbone load took359.874ms, inference/pooling/readback1382.183ms, engine11.610ms, calibration0.021667ms, whole inference12540.168ms including about10597ms of installation verification/reverification. Lifetime process peak RSS was5,261,180,928bytes, including earlier diagnostic attempts and input collection; it is not isolated CPU allocation or total GPU memory.

The public Parquet was verified and decoded in Rust. Frozen v2 corpus size is3,616,291bytes with SHA256 `09df504b0d50004499172c1cebf9fbf414ed91d72a703ebd0b0a634e3a7e3e88`:20development/23holdout originals,258judgment and129separate policy cases. No calibration fitting occurred. The later holdout inference step did not run because the earlier Metal model failure stopped the linear recipe; false-ready, abstention and coverage metrics remain unmeasured. Count and label-authority limitations remain unsatisfied.

Metal model loading failed with `Not supported platform!`: upstream `MambaCache::reset_all` calls CUDA-only Candle `Tensor::zero_`, while new state allocation and per-slot initialization have real Metal implementations. The current repair consumes/discards the one-shot owned GPU model on every attempted inference, synchronizes completion/release and rejects reuse instead of suppressing failed reset or retaining uncleared state. Private model proof explicitly checks reuse rejection. A new aggregate collector attempts diagnostics and both automatic-backend and explicit-CPU holdout phases independently, then returns nonzero if any required phase fails; its control-flow regression is authored, not yet executed. Original kernel time/byte limits and frozen risk/tolerances remain. Native CPU fallback instrumentation exercises an actual unsupported alternate GPU API with the same production retry classifier, followed by fresh CPU inference; this is not a mocked failure of available Metal or proof of the kernel custody/loader-companion publication protocol. These repairs require fresh exact-candidate validation and are not established by the older measurements above.

Failed full report: `/private/tmp/vega-1191-validation-20261018T232831Z.json`, controller evidence `decapod-readiness-HUH8Fy`. Retained CPU numeric export SHA256 `a03a3818871c2df966b0cbd1e0a1684e9954d81450605fe15a744e2e2f8a9234`; separate expected reference export `fd406bce68bdb4441ea5cd97c67867e003cd9e9733abab29f45421581d14ece4`. Private receipts remain outside Git. The required native model/Metal, exercised fallback, broader numerical coverage and held-out calibration gates are not all satisfied; no feature PR is ready.

Before observing any nonempty-chunk model outputs, the collector additionally freezes one numerical fixture: the literal ASCII sentence `Required coverage item: preserve deterministic checks and report explicitly missing evidence. ` repeated exactly16times. The original138-token fixture, weights, scalar equations and tolerances are unchanged. This extra fixture is not a holdout case, readiness label or calibration fitting. It must fit the reference's512-token range without truncation and produce nonempty memory-chunk masks on both model backends; missing activation evidence fails. The collector independently attempts both numerical fixtures and the automatic/explicit-CPU holdout, then returns nonzero on any failed required phase. New chunk comparisons remain unexecuted until the prescribed validation runs them.

## Holdout provenance audit and additional public sources

The actual frozen v2 corpus contains 138 held-out judgment cases: 23 benchmark-resolved, 69 not-ready and 46 abstention labels. They derive from 23 originals in four repositories: ripgrep2, axum7, tokio9 and coreutils5. These are lineage IDs, not established independent samples. In particular, axum PRs [682](https://github.com/tokio-rs/axum/pull/682) and [1119](https://github.com/tokio-rs/axum/pull/1119) both concern trailing-slash redirects, so distinct IDs do not prove independent requirements. Four positives have no pass-to-pass tests in the public record: ripgrep2209, axum1730, axum691 and tokio4384. Their scoped fail-to-pass oracle does not establish broader regression readiness.

A static serialization audit of all 138 projected states found 2,302–18,055 bytes per state, within the 32KiB byte bound. This is not tokenizer execution or proof that all states fit the 4,096-token limit. The corpus and frozen memberships, generation rules, upstream calibration and thresholds remain unchanged. No held-out model judgment informed this audit, and no variants or related issues are promoted to independent observations. A larger denominator alone would not resolve label authority or dependence.

Additional source inspected: [ReviewBench at 12e9d1aadbe7ddf4a480c6bd4ffb6ae37c7bcb67](https://github.com/review-bench/ReviewBench/tree/12e9d1aadbe7ddf4a480c6bd4ffb6ae37c7bcb67), MIT. Its public manifest has SHA256 `e74656b8c1c4bbab5ce85f1fcb742a7aaad550a4d7bd46e1f83a0746807bfa02`; the pinned methodology has SHA256 `2b69e9f9a4bfeb7d568fccaf159dbffc3993a5dd0f941ed1b3954bbc1ac608a4`. It covers 219 PRs from187 repositories. Findings have mixed origins, including models and human comments, with subsequent classification/audit. They can anchor scoped bug-finding challenges; absence of a finding cannot label a PR ready, and a model-origin finding must not be called human-origin ground truth. This source was inspected, not substituted for the frozen holdout or executed with its external harness.

[SWE-PRBench](https://huggingface.co/datasets/foundry-ai/swe-prbench) provides human review comments for350 PRs under CC-BY-4.0. Those comments support issue-detection evaluation, not complete all-obligations readiness labels. [SWE-Review](https://arxiv.org/html/2607.06065v1#S3.SS1) uses generated patches and executable benchmark resolve status; its approve/reject formulation does not turn those labels into additional human readiness adjudications. Neither source's harness was executed, no Python ran, and neither was used for fitting. Sufficient independent readiness-domain labels and acceptable held-out model behavior remain required before a production calibration claim.

## Current source-proof dependency and reproducible continuation

Full official v0.119.1 validation of `719d901e` stopped before native compilation/execution on public GitHub reader HTTP403; its supported retry retained another403 and a later reset time. Public logs independently show [Buildkite CI2201](https://buildkite.com/decapodlabs/decapod/builds/2201#01a151b0-3125-4a49-870b-667ce6ab65da) failing before workflow parsing: mise2026.8.4 could not launch `buildkite-gha` (`No such file or directory`). Same-commit [sibling2202](https://buildkite.com/decapodlabs/decapod/builds/2202#01a151b0-3270-481b-9b34-5f13076f1f7c) launched v0.103.0 successfully. The failed log does not establish whether installation state or PATH resolution caused the launch error. Both logs were public; diagnosis needs no authentication. Neither log is substitute proof, and no repository CI step ran in the failed bootstrap.

Continue through the original claimed workspace, record a substantive source/spec checkpoint, push that ordinary branch and run plain `decapod validate` against the exact candidate and accepted target. The reviewed recipe owns locked worker tests, real automatic-backend probe and all-phase collection. Honor provider recovery/reset instructions; retain every failed report. Required source proof remains a prerequisite to the native phase under the installed controller. Do not use an empty commit, a different source profile, a manual build or imported CI artifacts to bypass it. Passing diagnostic subsets cannot authorize the feature PR while Metal model, fallback, numerical coverage or calibration acceptance is unsatisfied.
