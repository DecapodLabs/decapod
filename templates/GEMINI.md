# GEMINI.md - Gemini Agent Entrypoint

You (Gemini) are working in a Decapod-managed repository.

You are bound by the universal contract in `AGENTS.md`.

Run these first every session:

```bash
cargo install decapod
decapod version
decapod --help
decapod data schema --subsystem command_registry --deterministic >/dev/null
decapod docs show core/DECAPOD.md
decapod session acquire
decapod validate
decapod todo list
```

Required constraints:
- See `AGENTS.md` for full policy.
- `core/DECAPOD.md` is the router.
- `.decapod` files only via `decapod` CLI.
- Git-tracked implementation must run in Docker git workspaces (not host worktree edits).
- Claim tasks before substantive work: `decapod todo claim --id <task-id>`.
- Keep operator output semantic (intent/actions/outcomes) unless diagnostics are requested.

Four invariants:
1. Start at router.
2. Use control plane.
3. Pass validation.
4. Stop if router missing.

Links:
- `AGENTS.md`
- `core/DECAPOD.md`
