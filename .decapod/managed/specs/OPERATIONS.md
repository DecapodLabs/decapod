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
`TYPESAFE_API_KEY` from the machine environment, falling back to the
machine-local `~/.local/share/decapod/secrets.json` file, and uses bounded HTTP
timeouts; the key is never persisted in repository state. Initialization with
`decapod init --decision-provider jev` stores an explicitly supplied environment
key in that file with restrictive permissions and preserves the provider choice
on non-interactive refresh. `decapod init --refresh` reopens the initialization
questionnaire for an existing project, using current values as defaults and
keeping the refresh non-destructive; Enter preserves a setting while another
choice can enable or disable it.

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

## Native Buildkite Pipeline Contract (#1340)

Buildkite is migrating from GitHub Actions workflow ingestion to the native
`.buildkite/pipeline.yml` definition. GitHub Actions describes an event/job/
action runtime, while Buildkite natively executes command steps and evaluates
its own `if`, `depends_on`, and agent-uploaded `if_changed` fields. Translating
the former into the latter is not a faithful source of truth: merge commit
`bf98ec2e8320400964a1e84af0cc50210ac2fa6c` produced Buildkite #56 failures in
setup, dependency lint, documentation build, and release publication before
useful command output, while the equivalent GitHub jobs passed.

The native pipeline therefore owns event and path selection. The initial
Buildkite upload must use an agent version that supports `if_changed` and must
refresh the pull-request diff base before upload. Release-plz pull
requests run planning only; ordinary pull requests run governance, living-spec,
lint, test, validation, and documentation gates selected by changed paths;
master pushes run the full source/release chain; and tags run cargo-dist and
container publication. The Buildkite pipeline must upload the repository file,
because `if_changed` is applied by the agent during upload rather than by a
pipeline definition stored only in the UI.

Native Buildkite steps call Bazelisk, Cargo, mdBook, release-plz, cargo-dist,
and Docker directly. Tool installation actions and GitHub artifact handoffs are
not portable dependencies. Buildkite agent images must carry the toolchain
contract, and Buildkite secrets must provide GitHub/Cargo credentials only to
publishing steps. Documentation deployment uses the `gh-pages` branch and must
be selected in the repository's GitHub Pages settings; this removes the
GitHub-Actions-only Pages artifact/OIDC assumption.

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

- Repository signal fingerprint: `50b59962f590deaa75043035077bd633f1b290cabc89cb63bee7f0c6c5e20f55`
- Significant implementation surfaces: `.github/` (9 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `docs/` (1 files), `src/` (109 files), `tests/` (4 files)
- Refreshed from the current codebase by `decapod specs.refresh`
<!-- decapod:codebase-attestation:end -->
