# AGENT.md - Agent Entrypoint

You are working in a Decapod-managed repository.
You are bound by the universal contract in **AGENTS.md**.

## Quick Start

```bash
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

- Plan first: Non-trivial changes require a plan artifact.
- Proof first: `decapod validate` MUST pass before claiming done.
- Minimal changes: Only change what is directly requested.
- Workspace isolation: run `decapod workspace ensure` and work only from `.decapod/workspaces/*`. Never main/master, never `.claude/worktrees`.
- CLI only: All `.decapod/` access through `decapod` CLI.
- Just-in-time context: load only the minimum required doc slices with `decapod docs show <path>`.
- Embedded constitution only: never read `constitution/*` directly; use `decapod docs show <embedded-path>`.

## Safety Invariants

- core/DECAPOD.md: Universal router.
- `decapod validate` must pass before claiming done.
- Stop if error or ambiguous state occurs; respect invocation heartbeat.
- Safe Environment: Use Docker git workspaces; request elevated permissions before Docker/container workspace commands.
- Security: DECAPOD_SESSION_PASSWORD required; .decapod files are accessed only via decapod CLI.
- Architecture: Respect the Interface abstraction boundary.
- Updates: cargo install decapod.

See **AGENTS.md** for the full universal contract.

## Optional Operator Guide

`decapod docs show docs/PLAYBOOK.md`
