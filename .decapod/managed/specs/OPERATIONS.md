# Operations## Operational Readiness Checklist
- [ ] On-call ownership defined.
- [ ] SLOs and alert thresholds defined.
- [ ] Dashboards for latency/errors/throughput are live.
- [ ] Runbooks linked for all Sev1/Sev2 alerts.
- [ ] Rollback plan validated.
- [ ] Capacity guardrails documented.## Deployment Model
Decapod is a daemonless CLI installed as a versioned Rust binary. Each invocation discovers the repository-local governance store and completes bounded work before exiting.## Installed-Version Upgrade Path
After `cargo install decapod`, the next normal governed command runs protected, idempotent schema migration and legacy-event reconciliation before runtime consumers read evidence. Existing-project `decapod init` executes the same reconciliation before regeneration. A prior successful single-datastore migration retires its JSONL inputs through a durable receipt; startup does not rescan them. Legacy SQLite stores recreated by an older binary are copied forward and removed without entering the full-backup loop. Human-authored `OVERRIDE.md` content is validated but never mechanically rewritten. Fresh import conflicts preserve source artifacts and stop with an actionable error.## Service Level Objectives
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
- Scaling triggers:## Logging
Use `tracing` + `tracing-subscriber` with structured JSON output and request correlation ids.## Secrets Management
| Secret | Source | Rotation | Consumer |
|---|---|---|---|
| External service auth material | managed runtime configuration | periodic | runtime services |
| Artifact signing material | managed signing service/local secure store | periodic | release pipeline |## Security Testing
| Test Type | Cadence | Tooling |
|---|---|---|
| SAST | each PR | language linters/scanners |
| Dependency scan | each PR + weekly | supply-chain tools |
| DAST/pentest | scheduled | external/internal |## Compliance and Audit
- Regulatory scope:
- Audit evidence location:
- Exception process:## Pre-Promotion Security Checklist
- [ ] Threat model updated for changed surfaces.
- [ ] Auth/authz tests pass.
- [ ] Dependency vulnerability scan reviewed.
- [ ] No unresolved critical/high security findings.

<!-- decapod:codebase-attestation:start -->
## Codebase Attestation

- Repository signal fingerprint: `6ff06934bff417a92408b7d7b9f11ebb99ba232b46dd477136dac974daffcc75`
- Significant implementation surfaces: `.github/` (9 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `docs/` (1 files), `src/` (101 files), `tests/` (4 files)
- Refreshed from the current codebase by `decapod specs.refresh`
<!-- decapod:codebase-attestation:end -->
