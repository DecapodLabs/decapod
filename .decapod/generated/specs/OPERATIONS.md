# Operations

## Operational Readiness Checklist
- [ ] Ownership and on-call rotation defined.
- [ ] SLOs/SLIs defined with alert thresholds.
- [ ] Dashboards for latency/errors/saturation active.
- [ ] Incident runbooks linked for Sev1/Sev2 alerts.
- [ ] Rollback policy validated.
- [ ] Capacity assumptions reviewed.

## Service Level Objectives
| SLI | Target | Window | Owner |
|---|---|---|---|
| Availability | 99.9% | 30d | Core maintainers |
| Validate latency (p95) | <= 10s | 7d | Core maintainers |
| Gate failure false-positive rate | <= 1% | 30d | Core maintainers |

## Monitoring
| Signal | Metric | Threshold | Alert Severity |
|---|---|---|---|
| Throughput | command executions/min | anomaly vs baseline | Sev3 |
| Latency | validate p95/p99 | p95 > 10s sustained | Sev2 |
| Reliability | failed gate ratio | > 20% unexpected | Sev2 |
| Saturation | DB lock contention | sustained contention | Sev2 |

## Health Checks
- Liveness: binary invocation and core command response.
- Readiness: workspace/session/store checks pass.
- Dependency: git + filesystem + SQLite reachable.

## Alerting and Runbooks
| Alert | Severity | Runbook |
|---|---|---|
| Validation timeout spike | Sev2 | docs/runbooks/validate-timeout.md |
| Workspace interlock bypass regression | Sev1 | docs/runbooks/interlock-regression.md |
| Store boundary violation increase | Sev2 | docs/runbooks/store-boundary.md |

## Incident Response
- Incident commander role rotates with core maintainers.
- Communication channel: issue + team channel + incident notes.
- Postmortem SLA: draft within 48h for Sev1/Sev2.
- Corrective actions are tracked as claimed todo tasks with due dates.

## Structured Logging
- Use structured fields: `trace_id`, `task_id`, `op`, `duration_ms`, `result`, `error_code`.

## Severity Definitions
| Severity | Definition | Response Time |
|---|---|---|
| Sev1 | release-blocking outage or integrity risk | immediate |
| Sev2 | major degradation or recurring gate failures | 30 minutes |
| Sev3 | partial degradation without user-blocking impact | next business day |

## Deployment Strategy
- Branch + PR + CI validate gating.
- Promote only with local + CI proof receipts attached.
- Rollback by reverting offending change and revalidating.

## Environment Configuration
| Variable | Purpose | Default | Secret |
|---|---|---|---|
| DECAPOD_SESSION_PASSWORD | session auth gate | unset | yes |
| RUST_LOG | log verbosity | info | no |
| DECAPOD_STORE | store mode selection | repo | no |

## Capacity Planning
- Baseline validates command frequency in CI and local usage.
- Track DB growth and log compaction needs over time.
- Keep headroom for parallel agent workspaces and validate runs.
