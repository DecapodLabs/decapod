## Repository-local readiness

Consumer repositories may select `proof.required_profile.id = "repo.pr-readiness.v1"`.
This supported profile proves the repository's reviewed local build/test commands,
not execution of GitHub Actions, Buildkite, release packaging, deployment, or an
unavailable platform matrix. Decapod's `decapod.pr-readiness.v1` profile retains its
complete finite source-CI contract. A candidate cannot replace an accepted Decapod
strict profile with repository-local proof. Unknown profile IDs still fail closed.

The repository profile requires `validation_buildkite_preflights = false`, a
nonempty set of unique `proof.commands` with `required = true`, and an explicit
`[proof.repository_contract]`:

- `reviewed_commands_digest`: review pin over commands, coverage and execution declarations
- `source_coverage`: one `{ source, commands }` row for every discovered workflow,
  with a nonempty list of configured required command names; no duplicate,
  unknown, omitted, or excluded workflow mappings
- `executions`: a table keyed by each required command name, containing `program`
  (absolute canonical installed executable), `toolchain_files` (nonempty map of
  labels to absolute canonical tool/runtime files), `environment` (explicit map),
  `directory` (repository-relative, default `.`), and `timeout_seconds` (1–7200)

`proof.required_profile.reviewed_sources_digest` separately pins the full
workflow/action/helper/build inventory. `init` and `rpc --op agent.init` report
`inventory_digest` and `command_contract_digest` as read-only review aids. Inspect
all inputs and the intended coverage before explicitly entering either digest.
Neither initialization nor a matching digest executes proof or approves itself.
A changed workflow, command, arguments, requiredness, tool path, environment or
coverage mapping needs renewed review. Init reruns preserve both declarations.

`proof.commands[].command` names the declared executable (or uses its absolute
path); `args` remain separate literal arguments. Runtime resolves no YAML, shell
text, auto-installed tools, or ambient environment. The installed executable and
all declared toolchain files are byte-measured before and after execution. Their
transitive completeness and semantic coverage are repository-authored review
assertions, not facts inferred by Decapod. Use pinned installations and declare the
complete runtime closure. An explicit PATH must contain only declared executables;
prefer direct executable paths. The default HOME and temporary files use owned
scratch, with no caller credentials inherited. Direct shells, privilege/container
wrappers, recursive validators, daemon/network/external-write operations and
publication/deployment are unsupported. This adapter is not an OS sandbox.

Real Python, Node and Make executables may run reviewed repository code through
this local profile. Source is copied to a detached, immutable execution snapshot;
write generated output to the provided temporary scratch instead of the source
checkout. Any required command failure, timeout, output overflow, source mutation,
tool change, stale accepted target or incomplete evidence blocks readiness. Native
proof reports `scope = "repository_local_readiness"`; it makes no hosted-CI or
release assurance claim. A provider job that itself invokes `decapod validate`
still runs CI-local prerequisites, never recursively proves full publication.

For Nix projects, installing this profile is not evidence that daemon-backed
builds, remote builders or deployment ran. A supported local contract can use
preinstalled pinned formatters/test tools directly; unavailable build/deployment
proof remains a limitation to reconcile, not a successful or skipped check.

Repository proof uses a sealed, argument-aware authorization path minted only
for the validated local contract and exact measured invocation. It does not add
Python, Make or other programs to the ordinary `proof_exec` default allowlist.
An authored `.decapod/EXTERNAL_ACTIONS.json` remains authoritative: an explicit
`proof_exec` or `repository_proof_exec` deny blocks the command, malformed policy
fails closed, and a present policy must explicitly allow proof execution. The
kernel never rewrites that policy to make a check pass.

## Supplemental Rust workers

A release containing `proof.rust_workers` supports required native Rust worker
verification after the full strict Decapod source profile passes. It does not
replace source CI, select the repository-local profile, install a controller or
authorize publication from a worker response. This source extension must first
be independently accepted and released before another feature uses it.

