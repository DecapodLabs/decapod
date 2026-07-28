# Operations## Operational Readiness Checklist
- [ ] On-call ownership defined.
- [ ] SLOs and alert thresholds defined.
- [ ] Dashboards for latency/errors/throughput are live.
- [ ] Runbooks linked for all Sev1/Sev2 alerts.
- [ ] Rollback plan validated.
- [ ] Capacity guardrails documented.## Deployment Model
Describe the operational runtime model, scheduling, and system deployment architecture.## Service Level Objectives
| SLI | SLO Target | Measurement Window | Owner |
|---|---|---|---|
| Availability | 99.9% | 30d | TBD |
| P95 latency | TBD | 7d | TBD |
| Error rate | < 1% | 7d | TBD |## Monitoring
| Signal | Metric | Threshold | Alert |
|---|---|---|---|
| Traffic | requests/sec | baseline drift | warn |
| Latency | p95/p99 | threshold breach | page |
| Reliability | error ratio | threshold breach | page |
| Saturation | cpu/memory/queue depth | sustained high | page |## Health Checks
- Liveness:
- Readiness:
- Dependency health:
- Synthetic transaction:## Incident Response
- Detection:
- Triage:
- Mitigation:
- Communication:
- Post-mortem:## Rollout Strategy
- Blue/green deployment:
- Canary release:
- Rolling update:
- Feature flags:## Capacity Planning
- Traffic patterns:
- Resource utilization:
- Scaling triggers:

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

<!-- decapod:codebase-attestation:start -->
## Codebase Attestation

- Repository signal fingerprint: `c95d945d65177ab861f944f7c022b996ab58002f3b464fa0c0f625d8456a0ff1`
- Significant implementation surfaces: `.github/` (8 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `docs/` (1 files), `src/` (99 files), `tests/` (4 files)
- Refreshed from the current codebase by `decapod specs.refresh`
<!-- decapod:codebase-attestation:end -->
