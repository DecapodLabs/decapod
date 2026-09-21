# Operations

## Operational Readiness Checklist
- [ ] On-call ownership defined.
- [ ] SLOs and alert thresholds defined.
- [ ] Dashboards for latency/errors/throughput are live.
- [ ] Runbooks linked for all Sev1/Sev2 alerts.
- [ ] Rollback plan validated.
- [ ] Capacity guardrails documented.

## Deployment Model
Decapod is a daemonless CLI installed as a versioned Rust binary. Each invocation discovers the repository-local governance store and completes bounded work before exiting.

### Optional Jev provider operation

The `[decision] provider` setting only changes whether `assurance.evaluate`
attempts the bounded `trajectory_satisfies_intent` observation. `none` is the
safe default and requires no network or service startup. `jev` reads
`TYPESAFE_API_KEY` from the machine environment and uses bounded HTTP timeouts;
the key is never persisted in repository state.

Operationally, Jev is an advisory dependency, not a readiness dependency. If it
is absent, unavailable, times out, or returns malformed data, the assurance
result records `no_observation` and Decapod continues to enforce its ordinary
interlocks and proof gates. There is no background provider process.

Successful and unsuccessful Jev attempts are retained in the current run's
`.decapod/governance/jev.json` file. The file is validated as a strict,
trajectory-bound artifact before publication. It is created only when Jev is
actually attempted, reset by a new trajectory initialization, and never stores
the `TYPESAFE_API_KEY`.

### Native SQLite prerequisite for local Dactyl

Stateful commands in a project configured with `repo.backend = "local"` require a host SQLite shared library for Dactyl's local adapter. The startup preflight first honors `DACTYL_SQLITE_LIBRARY`, then the machine-local `~/.config/decapod/runtime.toml` value. If neither is set, it probes the host and persists a discovered library path in that user-level file so later Decapod projects do not repeat the search. If no runtime is available, the command stops with `LOCAL_SQLITE_RUNTIME_REQUIRED` and gives platform installation commands plus a one-shell `export DACTYL_SQLITE_LIBRARY=...` fallback. Cloud-backed startup does not require or inspect SQLite.

### Headless Podman machine startup

When container preflight detects Podman on macOS or Windows but `podman info`
cannot reach the service, Decapod attempts the standard machine recovery command
with `podman machine start --quiet --no-info --update-connection=false`. The
startup process receives no terminal input, and its output is captured for a
failure diagnostic rather than shown as an interactive UI. Decapod does not
launch Podman Desktop or change the default connection; it retries `podman info`
after the machine command and leaves Docker and Linux Podman behavior unchanged.

## Nix packaging support matrix

The repository flake (`flake.nix`) exposes `packages.default` / `packages.decapod` via `flake-utils.lib.eachDefaultSystem`. **Evaluating an output is not the same as supporting a platform.**

| System | Status |
|---|---|
| `x86_64-linux` | **CI-proven**: native `nix build` + `decapod system version` on Ubuntu |
| `aarch64-darwin` | **CI-proven**: native `nix build` + `decapod system version` on GitHub `macos-latest` (Apple Silicon) |
| `x86_64-darwin` | Exposed by the flake; **not** continuously proven in CI |
| `aarch64-linux` | Exposed by the flake; **not** continuously proven in CI |

Darwin Cargo and Nix builds share one linker story: no host absolute `-fuse-ld=/usr/bin/ld` pin; the Apple toolchain selects the system linker. The package and `checks.<system>.rust-toolchain` use the same `buildToolchain` from `rust-toolchain.toml` through the locked `rust-overlay`. CI never mutates `flake.lock`; maintainers refresh with `nix flake update rust-overlay` when the channel changes (see CONTRIBUTING.md).

## Release-bound CI pins

