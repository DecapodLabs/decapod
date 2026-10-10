# CLI Reference

Decapod provides a unified CLI that supports both human-friendly text output and machine-readable JSON.

## Command Aliases

Decapod provides short aliases for common subcommands:
- `v` -> `validate`
- `i` -> `init`
- `t` -> `todo`
- `w` -> `workspace`
- `g` -> `govern`
- `s` -> `session`
- `d` -> `docs`

---

## Core Operations

### `validate` (alias: `v`)
Perform methodology compliance checks.
- `--store <repo|user>`: The task store to validate.
- `--format <text|json>`: Output formatting.
- `--verbose`: Enable detailed per-gate timing.

### `init` (alias: `i`)
Bootstrap or manage the Decapod lifecycle.
- `with`: Apply explicit options (non-interactive).
- `clean`: Remove all Decapod state from the directory.
- `--refresh`: Re-open the interactive initialization questionnaire for an
  existing project, preserving current values as defaults and keeping the
  refresh non-destructive. In non-interactive environments it preserves the
  current configuration without prompting.
- `--decision-provider <none|jev>`: Configure the optional advisory provider;
  when `jev` is selected, an explicitly supplied `TYPESAFE_API_KEY` is stored
  in the machine-local Decapod secret file, never in project configuration.

### `capabilities`
Discover the features supported by the current Decapod binary.

### `cloud`
Optional cloud credential operations. These commands do not enable cloud
storage or change the local SQLite default.

- `cloud login`: deprecated compatibility alias. Human setup belongs to
  `decapod init --backend cloud`; the alias remains only for existing scripts.
- `cloud status`: report credential availability and source without printing a
  token.

### Local database maintenance

These commands apply only to the canonical local `.decapod/data/decapod.db`
store and are explicit operator workflows:

```text
decapod data db verify
decapod data db backup --destination <unused-database-path>
decapod data db recover --preserve-original-at <unused-sibling-archive-path>
```

`verify` is read-only and reports Dactyl v0.10.0 integrity metadata. `backup`
uses Dactyl's online SQLite backup, so a WAL source is captured without
copying live `-wal`/`-shm` files into the destination. `recover` performs the
verified logical dump/reload only when explicitly requested; it preserves the
original, atomically publishes the replacement, rolls back on failure when
possible, and reports rollback failure distinctly. Recovery returns DELETE
journal mode while retaining application data and relevant SQLite metadata.

Do not run recovery while writers or same-process Dactyl connections are open.
The Decapod sidecar lock bounds contention between cooperating Decapod
processes, but it cannot coordinate arbitrary external SQLite writers or
unreliable filesystems/mounted storage. No command repairs a store
automatically, including startup, validation, event append, or connection
creation. JSON diagnostics distinguish `healthy`, `unavailable`, `locked`,
`corrupt`, `unsupported`, `recovery_failed`, and
`recovery_rollback_failed`; inspect `failure_code` for Dactyl's precise code.

---

## Workspace Management (alias: `w`)

### `workspace ensure`
Create or enter an isolated task worktree.
- `--branch <name>`: Provide a custom branch name.
- `--container`: Wrap the workspace in a Docker container.

### `workspace status`
Display active workspaces, their owners, and their current state.

### `workspace publish`
Prepare and bundle changes from an isolated workspace for promotion (PR/merge).
The promotion push is fast-forward-only. Decapod never force-pushes a governed
workspace branch. A non-fast-forward rejection is a blocker: reconcile the
remote divergence, rerun validation, and retry publication rather than rewriting
shared history.

---

## Task Tracking (alias: `t`)

### `todo list`
List tasks from the backlog.
- `--status <open|claimed|done|archived>`: Filter by state.
- `--category <name>`: Filter by task category.

### `todo claim`
Lock a task for active implementation.
- `--id <task-id>`: The specific ULID of the task.
- `--mode <exclusive|shared>`: Set the locking mode.

### `todo done`
Complete a work unit and generate proof artifacts.
- `--id <task-id>`: The task to close.
- `--validated`: Capture a cryptographic proof baseline of the changes.

---

## Governance & Subsystems (alias: `g`)

### `govern policy`
Classification and approval for high-risk actions.

### `govern health`
Claims, proofs, and system-wide integrity status.

### `govern artifacts`

Manage the single `.decapod/governance.json` document through logical sections:
Populate in dependency order: stable PR identity, intent/scope plan, current claims
and planned checks, work/evidence trajectory, validation, then staged-material
checkpoints. Record meaningful boundaries throughout the work, not only at completion.


- `migrate`: losslessly import legacy split files; reads never migrate implicitly.
- `begin-pr --id <change> --base-branch <branch>`: start an explicit PR boundary
  after verifying discarded evidence is recoverable from Git.
- `claim --id <id> --statement <text> --falsifier <text> --status <status>`:
  record a current-PR claim; status is `open`, `supported`, `refuted`, or `blocked`.
  Repeat `--proof-ref <ref>` for evidence; support requires proof.
- `resolve-obligation --id <id> --resolution <text> --proof-ref <ref>`:
  explicitly retire a carried obligation with one or more proof references.
- `checkpoint --id <unique-id> --summary <text> [--proof-ref <ref>]`: bind staged
  authored material and logical governance inputs. Stage the updated governance
  document afterward and commit both.
- `status`: read current normalized state without creating runtime state.
- `verify-checkpoints --base-branch <ref> [--head-ref HEAD]`: read-only verification
  of exact-material checkpoints across the full commit graph.
- `inventory [--base-branch <branch>]`: inspect logical sections, semantic
  currency, and PR participation. `--repair` initializes absent empty claims;
  `--claims-note <text>` records a current-PR checkpoint; `--compact` preserves
  legacy semantics while normalizing storage. Existing evidence is not overwritten.

Research claims are separate from Health Engine claims in `.decapod/data/decapod.db`.

### `govern capsule query`
Perform a deterministic query over the embedded constitution.
- `--topic <name>`: The subject of inquiry.
- `--scope <scope>`: The context boundary (e.g., "interfaces").
