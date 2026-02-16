# Infrastructure Blueprint

## Runtime topology
- CLI binary: `decapod` Rust process
- Data plane: repository-scoped state under `.decapod/`
- Storage: SQLite + append-only JSONL event logs
- Automation plane: cron/reflex/container plugins

## Failure domains
- Local process crash
- SQLite corruption or lock contention
- Event-log replay mismatch against DB snapshots
- External dependency failures (Docker, GitHub SSH, network)

## Baseline controls
- Event sourcing remains canonical for task/federation state
- Rebuild commands must recover DB deterministically from event logs
- Mutation operations route through brokered CLI commands only
- Every state-changing workflow has a bounded timeout and clear error path

## Scaling model
- Primary scale axis is repo complexity, not high-QPS runtime traffic
- Use per-repo isolated state; no cross-repo mutable shared DB
- Keep expensive validation and rebuild operations explicit and operator-triggered
