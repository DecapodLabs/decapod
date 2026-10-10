# State Model

Decapod manages a finite set of stateful entities. Understanding their lifecycles is critical for successful agentic operation.

These entities are the durable substrate for governed work. Each Decapod
invocation is ephemeral and no daemon owns the task. Repository state turns a
temporary agent conversation into work that can be resumed, audited, validated,
and handed off across processes, models, harnesses, and later invocations.

## 1. Tasks (Todos)
The primary unit of work.
- **States:** `open` -> `claimed` -> `done` | `archived`.
- **Ownership:** A task in the `claimed` state is locked to a specific `agent_id`.
- **Identity:** ULID-based (e.g., `code_01H2...`).

## 2. Workspaces
Isolated execution environments.
- **Types:** Git Worktree | Docker Container.
- **Relationship:** Each active workspace is mapped to exactly one `task_id` and one `agent_id`.
- **Artifacts:** Changes made in a workspace are transient until `workspace publish` is called.
- **Cleanup:** Stale/unused workspaces (associated with done/archived tasks, deleted branches, or matching no active claim) can be cleaned up using the `workspace prune` command.
- **Recovery bounds:** Workspace/control subprocesses have a 15-second deadline; image builds have a 600-second deadline. Output is captured without pipe backpressure, capped at 64 MiB per stream, and owned children are terminated and reaped on failure. Timeout is a recovery error, never evidence that a branch or claim is absent.
- **Claim preparation:** The broker commits and acknowledges a task claim before optional container preparation runs on the requesting client. Preparation holds no broker election or datastore lock. Its bounded failure remains a warning in the successful claim response. A durable per-request handoff receipt prevents retries from repeating a started container command; interrupted preparation requires inspecting workspace recovery before an explicit retry. `DECAPOD_CLAIM_AUTORUN=0` disables this optional preparation for callers that only need task state.
- **Cleanup ownership:** `workspace prune --force` may remove dirty or interrupted managed workspaces, but does not establish ownership. Unknown directories, symlinked workspace parents/candidates, active-claim local clones, and targets whose Git or container state cannot be verified are preserved with recovery information. Unregistered directories require a durable invocation receipt binding their exact canonical path and directory generation; names, even full completed-task IDs, do not prove ownership. The canonical `workspace_lifecycle` event-stream receipt is committed before an empty privately staged directory is exposed by atomic no-replace rename. Staging is on the workspace filesystem, so a separately mounted datastore remains supported. An advisory invocation lease prevents pruning live work and releases on interruption. Retry automatically removes only an empty, unfinished reservation; newly added files are preserved for explicit review. For a container.run invocation whose producer held the lease throughout execution, recovery may reconcile its abandoned container even while the task claim remains active, retaining all workspace files. Interactive launch hints do not imply that producer lease and are not reclaimed solely because the claim is active. Container cleanup checks Decapod ownership, workspace and invocation labels and removes immutable IDs, treating already-gone containers as success. Before launch the receipt also binds the creating Docker or Podman backend. Recovery uses that recorded backend even when discovery preferences or installed runtimes change; it never treats an empty inventory from another engine as proof of cleanup. Missing, invalid or unavailable runtime provenance preserves resources with a recovery diagnostic.
- **Directory generation:** On supported Linux/macOS targets, automatic recovery verifies device/inode plus immutable creation time or a persistent invocation nonce. When neither persistent identity facility is available, normal creation/work continues with a pinned directory handle and lease, but later unregistered cleanup is preserved for manual review. Missing identity does not grant deletion authority.
- **Interrupted containers:** A remote runtime may retain a container after an abrupt client death. Managed launches carry workspace labels so a later prune can discover and reconcile them. An invocation recorded as requiring a container is preserved if runtime cleanup is unavailable; a known plain-workspace invocation does not require Docker merely because its executable is installed; process tests and runtime mocks do not establish real Docker interruption behavior.
- **Broker election:** The kernel advisory lock on the stable `broker.election` inode owns the lease and releases it on process exit. `broker.lock` is only diagnostic PID metadata; recovery never signals a persisted PID. The election file remains reusable, not an active daemon or stale lease.

## 3. Sessions
Authentication and identity verification tokens.

