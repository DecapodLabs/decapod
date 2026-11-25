# Crash Durability, Transaction Boundaries, and Lease Fencing

This document describes Decapod's local persistence, crash-recovery boundaries, and multi-process lease/proof fencing model, grounded in the repository's source modules.

## 1. Systems of Record and Authority Boundaries

| Surface | Authoritative Store | Derived / Recoverable Views | Owner Module(s) |
| --- | --- | --- | --- |
| Task & claim state, journal checkpoints, events | `.decapod/data/decapod.db` (`tasks`, `task_owners`, `task_dependencies`, `task_tags`, `task_verification`, `events`) | CLI JSON projections, `todo` caches | `core/todo.rs`, `core/task_journal.rs`, `core/events.rs`, `core/broker.rs` |
| Workunit execution & proof history | `.decapod/data/decapod.db` (`events`, `stream='workunit'`) | `.decapod/data/workunits/<task_id>.json` manifests | `core/workunit.rs`, `core/events.rs` |
| Commit-addressed full validation history | `.decapod/data/decapod.db` (`events`, `stream='verification'`) | Temporary evidence dir (`historical-proof.json`, `publication-rejection-*.json`) | `core/validation_history.rs` |
| Workspace reservation & container ownership | `.decapod/data/decapod.db` (`events`, `stream='workspace_lifecycle'`; cloud mode: `~/.config/decapod/cloud-workspaces/...`) | `.decapod/workspaces/.staging/` and `.decapod/workspaces/<dir>` | `core/workspace_lifecycle.rs`, `core/workspace.rs` |
| Advisory datastore coordination | OS kernel `flock` open-file description on `.decapod/data/<db>.lock` & `.decapod/data/workspace-leases/*.lock` | Sidecar `.lock` / `-wal` / `-shm` files | `core/storage_lock.rs`, `core/workspace_lifecycle.rs` |
| Git refs, worktrees, and PR state | Local `.git` object store / worktree registry and remote Git / GitHub forge readback | Local tracking branches (`origin/*`) | `core/workspace.rs`, `core/workspace_publication.rs` |

Cloud mode (`core/storage.rs` `TodoStore` / `Dactyl` / `Propodus`) delegates remote task and lease authority to the authenticated hosted backend and never falls back to local SQLite custody (`VALIDATION_HISTORY_BACKEND_UNSUPPORTED`).

## 2. Transaction, File, and Git Crash Boundaries

### 2.1 SQLite Transactions and `DbBroker` (`core/broker.rs`, `core/events.rs`)
- All stateful mutations run through `DbBroker::with_transaction`, opening a SQLite `BEGIN IMMEDIATE` transaction via Dactyl (`core/db.rs`).
- Domain mutations (`tasks`, `task_verification`, `authoring_bindings`) and domain events + `task_journal` checkpoints (`task.journal.checkpoint` in `events` `stream='todo'`) commit in the **same** SQLite transaction (`todo::with_task_journal`).
- **Pre-commit crash (`SIGKILL` before `COMMIT`)**: SQLite rolls back the uncommitted transaction on next open; `tasks`, `events`, and `task_journal` remain at the prior committed state with no partial event or split checkpoint.
- **Post-commit crash**: Both domain row changes and their `task.journal.checkpoint` record are durable in `decapod.db`.
- **Suffix completeness (`task_journal::check_complete_suffix`)**: Any uncheckpointed domain event after `head.source_watermark` causes `TASK_JOURNAL_INCOMPLETE_HISTORY` instead of replaying stale snapshots over surviving evidence.

### 2.2 Advisory OS Locks (`core/storage_lock.rs`, `core/workspace_lifecycle.rs`)
- `StorageLock` (`Shared` for connections, `Exclusive` for maintenance/restore) and `WorkspaceLease` acquire OS advisory `flock` locks on sidecar `.lock` files.
- **Never unlink live locks**: On process crash (`SIGKILL`), the OS kernel automatically releases all `flock` descriptors held by the terminated process. `StorageLock::acquire` and `StorageLock::diagnose` never delete `.lock`, `-wal`, or `-shm` files as "stale lock repair".
- `StorageLock::release_owned_if_idle` only drops explicitly registered idle current-process descriptors and fails closed with `FOREIGN_LOCK_NOT_OWNED` if an external process holds the lock.
- Lock acquisition and probe timeouts use monotonic `std::time::Instant` elapsed budgets, unaffected by wall-clock jumps.

### 2.3 Recoverable Projection Files (`core/workunit.rs`)
- `.decapod/data/workunits/<task_id>.json` is a non-authoritative projection of the latest `workunit` event stream record.
- `workunit_journal_status` detects truncated/corrupt JSON (`projection_state = "invalid"`) or stale/divergent projection content (`"recovery_required"` or `"missing"`).
- `recover_workunit` restores the projection from the durable `workunit` journal in `decapod.db`, is idempotent on repeat calls, and refuses (`WORKUNIT_PROJECTION_CONFLICT`) to overwrite unrecognized valid JSON that does not match any journaled checkpoint.

