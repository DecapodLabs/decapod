# CLAUDE.md — Claude Code Entrypoint

Follow `AGENTS.md` as the primary contract.

Claude Code runtime rules in this repo:

- Call Decapod before planning, before mutation, and after changes for proof.
- Use only Decapod CLI/RPC for shared state and policy operations.
- Treat lock/timeout markers as blocking, not advisory.
- Do not invent operations not present in the `decapod` binary.

Minimum flow:

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

Before completion:

```bash
decapod validate
# plus required tests/build checks for changed surfaces
```

If intent is ambiguous or conflicting, stop and ask the human.
