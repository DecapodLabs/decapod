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

After a version bump (`Cargo.toml`), the evaluating binary rewrites release-bound
entrypoint headers (`AGENTS.md` / `CLAUDE.md` / `CODEX.md` / `GEMINI.md`), the
managed Dockerfile pin, and living-spec attestations via `validate --refresh-specs`.
Those regenerated files **must** land on the same branch before the PR drift gate
(added in #1170 / v0.95.4) will pass.

**Post-merge fingerprint policy:** dogfood CI builds a tree-local binary whose
expected release fingerprints differ from the last published pin. Post-merge
(`push` to master) and release heal jobs set
`DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES=1` so validate does not hard-fail on
entrypoint/Dockerfile/manifest fingerprint mismatch; PRs still enforce pins and
the drift gate. The env requires a truthy value (`1`/`true`/`yes`) so empty
GitHub Actions expressions do not accidentally skip on PRs.

**Master ruleset delivery:** master forbids direct push (PR required, required
signatures, update protection). Release heal never pushes to master; it force-
updates `chore/release-bound-sync` and opens/updates a PR labeled `release`.
Open release-plz branches still receive in-place pin heals.

## Installed-Version Upgrade Path
After `cargo install decapod`, the next normal governed command runs protected, idempotent schema migration and legacy-event reconciliation before runtime consumers read evidence. Existing-project `decapod init` executes the same reconciliation before regeneration. A prior successful single-datastore migration retires its JSONL inputs through a durable receipt; startup does not rescan them. Legacy SQLite stores recreated by an older binary are copied forward and removed without entering the full-backup loop. Human-authored `OVERRIDE.md` content is validated but never mechanically rewritten. Fresh import conflicts preserve source artifacts and stop with an actionable error.

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

- Repository signal fingerprint: `da2222f40d1547d10c94dc41e24b54c576f6e04811c2fad4e9517552b0bf4da0`
- Significant implementation surfaces: `.github/` (10 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `docs/` (1 files), `src/` (101 files), `tests/` (4 files)
- Refreshed from the current codebase by `decapod specs.refresh`
<!-- decapod:codebase-attestation:end -->
