# Configuration Schema (Auto-generated)

```rust
pub struct DecapodProjectConfig {
    #[serde(deserialize_with = "deserialize_config_schema_version")]
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
    #[serde(default, skip_serializing_if = "WorkspaceConfigSection::is_default")]
    pub workspace: WorkspaceConfigSection,
    /// Optional top-level shorthand for `[governance.db_maintenance]` (#1568).
    #[serde(
        default,
        skip_serializing_if = "crate::core::db_maintenance::DbMaintenanceConfig::is_default"
    )]
    pub maintenance: crate::core::db_maintenance::DbMaintenanceConfig,
    /// Optional top-level `[agent_maintenance]` shorthand for `[governance.db_maintenance]` (#1568).
    #[serde(
        default,
        skip_serializing_if = "crate::core::db_maintenance::DbMaintenanceConfig::is_default"
    )]
    pub agent_maintenance: crate::core::db_maintenance::DbMaintenanceConfig,
    #[serde(
        default,
        skip_serializing_if = "crate::core::laya_readiness::Configuration::is_default"
    )]
    pub laya: crate::core::laya_readiness::Configuration,
}
```

## Invalid configuration and repair (#1771)

`schema_version` must be `"1.0.0"`. Invalid TOML, known field types/enums,
unsupported versions and unknown fields in protected sections reject ordinary
mutation with `INVALID_CONFIG_SCHEMA`. The diagnostic identifies the offending
input and points to `decapod docs show config-schema --source embedded`.
Read-only `data schema` and RPC `schema.get` remain usable before config,
sessions, migrations or workspace preparation. Correct the candidate config
and retry the ordinary operation; schema validity does not grant authority.
Unknown extension sections retain their existing compatibility; this is not
a universal unknown-key prohibition.

## RBAC exceptions and Laya

`[governance.rbac]` declares `actor_roles`, `journal_repair_roles`,
`legacy_todo_reconcile_roles` and `validation_bypass_roles`, all empty/denied by
default. The main
repository's existing branch-safe resolver supplies the bounded policy read.
An unaccepted main-root feature checkout cannot grant exceptions, shared
claims or remove Git-backed approval requirements. Its base selector cannot
declare that same feature ref protected. Protected main/mainline policy and
independently selected accepted refs remain cooperative host-local authority;
Git administrator access and mounted host policy require their external host
boundary. Journal
repair requires an explicitly granted role and records the policy fingerprint
with the reviewed repairs. `validate --bypass-validation --bypass-reason`
requires its own role and leaves proof and publication readiness false.
These actor labels are cooperative identities, not authenticated users.

`[governance.db_maintenance]` is default-deny (`enabled = false`). When the
canonical main-repository `.decapod/config.toml` sets `enabled = true` on a
`local` backend, authenticated agents inside their claimed `.decapod/workspaces/*`
worktree may invoke the allowlisted safe capability slice (`db.inspect`,
`db.verify`, `db.lock.diagnose`, `db.lock.retry`, `db.lock.release_own`,
`db.backup` under `.decapod/backups/`). Worktree or unmerged-branch config edits
cannot self-elevate, `db.recover` remains reserved for repository `owner` repair
authority (`#1590`), `backend = "cloud"` fails closed (`CLOUD_PRINCIPAL_UNVERIFIED`)
until Decapod Cloud verifies a repository-privileged human principal, raw SQL /
`sqlite3` / vacuum / restore remain unmodeled, and every attempt writes a
redacted NDJSON audit entry (`implies_validation_ready = false`).

`[laya] mode = "disabled"` is the default. Set `mode = "required"`
and install the reviewed profile to opt in to post-proof Laya judgment.
Required accepted policy cannot be silently disabled.

## Direct main-merge policy (#1468)

