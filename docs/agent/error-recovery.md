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

### Orphan broker audit entries (`data broker verify`)

`data broker verify` checks audit lifecycle evidence, not SQLite structural
integrity. A pending entry without terminal evidence does not establish whether
the underlying write committed. Do not retry that write blindly.

1. Inspect `decapod data broker audit` and identify the pending event ID reported
   by `decapod data broker verify`. Establish that the original writer has stopped;
   an old timestamp alone is not evidence of a crash.
2. Preview `decapod data broker repair --event-id <id> --reason "<why the writer is stopped>"`.
   The command refuses events younger than 300 seconds, future-dated events,
   unknown targets, and entries already completed by a normal terminal event.
3. Apply that same command with `--apply` to append an abandonment acknowledgment.
   The original pending record remains, the reason and actor are recorded, and
   the original data outcome remains `unknown`. Repair targets exactly one event,
   even when other pending entries share its intent or correlation identifier.
   Repeating an applied repair returns `already_abandoned` without duplicating
   the acknowledgment. Applying uses normal session and mutation-policy gates.
4. Re-run `decapod data broker verify`, then the originally failing governed
   command. Clearing audit divergence does not prove datastore health or recover
   the original operation's state.

For a malformed SQLite image, corrupt index, an unsupported repair, or an older
release with no `broker repair` command: **stop and escalate to a human/maintainer**.
Raw `sqlite3`/`sqlite`, `PRAGMA`, `REINDEX`, and direct database file edits,
copies, moves, or replacement are never authorized by a failed governed command.
Do not infer database health from broker verification or the doctor's
"present and accessible" check. Preserve the reported error and use a supported
human-directed recovery path; audit abandonment does not repair corruption.

### `SPEC_REFRESH_AUTHORED_CONTENT_LOSS`

Refresh refused a transformation that would discard authored living-spec content.
The refresh preflights all existing canonical specs before writing any spec or
manifest. Preserve the original document and escalate malformed generated
boundaries to a human/maintainer. Do not remove project obligations to make refresh
pass. Ordinary sections such as `## Proof Surfaces`, `## Verification Method`,
and project-specific evidence sections are authorable and must survive refresh.

1.  **Parse the Error:** Decapod errors are strongly typed. Look for the `kind` and `message`.
2.  **Consult the Contract:** Cross-reference the command in `command-contracts.md`.
3.  **Remediate When Supported:** Identify the violated invariant, perform the sanctioned remediation, update the affected artifact, and re-run the failed validation.
4.  **Continue the Task:** A recoverable failure means the work remains incomplete. Continue toward publication after revalidation succeeds.
5.  **Escalate Real Blockers:** Do not attempt "brute-force" argument variations. Stop for human judgment when the result is a decision gate, contradiction, unsupported remediation, or unavailable proof.
