# GEMINI.md - Agent Entrypoint

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

## Control-Plane First

```bash
decapod capabilities --format json
decapod rpc --op context.scope --params '{"query":"<problem>","limit":8}'
decapod data schema --deterministic
```

## Operating Mode

- Use Docker git workspaces and execute in `.decapod/workspaces/*`.
- request elevated permissions before Docker/container workspace commands.
- `.decapod files are accessed only via decapod CLI`.
- `DECAPOD_SESSION_PASSWORD` is required for session-scoped operations.
- Read canonical router: `decapod docs show core/DECAPOD.md`.
- Use shared skills memory for human-taught preferences across sessions/providers: `decapod data memory add|get` (aliases: `decapod data skills`, `decapod data teammate`).
- Operator reference: `decapod docs show docs/PLAYBOOK.md`.
- Capability authority: `decapod capabilities --format json`.
- Scoped context feature: `decapod docs search --query "<problem>" --op <op>` or `decapod rpc --op context.scope`.

Stop if requirements are ambiguous or conflicting.
