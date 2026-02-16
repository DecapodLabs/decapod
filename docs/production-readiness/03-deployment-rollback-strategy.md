# Deployment and Rollback Strategy

## Release flow
- Build and test on branch
- Run validation gates before merge
- Promote with tagged release only when gates are green

## Mandatory gates
- Unit/integration tests pass
- CLI contract tests pass
- `decapod validate` passes or has an explicitly documented exception
- No unresolved critical security findings

## Rollback strategy
- Use previous known-good release tag
- Roll back binary and schema migrations together
- Rebuild state from event logs when DB parity checks fail
- Record rollback cause and incident artifact in runbook

## Migration strategy
- Forward migrations must be deterministic and replay-safe
- Keep compatibility checks for legacy task/federation events