### 2.4 Git Worktrees and External Effects (`core/workspace_lifecycle.rs`, `core/workspace.rs`, `core/workspace_publication.rs`)
- `workspace_lifecycle::reserve` separates durable intent from filesystem publication:
  1. Acquire per-target advisory `WorkspaceLease`.
  2. Create `.decapod/workspaces/.staging/invocation-*`, record `(dev, ino, birth/xattr nonce)` and fsync.
  3. Persist `workspace.reservation.*` receipt to `events` (`WORKSPACE_LIFECYCLE`) before rename.
  4. Atomically publish into `.decapod/workspaces/<name>` via `renameat2(RENAME_NOREPLACE)` (Linux) or `renamex_np(RENAME_EXCL)` (macOS) and fsync parent.
- **Crash recovery**: A crash between step 3 and step 4 leaves the staging directory intact; a subsequent `reserve` recovers the exact `Receipt` by directory identity and completes the no-replace rename without adopting arbitrary directories or deleting unrelated worktrees.
- **Remote publication (`workspace_publication.rs`)**: `git push` alone is never treated as proof (`PUBLICATION_INCOMPLETE`). `verify_remote` reads exact push-destination refs (`git ls-remote`), fetches exact commit OIDs (`--no-write-fetch-head`), verifies committed governance/spec bundles (`verify_committed_bundle`), and `ensure_and_verify_pr_for_target` re-runs local and remote admission checks before PR creation and again after PR diff/readback (quarantining non-admitted PRs via `quarantine_pr`).

## 3. Authoritative Lease Ordering and Proof Fencing

### 3.1 Lease Lifecycle and Generation Monotonicity (`core/fleet_coord.rs`, `core/todo.rs`)
- Every exclusive claim carries `(assigned_to, lease_generation, lease_lifecycle, lease_expires_at, revision)`.
- Authoritative state transitions (all serialized by `DbBroker` + `with_task_journal` CAS):
  1. **Initial claim (`todo.claim`)**: `lease_generation = 1`, `lease_lifecycle = claimed`, `revision += 1`.
  2. **Same-generation TTL extension (`todo.heartbeat` / `todo renew --heartbeat` / `DECAPOD_VALIDATE_LEASE_TTL=renew`)**: Requires active non-expired exclusive claim (`previous_secs > now_secs`), matching `assigned_to == agent_id`, and `expected_generation`. Updates **only** `lease_expires_at` (never shortens) with a single transaction timestamp shared between `task.lease_heartbeat` and its `task.journal.checkpoint`; preserves `lease_generation`, `revision`, and `contract_digest`.
  3. **Generation-rotating renewal (`todo.renew` without `--heartbeat`)**: Increments `lease_generation` (`extended_generation`), increments `revision`, sets `lease_lifecycle = extended`, and rotates `authoring_bindings`. Invalidates prior-generation validation bindings (`VALIDATION_HISTORY_TASK_STALE`) and execution checkpoints (`EXECUTION_STATE_CHANGED: EXECUTION_TASK_CUSTODY_CHANGED`).
  4. **Expiry & Reclaim (`todo.claim` when `lease_allows_reclaim`)**: Allowed only when `now_secs >= exp_secs` (`LeaseState::Expired`). Increments `lease_generation`, sets `lease_lifecycle = reclaimed`, increments `revision`, retires prior `authoring_bindings`, and prevents any delayed prior owner from completing, heartbeating, or publishing.
  5. **Handoff (`todo.handoff`) / Yield (`todo.yield`) / Release (`todo.release`)**: Advances `revision`; `handoff` advances `lease_generation` via `handoff_lease_generation`; `yield`/`release` clear `assigned_to` and set `lease_lifecycle` to `yielded`/`released` so prior evidence cannot be reused even if reacquired (`TASK_CUSTODY_REACQUIRED`).

### 3.2 Proof and Publication Fencing (`core/task_journal.rs`, `core/validation_history.rs`)
- **Validation capture (`capture_validation_binding`)**: Requires active exclusive custody (`holds_exclusive_custody()`, `LeaseState::Active`, `assigned_to == current_agent_id()`) and clean journal suffix.
- **Validation archival & workunit promotion (`check_validation_binding`, `verify_task_binding_current`)**:
  - Same-owner `todo.heartbeat` extensions preserve `(task_id, revision, status, contract_digest, assignee, lease_generation)` while advancing `lease_expires_at` and `checkpoint_digest`. `check_validation_binding` verifies each `todo.heartbeat` step (`check_heartbeat_step`) and live `LeaseState::Active`, so valid active renewals survive through `finish_archive`, `current_workunit_proof`, and `PublicationReadiness::verify_current` without false staleness.
  - Any expiry (`now >= lease_expires_at`), release, reclaim, handoff, generation rotation (`todo.renew`), or contract drift fails closed with `VALIDATION_HISTORY_TASK_STALE` / `PUBLICATION_CUSTODY_CHANGED` / `WORKUNIT_PROOF_STALE`.
- **Delayed publication fences (`#1763`)**: `PublicationReadiness::verify_subject`, `PublicationReadiness::verify_current`, `verify_committed_bundle`, and `verify_remote_unchanged` re-verify candidate commit `HEAD`, accepted base commit, governance/policy digests, epoch, workunit subject, and live task custody at the push/PR admission boundary before and after remote PR creation.