```toml
[proof.rust_workers]
reviewed_recipes_digest = "sha256:<reviewed recipe digest>"

[[proof.rust_workers.workers]]
name = "native-worker"
manifest = "assets/tools/native-worker/Cargo.toml"
features = []
metal_toolchain = true
timeout_seconds = 900
runs = [{ arguments = ["--probe"], proof_output = false }]
```

The manifest must be committed, declare a separate `[workspace]` and an explicit
`package.rust-version`, and have a committed sibling Cargo.lock. The controller
selects the installed Rust toolchain, runs locked release tests and each configured
run with literal arguments after Cargo's `--`, in an immutable source snapshot.
Shared recipes contain no installed tool paths. `agent.init` reports
`rust_worker_recipes_digest` as a read-only review aid; inspect the recipe before
explicitly updating its pin. Init preserves authored recipes. Accepted recipes
cannot be removed or changed by candidate policy; new recipes add required checks.

Workers have unique names. One manifest can have distinct feature-set variants
(for example CPU and CUDA); duplicate variants, including reordered features, fail.
Each variant uses its own target directory. Limits are four workers, eight runs per worker, sixteen unique feature names,
64 arguments/16 KiB per run, 4 KiB per argument and 1–1800 seconds per test/run.
The original validation deadline bounds the entire operation. Tool discovery does
not install Rust or copy an authenticated Cargo home. Optional Metal inventory on
macOS measures selected tools and macOS/iOS SDK metadata; it does not prove GPU
dispatch. This remains the existing reviewed-source execution model, without an
OS sandbox against malicious build scripts or attestation of every OS dependency.

`proof_output = true` appends a fresh private `--proof-output` destination. The
worker must write a nonempty JSON object of at most 1 MiB using private regular
file permissions and a single link. Missing, unsafe or malformed diagnostics
fail the phase. Private command output, timing, termination, failure observations
and diagnostic hashes remain under the validation evidence directory. Their
`publication_ready=false` scope cannot replace the outer complete validation
result. Source, policy, tools and a fresh accepted target are rechecked after the
phase. Numeric/model claims still need independent reference and calibration
evaluation evidence; command success alone establishes neither.

## Optional CI-attested validation policy

`proof.ci_attested` selects provider-attested validation instead of execution
archive collection. It is absent by default. Its schema is
`decapod.ci-attested-validation.v1`. The exact policy and Buildkite enablement
must already be accepted on the target: a candidate cannot enable or change its
own acceptance policy. Each workflow lists all required native step keys,
explicit auxiliary step keys, and mappings from the independently compiled
required check catalog to native steps. Missing, unknown, duplicate, retried,
skipped, soft-failed or unsuccessful jobs fail closed.

The adapter verifies provider-owned HTTPS metadata for the configured repository,
pipeline, workflow, branch and exact commit. All declared jobs must terminate
successfully. The resulting observation binds source, accepted base, intent,
configuration, required checks and original metadata digests. This explicitly
trusts the configured CI executor to implement the declared checks. It does not
claim measured remote binary, toolchain, log or output hashes. A successful
CI-local validation job alone does not satisfy the complete required profile.

This optional controller capability requires a separately reviewed, accepted job
mapping before activation. Its historical rollout exception does not authorize
later changes. New prerequisites and features must pass normal full validation
using an independently supported released controller before publication.

Validation establishes whether the agent's work has converged on the user's
declared intent and satisfies the conditions for integration. Its assurance is
bounded by the declared requirements and checks. A subsequent readiness decision
can evaluate that result; the agent publishes through the applicable gate and
the human retains merge authority. This principle adds no unimplemented guarantee.

## Optional vega post-validation assessment

Vega runs only after the complete deterministic proof profile passes. Failed, missing or stale required validation returns before model inference and blocks PR readiness directly. Vega cannot supply proof, waive checks, alter policy or publish. The agent creates the PR through the existing gate; the human merges. Active Jev integration is removed; historical observations remain readable without governing work. Selecting Vega during init does not add this repository's scientific model-verification corpus or Metal development recipe to other projects.

`init --vega-decisions[=true|false]` and `init with --vega-decisions[=true|false]`
select required or disabled Vega assessment. Interactive init offers the same
choice. Refresh preserves the existing choice and installation pins unless
explicitly changed. Selection installs no model and executes no inference.

