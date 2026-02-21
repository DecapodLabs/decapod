# Example 1: Claude Code Style Workflow

## Flow

1. `decapod session init --scope "feature: governed change" --proof "decapod validate"`
2. implement code in claimed worktree
3. `decapod validate`
4. `decapod handshake --scope "feature: governed change" --proof "decapod validate"`
5. `decapod workspace publish`

## Expected Outcomes

- Session stubs (`tasks/todo.md`, `INTENT.md`, `HANDSHAKE.md`) are present.
- Handshake record is emitted into `.decapod/records/handshakes/`.
- Publish is blocked unless provenance manifests exist.
