# Datastore & Evidence Encoding Compatibility Policy (#1937)

This document defines Decapod's versioned storage format, evidence encoding,
upgrade/rollback, and cross-backend logical compatibility policy, qualified by
the versioned fixture corpus under [`tests/fixtures/migration/`](../tests/fixtures/migration/corpus_manifest.json)
and [`tests/datastore_encoding_compat_1937.rs`](../tests/datastore_encoding_compat_1937.rs).

Related cross-repository evolution contracts:
- Wire & envelope compatibility across Decapod/Dactyl/Propodus: [`DecapodLabs/dactyl#146`](https://github.com/DecapodLabs/dactyl/issues/146)
- Hosted PostgreSQL 17 / Edge mixed-generation deployment skew: [`DecapodLabs/propodus#227`](https://github.com/DecapodLabs/propodus/issues/227)

## 1. Supported Storage Format & Binary Version Floors

Physical compatibility metadata lives in the `meta` table under namespace
`"storage"` (`src/decapod/core/schemas.rs`, `src/decapod/core/storage_compaction.rs`):

| Key | Current Value | Constant | Purpose |
| --- | --- | --- | --- |
| `format_version` | `2` | `schemas::STORAGE_FORMAT_VERSION` | Physical datastore format generation (`1` = pre-`_ce_v1`/inline; `2` = compact `_ce_v1` + externalized components). |
| `min_reader_version` | `0.144.2` | `schemas::STORAGE_MIN_READER_VERSION` | Minimum Decapod binary semver permitted to read a v2 store. |
| `min_writer_version` | `0.144.2` | `schemas::STORAGE_MIN_WRITER_VERSION` | Minimum Decapod binary semver permitted to mutate or migrate a v2 store. |

Enforcement rules (`storage_compaction::check_storage_format_compatibility_for_version`):

1. **Read compatibility (`check_read_compatibility`, `require_write = false`)**:
   - Rejects `format_version == 0`, `format_version > STORAGE_FORMAT_VERSION` (`2`), or non-integer `format_version`.
   - Rejects any evaluating binary with `semver < min_reader_version` or malformed `min_reader_version`.
   - Refuses safely with `STORAGE_FORMAT_INCOMPATIBLE` without creating or mutating on-disk files.
2. **Write compatibility (`check_storage_format_compatibility`, `require_write = true`)**:
   - Enforces all read checks plus `semver >= min_writer_version` (and rejects malformed `min_writer_version`).
   - When `min_writer_version` exceeds the running binary while `min_reader_version` is satisfied, read-only paths (`events::query`, `task_journal_verify`, `compact_datastore_lossless(..., dry_run = true)`) succeed and mutating paths (`todo::initialize_todo_db`, `events::append_on_conn`, `apply_v2_compaction_migration`, `migration::check_and_migrate_with_backup_report`) fail closed without modifying disk.
3. **Stale legacy v2 floor reconciliation (`reconcile_v2_storage_format_metadata`)**:
   - Stores migrated under `v0.144.2..v0.148.13` that stamped `0.141.0` are safely restamped to `0.144.2` on write-capable startup/migration without altering event or task rows.
   - Read-only connections never attempt restamping; valid higher floors (e.g., `0.146.0`) are preserved monotonically (`max_semver_floor`) and never lowered.

## 2. Evidence & Storage Encodings

### 2.1 Event Envelopes (`src/decapod/core/events.rs`)
- **Legacy verbose JSON envelopes**: Full JSON objects stored in `events.payload` (with `event_id`, `ts`, `event_type`, `actor`, `task_id`, etc. inline).
- **Compact `_ce_v1` envelopes**: Non-federation streams omit column-redundant fields (`CE_BIT_EVENT_ID` through `CE_BIT_EMPTY_CALLER_AGENT_ID`, 17 defined bits) and store the bitmask under `"_ce_v1"`.
- **Collision escape `_ce_raw`**: Caller payloads already containing `"_ce_v1"` or `"_ce_raw"` are wrapped as `{"_ce_raw": <original>}` for exact round-trip preservation.
- **Federation payloads**: Stored as bare inner domain objects (`stream = 'federation'`) and never wrapped in `_ce_v1`.
- **Fail-closed guards**: `reconstruct_stored_payload` rejects `_ce_v1 == 0`, unknown bitmask bits (`mask & !CE_ALL_BITS != 0`), conflicting inline keys alongside `_ce_v1`, missing `subject_id` when `CE_BIT_TASK_ID` / `CE_BIT_CANDIDATE_SHA` is set, and invalid JSON.
- **Canonical digest invariance**: `storage_compaction::logical_events_inventory` hashes reconstructed payloads so `events_logical_sha256` is identical across legacy verbose and `_ce_v1` compact representations.

### 2.2 Task Journal Checkpoints & Externalized Components (`src/decapod/core/task_journal.rs`)
- **Supported checkpoint `schema_version` range**: `1..=2` (`MIN_SCHEMA_VERSION = 1`, `SCHEMA_VERSION = 2`).
- **v1 checkpoints**: Inline `TaskProjection` with empty `component_refs` and optional `ts`. Any v1 checkpoint carrying `component_refs` fails closed.
- **v2 checkpoints**: Externalizes immutable strings `>= 128` UTF-8 bytes (`COMPONENT_EXTERNALIZE_MIN_BYTES`) for `task.description`, `task.scope`, `verification.proof_plan`, `verification.verification_artifacts`, and `verification.last_verified_notes` into `storage_components` + `storage_component_refs` keyed by `(component_kind, codec_version, byte_len, content_sha256)`.
- **Codec versions**:
  - `codec_version = 0`: Raw UTF-8 bytes.
  - `codec_version = 1`: `DCPLCM1\0` zlib-framed payload (`src/decapod/core/storage_codec.rs`).
  - Any other `codec_version` fails closed with `TASK_JOURNAL_CONFLICT`.
- **Canonical digest & proof preservation**: Both v1 and v2 reconstruct the exact `TaskProjection` and preserve identical `projection_digest` (`Sha256`) and original proof bytes (`proof_plan`, `verification_artifacts`, `last_verified_notes`). Historical proofs are never reinterpreted or re-stamped as passed.

### 2.3 Cold LCM Codec (`src/decapod/core/storage_codec.rs`)
- Plain UTF-8 `TEXT` values (`< 1024` bytes or incompressible) decode directly.
- Compressed `BLOB` values use 44-byte header `DCPLCM1\0` + `u32_le(decoded_len)` + `sha256(raw_utf8)` + zlib level-6 payload (`MAX_TEXT = 64 MiB`).
- Truncated headers, unknown magic, oversized length headers, zlib stream errors, length/SHA-256 mismatches, and invalid UTF-8 fail closed with `LCM_STORAGE_CODEC_CORRUPT`.

### 2.4 Task Transfer Bundles (`src/decapod/core/task_transfer.rs`)
- Schema `decapod.task-transfer.v1` enforces `#[serde(deny_unknown_fields)]`, validates every `TransferredComponent` (`codec_version` in `{0, 1}`, `byte_len`, `content_sha256`), reconstructs the complete source checkpoint chain, and resets imported verification readiness (`execution_ready = false`, `last_verified_status = "unavailable"`) while keeping historical proof events immutable.

## 3. Additive vs Breaking Evolution Policy

- **Additive-compatible surfaces**:
  - Event envelope payloads (`events.payload`, including `_ce_v1`-compacted rows and `federation` inner objects) preserve additive optional fields losslessly across compaction and reconstruction.
  - `task_verification.verification_artifacts` JSON payloads preserve additive metadata fields losslessly.
  - Migration ledger/counter JSON artifacts (`applied.json`, `catalog.json`, `version_counter.json`) tolerate additive optional fields on deserialize while `inspect_migration_state_at` surfaces `UNKNOWN_APPLIED_MIGRATIONS` and `CATALOG_DIVERGES_FROM_BINARY` if newer migration IDs are encountered.
- **Strict proof-bound surfaces**:
  - `Checkpoint`, `TaskProjection`, `TaskRow`, `OwnerRow`, `DependencyRow`, `VerificationRow`, `ComponentRef`, and `task_transfer::Bundle` enforce `#[serde(deny_unknown_fields)]` so unmodeled mandatory or structural fields never silently drop out of `projection_digest` or `record_digest`. Adding fields to these structs requires an explicit `SCHEMA_VERSION` bump and migration/compatibility handling.

## 4. Duplicate Runs, Interrupted Upgrades & Backup Rollback

- **Atomic SQLite compaction (`apply_v2_compaction_migration`)**: Executes under `BEGIN IMMEDIATE`, verifies pre/post `events_logical_sha256`, `distinct_agent_prompt_groups`, and `pragma_foreign_key_check`, and rolls back completely on mismatch or interruption. Re-running is idempotent.
- **Governed backup & rollback (`migration::check_and_migrate_with_backup_report`)**:
  - Acquires `StorageLockMode::Exclusive` when a schema or compiled-migration upgrade is pending on an existing datastore.
  - Creates a `.decapod/data/.migration_backup_<version>_<ulid>` snapshot (excluding runtime `.lock`/`.election`/`broker-runtime`/`workspace-leases` coordination entries and prior stale `.migration_backup_*` directories) and snapshots `.decapod/managed/migrations/{applied,catalog}.json` and `.decapod/managed/version_counter.json`.
  - If any migration step or the caller's post-migration `verify` hook fails, both `.decapod/data/` and the managed migration ledger/counter files are restored to their exact pre-migration state so a subsequent retry can proceed cleanly.

## 5. Local vs Hosted Logical Schema Parity

Local SQLite (`decapod.db`) and hosted PostgreSQL (`Propodus` via `Dactyl`, tracked in `DecapodLabs/dactyl#146` and `DecapodLabs/propodus#227`) enforce **logical** schema parity without imposing physical DDL or byte-level engine equality:

- **Shared logical contracts** (`docs/datastore-v2/logical_contract_v1.json`, `src/decapod/core/storage_contracts.rs`, `src/decapod/core/cloud_parity_matrix.rs`):
  - Identical stable entity IDs, stream/event ordering semantics, CAS `revision` and `lease_generation` fencing, NUL-byte (`0x00`) rejection, JSON nesting bounds, and UTF-8 byte length measurement (`str::len()` == SQLite `length(CAST(x AS BLOB))` == PostgreSQL `octet_length(x)`).
- **Explicit physical differences**:
  - Local SQLite represents a single repository per file (`repository_scope` singleton, `WITHOUT ROWID` junction/CAS tables, `_ce_v1` SQLite payload compaction, `DCPLCM1\0` zlib blobs).
  - Hosted PostgreSQL 17 (`Propodus`) binds multi-tenant rows to server-derived repository/organization keys, enforces RLS/grants/advisory-locked SQL migrations, and manages server-side storage compaction (`data.compact` fails closed in `backend = "cloud"` with `DATA_COMPACT_CLOUD_UNSUPPORTED` rather than falling back to SQLite).
