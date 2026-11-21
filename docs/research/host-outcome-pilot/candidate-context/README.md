# Candidate context follow-up: partial, negative outcome evidence

These two additional predeclared pairs exercise the proposed context code with
real Codex processes. They supplement the released-kernel pilot in the parent
directory. They do not replace its failed attempts or establish three-host
acceptance, independently observed provider delivery, or a performance gain.

| Workflow | Arm | Process elapsed seconds | Reported input / cached input tokens | Independent greeting check | Current governed completion |
| --- | --- | ---: | ---: | --- | --- |
| Fresh ephemeral host, precompiled toolchain | Baseline | 29.030538 | 145,345 / 119,424 | Passed | Not applicable |
| Fresh ephemeral host, precompiled toolchain | Governed | 486.989264625 | 4,802,036 / 4,711,552 | Passed | Unproven |
| Warm project/compiler cache, fresh host conversation | Baseline | 33.489869125 | 173,000 / 145,280 | Passed | Not applicable |
| Warm project/compiler cache, fresh host conversation | Governed | 77.486072667 | 491,533 / 448,640 | Passed | Blocked |

Elapsed values use the capture runner's monotonic process clock. The fixed
independent checker is the parent fixture's `independent-check.rs`,
SHA256 `e1bc52f6d5e254455909ddde8ec0b41330a7c04b155afe004457a86cd4127810`.
Every resulting `src/lib.rs` has SHA256
`b839a9bf96a238f0e96c595bd0fafba22152cb16beee94428a75d75183f0a9f6`.
That hash covers this one source file, not the entire repository or its proof.
Candidate commits, dirty states and task-source identities are recorded separately.

## Treatment, identities and limits

Each pair was declared before its first inference, baseline first and governed
second. The fresh protocol hash is
`4205afbe96a0c9d0c02083a7a08a049dfe149fe9ddff6b0d82301bdf1fbaea21`;
the separate warm protocol hash is
`565b84300f38eeb20a8bac29d53c1754838bec3241e6019dee355245f3620189`.
Both use Codex0.169.0, model alias `gpt-6.1-sol`, ephemeral conversations and the
same natural request: “Continue the fixture greeting update and finish its
required verification.” No Decapod command sequence was supplied in that prompt.
The executable host SHA256 is
`739cda86e15d30a6e2b40cf8dcbbfb988aaf48b305b27065573bb8e9fd41d002`.
The model alias does not pin provider weights or authenticate request delivery.

The frozen, privately built candidate kernel SHA256 is
`1f86d562b4a82b9be6bc85061f342366db59398c874b0cb44ad1f5736ec6c329`,
operator-associated with source `f68666f74fe2a49e30b84d350e1daa5d9542ff37`.
It is an **unreleased candidate**, despite printing the package version0.148.10;
it is not the independently released controller used for production validation.
Host and kernel file hashes were checked unchanged before and after every run.
The captured environment digest describes a declared subset; additional ambient
environment and provider internals were not captured. Compiler caches were
prepopulated before the fresh runs. Neither pair is a randomized cold-cache study.

The baseline has no repository agent file. The governed treatment includes native
entrypoints, knowledge, task custody, living specs and a deliberately local
fixture proof contract accepted into the fixture before inference. This does not
alter Decapod's production Buildkite profile. Task-source files match within each
pair; governance inputs, source commits and custody intentionally differ. The
operator selected fixture requirement A; that setup is not authenticated human
approval and must not be promoted to policy by its labels.

Only two arms were run. Developer-written and generated static guidance remain
unrun. Generation overhead, complete setup time, active human attention, semantic
question review, model-call count and monetary cost are unavailable. The records
retain nulls; cached tokens are reported counters, not inferred dollar savings.
Raw captures and machine-owned state remain private. Digest references and
producer labels do not make those captures publicly replayable or authenticated.

## Observed behavior and failures

A separate preparer used native knowledge writes to supersede B with A. The
historical lower-trust file repeats B20 times. In the fresh governed run,
`context.resolve` returned only active A, 222 knowledge text bytes, before the
source edit. The knowledge source hash is
`6c869db59b9ecd2b2abef4586c278165c9898822f363f24311314c7fb57738f6`.
The result explicitly labels it advisory and recorded-source-unverified. An
explicit audit query still returned superseded B. The host also read the
historical file directly: this is no claim of removing existing provider tokens.
Operational return of a context packet does not establish provider-correlated
delivery or consumption. Both hosts actually executed a failing Rust test before
repairing the greeting; the independent check afterward passed.

The governed host's full local validation produced historical receipts for
`8e4431fbb93de935390b1d21da1dcd5b49d48f02`, and `todo done --validated`
returned success. A separate completion-record lookup found no record. Independent
verification under the released controller refused a changed validation epoch;
under the exact candidate it refused current observer custody. These are separate
evidence classes, not a current independently admitted completion.

The warm baseline investigated a stale executable in its shared Cargo target and
passed using a fresh target. The warm governed host inspected the existing task,
preserved historical proof, encountered `AUTHORING_CUSTODY_STALE`, and was refused
historical completion because the candidate was absent from accepted history.
The last write also returned an UNKNOWN broker outcome; its identity and captures
are retained privately and the ambiguous write was not replayed. The host's final
report explicitly declined to claim verification complete.

These observations show higher governed time and reported tokens without an
independently admitted completion benefit in this tiny fixture. They justify no
causal, statistical or general efficiency claim. The renderer correctly leaves
the governed rows partial and human-attention ratios unavailable. Its summary's
`reported_independent_correct` counts completed real-host rows; the per-run
functional field retains the passing independent checks even for partial rows.

Re-render each input through `decapod trace flight-recorder compare --input FILE`
inside a normally initialized project. The comparison is a strict local renderer;
it does not authenticate observations, execute their checks or authorize merge.
