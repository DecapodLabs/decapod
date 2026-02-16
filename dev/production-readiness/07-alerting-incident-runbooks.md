# Alerting and Incident Runbooks

## Paging triggers
- Repeated validation failures on default branch
- Persistent DB parity mismatch after rebuild attempts
- Any security boundary breach or secret exposure event
- Repeated external auth failures blocking automation

## Severity model
- Sev1: data loss risk, security breach, unrecoverable state corruption
- Sev2: broken release path, repeated failed validation gates
- Sev3: degraded reliability with known workaround

## First-response runbook
- Confirm blast radius and affected commands
- Capture current validation output and relevant logs
- Execute deterministic rebuilds for task/federation stores
- Re-run validation and record delta
- Escalate if unresolved after one bounded recovery cycle

## Post-incident
- Record root cause, corrective action, and prevention task
- Add/refresh debt register entry with owner and due window