`[governance].main_merge` defaults to `false` (fail-closed) and is omitted when
false. A human maintainer may explicitly opt in to direct merge to main after
full validation passes by setting `main_merge = true` in the accepted base
repository's `.decapod/config.toml`. A candidate workbranch cannot self-authorize
`main_merge = true` beyond accepted policy (`POLICY_SELF_AUTHORIZATION_REJECTED`),
and direct merge always fails closed if the target base moves after validation.

## Cooperative shared-claim policy

`[governance].allow_basic_shared_claims` defaults to `false`. A project may set
it to `true` in the main repository's `.decapod/config.toml` to allow a
basic-trust agent to join an existing task with a different primary owner as a
secondary owner. This setting affects the local backend's `todo.claim.shared`
verified-trust threshold only. It does not raise the stored trust level, allow
untrusted agents, create a primary claim, weaken a stronger trust requirement,
or bypass approval, dependency or other gates. Cloud-backed claims do not use
this policy. The false default is omitted when Decapod serializes config;
older versions may reject a config that explicitly contains the new field.

This is a cooperative local policy, not authentication. An independently
accepted main policy can enable the switch, and agent labels can be impersonated. The
config fingerprint in an audit record shows the bytes used, not who edited them
or who invoked the claim. The main repository config is read rather than the
candidate workspace config; if the main root cannot be resolved or the config
is malformed, oversized, a symlink or not a regular file, the opt-in fails
closed. This feature does not defend against hostile processes with the same
filesystem or datastore access.

A successful use records the asserted agent and primary owner labels, task and
config SHA-256 in a `policy.basic_shared_claim.used` event in the ownership
transaction. Each operation snapshots config before acquiring the write lock;
a change after that snapshot may not stop an in-flight claim. Turning the flag
off prevents later uses but does not remove existing secondary owners. This is
not an audited grant of verified trust or proof of human approval.


## Repository Backends: Decapod Local and Decapod Cloud

- **Decapod Local (`repo.backend = "local"`, default):** Local-first SQLite and Git governance runtime stored under `.decapod/`. Works offline with no account or external endpoint required.
- **Decapod Cloud (`repo.backend = "cloud"`):** Optional hosted repository-bound backend via Dactyl (`decapod init --backend cloud`, `decapod cloud login|status|projects|project`). Cloud mode never falls back to local SQLite.
- **Explicit cloud adapter environment:** Explicit adapter/test callers configure `DECAPOD_CLOUD_API_URL`, `DECAPOD_CLOUD_DATASTORE`, and `DECAPOD_CLOUD_PROVIDER` (`DECAPOD_PROPODUS_API_URL` remains a deprecated backwards-compatible alias that emits a structured deprecation warning; no obsolete preview URL default is assumed when unconfigured).

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
- `source_coverage`: one row for every discovered workflow; no duplicate,
  unknown, omitted, or excluded workflow mappings. A row either maps the
  workflow to real local proof (`{ source = "...", commands = ["..."] }`,
  nonempty list of configured required command names) or declares a reviewed
  CI-only limitation (`{ source = "...", ci_only = true, reason = "..." }`,
  `commands` omitted or empty). A `ci_only` row requires a nonempty `reason`,
  forbids mapped local commands (a `reason` without `ci_only`, or `ci_only`
  with commands, fails closed as a false coverage claim), is pinned by
  `reviewed_commands_digest`, and is reported in `validation-results.v3` as a
  `NonApplicable` limitation stage for that workflow — never counted as a
  passed or skipped check. When every discovered workflow is `ci_only`, the
  required local commands still execute normally as the repository's local
  build/test contract.
