# Derived Data, Deterministic Replay, Compaction & Knowledge Convergence

> Scope: Decapod local canonical datastore (`.decapod/data/decapod.db`) and derived runtime/retrieval projections. Covers Phase 3 hardening across [#1936](https://github.com/DecapodLabs/decapod/issues/1936), [#1630](https://github.com/DecapodLabs/decapod/issues/1630), and [#657](https://github.com/DecapodLabs/decapod/issues/657).
> Grounded against `src/decapod/core/{events,task_journal,storage_compaction,validation_history,workunit,workspace_lifecycle,todo,context_capsule,context_bundle}.rs` and `src/decapod/plugins/{knowledge,federation,lcm}.rs`.

## 1. Canonical Streams vs. Derived Views Inventory

All canonical event streams live in the single `events` table (`src/decapod/core/events.rs`: `EVENTS_TABLE = "events"`, `schemas::EVENTS_TABLE_SCHEMA_V2`) with `PRIMARY KEY (event_id)` and `UNIQUE(stream, seq)` (`idx_events_stream_seq_unique`). Legacy `*.jsonl` files under `.decapod/data/` are one-shot migration inputs only (`RETIRED_JSONL_DIR = ".retired-jsonl"`).

| Domain | Canonical Authority | Derived / Reconstructible Views | Primary Code Owners |
| :--- | :--- | :--- | :--- |
| **Broker & Audit** | `events` (`stream = 'broker'`) | Broker diagnostics, health audit summaries | `src/decapod/core/broker.rs`, `src/decapod/core/events.rs` |
| **Task State** | `events` (`stream = 'todo'`, `task.journal.checkpoint` v1/v2 + `task.journal.gap_accepted`) & `storage_components` / `storage_component_refs` | `tasks`, `task_owners`, `task_dependencies`, `task_tags`, `task_verification` tables; `TaskProjection` & `current_digest` | `src/decapod/core/task_journal.rs`, `src/decapod/core/todo.rs` |
| **Workunit & Workspace** | `events` (`stream = 'workunit'`, `workunit.snapshot`; `stream = 'workspace_lifecycle'`) | `.decapod/data/workunits/<task_id>.json`, workspace custody/lease state (`workunit_journal_status`) | `src/decapod/core/workunit.rs`, `src/decapod/core/workspace_lifecycle.rs`, `src/decapod/core/authoring.rs` |
| **Knowledge & Context** | `knowledge` table, `events` (`stream = 'knowledge'`: `knowledge.before_correction`, `knowledge.write_attestation`, `knowledge.decay`, retrieval feedback, promotion firewall events), `events` (`stream = 'federation'`) | `KnowledgeContext` (`resolve_context`), `DeterministicContextCapsule` (`context_capsule`), `nodes` / `node_edges` (`federation::rebuild_from_events`), `summaries` (`lcm::rebuild_index`) | `src/decapod/plugins/knowledge.rs`, `src/decapod/plugins/federation.rs`, `src/decapod/plugins/lcm.rs`, `src/decapod/core/context_capsule.rs` |
| **Validation Proof** | `events` (`stream = 'verification'`, `subject_kind = 'commit'`, `subject_id = candidate_sha`, `validation.completed`) | `HistoricalValidationProof::diagnostic_summary`, `latest_candidate_for_task`, `historical-proof.json` | `src/decapod/core/validation_history.rs` |

All 13 registered canonical streams (`events::STREAMS`): `broker`, `workspace_lifecycle`, `workunit`, `todo`, `federation`, `external_actions`, `traces`, `verification`, `watcher`, `map`, `lcm`, `knowledge`, `assurance`. No parallel event broker or second memory store is introduced.

---

## 2. Cursor Scope, Causal Ordering, Duplicate & Gap Handling, and Watermarks

### 2.1 No Global Total Order Assumption
`seq` is monotonically assigned per stream (`SELECT COALESCE(MAX(seq), 0) + 1 FROM events WHERE stream = ?1` under `BEGIN IMMEDIATE`). Streams do **not** share a global total causal order:
- **Single-stream cursors** order by `(seq ASC, event_id ASC)` (`events::query_all_on_conn`, `events::query_subject_on_conn`, `task_journal::verify_with_budget`, `knowledge::read_attestations_and_feedback_on_conn`).
- **Cross-stream cursors** (e.g., `storage_compaction::compact_historical_events`, `storage_compaction::logical_events_inventory`) **must** use the composite keyset predicate `(seq > ?1 OR (seq = ?1 AND event_id > ?2)) ORDER BY seq ASC, event_id ASC`. Using `seq > ?1` alone drops tied `seq` rows from other streams whenever a 256-row page boundary splits a shared `seq`.

### 2.2 Causal Ordering & Versioned Reconstruction
- **Task Journal (`src/decapod/core/task_journal.rs`)**:
  - `CheckpointKind::ObservedBaseline` must have `ordinal == 1`, `predecessor == None`, `previous_digest == None`.
  - Every subsequent `CheckpointKind::Transition` must have `ordinal == prev.ordinal + 1`, `predecessor == Some(prev.record_digest)`, `previous_digest == Some(prev.projection_digest)`, non-decreasing `task.revision` and `lease_generation`, and `source_watermark >= prev.source_watermark` with `source_watermark < event.seq`.
  - Schema v2 checkpoints externalize fields `>= 128` UTF-8 bytes (`task.description`, `task.scope`, `verification.proof_plan`, `verification.verification_artifacts`, `verification.last_verified_notes`) into content-addressed `storage_components` while computing `projection_digest` over the **hydrated** `TaskProjection` and `record_digest` over the **cleared** wire projection (`clear_externalized_projection_fields`), ensuring exact digest parity across v1 and v2 checkpoints.
- **Workunit Journal (`src/decapod/core/workunit.rs`)**:
  - Each `workunit.snapshot` carries monotonic `generation`, `predecessor` (`record_hash` of prior snapshot), `operation_id`, `manifest_hash`, optional `ExecutionStateBinding`, and `proof_hashes`.
  - Reordered or tampered predecessors fail closed (`WORKUNIT_JOURNAL_CONFLICT` / `WORKUNIT_JOURNAL_INVALID`).
- **Validation History (`src/decapod/core/validation_history.rs`)**:
  - Records are commit-addressed (`subject_kind = "commit"`, `subject_id = candidate_sha`) and verified via `CompletionRecord::verify()` (`record_digest`, `proof.verify()`, envelope/row parity).

### 2.3 Duplicate Policy & Conflict Detection
- **`events::append_on_conn`**: If `event_id` already exists in `stream`:
  - Identical canonical payload (`canonical_event_matches`) is **idempotent** and returns the existing `seq`.
  - Differing payload fails closed with `EVENT_ID_CONFLICT`.
- **`workunit` (`operation_id`)**: Replaying the same `operation_id` with identical manifest/generation is idempotent; reusing an `operation_id` with changed contents fails with `WORKUNIT_OPERATION_CONFLICT`.
- **`knowledge`**: `merge_key` + `scope` conflicts are explicitly governed by `KnowledgeConflictPolicy::{Merge, Supersede, Reject}`. Before any `Merge` or `Supersede` mutation, the prior row is archived to `events` (`stream = 'knowledge'`, `event_type = 'knowledge.before_correction'`). Two active entries with conflicting content under the same `merge_key` or typed `(scope, node_type, lower(title))` trigger `status = "unresolved_conflict"` in `KnowledgeContext` and withhold both bodies from `records`.

### 2.4 Missing Predecessors, Gaps, and Historical Uncheckpointed Suffixes
- **Never invent prior state**: `todo::rebuild_from_events` refuses to synthesize task history from incomplete legacy domain events (`task_projection_replayed: false`). `task_journal` treats legacy pre-checkpoint baselines explicitly as `ObservedBaseline` (`history_origin: "observed_current_state_baselines"`).
- **Uncheckpointed suffix rule**: Any surviving non-exempt domain event on `stream = 'todo'` with `seq > head.source_watermark` fails closed with `TASK_JOURNAL_INCOMPLETE_HISTORY` across `check_complete_suffix_for_task`, `record_changes`, `verify_with_budget`, and `checkpoint_with_gaps`. Non-mutating audit events (`task.journal.checkpoint`, `task.journal.restored`, `task.journal.gap_accepted`, `task.comment`, `task.worker.run`, `task.verify.capture_failed`, `task.delegation.policy`, `task.delegation.reserve`) are uniformly exempt.
- **Explicit gap acceptance**: Only `checkpoint_with_gaps` (`--accept-historical-gap`) may advance past an uncheckpointed suffix by appending `task.journal.gap_accepted` with `[accepted_from_seq, accepted_through_seq]`, `accepted_event_count`, and bounded `reason`, preserving all historical events intact.
- **Stale / recovered views never grant custody or publication**:
  - `task_journal::check_validation_binding` rejects any `task.journal.restored` or `task.journal.gap_accepted` event after `captured_watermark` (`TASK_CHRONOLOGY_RESTORED_OR_GAPPED`).
  - `workunit::recover_workunit` restores missing/corrupt `workunits/<task_id>.json` only from verified `workunit.snapshot` records (preserving unrecognized valid JSON with `WORKUNIT_PROJECTION_CONFLICT`), and sets `resumable = false` unless `verify_resume_record` independently validates current execution binding and proof hashes.
  - `validation_history::HistoricalValidationProof::diagnostic_summary` always emits `"publication_ready": false` and never substitutes for live `PublicationReadiness`.

### 2.5 Watermarks, Bounds, and Snapshot Consistency
- `task_journal::VerifyBudget` / `VerifyCursor` supports bounded keyset-paged verification across `VerifyPhase::Checkpoints` and `VerifyPhase::Tasks`, returning `"status": "partial"` with `continuation_cursor` when `max_checkpoint_rows` or `max_task_rows` is reached.
- `VerifyCursor` pins `(snapshot_watermark, snapshot_task_count, snapshot_checkpoint_count, snapshot_revision_sum)` via `read_snapshot_fingerprint`. Any concurrent mutation or compaction between cursor calls aborts with `TASK_JOURNAL_SNAPSHOT_CHANGED`.
- Subject journal bounds (`events.rs`: `MAX_SUBJECT_RECORDS = 100_000`, `MAX_SUBJECT_BYTES = 32 MiB`, `MAX_SUBJECT_RECORD_BYTES = 8 MiB`, `QUERY_PAGE_SIZE = 1024`) are enforced transactionally before append (`check_subject_append_capacity`) and before capture (`query_subject_on_conn`).

---

## 3. Compaction, Growth Bounds & Retention Tradeoffs (`#1630`)

### 3.1 Physical & Relational Compaction Invariants
- **Compact Event Envelope (`_ce_v1`)**: `events::compact_envelope_for_storage` strips envelope fields duplicated in SQL columns (`event_id`, `ts`, `event_type`, `actor`, `task_id`, `candidate_sha`) and default `BrokerEvent` fields (`status = "success"`, `actor_id == actor`, `schema_version = "1.1.0"`, `db_id = "decapod.db"`, empty/null optional IDs), storing a compact bitmask under `_ce_v1`. Payloads containing `_ce_v1` or `_ce_raw` escape losslessly under `{"_ce_raw": ...}`. `federation` payloads remain bare inner objects.
- **Externalized Components**: `storage_components` stores zlib-compressed or plain UTF-8 bytes keyed by `(component_kind, codec_version, byte_len, content_sha256)`, verified on every read via `decode_and_verify_component_bytes`.
- **Relational Compaction (`storage_compaction`)**:
  - `apply`: Converts `meta` and `task_tags` to `WITHOUT ROWID` tables after exact relational `EXCEPT` parity and FK checks, and compresses cold `summaries` (`lcm`).
  - `apply_v2_compaction_migration` / `compact_datastore_lossless`: Runs under `BEGIN IMMEDIATE`, validates pre/post `logical_events_inventory` (`(count, events_logical_sha256)`) and `pragma_foreign_key_check`, records `storage_migration_manifests`, and stamps `storage.{format_version, min_reader_version, min_writer_version}` in `meta`.
  - **Downgrade refusal**: `check_storage_format_compatibility_for_version` fails closed (`STORAGE_FORMAT_INCOMPATIBLE`) when `format_version > STORAGE_FORMAT_VERSION` or evaluating binary semver `< min_reader_version` / `< min_writer_version`.
  - **Reachable-history GC & explicit page reclamation**: `gc_unreferenced_components` preserves every component referenced by `storage_component_refs` OR embedded in historical `task.journal.checkpoint` / `task.transfer.{exported,imported}` events. `reclaim_pages_explicit` (`VACUUM`) requires autocommit and never runs implicitly on hot paths.

### 3.2 Immutable Proof vs. Sensitive Payload Retention Tradeoffs
1. **No secret admission into immutable history**: `gatekeeper::contains_secret_material` rejects secret/credential patterns before admission into `knowledge` (`add_scoped_knowledge`) and filters unsafe records during `resolve_context` (`unsafe_or_oversized_record` / `credential_material`). Because canonical `events` and `storage_components` are append-only/proof-hashed, secret redaction happens **before** write rather than mutating historical digests.
2. **Repository-scoped deduplication**: `storage_components` deduplicates immutable text strictly within one repository's `.decapod/data/decapod.db` (or tenant-isolated cloud scope). A content SHA-256 hash alone never authorizes cross-repository or cross-tenant reads.
3. **Bounded proof witnesses instead of raw logs**: `validation_history` persists commit-addressed `CompletionRecord` digests, `ExpectedPlan`, `MaterialIdentity`, and `CurrentEvidenceInputs` (tool/input hashes) under the `8 MiB` record cap, rather than embedding full stdout/stderr build logs or worktree snapshots.
4. **Aggregate-only compaction telemetry**: `DataCompactionReport` and `CompactionMigrationManifest` expose byte counts, freelist/WAL sizes, and `events_logical_sha256` without emitting raw payload text or customer content.

---

## 4. Knowledge Convergence & Procedural-Promotion Firewall (`#657`)

- **Rebuildable derived view**: `KnowledgeContext` (`plugins::knowledge::resolve_context`) and `DeterministicContextCapsule` (`core::context_capsule`) are disposable projections derived deterministically from `knowledge`, `nodes` (`federation`), `events` (`stream = 'knowledge'`), and live repository source digests (`observe_source`).
- **Correction & revocation convergence**:
  - `superseded`, `rejected`, `deprecated`, and `stale` entries are excluded from operative `KnowledgeContext.records` regardless of textual repetition count, while remaining inspectable by ID (`get_knowledge`) and audit log (`knowledge_history`).
  - Expiry (`expires_ts <= reference`) is enforced at read time in both `search_knowledge` and `resolve_context` without requiring a prior `decay_knowledge` batch job.
  - Unscoped/private/cross-task/cross-path records never enter another task's packet (`validate_scope` + `scopes` filter).
  - Write-time attestations (`knowledge.write_attestation`) and retrieval feedback (`helped`/`neutral`/`hurt`) are read via `(seq, event_id)` keyset pagination with `events::reconstruct_stored_payload`, ensuring no truncation past 2048 events and distinguishing `source_attested_current`, `source_changed_since_write`, `source_unavailable`, and `malformed_provenance`.
- **Procedural-promotion firewall**:
  - `procedural/` knowledge entries require `provenance = "event:<promotion_event_id>"` backed by a `knowledge` promotion event carrying `target_class == "procedural"`, non-empty `evidence_refs`, and non-empty `approved_by`.
  - Both `record_promotion_event` and `add_scoped_knowledge` (for `procedural/...`) reject any `source_entry_id` whose existing `knowledge` row is non-`active` (`superseded`, `rejected`, `deprecated`, `stale`, `disputed`) or expired (`PROMOTION_FIREWALL_BLOCKED`).
  - Even promoted `procedural/` entries remain advisory (`authority = "advisory_below_current_intent_and_policy"`) in `KnowledgeContext` and never silently promote to Constitution or binding execution authorization.