Init refresh retires only a recognized `[decision].provider = "none" | "jev"`
selector, removes an empty retired section, and preserves unrelated authored
settings, including extension keys in that section. Unknown or malformed provider
selectors stop refresh without rewriting the configuration. Dry-run remains read-only.

By default `[vega]` is disabled. Required mode additionally needs a positive
singleton yes, no abstention or unbound/OOD signal, and unchanged exact validated
material, intent, proof, configuration and model/runtime pins. Negative singleton
rejects; ambiguous/missing sets, invalid numerical output and unavailable runtime
abstain. Upstream confidence and conformal settings are not established
code-readiness risk guarantees.
The bounded `validation-results.v3` input retains declared proof hooks,
unresolved and deferred questions, contradictions, stop conditions and phase
gates alongside intent and actual check observations. Gate satisfaction flags
remain plan declarations, not independent execution proof. Oversized inputs
are refused without truncating obligations; administrative metadata and raw
logs are not supplied. The final custody check rejects changed governance.
Supported `alpha` values are `"0.1"` and `"0.05"`; `risk` values are `"0.05"`,
`"0.1"` and `"0.2"`. These select pinned upstream calibration entries, not
readiness-domain guarantees. `[vega].model_manifest_digest` is the shared immutable
compound model pin. `setup vega --runtime <installed-worker>` explicitly registers
each maintainer's canonical runtime path and executable hash in private local cache
metadata. Registration does not build or execute the worker or certify its source.
The team configuration contains no maintainer-specific path. Legacy explicit
`[vega.installation]` directory/runtime/manifest/hash bindings remain supported;
if both forms are present their model identities must agree.

An already-required accepted-target vega policy cannot be disabled, repinned or
have its calibration settings changed by the candidate it governs. Validation
rejects that transition before proof execution with `VEGA_ACCEPTED_POLICY_CHANGED`;
it needs independent acceptance of the policy change. A disabled accepted target
retains ordinary behavior and may add an explicit required assessment. Setup
refresh changes the cache only and never silently rewrites publication policy.

`validation-results.v3` contains actual check outcomes, stage dispositions,
exclusions, simulated stages, blockers and unresolved trajectory facts, plus
declared intent, explicit plan obligations and changed paths. Identical semantic observations and
stage definitions are indexed once. Every check keeps its identity and outcome;
missing checks remain explicit. Full custody identities bind the complete report
outside the model prompt. No whole plan, source diff or raw log enters the prompt;
unrepresented warnings and untested intent remain unknown. The 32 KiB projection
and 4,096 total-token limits fail closed rather than clipping evidence.

### Explicit installation

`decapod setup vega` installs verified external model data in a private cache.
`--check-updates` checks bounded official metadata, `--refresh` explicitly selects
a compatible new revision, `--offline` verifies cached files without networking,
and `--rollback <manifest-digest>` atomically activates a verified prior install.
Use `--cache-directory` to select another private cache. Engine/config/calibration,
backbone, tokenizer and source/license identities have independent immutable pins.
Unchanged backbone components are reused by content hash without another download.
Each immutable installation receives a separately verified private copy, requiring
additional disk space while retaining the single-link storage safety contract.
Incomplete or cancelled copies never activate an installation. Unsupported architecture,
quantization, tokenizer or adapter changes fail compatibility checks. A changed
upstream source revision is surfaced for review, never substituted as runtime code.

Setup prints disabled portable TOML with the verified compound model digest.
It does not pretend to install a native executable. The Rust worker is built from
`assets/tools/vega-native/Cargo.toml`. A source-install command from
a reviewed release is `cargo install --path assets/tools/vega-native --locked`.
Repository development builds/tests remain owned by ordinary governed validation.
Run the worker's `--probe` to require a genuine finite Metal numerical result.
Register the installed worker with `decapod setup vega --offline --runtime <canonical-worker-path>`.
Apply the shared `model_manifest_digest` from setup and explicitly choose
`[vega] mode = "required"`, or select `--vega-decisions` during init. Each maintainer
performs local setup; validation freezes that binding before proof and rechecks it
before inference and publication. Missing, replaced or malformed local metadata
cannot authorize readiness. Runtime bytes and model files are reverified locally.
Runtime digests use `sha256:<64 lowercase hex>`.