- `executions`: a table keyed by each required command name, containing either
  single-platform `program` (absolute canonical installed executable) and
  `toolchain_files` (nonempty map of labels to absolute canonical tool/runtime
  files), or a per-platform `platforms` table keyed by explicit canonical
  platform keys (`linux-x86_64`, `linux-aarch64`, `macos-x86_64`,
  `macos-aarch64`) where each platform entry provides its own `program` and
  nonempty `toolchain_files` (mutually exclusive with top-level `program` and
  `toolchain_files`), together with `environment` (explicit map), `directory`
  (repository-relative, default `.`), and `timeout_seconds` (1–7200). Every
  declared platform alternative is pinned by `reviewed_commands_digest` and
  preserved across accepted targets (`ACCEPTED_REPOSITORY_CONTRACT_CHANGED`); at
  execution time the host selects and byte-measures only the current host's
  reviewed platform entry, failing closed if that platform is missing or
  unsupported.

`proof.required_profile.reviewed_sources_digest` separately pins the full
workflow/action/helper/build inventory. `init` and `rpc --op agent.init` report
`inventory_digest` and `command_contract_digest` as read-only review aids. Inspect
all inputs and the intended coverage before explicitly entering either digest.
Neither initialization nor a matching digest executes proof or approves itself.
A changed workflow, command, arguments, requiredness, tool path, environment or
coverage mapping needs renewed review. Init reruns preserve both declarations.

