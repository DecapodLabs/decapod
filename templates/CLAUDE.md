# CLAUDE.md — Claude Agent Entrypoint

You (Claude) are working in a Decapod-managed repository.
You are bound by the universal contract in **AGENTS.md**.

## Quick Start

```bash
decapod validate
decapod docs ingest
decapod session acquire
decapod rpc --op agent.init
decapod rpc --op context.resolve
decapod todo claim --id <task-id>
```

## Operating Mode

- **Plan first**: Non-trivial changes require a plan artifact.
- **Proof first**: `decapod validate` MUST pass before claiming done.
- **Minimal changes**: Only change what is directly requested.
- **Workspace isolation**: `decapod workspace ensure`. Never main/master.
- **CLI only**: All `.decapod/` access through `decapod` CLI.

## Safety Invariants

- core/DECAPOD.md: Universal router.
- `decapod validate` must pass before claiming done.
- Stop if error or ambiguous state occurs; respect invocation heartbeat.
- Safe Environment: Use Docker git workspaces; request elevated permissions before Docker/container workspace commands.
- Security: DECAPOD_SESSION_PASSWORD required; .decapod files are accessed only via decapod CLI.
- Architecture: Respect the Interface abstraction boundary.
- Updates: cargo install decapod.

See **AGENTS.md** for the full universal contract.

## Session Bootstrap Templates

Use `decapod session init` at the start of a work session.
Required templates: `templates/INTENT.md`, `templates/SPEC.md`, `templates/ADR.md`, `templates/CLAIM_NODE.md`, `templates/DRIFT_ROW.md`.
