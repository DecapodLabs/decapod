# Error Recovery

Decapod uses deterministic error messages and exit codes. Treat these as **Operational Instructions**, not just failure reports.

## Standard Exit Codes

| Code | Label | Meaning | Recovery Path |
|---|---|---|---|
| `1` | `Validation` | Methodology gate failed. | Read the error, fix the code/state, and re-run `validate`. |
| `2` | `Config` | `config.toml` error. | Verify key names and types in `config.toml`. |
| `3` | `Auth` | Missing session. | Run `decapod session acquire`. |
| `4` | `NotFound` | Entity missing. | Verify the ID with `todo list` or `workspace status`. |
| `5` | `Conflict` | Resource locked. | Select a different task; the resource is owned by another agent. |

## Common Error Patterns

### `Conflict("TODO already claimed")`
- **Reason:** Another agent instance is working on this task.
- **Protocol:** **STOP**. List other tasks with `decapod todo list` and select an unclaimed one.

### `ValidationError("AGENTS.md missing")`
- **Reason:** The repository has been corrupted or not initialized.
- **Protocol:** Run `decapod init` to restore the mandatory agent entrypoints.

### `RiskGate("Action requires approval")`
- **Reason:** You are attempting a high-risk operation (e.g., handoff or policy change).
- **Protocol:** Notify the human operator. They must approve the action via `decapod govern policy approve`.

### `WorkspaceError("Dirty tree")`
- **Reason:** Uncommitted changes exist in a directory where a worktree is being created.
- **Protocol:** Commit or stash your changes before running `workspace ensure`.

### `OVERRIDE_DUPLICATE_DIRECTIVE`
- **Reason:** `.decapod/OVERRIDE.md` contains the same registered directive section more than once.
- **Protocol:** Combine the policy into one generated directive section's fenced source block, then re-run the command. Decapod applies none of the ambiguous override unit.

### `OVERRIDE_MALFORMED_DIRECTIVE`
- **Reason:** A non-empty Markdown body appears beneath an H3 that claims a Decapod directive namespace but is not an exact current registered directive ID. Empty retired generated sections are migration-compatible and ignored.
- **Protocol:** Move the body to the appropriate current generated directive section, or make the heading ordinary prose that does not claim the Decapod namespace. Nested non-directive headings are valid override content.

### `OVERRIDE_UNCLOSED_BODY_FENCE`
- **Reason:** A generated directive subsection begins a four-or-more-backtick documentation body but does not close it.
- **Protocol:** Close the source block with at least the same number of backticks. Keep nested examples shorter than the outer fence, then re-run validation.

### `LEGACY_EVENT_CONFLICT`
- **Reason:** A legacy JSONL migration record reuses an event ID whose payload differs from canonical SQLite evidence.
- **Protocol:** Preserve both artifacts and request human review. Decapod does not guess which event is authoritative or continue with a partial import.

## General Strategy
1.  **Parse the Error:** Decapod errors are strongly typed. Look for the `kind` and `message`.
2.  **Consult the Contract:** Cross-reference the command in `command-contracts.md`.
3.  **No Guessing:** Do not attempt "brute-force" argument variations. If a recovery path is not obvious, stop and request human assistance.
