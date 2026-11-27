# Datastore Schema & Single-DB Inventory Reconciliation (`#1118`, `#1132`)

## Summary

Legacy milestone trackers `#1118` (*12 subsystems consolidated to one SQLite DB with zero per-subsystem `.db` / `.events.jsonl` writes*) and `#1132` (*schema & table-count reconciliation*) are reconciled directly against the current `v0.166` architecture:

1. **Single Local SQLite Runtime (`#1118`)**:
   - All 13 active local initializers (`todo::initialize_todo_db`, `health::initialize_health_db`, `federation::initialize_federation_db`, `decide::initialize_decide_db`, `cron::initialize_cron_db`, `reflex::initialize_reflex_db`, `lcm::initialize_lcm_db`, `aptitude::initialize_aptitude_db`, `archive::initialize_archive_db`, `feedback::initialize_feedback_db`, `policy::initialize_policy_db`, `obligation::initialize_obligation_db`, `db::initialize_knowledge_db`) persist exclusively into `.decapod/data/decapod.db`.
   - Zero legacy `*.db` sidecars (`todo.db`, `governance.db`, `memory.db`, `automation.db`, `lcm.db`, `feedback.db`, `broker.dedupe.db`, etc.) and zero `*.events.jsonl` event streams are created at runtime (`tests/single_datastore.rs`).
   - Cloud mode (`backend = "cloud"`) uses the single remote Propodus/Dactyl backend for governed tasks & leases and never creates or falls back to `.decapod/data/decapod.db`.

2. **Schema DDL vs Runtime Table Inventory (`#1132`)**:
   - `src/decapod/core/schemas.rs` currently contains **46 `CREATE TABLE` DDL definitions**, because it retains historical/deprecated migration inputs alongside active tables (for example, `EVENTS_TABLE_SCHEMA` v1 alongside `EVENTS_TABLE_SCHEMA_V2`, duplicate `meta` schema constants per subsystem, and retired legacy compatibility tables such as `sources`, `edges`, `federation_events`, `task_events`, `agent_category_claims`, `agent_presence`, `agent_trust`, `agent_expertise`, `patterns`, `observations`, `consolidations`).
   - Initializing every active runtime subsystem + `storage_scope` (`repository_scope`) + `storage_compaction` + `request_dedupe` + `legacy_event_imports` produces the **36 logical runtime tables** enumerated in [logical_contract_v1.json](logical_contract_v1.json), verified structurally against `PRAGMA table_info` in `tests/datastore_contract.rs::logical_schema_inventory_is_structurally_valid_and_matches_runtime_core` and `tests/single_datastore.rs::all_subsystems_share_single_db_with_reconciled_table_inventory`.
   - Target table counts are NOT artificially squeezed by reintroducing competing datastores or merging unrelated relational tables (`storage_components`, `task_tags`, `obligations`); logical parity across local SQLite and Propodus PostgreSQL remains defined by [logical_contract_v1.json](logical_contract_v1.json) and `core::cloud_parity_matrix`.