Runtime never follows a floating active pointer, downloads artifacts or selects a
different model during fallback. `[vega] backend = "auto"` (the default) prefers
Metal on macOS and CUDA elsewhere; `metal`, `cuda` and `cpu` explicitly select a
backend. `cpu_fallback` defaults true. A CUDA-enabled non-Apple worker requires
its `cuda` Cargo feature and NVIDIA toolchain; ordinary CPU builds need no CUDA
toolkit. AMD/Intel GPU acceleration is not implemented. These source paths still
require execution evidence before portability can be claimed.

A CUDA-linked executable may fail in the OS loader before it can select CPU.
`setup vega --runtime <gpu-worker> --cpu-runtime <cpu-only-worker>` verifies and
atomically registers both executable pins in private local metadata. The CPU
companion uses the same reviewed Rust source with CUDA disabled. Neither path
belongs in shared project policy; no runtime is built or executed during setup.

After deterministic proof passes, a bounded `--backend-check <metal|cuda>` process
checks GPU startup and real model-free arithmetic without receiving model paths,
validation state or an inference request. A typed hardware-unavailable result may
select CPU. A verified executable reporting an absent ELF interpreter or empty
loader exit 127 may use only an explicitly pinned CPU companion. Every executable
pin and the model binding are rechecked before fallback; the overall deadline
and model/calibration identities stay fixed. Missing companions fail closed.

Malformed or contradictory capability reports, crashes, timeouts and numerical
errors fail closed. Once inference starts, only typed hardware unavailability
without any tokens or outcome permits a fresh CPU attempt. Negative decisions,
abstention and malformed model results never trigger a retry to seek acceptance.
Set `cpu_fallback = false` to require the selected accelerator. Receipts distinguish
capability checks from inference and disclose each runtime digest and actual final
backend. CPU success never proves Metal ran. These paths need actual native
execution evidence; authored tests and source inspection do not establish it.

The pinned Candle GPU path captures raw blocks 13/19 and pools
F32 state/question/option/chunk features. The separate Rust CPU path uses the same
checkpoint and pooling with CPU attention/recurrence, without a generation API.
It is not an independently reviewed parity reference. The verified BF16 backbone is
upcast to F32, increasing weight memory accordingly. Base dynamics run in Rust F32
on CPU; calibration uses F64 probabilities. This profile has adapters off, no image
path and fresh recurrent state for every request.

### Honest native evidence

The current pinned base profile is not qualified for code-readiness judgment.
The [contract diagnosis](../vega-readiness-decision.md) separates the completed
native/numerical verification from the exposed benchmark's different input path,
wrong-stage cases and visible semantic misses. Its 23 proxy-label conflicts are
not a production false-ready rate. The original acceptance remains unsatisfied;
no activation guard or advisory downgrade has been silently applied. A proposed
inactive delivery stage requires a maintainer scope decision before code or
proof-contract changes.

The private bounded `--proof-output` diagnostic contains exact tokens/spans, pooled vectors, engine readouts, calibrated outcomes, hashes and synchronized stage durations. Ordinary responses retain `parity=unverified`. Candidate `30015be0` executed 43 native tests, real M1 Max recurrence and three fixed 138/314/4,096-token Metal/CPU model comparisons against separately implemented pinned Rust calculations. Exact token-limit rejection and all 16 memory slots were exercised. This is bounded fixture evidence with shared infrastructure/authorship, not all-input or independently reviewed equivalence. CPU fallback was exercised through actual unsupported CUDA and fresh CPU inference; Linux loader-companion routing remains separate. All six frozen holdout groups completed, but the final acceptance check failed risk/coverage with 23 false-ready proxy outcomes per backend. See `docs/vega-reference-contract.md` for revisions, tolerances, sample counts, timings, lifetime memory and preserved failures. Collection-only success never removes the required final scientific aggregate, risk/coverage/independent-label conditions or kernel validation gates.