- **Dual-Token Architecture:**
  - **Local Agent Sessions:** Ephemeral, short-lived tokens generated on-the-fly via `session acquire`. Stored machine-locally under `~/.config/decapod/sessions/<project-hash>/<agent-id>.json` when available, with secure workspace-local fallback under `.decapod/managed/sessions/<agent-id>.json` when machine-local storage is unusable. Gates local coordination, TODO subsystem access, and database locking in the workspace (verified using the process-local or environment-provided `DECAPOD_SESSION_PASSWORD`).
  - **Cloud Session Token:** Long-lived global OAuth identity token stored as JSON (`{"token": "..."}`) under `~/.local/share/decapod/session_token.json`. Used to authenticate the user's client with the Propodus cloud backend when cloud storage modes are enabled.
  - **Optional Jev Credential:** TypeSafe API credentials are stored separately as `typesafe_api_key` in machine-local `~/.local/share/decapod/secrets.json` when `decapod init --decision-provider jev` is given an environment key. This file is never repository state and is not the Propodus session-token file.
- **Lifecycle:** Local sessions are acquired via `session acquire` and released via `session release`.
- **Restriction:** Most repository mutation commands (e.g., `todo add`, `workspace ensure`) require an active local session.

## 4. Constitution
The static/override rules of the repository.
- **Authority:** Immutable (Global) | Mutable (Local `OVERRIDE.md`).
- **Access:** Read-only via `rpc` or `docs`.
- **Authoring:** Each exact current generated directive subsection owns a four-backtick source block. Humans replace the visible instruction inside it with Markdown or any documentation style they prefer. The content does not render as outer `OVERRIDE.md` structure. Decapod derives structure, hashes, byte counts, source, and precedence.
- **Resolution:** Decapod extracts the wrapper-free body. Duplicate exact registered directives, unclosed wrappers, or non-empty unknown IDs in Decapod namespaces invalidate the complete repository overlay. Empty retired generated sections are ignored for upgrade compatibility. Nested headings and triple-backtick examples remain body content.

## 5. Event Evidence
Append-only operational evidence.
- **Authority:** Canonical tables in `.decapod/data/decapod.db` only (`events` streams + projection tables), accessed through `core::events`.
- **No live JSONL:** Runtime writers never append to `*.jsonl`. Historical files under `.decapod/data/` are one-shot migration inputs: imported into `events`, then moved to `.decapod/data/.retired-jsonl/`. Validate fails if known live legacy JSONL reappears.
- **Migration:** `events.retire_legacy_jsonl.v001` and related migrations import residual logs, unwrap double-wrapped federation payloads, and migrate assurance attestations into `events` (stream=`assurance`). Recreated legacy SQLite stores are copied forward and removed.
- **Federation payload shape:** Native federation writers and imports store only the inner domain `payload` object in `events.payload`. Older double-wrapped rows are unwrapped automatically. Operators must not hand-edit the SQLite store; verify recovery with `decapod validate --projections` and a green `federation.rebuild_determinism` gate.

## 6. Knowledge (Memory)
The persistent, shared understanding of the project.
- **Class:** Advisory (Aptitude) | Procedural (Federated Knowledge).
- **Persistence:** Surmounts individual sessions and agents.

## 7. Governance and Proof Artifacts

- **Intent and Plans:** The human supplies intent; the agent records and updates its governed interpretation through Decapod. Plans are execution state, not proof by themselves.
- **Claims:** Falsifiable repository-owned statements linked to a baseline, failure mode, measurement, and proof gate. They change as research evidence changes.
- **Trajectories:** Agent-recorded custody evidence for intent, boundaries, inspected and modified files, assumptions, tool calls, checks, and proof references across a run.
- **Living Specifications:** Agent-authored interpretations under `.decapod/managed/specs/`. Decapod requires and validates them but does not independently author their semantic claims.
- **Validation Receipts and Evidence:** Decapod records validation outcomes and binds required evidence to identified repository state. A failed result leaves the task incomplete.
- **Projections:** Generated views derived from supported authoritative inputs. Refresh may update them, but a projection does not become a second source of truth.
- **Publication State:** A governed transition that remains blocked while required validation, evidence, or approval is unsatisfied.

External systems such as GitHub Issues, Jira, Linear, or Beads may remain the
organizational system of record. Decapod todos and claims govern the accepted
work at the execution layer.
