# Configuration Schema (Auto-generated)

```rust
pub struct DecapodProjectConfig {
    pub schema_version: String,
    pub init: InitConfigSection,
    pub repo: RepoContext,
    #[serde(default)]
    pub governance: GovernanceConfig,
    #[serde(default)]
    pub proof: crate::core::proof::ProjectProofConfig,
    #[serde(default)]
    pub custody: CustodyConfig,
    #[serde(default)]
    pub tracker: TrackerConfig,
    #[serde(default)]
    pub context: DeclaredContextConfig,
    #[serde(
        default,
        skip_serializing_if = "crate::core::vega_readiness::Configuration::is_default"
    )]
    /// Preserved legacy policy; never silently reinterpreted as Laya or none.
    pub vega: crate::core::vega_readiness::Configuration,
    #[serde(
        default,
        skip_serializing_if = "crate::core::laya_readiness::Configuration::is_default"
    )]
    pub laya: crate::core::laya_readiness::Configuration,
}
```

## Required proof and execution provider

`[proof].validation_buildkite_preflights` is a boolean, default false. False runs
the native readiness backend; it does not disable compile, test or CI proof.
True pushes or reuses the exact current Decapod workspace branch commit, waits
for its real required Buildkite checks, and downloads verified execution evidence.
The eventual PR uses that same branch; validation creates no temporary branches. It does not run or collect GitHub Actions; normal
PR workflows on that service remain independent.
`[proof.required_profile]` selects the reviewed repository contract. The Decapod
profile binds its finite source-CI contract; `repo.pr-readiness.v1` binds the
explicit local-proof contract below without hosted-CI or release assurance.
A missing, unknown or changed contract fails readiness; an empty
`commands` list cannot prove completion. Init and initial agent execution
reconcile existing repository contracts, preserve user-authored choices, and
report missing or conflicting obligations for repair. Successful init is not
successful validation. Existing repositories must reconcile their contract and
provide the required capabilities before validate can authorize publication.
A declaration matching the current inventory does not override the controller's
compiled workflow/helper contract. For supported remote source-CI contracts, validation uses the accepted target’s independently verified executor to build and test the candidate, even when candidate helper bytes change. The controller keeps the required graph and final verdict; candidate output cannot authorize itself. `CONTROLLER_CONTRACT_UNSUPPORTED` identifies an unsupported carrier, protocol or accepted executor. Rerunning the same controller, repinning configuration or running a manual build cannot resolve that blocker; use an independently supported controller for this exact contract, and preserve the blocker if none is available. No automatic controller replacement is performed.

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

## Optional Laya post-validation assessment

