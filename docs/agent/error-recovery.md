# Error Recovery

Decapod uses deterministic error messages and recovery guidance. Treat these as **Operational Instructions**, not just failure reports.

## Process Exit Status

| Status | Meaning | Recovery Path |
|---|---|---|
| `0` | Operation succeeded. | Continue to the next governed action. |
| `1` | A Decapod domain, validation, configuration, session, I/O, or storage operation failed. | Inspect the typed message or structured result and follow its recovery guidance. |
| `2` | CLI syntax was rejected before the operation ran. | Consult the command contract or `--help`, correct the invocation, and retry. |
| `127` | The calling shell could not find a command. | Install or select the required executable; Decapod does not emit this status for its Rust domain errors. |

Do not infer a distinct process status for configuration, authentication,
not-found, or conflict errors. Current domain errors use status 1. Use structured
output when a caller must distinguish failure kinds.

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
- **Reason:** A one-shot legacy JSONL migration record reuses an event ID whose payload differs from canonical SQLite evidence.
- **Protocol:** Preserve both artifacts and request human review. Decapod does not guess which event is authoritative or continue with a partial import. After a successful import the JSONL file is retired under `.decapod/data/.retired-jsonl/` and is never runtime authority.

### Live legacy JSONL under `.decapod/data/`
- **Reason:** A historical event log file is still present in the live data directory (not yet imported/retired).
- **Protocol:** Run `decapod activate` (or any path that runs migrations). Decapod imports into `decapod.db` and moves the file to `.decapod/data/.retired-jsonl/`. Do not hand-edit or re-create JSONL ledgers as dual authority.

### `LEGACY_EVENT_PAYLOAD` / federation rebuild wiped `node_type` or titles
- **Reason:** Older pre-0.95 one-shot imports copied a full event envelope into `events.payload` instead of the inner domain object. Runtime authority is **only** `.decapod/data/decapod.db`. Replay then reads `node_type`, `title`, and edge endpoints as absent, which can make `federation.rebuild_determinism` diverge and turn the lineage gate red after rebuild.
- **Automatic recovery:** Current Decapod repairs double-wrapped federation payloads on activate/migration (`federation.events.unwrap_legacy_payload.v001`) and again as defense-in-depth during `decapod data federation rebuild`. Replay normalizes known legacy shapes through one shared boundary before projection. Rebuild always reads the `events` table (`stream = 'federation'`), not a live JSONL log.
- **Protocol:**
  1. Do **not** edit `.decapod/data/decapod.db` by hand and do **not** disable approval governance as a workaround.
  2. Run `decapod activate` (or any path that runs migrations) so the unwrap repair commits, then `decapod data federation rebuild`.
  3. Rebuild is transactional: either the full repaired log is replayed into a valid projection and committed, or the pre-rebuild projection is preserved.
  4. Verify with `decapod validate --projections`. Confirm `federation.rebuild_determinism` is green and that a second rebuild makes no further semantic change.
- **If repair aborts:** The error names the event ID and reason (type/identity mismatch, non-object payload). Preserve the store and request human review; partial unwrap is never committed.

## General Strategy
1.  **Parse the Error:** Decapod errors are strongly typed. Look for the `kind` and `message`.
2.  **Consult the Contract:** Cross-reference the command in `command-contracts.md`.
3.  **Remediate When Supported:** Identify the violated invariant, perform the sanctioned remediation, update the affected artifact, and re-run the failed validation.
4.  **Continue the Task:** A recoverable failure means the work remains incomplete. Continue toward publication after revalidation succeeds.
5.  **Escalate Real Blockers:** Do not attempt "brute-force" argument variations. Stop for human judgment when the result is a decision gate, contradiction, unsupported remediation, or unavailable proof.
