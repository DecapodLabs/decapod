# Lessons Learned

Rules derived from past corrections. Each rule prevents a specific repeat failure.

---

## Session Management

- **Rule**: Decapod sessions expire. When you see `SessionError("No active session")`, re-run `decapod session acquire` and re-export the environment variables. Do not retry the failed command without re-acquiring.
- **Rule**: Environment variables (`DECAPOD_AGENT_ID`, `DECAPOD_SESSION_PASSWORD`) must be set in each shell invocation. Background processes inherit the environment at launch time — if a long-running process outlives the session TTL, it will fail partway through.

## State Mutations

- **Rule**: `decapod todo archive` is classified as high-risk. Requires `decapod govern policy approve --id task.archive` before batch operations.
- **Rule**: SQLite `database is locked` errors occur under concurrent access. Add retry logic or serialize operations with short delays.

## Working on Master

- **Rule**: `decapod validate` will fail on master/main. This is by design. Use `decapod workspace ensure` to create a worktree before making changes.
- **Rule**: For repo housekeeping (todo cleanup, doc edits), you may need to operate from master with awareness that validate will not pass the branch gate.

## Testing

- **Rule**: Golden vectors are immutable. If they need to change, the spec version must bump (v1 → v2). Never silently update golden outputs.
- **Rule**: `cargo test --locked` is the canonical test command. Always use `--locked` to ensure reproducible builds.

## Documentation

- **Rule**: CLAUDE.md, CODEX.md, and GEMINI.md should NOT be identical copies of AGENTS.md. Each should reference AGENTS.md for the universal contract and add agent-specific operating instructions.
- **Rule**: Constitution docs are embedded at compile time via `rust-embed`. Changes to `constitution/` files take effect on next build.

## Co-Player Policy

- **Rule**: Co-player policies derived from trace snapshots MUST only tighten constraints, never loosen them. `require_validation` is always true regardless of reliability score.
- **Rule**: Diff limits are deterministic functions of risk profile: unknown=100, high=150, medium=300, low=500 lines. These are fixed in `derive_policy()` and must not change without a version bump.
