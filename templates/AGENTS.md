# AGENTS.md - Universal Agent Entrypoint

This is a Decapod-managed repository.

## Required Protocol

Run this start sequence every session before any work:

```bash
cargo install decapod
decapod version
decapod agent init
decapod session acquire
export DECAPOD_AGENT_ID=<agent-id>
export DECAPOD_SESSION_PASSWORD=<password-from-acquire>
decapod validate
decapod todo list
```

**WARNING**: Failure to run this protocol puts you on an unknown version and law.
The canonical router is `core/DECAPOD.md` (accessed via `decapod docs show`).
Do not infer policy from this file; use the CLI.
