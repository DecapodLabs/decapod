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
mapping before activation. This bootstrap change does not enable it. The PR's
direct test results are disclosed as a one-time maintainer-approved bootstrap,
not a successful full validation verdict from the previously released controller.

Validation establishes whether the agent's work has converged on the user's
declared intent and satisfies the conditions for integration. Its assurance is
bounded by the declared requirements and checks. A subsequent readiness decision
can evaluate that result; the agent publishes through the applicable gate and
the human retains merge authority. This principle adds no unimplemented guarantee.

## Optional vega post-validation assessment

Vega runs only after the complete deterministic proof profile passes. It cannot
supply proof, waive checks, alter policy or publish. The agent creates the PR
through the existing gate; the human merges. Active Jev integration is removed,
including selectable providers, transport and credential lookup. Historical
observations remain readable without governing current work.

By default `[vega]` is disabled. Required mode additionally needs a positive
singleton yes, no abstention or unbound/OOD signal, and unchanged exact validated
material, intent, proof, configuration and model/runtime pins. Negative singleton
rejects; ambiguous/missing sets, invalid numerical output and unavailable runtime
abstain. Upstream confidence and conformal settings are not established
code-readiness risk guarantees.
Supported `alpha` values are `"0.1"` and `"0.05"`; `risk` values are `"0.05"`,
`"0.1"` and `"0.2"`. These select pinned upstream calibration entries, not
readiness-domain guarantees. `[vega.installation]` binds canonical absolute
`directory` and `runtime` paths plus `manifest_digest` and `runtime_sha256`.

An already-required accepted-target vega policy cannot be disabled, repinned or
have its calibration settings changed by the candidate it governs. Validation
rejects that transition before proof execution with `VEGA_ACCEPTED_POLICY_CHANGED`;
it needs independent acceptance of the policy change. A disabled accepted target
retains ordinary behavior and may add an explicit required assessment. Setup
refresh changes the cache only and never silently rewrites publication policy.

`validation-results.v2` contains actual check outcomes, stage dispositions,
exclusions, simulated stages, blockers and unresolved trajectory facts, plus
minimal declared intent and changed paths. Identical semantic observations and
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
Unchanged backbone components are reused by content hash. Unsupported architecture,
quantization, tokenizer or adapter changes fail compatibility checks. A changed
upstream source revision is surfaced for review, never substituted as runtime code.

Setup prints disabled TOML with the verified installation path and manifest digest.
It does not pretend to install a native executable. The Rust worker is built from
`assets/tools/vega-native/Cargo.toml` on Apple-arm64. A source-install command from
a reviewed release is `cargo install --path assets/tools/vega-native --locked`.
Repository development builds/tests remain owned by ordinary governed validation.
Run the worker's `--probe` to require a genuine finite Metal numerical result.
Then supply its canonical absolute `runtime` path and `runtime_sha256` under
`[vega.installation]` and explicitly choose `[vega] mode = "required"`.
Runtime digests use `sha256:<64 lowercase hex>`.

Runtime never follows a floating active pointer, downloads artifacts or selects a
CPU backbone fallback. The pinned Candle path captures raw blocks 13/19 and pools
F32 state/question/option/chunk features on Metal. The verified BF16 backbone is
upcast to F32, increasing weight memory accordingly. Base dynamics run in Rust F32
on CPU; calibration uses F64 probabilities. This profile has adapters off, no image
path and fresh recurrent state for every request.

### Honest native evidence

The private bounded `--proof-output` diagnostic contains exact tokens/spans,
numeric pooled vectors, engine readouts, calibrated outcome, hashes and measured
stage durations. It explicitly retains parity=unverified. Numeric diagnostics
allow comparison; they are not independent reference evidence or readiness.
Real Apple compilation/checkpoint loading/Metal dispatch, independently authenticated
pinned-upstream numerical comparison, domain calibration and latency/peak-memory
measurements remain required. Exercise repeated requests, error cleanup and the
4,096/4,097-token boundary. No arbitrary latency threshold or synthetic native pass
is supplied. An unavailable Apple executor or reference corpus is an explicit
acceptance blocker, not a reason to weaken ordinary publication gates.
