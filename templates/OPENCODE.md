# OPENCODE.md - Open-Source Coding Agent Entrypoint

You (open-source coding agent) are working in a Decapod-managed repository.

**You are bound by the universal agent contract:** See `AGENTS.md` for the complete protocol.

## Quick Start

**MANDATORY FIRST STEPS** - Run these EVERY session:

```bash
decapod --version                   # Check current version
decapod --help                      # Verify available commands
decapod docs show core/DECAPOD.md  # Refresh constitution
decapod validate                    # System state
decapod todo list                   # Active work
```

**Why this matters:** The decapod binary and constitution evolve. Always verify what commands are available and refresh your understanding of the latest contract before acting.

## Open-Source Agent Notes

- You may have varying capabilities - adapt the protocol to your tooling
- If you can execute commands: use `decapod` CLI directly
- If you cannot: output commands for human execution
- The contract is the same regardless of your implementation

## The Contract

Same four invariants as all agents:

1. ✅ Start at router (`core/DECAPOD.md`)
2. ✅ Use control plane (`decapod` commands only)
3. ✅ Pass validation (`decapod validate` before done)
4. ✅ Stop if missing (ask for guidance)

**All authority defers to AGENTS.md and the embedded constitution.**

## Links

- `AGENTS.md` — Universal agent contract (binding)
- `core/DECAPOD.md` — Router
- `.decapod/OVERRIDE.md` — Project customizations
