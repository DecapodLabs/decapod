# GEMINI.md — Gemini Entrypoint

Follow `AGENTS.md` as the primary contract.

Gemini runtime rules in this repo:

- Use Decapod as the control plane at plan, mutation, and proof checkpoints.
- Operate only through Decapod CLI/RPC for shared state and governance.
- Handle lock/contention failures explicitly; do not proceed on uncertain state.
- Do not claim unsupported features or hidden workflows.

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

If intent or policy is unclear, stop and ask the human.
