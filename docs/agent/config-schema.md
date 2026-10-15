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
    #[serde(default)]
    pub decision: DecisionConfig,
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

## Native proof bootstrap and collection

The Decapod source profile recognizes finite, independently compiled workflow
contracts. `assets/tools/source_ci/contracts/ci-native.yml.fixture` is a reviewed
future CI definition, not an active workflow. The current `.github/workflows/ci.yml`
keeps its complete existing graph. Only the exact future bytes add the four
`native-vega-*` obligations; changing a source-inventory pin cannot admit other
workflow bytes or a different accepted executor. Prerequisite controller changes
must first pass the existing full suite, be human-accepted, and ship through an
official controller release before a feature activates that future definition.

`decapod validate --collect-proof` is an explicit collection-only mode for the
independently accepted native source graph. It preserves ownership, prompt,
authorization, exact source/base, provider-custody and timeout checks, runs the
configured graph, and always reports `publication_ready = false`. It does not
write an ordinary ready verdict. Failed publication-governance checks remain
failed; successful native-job evidence may still be collected from a terminal
failed pipeline. Canceled, denied, stale or unauthenticated execution remains a
blocker. Normal validate and publish continue to enforce every publication gate.

Untyped trajectory `blockers` always halt collection too. Explicit `proof_blockers`
identify the finite `native-apple-metal` obligation and retain detail/evidence
references. The supported trajectory record operations can add a typed proof
blocker, deliberately classify an exact existing blocker, or resolve it after
recording all four checks and evidence. These are journal declarations, not proof.
A later ordinary full validation independently reruns and authenticates the whole
required graph before any PR can be created. Collection never changes or clears
a blocker on the caller's behalf.

The native recipe separately proves real GPU shader execution, compiles/tests the
Rust optional worker through its separate locked manifest, explicitly stages the
pinned Vega and Qwen components, and executes the actual Candle/Metal backbone
and pooling followed by the CPU base engine. Backbone/engine precision is f32;
thermal calibration uses f64. Both macosx and iphoneos SDK tools are required by
the pinned Metal dependency build. OS/build, selected SDK settings and compiler
hashes, worker/lock/kernel identities, nine component file hashes and the exact
model/backbone/source/calibration manifest are bound to small custody receipts.
This is bounded SDK/runtime provenance, not a complete hash of every system
framework or library. Bulk model weights are never uploaded as proof artifacts.

A successful native job may be retained from a failed pipeline, but partial checks
inside a failed native job are not accepted separately. A runner label, Linux
fixture, finite probability, or passing hardware smoke test cannot establish
upstream numerical parity or readiness-domain calibration. Those remain
unverified; the base-only profile rejects adapters. The prerequisite itself does
not activate this job or claim any native execution occurred.
