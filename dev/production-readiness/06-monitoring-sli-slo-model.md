# Monitoring / SLI / SLO Model

## SLIs
- Validation success rate
- Task DB parity success rate after rebuild
- Command success/error ratio per subsystem
- Mean time to recover from state integrity failures

## SLO targets
- Validation pass rate >= 99% on clean repos
- Rebuild parity success >= 99.9%
- Critical workflow command success >= 99.5%

## Ownership
- Core subsystem owners: DB/store/validate reliability
- Plugin owners: task/federation/automation surfaces
- On-call owner for incident triage and escalation

## Dashboard requirements
- Separate panels for integrity, dependency failures, and security exceptions
- Trend lines for parity mismatches and lineage violations
