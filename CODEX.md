# CODEX.md — Codex Entrypoint

Follow `AGENTS.md` as the primary contract.

Codex runtime rules in this repo:

- Call Decapod before committing to a plan, before edits, and after edits for proof.
- Use Decapod command surfaces; never mutate `.decapod/` directly.
- Treat contention and timeout failures as hard stops until resolved.
- Do not hallucinate capabilities: if `decapod` cannot do it, report the gap.

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

If requirements are ambiguous, stop and ask the human before irreversible work.