During workspace iteration, the evaluating binary rewrites release-bound
entrypoint headers (`AGENTS.md` / `CLAUDE.md` / `CODEX.md` / `GEMINI.md`), the
managed Dockerfile pin, and living-spec attestations via
`validate --refresh-specs` **inside the claimed worktree**. Those projections
**must** land on the same code branch before the PR drift gate passes. Decapod
must not generate `.decapod/managed/specs/*` in the protected root checkout
and later ask the agent to clean that dirt up; the agent is directed into
`.decapod/workspaces/*` instead of stashing or resetting the host tree
(GitHub #1255).

**Release boundary:** release-only commits are excluded from push validation.
The code PR carries the release-bound projections; after review, the release
workflow compiles, publishes the release and tag, and uploads the packages to
crates.io and Nix. It does not mutate master, open a post-release heal PR, or
run `decapod validate` after the release merge. The next agent iteration
starts by installing that release and refreshes projections for the next code
PR.

**Master ruleset delivery:** master forbids direct push (PR required, required
signatures, update protection). Agents publish release-bound changes through
the code PR, and release automation handles only compilation and publication.

**Release matrix safety:** the release workflow declares the four targets from
`workspace.metadata.dist.targets` as an explicit artifact matrix. Plan-only
runs therefore validate without runtime matrix expansion because GitHub Actions
and Buildkite validate a job matrix before applying the job-level skip
condition. Publishing and upload-mode runs still build the same cargo-dist
targets, while changes to the target list require an intentional workflow and
spec update.

## Buildkite GitHub Actions Migration Contract (#1340, #1344)

The repository keeps `.github/workflows/*.yml` as the sole workflow contract.
Buildkite's implemented GitHub Actions migration executes those files without
requiring a second repository-native pipeline definition. Event triggers,
changed-path filters, matrices, dependencies, artifacts, permissions, tokens,
Pages/OIDC behavior, release environments, and publishing gates therefore
remain authored and reviewed in the GitHub workflow files. Historical adapter
failures from merge `bf98ec2e8320400964a1e84af0cc50210ac2fa6c` remain
compatibility evidence, not reasons to remove workflow gates.

The obsolete `.buildkite/` directory is not part of the repository contract.
Buildkite service configuration owns adapter selection; repository users should
not maintain a parallel `.buildkite/pipeline.yml` translation. When a migrated
Buildkite step fails, capture the workflow path and job, commit, adapter/plugin
and agent versions, complete logs, and environment or permission context.
Classify the fix as migration support, service configuration, agent image,
secret/permission setup, or workflow behavior before changing
`.github/workflows/*.yml`. Verify pull requests, master pushes, release pull
requests, documentation deployment, and tagged publication independently
before disabling the GitHub Actions service.

### Current migration compatibility cases

The workflow files intentionally retain their native GitHub Actions paths and
add a Buildkite branch only where the imported runtime cannot provide the
GitHub-hosted behavior. The current branch is the executable record of the
following compatibility decisions:

| Workflow surface | Observed Buildkite symptom | Workflow-side compatibility path |
|---|---|---|
| CI dependency lint | `deps-lint` exited before producing useful Buildkite output while the native job passed | Install the pinned Rust toolchain first, then install `cargo-machete`, `cargo-deny`, and `cargo-audit` with shell commands instead of adapter-sensitive installer actions. |
| Bazel setup and shared binary | `setup` exited after the Bazel build when the artifact path was represented by Bazel's `bazel-bin` symlink | Install the pinned Bazelisk launcher in each job, stage the binary as a regular workspace file, and use the audited `upload-artifact`/`download-artifact` v4 pair. |
| Release plan | `Release / plan` failed on release PR #1353 in Buildkite while the native GitHub Actions plan passed; the imported `run` block was interpolated before runtime, and the hosted job did not reliably expose an event/PR marker | Run cargo-dist planning only for `release-plz-*` PRs and explicit tags, keep runtime selection in `.github/scripts/release-plan.sh`, treat an explicit tag as the only publishing context, default all other invocations to a non-publishing plan, and emit manifest/tag/publishing outputs from one shell step. |
| Release-plan artifact transport | Release PR #1355 was the first live Buildkite execution of `Release / plan`; the job failed while the workflow still uploaded and downloaded `cargo-dist` through `~/.cargo/bin`, a home-directory path outside the adapter's workspace-relative artifact contract | Stage the installed binary under `target/cargo-dist-cache`, upload and download that workspace-relative directory, then restore the binary into `$HOME/.cargo/bin` with a shell step in each consuming job. |
| Nix packaging | Linux and macOS jobs appeared on an open PR even though the workflow was `pull_request: closed` and release-merge gated | Trigger from the durable master push, then gate the matrix in shell on a release merge commit or explicit dispatch. |
| Documentation | `Deploy Docs / build` failed on the PR #1350 merge push after the Buildkite-only `buildkite-agent artifact upload` path ran | Keep the GitHub Pages action for native GitHub Actions; use the supported generic `actions/upload-artifact@v4` for the Buildkite branch. Pages deployment remains a service capability gap and is explicitly skipped there. |
| Release publication | `Release / release-publish` failed on the PR #1350 merge push because the imported job did not statically request its GitHub workflow token or registry secret | Keep the App-token/action path for GitHub; use the release-plz CLI in Buildkite and declare `${{ secrets.GITHUB_TOKEN }}` plus `${{ secrets.CARGO_REGISTRY_TOKEN }}` in the Buildkite branch so the adapter can issue the scoped token and resolve the named registry secret. |
| GHCR image publication | tag-only Docker setup actions were scheduled on the master push and failed in Buildkite | Use a simple tag-event condition and the Docker CLI/buildx path in Buildkite; retain the Docker actions for native GitHub Actions. |
| Validation in detached CI checkouts | The validator's worktree guard cannot establish an agent workspace from a hosted checkout, and `validate --refresh-specs` refreshes generated manifest metadata on every run | CI initializes a run cookie with `DECAPOD_VALIDATE_SKIP_GIT_GATES=1` while retaining the validation gates; the drift check compares semantic projections and normalizes only `generated_at` and `repo_signal_fingerprint` generated metadata. |
| Selective test diff base | Buildkite's migrated PR checkout honored the workflow's shallow `fetch-depth: 1`, so every matrix target exited before selecting tests | Request `fetch-depth: 0` for the test job, prefer `BUILDKITE_PULL_REQUEST_BASE_BRANCH`, and fall back only to refs or the parent already present locally; preserve the original path-based selection instead of running unrelated suites. |
| Test matrix throughput | The four target jobs waited on the unrelated shared-binary setup and each ran several Bazel targets sequentially, multiplying cold analysis and extending feedback time | Let independent test gates start without `setup`, batch each target group's Bazel labels into one invocation so analysis is shared and tests execute in parallel, and cap the four-way matrix with `strategy.max-parallel: 4`. |

These changes do not claim that Buildkite supplies GitHub's environments,
Pages deployment records, GitHub App token exchange, or Docker action setup.
Those are explicit adapter/service inputs. A green Buildkite check means the
workflow reached the supported execution path; release secrets, Docker
privileges, artifact storage, runner mappings, and Pages publication still
need independent Buildkite configuration proof.

### Dedicated governance follow-up boundary

PR #1354 is the merged implementation reference for the Buildkite workflow
compatibility changes. This follow-up is intentionally governance-only: it
records the operational context and proof for that merged change without
editing workflow behavior or reopening the implementation PR. The four files
under `.decapod/governance/` remain one coordinated review unit; a dedicated
governance PR must update `claims.json`, `plan.json`, `trajectory.json`, and
`validation.json` through Decapod, then run bounded validation before
publication. This separation keeps adapter behavior in the workflow PR while
keeping the machine-facing proof contract independently reviewable.

## Installed-Version Upgrade Path
After `cargo install decapod`, the next normal governed command runs protected, idempotent schema migration and legacy-event reconciliation before runtime consumers read evidence. Existing-project `decapod init` executes the same reconciliation before regeneration. A prior successful single-datastore migration retires its JSONL inputs through a durable receipt; startup does not rescan them. Legacy local database sources are opened through the Dactyl v0.10.0 facade, while Decapod owns row translation, schema policy, the explicit maintenance command policy, and idempotency ledgers. Dactyl opens the canonical path directly through its host runtime and owns the physical backup/recovery contract; no bundled fallback or second local authority is used. Human-authored `OVERRIDE.md` content is validated but never mechanically rewritten. Fresh migration conflicts preserve source artifacts and stop with an actionable error.

## Agent-Triggered Migration Runbook
1. Let the first governed command after installing a new Decapod release run
   the normal migration check.
2. If the command emits a migration notice, inspect the managed migration
   ledger and catalog before further agent mutations.
3. Confirm the reported migration completed and the command verification
   callback passed; do not treat the warning alone as proof.
4. For a failed migration, follow the bounded backup/restore diagnostic,
   preserve the source artifact, and stop for human review if supported
   recovery cannot establish a clean state.
5. For trajectory work, confirm the cookie parses as one object and that the
   intended run ID is the only current cookie identity.

## Service Level Objectives
| SLI | SLO Target | Measurement Window | Owner |
|---|---|---|---|
| Availability | 99.9% | 30d | TBD |
| P95 latency | TBD | 7d | TBD |
| Error rate | < 1% | 7d | TBD |

## Monitoring
| Signal | Metric | Threshold | Alert |
|---|---|---|---|
| Traffic | requests/sec | baseline drift | warn |
| Latency | p95/p99 | threshold breach | page |
| Reliability | error ratio | threshold breach | page |
| Saturation | cpu/memory/queue depth | sustained high | page |

## Health Checks
- Liveness:
- Readiness:
- Dependency health:
- Synthetic transaction:

## Incident Response
- Detection:
- Triage:
- Mitigation:
- Communication:
- Post-mortem:

## Rollout Strategy
- Blue/green deployment:
- Canary release:
- Rolling update:
- Feature flags:

## Capacity Planning
- Traffic patterns:
- Resource utilization:
- Scaling triggers:

## Logging
Use `tracing` + `tracing-subscriber` with structured JSON output and request correlation ids.

## Storage Diagnostics and Recovery Boundary (#1313)

The operator workflow for the canonical local store is:

```text
decapod data db verify
decapod data db backup --destination <unused-database-path>
decapod data db recover --preserve-original-at <unused-sibling-archive-path>
```

`verify` is read-only and uses Dactyl v0.10.0's native typed integrity API.
`backup` uses Dactyl's SQLite online backup, including WAL/SHM correctness,
then verifies and atomically publishes the standalone destination. `recover`
is never implicit: it invokes Dactyl's logical dump/reload recovery, preserves
the original and sidecars at the operator-selected unused sibling path, and
atomically replaces the active database only after verification. Dactyl
reports rollback and recovery failures; Decapod does not silently repair a
store during startup, ordinary validation, event append, or connection
creation. Successful recovery preserves application data, schema,
`user_version`, and `application_id`, and activates DELETE journal mode.

The canonical Decapod connection holds the adjacent `decapod.db.lock` sidecar
with bounded exclusive advisory coordination. Recovery additionally requires
writer quiescence and no same-process Dactyl connection to the target.
Contention, unavailable runtime/storage, corrupt or malformed databases,
unsupported capabilities, and recovery/rollback failure are distinct JSON
diagnostics; Dactyl's `failure_code` remains available for detail. The sidecar
only coordinates cooperating Decapod processes. Arbitrary external SQLite
writers, unreliable filesystems, and network/container mounts remain outside
the guarantee, and filesystem/path limitations from Dactyl must be respected.

## Governance Artifact Portability (#1314)

Trajectory and validation output may be shared through Git, CI, or issue
comments. Persisted path fields are normalized at write time: paths inside the
project are relative with forward slashes, and paths outside it are rendered
as <external-path>. Absolute paths remain available to the active operation
for filesystem work and are not used as the artifact representation.

## Secrets Management
| Secret | Source | Rotation | Consumer |
|---|---|---|---|
| External service auth material | managed runtime configuration | periodic | runtime services |
| Artifact signing material | managed signing service/local secure store | periodic | release pipeline |

## Security Testing
| Test Type | Cadence | Tooling |
|---|---|---|
| SAST | each PR | language linters/scanners |
| Dependency scan | each PR + weekly | supply-chain tools |
| DAST/pentest | scheduled | external/internal |

## Compliance and Audit
- Regulatory scope:
- Audit evidence location:
- Exception process:

## Pre-Promotion Security Checklist

- [ ] Threat model updated for changed surfaces.
- [ ] Auth/authz tests pass.
- [ ] Dependency vulnerability scan reviewed.
- [ ] No unresolved critical/high security findings.

<!-- decapod:capability-overlay:background-processing:start -->

## Background Processing Operations Overlay

### Queue Visibility
- Queue depth, processing rate, and latency MUST be monitored
- Dead letter queue MUST be visible and alerted
- Worker health and processing rate metrics required

### Shutdown Behavior
- Graceful shutdown: stop accepting new work, finish current job
- Drain behavior and timeout MUST be selected for the deployment
- Termination and requeue behavior MUST be selected and proven for the deployment

### Worker Health
- Worker liveness and readiness probes
- Queue depth alerts for backpressure detection
- Processing latency percentiles (p50, p95, p99)
<!-- decapod:capability-overlay:background-processing:end -->

<!-- decapod:capability-overlay:persistent-state:start -->

## Persistent State Operations Overlay

### Backup & Recovery
- Backup scope, schedule, retention, and restore evidence MUST be selected for the project
- Recovery point objectives MUST be explicit project decisions, not assumed values
- Recovery time objectives MUST be explicit project decisions, not assumed values
- Restore verification cadence MUST be recorded with the operational proof plan

### Migration Operations
- All schema changes via migration files
- Migration rollback procedures documented
- Zero-downtime migration strategy for production
- Migration health checks and rollback triggers
<!-- decapod:capability-overlay:persistent-state:end -->

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `2a8aeb18ea02f8431fad4818139bcceb6fedaf631e6b4b1896a98e807b916e51`
- Significant implementation surfaces: `.github/` (9 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `docs/` (1 files), `src/` (109 files), `tests/` (4 files)
- Refreshed from the current codebase by `decapod specs.refresh`
<!-- decapod:codebase-attestation:end -->