This section describes the current candidate implementation. The Laya migration
has not passed functional qualification or final publication validation. Three
pinned profiles failed corrected development controls; a bounded final-affine
fit also failed before its frozen evaluation could run. Do not treat this
candidate guide as a released, qualified model profile. See the measured
[qualification outcome](../laya-readiness-migration.md#qualification-outcome-on-2026-10-25).

Laya runs after complete mandatory deterministic validation passes. Failed,
missing or stale required proof blocks readiness before model inference. Laya
cannot supply proof, waive checks, change policy, orchestrate the agent or
publish. The agent creates an authorized PR through the resulting gates; the
human reviews and merges.

`init --laya-decisions[=true|false]` and
`init with --laya-decisions[=true|false]` select required or disabled assessment.
Interactive init offers the same choice. Refresh preserves selection, local
pins and unrelated authored settings unless explicitly changed. Selection
installs no model and executes no inference. By default Laya is disabled.

```toml
[laya]
mode = "required"
backend = "auto"
cpu_fallback = true
model_manifest_digest = "sha256:<digest printed by setup laya>"
```

The team shares an immutable model manifest digest. Each maintainer independently
installs and verifies that profile and registers a native worker in private local
metadata. Team configuration contains no machine-specific path or credential.
Optional explicit `[laya.installation]` bindings must agree with the shared pin.
`setup laya --runtime <canonical-native-worker>` performs registration; source
installation uses `cargo install --path assets/tools/laya-native --locked` from
a reviewed release. Weights are external data and are not in the published
Decapod crate. No hosted Jev endpoint, API token or Python runtime is involved.

`setup laya --offline` verifies and reuses cached artifacts without networking.
An initial installation downloads only pinned public files, verifies sizes and
SHA-256, copies cache hits into separately verified single-link artifacts and
atomically publishes a complete immutable installation. Unsafe hardlinks remain
rejected. A failed or interrupted installation leaves earlier complete versions
and runtime bindings intact. `--cache-directory` chooses a private cache location;
validation never follows an active pointer or silently updates an asset.
`--check-updates` reports current immutable upstream identities without activation.
Only the identical reviewed revisions are compatible automatically; changed
source/model/conversion revisions need explicit review and a new qualified pin.
This metadata check is not weight, numerical or readiness-accuracy verification.

`backend = "auto"` retains Metal preference on macOS and CUDA preference on
other hosts. `"cpu"`, `"metal"` and `"cuda"` select explicit execution. The worker
reports the actual backend. CPU uses the pinned maintained Laya Rust SDK and
ONNX Runtime; the separate Candle path runs the original checkpoint on supported
Metal/CUDA devices. A model-free GPU probe dispatches arithmetic, synchronizes
and checks numerical readback. It cannot authorize readiness.

With `cpu_fallback = true`, a typed hardware/unsupported-backend result carrying
no model answer permits a fresh bound CPU process under the original deadline.
A GPU loader failure requires a separately registered CPU companion:
`setup laya --runtime <gpu-worker> --cpu-runtime <cpu-only-worker>`.
Negative answers, abstention, malformed IPC, arithmetic failures, crashes and
timeouts never authorize retrying for a better answer. Failed required inference
blocks readiness. NVIDIA hardware execution must be reported separately from
an exercised unsupported-CUDA-to-CPU fallback.

### Judgment and evidence boundaries

The production `validation-results.v3` projection preserves intent, declared
proof hooks, unresolved/deferred questions, contradictions, phase gates, stop
conditions and actual validation observations. Gate satisfaction declarations
are not execution proof. Repeated check facts are indexed once; every outcome
and adverse fact remains represented. Source diffs and raw logs are omitted;
unrepresented warnings and untested intent remain unknown.

The entire serialized state, fixed question and false/true alternatives must
fit the selected checkpoint. The reviewed multilingual profile permits 1,024
total tokens and a 32 KiB state-byte bound. Oversize inputs, injected marker
tokens or SDK truncation of any state/question/option are refused. The
most-confident-window API is not used to discard obligations.

A required decision needs a valid affirmative binary result and the trained
`act` disposition. Rejection, escalation/abstention, invalid probabilities,
malformed output and execution errors block it. Interpretation accounts for the
SDK's four-decimal probability resolution; a rounding interval touching a
boundary abstains. This follows the upstream decision contract, not a fitted
readiness-domain risk bound. The receipt binds the exact material, intent,
governance, evidence, policy, rubric, model manifest and runtime executable.
Relevant changes invalidate it. The controller independently rechecks proof
and accepted target after inference; a worker cannot declare publication ready.

An already-required accepted policy cannot be disabled or repinned by the
candidate it governs. Legacy required `[vega]` configuration produces explicit
migration guidance; it is never silently translated into Laya or disabled.
Historical Vega observations and scientific recipes remain readable and
mandatory where already accepted. Jev selector retirement still removes only
recognized `none`/`jev` keys, preserving unrelated settings and rejecting unknown
or malformed selectors before rewrite.

### What verification establishes

Real model inference, exact token/marker comparisons and raw-logit comparisons
against published pinned upstream outputs establish bounded numerical behavior.
Backend enumeration and compilation alone do not. Receipts distinguish model
load, synchronized inference, integrity hashing, wall time and the process RSS
high-water mark; host RSS is not isolated GPU allocation. Complete validation
latency includes Buildkite and other required proof, independently of inference.

Functional qualification must demonstrate useful ready-work approval and safe
handling of unmet obligations, uncertainty, stale/failed proof and malformed
responses through the production path on actual supported backends. A ready
control may safely abstain, but that assessed work remains blocked and its
expected-accept mismatch remains visible. Always accepting or always abstaining
is not working qualification. Deterministic policy rejection does not establish
model understanding.

Readiness-domain reliability and production-calibrated accuracy remain
**unestablished**. The 2026-10-20 maintainer revision replaced the former
59-acceptance/5%-risk/95%-confidence/50%-coverage research prerequisite with
functional integration acceptance. Original studies, failed results, numerical
tolerances and safe-abstention mismatches remain historical evidence, not a
passing old criterion. See [Laya migration and provenance](../laya-readiness-migration.md)
and [the historical Vega reference contract](../vega-reference-contract.md).