Reconciling `source_coverage` and the reviewed digests is a repository-maintainer
review step on `.decapod/config.toml` (the live authority): edit the coverage
declaration, inspect the freshly reported `command_contract_digest` from
`rpc --op agent.init`, and enter the new reviewed digest yourself. There is no
agent-writable mutation operation that updates coverage or digests; `init` and
`agent.init` only report them as read-only review aids. This manual
reconciliation path is an explicit, separate limitation of the profile (#1531),
not a gap agents can close through any RPC op.

Review digests establish internal content consistency. Once the accepted target
has a repository-local contract, the candidate must preserve its required command
names, programs, arguments, measured-tool declarations, environment, directory,
timeouts and workflow coverage. Additional required checks are allowed. Replacing
or removing accepted checks, converting their coverage to CI-only, or changing
profiles returns `ACCEPTED_REPOSITORY_CONTRACT_CHANGED` before execution selection.
Editing and repinning candidate configuration cannot authorize that transition.
Replacement policy must first be independently accepted on the target or
authorized through a bounded, `policy.control`-gated transition approval in the
canonical main repository policy ledger. There is no agent-side waiver or
automatic policy migration. First adoption still requires explicit operator
review; a matching digest does not authenticate that reviewer.

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
proof remains a limitation to reconcile, not a successful or skipped check —
declare it honestly with a `ci_only` coverage row instead of mapping the
workflow to an unrelated local command.

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
evaluates that result; the agent publishes through the applicable gate. By
default (`[governance].main_merge = false` without standing PR merge
delegation), the human retains merge authority. When accepted base policy
explicitly delegates direct merge (`[governance].main_merge = true`) or standing
PR auto-merge authority and exact candidate/target validation passes cleanly, the
agent advances autonomously to the authorized terminal delivery endpoint without
asking redundant follow-up confirmation, while candidate edits can never expand
merge authority beyond accepted policy. This principle adds no unimplemented
guarantee.

## Optional Laya post-validation assessment

```toml
[laya]
mode = "disabled" # default; no model/runtime needed
```

Explicit opt-in uses `mode = "required"` and the reviewed `model_manifest_digest`
printed by `decapod setup laya`. Init can select that reviewed digest without
installing or executing a model. Omitted options preserve current Laya policy,
backend preference, CPU fallback and local binding settings. Dry-run is read-only.

### Diagnostic gate overrides and full readiness

`DECAPOD_VALIDATE_SKIP_GIT_GATES` and
`DECAPOD_VALIDATE_SKIP_TOOLING_GATES` are presence-based diagnostic flags.
`DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES` uses its existing truth-value parser.
An active override produces `READINESS_GATE_OVERRIDE_UNSUPPORTED` for full
validation, supported workspace publication and validated completion, including
historical completion admission. Remove it and collect fresh applicable proof;
reusing a cached receipt does not grant the missing authority.

CI-local and project-prerequisite diagnostics keep explicit partial scope and
cannot authorize publication. Historical observations remain inspectable. Broker
requests preserve their caller diagnostic scope, and local validated completion
rejects before broker handoff. An unknown write outcome still requires inspection;
never replay it merely because an override was removed. Direct unrestricted Git
and filesystem access remain outside this local operation boundary.

### Updating an accepted local verification-source inventory or proof policy (#1738)

For an already accepted `repo.pr-readiness.v1` contract, candidate review cannot
self-authorize a changed `reviewed_sources_digest` or replacement
`commands`/`executions`/`source_coverage` declarations. Without independent
acceptance, `decapod validate` fails closed before execution selection with
`ACCEPTED_REPOSITORY_CONTRACT_CHANGED`, reporting the old-to-new digests and the
exact deterministic transition command:

```console
decapod govern policy approve --id <transition-id> --scope repo.pr-readiness.v1
```

`<transition-id>` (`repository_profile::repo_policy_transition_id`) is a
64-hex-character SHA-256 digest bound to the exact
`(accepted_base_sha, candidate_head_sha, accepted_sources_digest,
candidate_sources_digest, accepted_commands_digest, candidate_commands_digest)`
tuple under scope `repo.pr-readiness.v1`.

1. **Structural self-consistency first:** Update the candidate's reviewed
   digests (`reviewed_sources_digest` via `rpc --op agent.init` and
   `reviewed_commands_digest` via `command_contract_digest`), stage and commit
   the candidate, then run `decapod validate`. Unreconciled source inventories or
   stale command digests still fail with `PROOF_PROFILE_RECONCILIATION_REQUIRED`
   or `STALE_COMMAND_CONTRACT`.
2. **Privilege-gated local maintainer approval:** `decapod govern policy approve` is
   gated on `"policy.control"` (core trust) in the canonical main `.decapod/data`
   policy ledger (`decapod.db`). Ordinary agents and `"unknown"` callers are
   rejected fail-closed, and candidate `.decapod/config.toml` or `governance.json`
   plan prose can never authorize a transition.
3. **Bounded, exact-subject validity:** Approvals carry a bounded lifetime
   (`--expires-in-seconds`, default 900) and can be revoked at any time via
   `decapod govern policy revoke --id <approval-id>`. Any change to the accepted
   base SHA, candidate head SHA, `reviewed_sources_digest`, or
   `reviewed_commands_digest`, as well as wrong scope, expiration, or revocation,
   invalidates the approval and fails closed with
   `ACCEPTED_REPOSITORY_CONTRACT_CHANGED`.
4. **Full execution through the standard gate:** When a valid active approval
   matches the exact transition, validation admits the candidate's reviewed
   `repo.pr-readiness.v1` contract into the normal validation engine so all
   transitioned required checks run against the exact candidate before
   `workspace publish`. First adoption remains supported; additive commands under
   the existing inventory remain possible without transition approval; and
   downgrading an accepted `decapod.pr-readiness.v1` strict profile to
   `repo.pr-readiness.v1` remains unconditionally blocked with
   `REQUIRED_PROFILE_DOWNGRADE`.

#### Approvals in fresh CI checkouts and Decapod Cloud

CI uses a signing key accepted on the protected target. On that target, maintain
`.decapod/repository-policy-signers.json` with this shape. Each value holds
the raw 32-byte Ed25519 public key encoded as 64 hexadecimal characters.

```json
{
  "schema": "decapod.repository-policy-signers.v1",
  "keys": {"maintainer": "<raw-public-key-hex>"}
}
```

For a reviewed transition, sign the compact UTF-8 JSON array
`["decapod.repository-policy-approval.v1",key_id,transition_id,issued_at,expires_at]`
with the corresponding Ed25519 private key. Both times are integer Unix
seconds and the interval must be at most 24 hours. For example, OpenSSL 3
`pkeyutl -sign -rawin -inkey signer.pem -in message.json` emits the raw
64-byte signature. Serialize it as 128 hexadecimal characters and supply the
following JSON as `DECAPOD_REPO_TRANSITION_APPROVAL` in the validation job.
Keep the private key in the maintainer's signing environment.
[OpenSSL Ed25519 signing documentation](https://docs.openssl.org/3.0/man1/openssl-pkeyutl/#ed25519-and-ed448-algorithms).

```json
{
  "schema": "decapod.repository-policy-approval.v1",
  "key_id": "maintainer",
  "transition_id": "<exact-transition-id>",
  "issued_at": 1800000000,
  "expires_at": 1800000900,
  "signature": "<raw-signature-hex>"
}
```

Run CI with an independently configured checkout of the candidate commit and
the protected target available as a remote-tracking ref under `origin`. For
pull requests, the verifier uses the provider's base branch metadata. A job
without a PR base uses `origin/HEAD` or an unambiguous `origin/main` or
`origin/master`. An approval expires at its deadline or when any bound Git
revision or digest changes. Updating the accepted signer list also changes the
protected base, so it invalidates earlier approvals.

In Cloud mode, a repository maintainer can run the same `govern policy approve`
command in a checkout with the exact candidate and a current `origin` base ref.
The authenticated Dactyl client records an exact pre-execution policy decision
in the Cloud governance service. Pass the printed
`DECAPOD_REPO_TRANSITION_DECISION_ID` to a Cloud validator with authenticated
repository access. The validator reads the decision from the service and checks
the routed project, authority generation, expiration, both commits, candidate
tree and full transition ID. Cloud validators can also use an accepted signed
approval. CI uses the signed carrier, so a CI job needs only public verifier data.

A release that changes how the source inventory is measured can require a
reviewed digest update even if the inventoried files are unchanged. Refresh the
reviewed digest with the current toolchain and approve the resulting exact
transition. Approval admits the changed policy to the normal required checks.
The `ci_local` result remains bounded to CI-local verification.

## Offline canonical local Git targets

For a solo local repository, an operator may explicitly accept
`proof.repository_contract.canonical_local_target = true` together with
`repo.backend = "local"`, an explicit `repo.base_branch`, the existing
`repo.pr-readiness.v1` profile and `validation_buildkite_preflights = false`.
Review the complete commands, execution closure and source coverage first;
`agent.init` reports the new command-contract digest without accepting it.
The target choice is part of that digest. Commit this reviewed contract on the
canonical repository's accepted branch **before** creating the worker. A
candidate-only flag, repinned digest or changed branch cannot authorize it.
Absence/false retains the existing GitHub target transport and prior digests.

Create/claim the task and use `workspace ensure` normally. From its clean,
checkpointed, committed candidate, the runtime contract is the ordinary command:

```console
decapod validate --format json
decapod qa verify commit <exact-candidate-sha>
```

The first command runs the real declared checks through the existing sealed
snapshot runner; the second independently inspects durable canonical history.
No hosted CI invocation, forge API, model prompt instruction, transport override
or replacement verifier is needed for this target. The JSON readiness result
uses `scope=repository_local_git_readiness`, with `ready=true` only when the
required local proof passes, and outer `publication_ready=false`. The separate
`workspace publish` boundary rejects this authority before remote mutation.
Local readiness does not authenticate a forge target or grant a merge right.

The identity is bound to the canonical local repository location and Git metadata,
not a user-authored repository label. The kernel reads that repository's exact
`refs/heads/<accepted-branch>` with installed Git and disabled ambient redirects,
replacement objects, transport and hooks. It checks authority and target before
and after proof, alongside the existing candidate, computed integration,
requirements, tools and custody bindings. Changing source, accepted target,
authority or required policy makes old proof inapplicable; historical success is
preserved. Refresh the workspace's accepted base through governed synchronization
and revalidate the resulting candidate. A fake success report cannot supply
missing current canonical proof.

This first adapter supports a canonical non-bare local checkout and its governed
Git worktrees/local-clone workspaces. Arbitrary bare upstreams, portable identities
across moved repositories, isolated execution and authenticated cloud/forge
acceptance remain unsupported. Local policy, verifier, task state and source
share the OS user's trust domain: protect the canonical checkout and toolchain;
hashes/session files are not an independent security boundary. Cloud selection
never falls back to local proof. An accepted strict source-PR or external proof
requirement remains required. A reviewed `ci_only` limitation is not execution
of the external job or equivalent assurance.

`tests/entrypoint_correctness.rs::canonical_local_target_cli_readiness_is_source_bound`
reproduces operator-reviewed acceptance, task claim, governed workspace, actual
CLI proof, history inspection, unchanged revalidation, and stale/forged-source,
target-movement, revocation and publication refusals. It uses the actual binary
without a replacement `Host` or fake command output. Required GitHub execution
supplies regression evidence; the fixture's own acceptance path invokes no
hosted provider. Test workers are deterministic, not LLM integration proof.
The test emits bounded timing and retained-evidence-size observations. No new
journal/table or duplicate source snapshot store is introduced; existing evidence
retention and validation history apply. Full ten-worker A–H proof and independent
remote publication remain separate unresolved requirements.

## Target-scoped repository proof (#1740)

Repositories using `repo.pr-readiness.v1` may declare an optional reviewed map
`[proof.repository_contract.targets.<name>]` (`DeploymentTargetContract`) with:

- `commands`: non-empty list of configured required command names for that target slice
- `depends_on`: optional upstream target names included transitively
- `shared_inputs`: optional repository-relative prefixes or globs (`*`, `**`) whose
  intersection with `<accepted_base>..HEAD` changed paths expands proof to include
  that target and its dependencies
- `platforms`: optional canonical host platform keys (`linux-aarch64`, `linux-x86_64`,
  `macos-aarch64`, `macos-x86_64`) compatible with that target's executor

`command_contract_digest` preserves pre-#1740 digests byte-for-byte when `targets` is
empty and binds `targets` when non-empty. Adding, removing, or editing a target
requires a policy transition approval bound to the exact candidate, accepted
base, and reviewed command and source digests. Unapproved changes fail with
`ACCEPTED_REPOSITORY_CONTRACT_CHANGED`.

Passing `--target <name>` to `decapod validate` (or `target` on RPC `validate.run`)
resolves the selected target's `depends_on` closure plus any targets matched by
changed `shared_inputs`, and runs only those obligations on compatible executors.
Unrelated targets and unavailable foreign-host executables are excluded without
weakening full validation (`decapod validate` without `--target`). Target-scoped
results carry `scope = "repository_target_readiness"` (or
`"repository_local_git_target_readiness"`), bind the target closure and exact
candidate SHA into `native-readiness-receipt.json` and `CompletedReadinessEvidence`,
keep `publication_ready = false`, and are rejected by full-repository PR publication,
workunit `VERIFIED` promotion, and unrelated target lookups.
When the canonical journal holds only target-scoped proof for the candidate,
failed full publication or verification reports `TARGET_SCOPED_PROOF_REJECTED`
and directs the caller to run unscoped `decapod validate`.

## Explicit legacy TODO retirement permission

`governance.rbac.legacy_todo_reconcile_roles = ["maintainer"]` grants only
`todo.legacy.reconcile` to actors holding that role in the canonical main
repository's `actor_roles`. It defaults to empty and is separate from journal
repair, maintenance and validation bypass. Worktree-only edits cannot grant it.
The versioned physical recovery command supports empty input/exact canonical
copies only, requires an active local session for apply, and never establishes
task completion or publication readiness. Actor labels and policy files remain
in the local host's trust domain; protect them independently when needed.
