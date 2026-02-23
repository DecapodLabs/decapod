# AGENT.md - Agent Entrypoint

You are working in a Decapod-managed repository.
See `AGENTS.md` for the universal contract.

## Quick Start

```bash
cargo install decapod

decapod validate
decapod docs ingest
decapod session acquire
decapod rpc --op agent.init
decapod rpc --op context.resolve
decapod todo add "<task>"
decapod todo claim --id <task-id>
decapod workspace ensure
```

## Operating Mode

- Use Docker git workspaces and execute in `.decapod/workspaces/*`.
- request elevated permissions before Docker/container workspace commands.
- `.decapod files are accessed only via decapod CLI`.
- `DECAPOD_SESSION_PASSWORD` is required for session-scoped operations.
- Read canonical router: `decapod docs show core/DECAPOD.md`.
- Operator reference: `decapod docs show docs/PLAYBOOK.md`.

Stop if requirements are ambiguous or conflicting.
