# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- schema/interface: expand canonical project specs set to include `SEMANTICS.md`, `OPERATIONS.md`, and `SECURITY.md`
- docs/scaffold: scaffold-v2 project specs with adaptive topology/sequence diagrams, richer interface contracts, validation decision flow, and security/operations directives

### Changed

- validate: add architecture runtime/deployment section enforcement and conditional structure checks for semantics/operations/security specs
- docs/specs: upgrade Decapod's checked-in generated specs to industry-grade operational/security semantics with explicit proof surfaces

## [0.44.6](https://github.com/DecapodLabs/decapod/compare/v0.44.5...v0.44.6) - 2026-02-25

### Fixed

- update branch task ID extraction to support task_type_ prefix
- fix tests: use task_type prefix instead of deprecated R_ prefix
- replace legacy R_ ID prefix with test_ in test files
- formatting
- clippy in test files
- formatting and clippy
- clippy warnings
- handle missing task in test_todo_state_machine gracefully
- optimize spec_conformance tests and todo list default to open
- optimize spec_conformance tests and todo list default to open

### Other

- optimize with rustc-hash and inline hints
- update Rust version to 1.90 and edition to 2026
- 10 parallel shards for test execution with --test-threads=4
- run all integration tests in single runner with high parallelism

## [0.44.5](https://github.com/DecapodLabs/decapod/compare/v0.44.4...v0.44.5) - 2026-02-25

### Added

- Interlock - preflight + impact for predictive governance

### Fixed

- make preflight/impact work without worktree, skip git gates in tests
- test setup with worktree
- test error handling

### Other

- fmt

## [0.44.4](https://github.com/DecapodLabs/decapod/compare/v0.44.3...v0.44.4) - 2026-02-25

### Added

- Interlock - preflight + impact for predictive governance

## [0.44.3](https://github.com/DecapodLabs/decapod/compare/v0.44.2...v0.44.3) - 2026-02-25

### Fixed

- allowlist kcr trend artifact and hard-shard integration tests

### Other

- add generated decapod specs and fix interop evidence test paths

## [0.44.2](https://github.com/DecapodLabs/decapod/compare/v0.44.1...v0.44.2) - 2026-02-25

### Other

- consolidate repo - remove stale templates/, docs/, project/, crates/, artifacts/ dirs

## [0.44.1](https://github.com/DecapodLabs/decapod/compare/v0.44.0...v0.44.1) - 2026-02-25

### Added

- add skills section to .decapod/README.md template

### Other

- run cargo fmt
- move skill cards from .decapod/governance/skills to .decapod/skills

## [0.44.0](https://github.com/DecapodLabs/decapod/compare/v0.43.1...v0.44.0) - 2026-02-25

### Added

- move broker sockets to data and add noninteractive spec seeds
- add release lineage-sync command
- auto-stamp release provenance policy lineage
- require lineage consistency across release manifests
- enforce release provenance policy lineage
- auto-bind written capsules into workunit state refs
- require workunit state_ref binding for capsule lineage
- enforce capsule policy lineage at workunit promotion gate
- policy-bound deterministic context capsule issuance

### Fixed

- reconcile merged capsule schema and lineage artifacts

### Other

- run cargo fmt after rebase
- fail release check on lineage capsule drift
- harden capsule verification and RPC fail-closed coverage
- pin JIT capsule policy verification flow

## [0.43.1](https://github.com/DecapodLabs/decapod/compare/v0.43.0...v0.43.1) - 2026-02-25

### Added

- add allowed_next_ops to context.resolve RPC response

## [0.43.0](https://github.com/DecapodLabs/decapod/compare/v0.42.1...v0.43.0) - 2026-02-25

### Added

- add migration for one_shot column
- add one_shot column to TODO schema

### Fixed

- update migration sequence test to expect 400
- add one_shot field to test TodoCommand::Add initializers

### Other

- run cargo fmt
- run cargo fmt
- bump integration test runners from 8 to 10

## [0.42.1](https://github.com/DecapodLabs/decapod/compare/v0.42.0...v0.42.1) - 2026-02-25

### Added

- add meta-skills for agent-decapod and human-agent interaction

## [0.42.0](https://github.com/DecapodLabs/decapod/compare/v0.41.2...v0.42.0) - 2026-02-25

### Added

- *(init)* dynamic scaffolds and validate-driven living spec tasks
- *(init)* upgrade generated specs templates for day-0 onboarding

### Fixed

- enforce strict broker bypass and skip user-store schema gate

### Other

- split hot integration shards 3 and 5 into 7 and 8
- rename integration shard labels to 1..6
- Merge remote-tracking branch 'origin/master' into agent/unknown/todo-r_01kj8n80zyk4qamd4v8g4bpfe0-1771964773
- enforce versioned db schema checks and ordered migrations
- scale plan metadata and catalog for long histories
- run migration-script tests only when sql migrations change
- add version-gated ledger and generated version counter
- scope worktree and branch naming by todo hash
- auto-upgrade legacy todo ids on activate/startup
- typed ids with hash ledger and refresh decapod readme template
- normalize .decapod layout and enforce startup workspace flow
- split slow integration shards 3 and 4 into subshards
- stabilize group broker test and relocate project support dirs

### Other

- todo: migrate task IDs to typed format `<type4>_<16-alnum>`, add task `hash` field, and align workspace scope checks
- migration: add startup SQL-backed todo ID rewrite for legacy stores (DB + events log) on first post-upgrade activation
- cli: add `decapod activate` as explicit first-run activation surface to trigger migrations/bootstrap
- migration: add version-gated migration registry (`min_version` + `target_version`) with applied ledger at `.decapod/generated/migrations/applied.json`
- init: seed `.decapod/generated/version_counter.json` and track binary-version transitions for safe migration orchestration
- ci: add PR-only migration script test gate that runs migration tests only when `src/core/sql/*.sql` changes
- migration: add sequence/scope metadata, duplicate/order guards, and generated migration catalog for long-horizon schema evolution
- validate: add database schema version gate to verify versioned DBs match this decapod binary expectations
- schema/interface: update TODO schema contract with typed ID and hash invariants
- docs/scaffold: refresh `.decapod/README.md` template with Decapod harness positioning and canonical control-plane layout

## [0.41.2](https://github.com/DecapodLabs/decapod/compare/v0.41.1...v0.41.2) - 2026-02-24

### Added

- *(broker)* enforce strict must-route guard for routed mutators
- *(broker)* add request-id dedupe ledger and recovery retry semantics
- *(broker)* add ephemeral CLI-boundary group broker runtime

### Other

- fix clippy useless_vec in broker concurrency env setup
- *(broker)* isolate session state per worker to avoid concurrent parse race
- *(broker)* harden concurrent mutator gate with idempotent retry
- format strict broker guard test
- shard integration tests across 4 runners and fix lint regressions
- *(broker)* add protocol mismatch and phase crash injection proof harness
- *(broker)* add phase hooks and crash-safe lease recovery primitives
- *(broker)* add concurrency and dedupe proof tests
- *(sqlite)* tighten routed mutator boundaries and brokered init paths
- *(broker)* allow ephemeral local cross-process socket mode

## [0.41.1](https://github.com/DecapodLabs/decapod/compare/v0.41.0...v0.41.1) - 2026-02-24

### Other

- *(intent)* enforce intent->context->spec flow across runtime and constitution
- *(readme)* encode intent->context->spec flow in hero line
- *(init)* elevate interactive form to product-grade prompt flow
- *(init)* polish interactive setup prompts and summary

## [0.41.0](https://github.com/DecapodLabs/decapod/compare/v0.40.1...v0.41.0) - 2026-02-23

### Added

- *(init)* move project specs to generated path and deepen architecture scaffold

### Fixed

- *(readme)* link constitution note to core DECAPOD doc
- *(readme)* correct constitution directory link
- *(readme)* route constitution via docs show and sync sha
- *(release)* sync README sha manifest to pinned readme
- *(readme)* route constitution access via docs show
- *(readme)* route constitution access through decapod docs show
- *(release)* refresh README artifact manifest hash
- *(validate)* allow tracked generated specs artifacts and normalize specs naming

### Other

- *(entrypoints)* allow README link to core constitution doc
- *(readme)* restore pinned readme wording
- *(readme)* restore README from eaa291ea
- Update README.md
- *(specs)* standardize generated specs paths to uppercase filenames

## [0.40.1](https://github.com/DecapodLabs/decapod/compare/v0.40.0...v0.40.1) - 2026-02-23

### Other

- bump Cargo.toml

## [0.40.0](https://github.com/DecapodLabs/decapod/compare/v0.39.0...v0.40.0) - 2026-02-23

### Other

- consolidate unreleased changelog for init/specs kernel rollout
- hardcode canonical local specs contract and runtime mapping
- separate intent purpose from architecture direction
- enforce intent-first scaffolding and config anchors
- infer repo context into config and add interactive init with mode
- scaffold project specs docs with diagram style and architecture gate

### Other

- init: add `decapod init with` (alias `wtih`) and `.decapod/config.toml` schema-backed repo context
- init: infer repo purpose/architecture signals from prominent files (`README.md`, manifests, repo surfaces)
- init: scaffold canonical local project specs set (`specs/README.md`, `intent.md`, `architecture.md`, `interfaces.md`, `validation.md`)
- init: support diagram style selection (`ascii` or `mermaid`) for architecture topology generation
- init: seed local specs content from inferred/config context and enforce intent-purpose vs architecture-direction separation
- validate: add project config + canonical local specs gates, including placeholder rejection for intent/architecture content
- runtime: surface canonical local specs context and constitution mapping from `decapod rpc --op context.resolve`

### Other

- schema/interface: add `interfaces/PROJECT_SPECS.md` and register claim `claim.project_specs.canonical_set_enforced`
- schema/interface: bind hardcoded local specs registry in binary to constitution dependencies and control-plane sequencing docs

## [0.39.0](https://github.com/DecapodLabs/decapod/compare/v0.38.13...v0.39.0) - 2026-02-23

### Other

- remove teammate legacy aliases; keep aptitude as sole subsystem name
- rename teammate subsystem to aptitude and elevate README feature list
- Merge master and resolve AGENTS entrypoint conflicts

### Other

- schema/interface: rename aptitude subsystem across CLI/docs/contracts as canonical memory surface

## [0.38.13](https://github.com/DecapodLabs/decapod/compare/v0.38.12...v0.38.13) - 2026-02-23

### Other

- Merge master into PR branch and resolve README/gitignore conflicts
- templatize decapod gitignore policy and surface shared skills memory

### Other

- schema/interface: merge eval + gitignore governance surface updates and keep deterministic whitelist enforcement in init/validate

## [0.38.12](https://github.com/DecapodLabs/decapod/compare/v0.38.11...v0.38.12) - 2026-02-23

### Other

- Refresh README hash in artifact manifest after feature-line split
- Refine feature descriptions in README
- Resolve CHANGELOG merge conflict with master release entries
- Add eval kernel to README feature checklist
- Add eval governance kernel and tighten decapod artifact allowlists

### Other

- schema/interface: add eval governance kernel interfaces, claims, and deterministic proof tests

## [0.38.11](https://github.com/DecapodLabs/decapod/compare/v0.38.10...v0.38.11) - 2026-02-23

### Other

- Tighten generated artifact whitelist and simplify gitignore
- Enforce generated artifact whitelist in init and validate
- Keep generated Dockerfile tracked while ignoring runtime outputs
- Ignore .decapod/generated runtime artifacts

## [0.38.10](https://github.com/DecapodLabs/decapod/compare/v0.38.9...v0.38.10) - 2026-02-23

### Other

- Merge pull request #412 from DecapodLabs/agent/unknown/todo-r_01kj562ftvxcpe8def7gqb8vgb-plus-3-1771863796
- Harden sqlite contention paths and stabilize validate/release checks

## [0.38.9](https://github.com/DecapodLabs/decapod/compare/v0.38.8...v0.38.9) - 2026-02-23

### Other

- add top feature checklist with interfaces pointer

## [0.38.8](https://github.com/DecapodLabs/decapod/compare/v0.38.7...v0.38.8) - 2026-02-23

### Other

- add phase 4 regression and daemonless lifecycle gates

## [0.38.7](https://github.com/DecapodLabs/decapod/compare/v0.38.6...v0.38.7) - 2026-02-23

### Other

- enforce promotion firewall for procedural knowledge writes

## [0.38.6](https://github.com/DecapodLabs/decapod/compare/v0.38.5...v0.38.6) - 2026-02-23

### Fixed

- fix clippy filter-next lint in knowledge promotion test

### Other

- add knowledge promotion firewall ledger command

## [0.38.5](https://github.com/DecapodLabs/decapod/compare/v0.38.4...v0.38.5) - 2026-02-23

### Other

- split fmt clippy and core integration test jobs
- stabilize context capsule CLI tests under CI gates
- add rpc context capsule query operation
- add optional context capsule artifact writes
- add deterministic context capsule query command

## [0.38.4](https://github.com/DecapodLabs/decapod/compare/v0.38.3...v0.38.4) - 2026-02-23

### Other

- enforce workunit verified gate before publish
- *(workunit)* enforce proof recording and status transitions
- *(workunit)* add attach-spec/state and proof-plan primitives

## [0.38.3](https://github.com/DecapodLabs/decapod/compare/v0.38.2...v0.38.3) - 2026-02-23

### Other

- *(workunit)* add init/get/status manifest commands
- add optional artifact integrity gates for new kernel schemas
- *(schema)* add deterministic workunit and context capsule models

## [0.38.2](https://github.com/DecapodLabs/decapod/compare/v0.38.1...v0.38.2) - 2026-02-23

### Other

- *(contract)* pin phase-0 kernel interfaces and embed new docs

## [0.38.1](https://github.com/DecapodLabs/decapod/compare/v0.38.0...v0.38.1) - 2026-02-23

### Other

- Harden daemonless validate startup and lock resilience

## [0.38.0](https://github.com/DecapodLabs/decapod/compare/v0.37.11...v0.38.0) - 2026-02-23

### Added

- *(context)* add scoped constitution query via docs search and rpc

### Other

- *(release)* refresh README hash in artifact manifest
- *(entrypoints)* use file-specific headers for agent files
- move agent guidance out of source comments
- split human README from agent entrypoint contracts

## [0.37.11](https://github.com/DecapodLabs/decapod/compare/v0.37.10...v0.37.11) - 2026-02-23

### Fixed

- *(broker)* replace per-DB mutex with SqlitePool for read/write separation
- *(release)* update README.md sha256 in artifact manifest

### Other

- Merge pull request #383 from DecapodLabs/fix/sqlite-pool-contention
- run rustfmt for sqlite pool contention changes
- update crate description and categories

## [0.37.10](https://github.com/DecapodLabs/decapod/compare/v0.37.9...v0.37.10) - 2026-02-23

### Other

- *(readme)* remove agent-ops line and neutralize research callout

## [0.37.9](https://github.com/DecapodLabs/decapod/compare/v0.37.8...v0.37.9) - 2026-02-22

### Other

- bump OVERRIDE

## [0.37.8](https://github.com/DecapodLabs/decapod/compare/v0.37.7...v0.37.8) - 2026-02-22

### Other

- Merge pull request #377 from DecapodLabs/agent/unknown/todo-r_01kj2c415q0y14c6e1fzncdtjd-1771753844
- Stabilize chaos replay under concurrent todo add contention
- Fix constitution path validators for architecture directives
- Keep architecture directives constitution-only
- Add architecture foundations artifact gate for governed execution

## [0.37.7](https://github.com/DecapodLabs/decapod/compare/v0.37.6...v0.37.7) - 2026-02-22

### Other

- Merge pull request #374 from DecapodLabs/agent/unknown/todo-r_01kj23h4vkwzjs7mze87xkv737-plus-1-1771744859
- stabilize plan-governed test harness under canonical worktree gate
- remove literal non-canonical path examples from agent entrypoints
- relax canonical-worktree checks for validate and negative path mentions
- enforce canonical decapod workspaces and startup sequence

## [0.37.6](https://github.com/DecapodLabs/decapod/compare/v0.37.5...v0.37.6) - 2026-02-22

### Added

- beautify CLI output for init and validate commands

### Fixed

- ignore broken verify_mvp test (fails on master)

## [0.37.5](https://github.com/DecapodLabs/decapod/compare/v0.37.4...v0.37.5) - 2026-02-22

### Other

- Trim runtime deps by replacing chrono/colored/which

### Added

- *(interfaces)* add plan-governed execution contract and typed pushback markers

### Changed

- *(governance)* enforce plan approval/proof-hook readiness in execute/publish/validate paths

## [0.37.4](https://github.com/DecapodLabs/decapod/compare/v0.37.3...v0.37.4) - 2026-02-22

### Added

- *(release)* ship 5 one-shot governance gates for intent convergence

### Other

- Merge pull request #362 from DecapodLabs/agent/unknown/r-01kj1n50qe34xkvq6c30s28np9

## [0.37.3](https://github.com/DecapodLabs/decapod/compare/v0.37.2...v0.37.3) - 2026-02-22

### Added

- auto-acquire session in ensure_session_valid (entrypoint funnel)

## [0.37.2](https://github.com/DecapodLabs/decapod/compare/v0.37.1...v0.37.2) - 2026-02-22

### Other

- Rename LEVIE_GOVERNANCE_AUDIT.md to GOVERNANCE_AUDIT.md
- Update and rename docs/LEVIE_GOVERNANCE_AUDIT.md to constitution/docs/GOVERNANCE_AUDIT.md

## [0.37.1](https://github.com/DecapodLabs/decapod/compare/v0.37.0...v0.37.1) - 2026-02-22

### Other

- *(audit)* map Levie capability buckets to kernel primitives

## [0.37.0](https://github.com/DecapodLabs/decapod/compare/v0.36.7...v0.37.0) - 2026-02-22

### Other

- Merge pull request #354 from DecapodLabs/agent/unknown/rustify-init-docker-1771726265

## [0.36.7](https://github.com/DecapodLabs/decapod/compare/v0.36.6...v0.36.7) - 2026-02-22

### Added

- add intent refinement requirement for agents

### Fixed

- sync templates with AGENTS.md golden rules

## [0.36.6](https://github.com/DecapodLabs/decapod/compare/v0.36.5...v0.36.6) - 2026-02-22

### Other

- *(container)* model Dockerfile template as schema component

## [0.36.5](https://github.com/DecapodLabs/decapod/compare/v0.36.4...v0.36.5) - 2026-02-22

### Other

- *(constitution)* tighten foundation demands and liveness contract

## [0.36.4](https://github.com/DecapodLabs/decapod/compare/v0.36.3...v0.36.4) - 2026-02-21

### Added

- *(validate)* enforce commit-often dirty file limit

### Other

- Merge pull request #347 from DecapodLabs/agent/unknown/commit-often-mandate-1771715984
- *(validate)* add commit-often gate integration coverage

## [0.36.3](https://github.com/DecapodLabs/decapod/compare/v0.36.2...v0.36.3) - 2026-02-21

### Other

- Merge remote-tracking branch 'origin/master' into agent/unknown/entrypoint-constitution-docs-1771714881

## [0.36.2](https://github.com/DecapodLabs/decapod/compare/v0.36.1...v0.36.2) - 2026-02-21

### Added

- add map and lcm events to flight-recorder timeline
- add worktree exemption for schema commands
- add safe validate diagnostics and contention gate

### Other

- Merge pull request #341 from DecapodLabs/agent/unknown/validate-diagnostics-dedicated-1771713199
- update todo.md with completed items
- Merge branch 'master' into agent/unknown/validate-diagnostics-dedicated-1771713199
- enforce validate diagnostics sanitization

## [0.36.1](https://github.com/DecapodLabs/decapod/compare/v0.36.0...v0.36.1) - 2026-02-21

### Added

- prune stale worktree config sections routinely

### Other

- Merge pull request #342 from DecapodLabs/agent/unknown/worktree-config-cleanup-1771713742

## [0.36.0](https://github.com/DecapodLabs/decapod/compare/v0.35.8...v0.36.0) - 2026-02-21

### Added

- wire LCM/Map into capabilities, schema, and add rebuild command

### Fixed

- KCR trend - all enforced claims have gate mappings (KCR=1.0)
- fmt, clippy, and update KCR trend for new LCM claims

### Other

- Merge pull request #339 from DecapodLabs/feat/lcm-work

## [0.35.8](https://github.com/DecapodLabs/decapod/compare/v0.35.7...v0.35.8) - 2026-02-21

### Added

- add safe validate diagnostics and contention gate

## [0.35.7](https://github.com/DecapodLabs/decapod/compare/v0.35.6...v0.35.7) - 2026-02-21

### Added

- implement Phase 3 LCM + Map operators

## [0.35.6](https://github.com/DecapodLabs/decapod/compare/v0.35.5...v0.35.6) - 2026-02-21

### Other

- Rename PLAYBOOK.md to docs/PLAYBOOK.md

## [0.35.5](https://github.com/DecapodLabs/decapod/compare/v0.35.4...v0.35.5) - 2026-02-21

### Other

- Remove top-level non-essential docs and purge non-Rust shim code

## [0.35.4](https://github.com/DecapodLabs/decapod/compare/v0.35.3...v0.35.4) - 2026-02-21

### Added

- add coplayer policy tightening gate + instruction stack hardening

### Fixed

- update artifact manifest SHA256 for README.md

## [0.35.3](https://github.com/DecapodLabs/decapod/compare/v0.35.2...v0.35.3) - 2026-02-21

### Other

- Speed up RPC suite and split CI test load
- Harden validate lock handling and RPC suite contention retries

## [0.35.2](https://github.com/DecapodLabs/decapod/compare/v0.35.1...v0.35.2) - 2026-02-21

### Added

- enforce provenance manifest validity in release check
- harden control-plane contracts and bound validate termination

### Fixed

- keep CLAUDE template in sync with root entrypoint
- *(ci)* raise health validate timeout and refresh KCR trend
- satisfy CLAUDE line gate and self-heal knowledge schema in validate

### Other

- drop non-rust SDK shims and keep interop rust-native

## [0.35.1](https://github.com/DecapodLabs/decapod/compare/v0.35.0...v0.35.1) - 2026-02-21

### Added

- harden control-plane contracts and bound validate termination

### Fixed

- keep CLAUDE template in sync with root entrypoint
- *(ci)* raise health validate timeout and refresh KCR trend
- satisfy CLAUDE line gate and self-heal knowledge schema in validate

## [0.35.0](https://github.com/DecapodLabs/decapod/compare/v0.34.0...v0.35.0) - 2026-02-21

### Other

- remove unused code and deprecated modules

## [0.34.0](https://github.com/DecapodLabs/decapod/compare/v0.33.0...v0.34.0) - 2026-02-21

### Other

- remove unused code and add tests

## [0.33.0](https://github.com/DecapodLabs/decapod/compare/v0.32.4...v0.33.0) - 2026-02-21

### Added

- add secret redaction, gatekeeper CLI, and doctor preflight checks

### Other

- Merge pull request #315 from DecapodLabs/feat/oneshot-batch

## [0.32.4](https://github.com/DecapodLabs/decapod/compare/v0.32.3...v0.32.4) - 2026-02-20

### Other

- remove plankton bash hooks - keep multi-language validation

## [0.32.3](https://github.com/DecapodLabs/decapod/compare/v0.32.2...v0.32.3) - 2026-02-20

### Added

- add Dockerfile template that explodes to .decapod/generated/

## [0.32.2](https://github.com/DecapodLabs/decapod/compare/v0.32.1...v0.32.2) - 2026-02-20

### Added

- add multi-language tooling gates and config protection to validation

## [0.32.1](https://github.com/DecapodLabs/decapod/compare/v0.32.0...v0.32.1) - 2026-02-20

### Added

- integrate Plankton write-time enforcement into Decapod

## [0.32.0](https://github.com/DecapodLabs/decapod/compare/v0.31.1...v0.32.0) - 2026-02-20

### Other

- added to ai category
- Fix clippy warnings and simplify lineage validation
- Add ObligationNode test suite
- Phase 1: Enforce derived completion in ObligationNode

## [0.31.1](https://github.com/DecapodLabs/decapod/compare/v0.31.0...v0.31.1) - 2026-02-20

### Added

- *(core)* implement ObligationNode governance-native primitive

### Other

- Merge pull request #303 from DecapodLabs/feat/R_01KHY5A2HF1F8P50FQZB2HBC2A/obligation-primitive
- Add architecture memo: filesystem task abstraction decision

## [0.31.0](https://github.com/DecapodLabs/decapod/compare/v0.30.0...v0.31.0) - 2026-02-20

### Fixed

- *(verify)* strip elapsed timing from validate output before hashing
- revert schema determinism parallelization to avoid shared state conflicts

### Other

- fix fmt and clippy warnings, fix test compilation
- *(validate)* add --verbose timing and parallelize expensive gates

## [0.30.0](https://github.com/DecapodLabs/decapod/compare/v0.29.6...v0.30.0) - 2026-02-20

### Added

- *(core)* implement gatekeeper safety gates and co-player inference

### Other

- fix formatting in coplayer and gatekeeper

## [0.29.6](https://github.com/DecapodLabs/decapod/compare/v0.29.5...v0.29.6) - 2026-02-20

### Fixed

- speed up validation

### Other

- add governance kernel architecture review (codex_analysis.md)

## [0.29.5](https://github.com/DecapodLabs/decapod/compare/v0.29.4...v0.29.5) - 2026-02-20

### Added

- improve trace/docs integration and validation workflows

### Fixed

- guard knowledge migration against concurrent table creation race

### Other

- Merge branch 'master' into agent/unknown/task-1771394863

## [0.29.4](https://github.com/DecapodLabs/decapod/compare/v0.29.3...v0.29.4) - 2026-02-19

### Other

- update CHANGELOG with packaging fix

### Fixed

- *(packaging)* add missing symlink target and exclude test fixtures from crate

## [0.29.3](https://github.com/DecapodLabs/decapod/compare/v0.29.2...v0.29.3) - 2026-02-19

### Added

- *(state_commit)* implement STATE_COMMIT v1 protocol

## [0.29.2](https://github.com/DecapodLabs/decapod/compare/v0.29.1...v0.29.2) - 2026-02-19

### Added

- *(claims)* add KCR evidence gate test and trend baseline

### Fixed

- use rfind instead of filter().next_back() for clippy

## [0.29.1](https://github.com/DecapodLabs/decapod/compare/v0.29.0...v0.29.1) - 2026-02-19

### Added

- *(broker,flight-recorder)* crash consistency and governance timeline

## [0.29.0](https://github.com/DecapodLabs/decapod/compare/v0.28.12...v0.29.0) - 2026-02-19

### Added

- harvest knowledge lifecycle, broker audit, health cleanup, and CI health from stale branches

### Fixed

- *(ci)* skip git worktree gates in CI health job
- *(tests)* resolve agent_rpc_suite flake and chaos_replay IOERR
- *(federation)* eliminate drift window and downgrade determinism gates

## [0.28.12](https://github.com/DecapodLabs/decapod/compare/v0.28.11...v0.28.12) - 2026-02-18

### Fixed

- *(workspace)* implement publish and wire --container flag for constitution parity

## [0.28.11](https://github.com/DecapodLabs/decapod/compare/v0.28.10...v0.28.11) - 2026-02-18

### Other

- Fix typo in README.md

## [0.28.10](https://github.com/DecapodLabs/decapod/compare/v0.28.9...v0.28.10) - 2026-02-18

### Other

- Update README with constitution info and typo fix
- *(readme)* add research links, proof-gate example, context philosophy

## [0.28.9](https://github.com/DecapodLabs/decapod/compare/v0.28.8...v0.28.9) - 2026-02-18

### Other

- *(readme)* add research links, proof-gate example, validate output

## [0.28.8](https://github.com/DecapodLabs/decapod/compare/v0.28.7...v0.28.8) - 2026-02-18

### Fixed

- *(clippy)* resolve denied lint violations
- *(tests)* acquire session before validate in rpc suite
- *(validate)* require session password before worktree gate

### Other

- *(fmt)* apply rustfmt-normalized ordering and wrapping

## [0.28.7](https://github.com/DecapodLabs/decapod/compare/v0.28.6...v0.28.7) - 2026-02-18

### Fixed

- *(ci)* restore session-first gating and thin-file threshold alignment
- *(ci)* stabilize rpc suite and ensure schema init on startup
- *(tests)* harden schema bootstrap and parallel trace assertions
- resolve -D warnings failures blocking tests

### Other

- Merge branch 'master' into agent/codex/r-01khqw3kvtbtpzmchtq7s9azmn
- *(gitignore)* ignore generated awareness artifacts
- *(constitution)* add testing and ci/cd methodology guides

## [0.28.6](https://github.com/DecapodLabs/decapod/compare/v0.28.5...v0.28.6) - 2026-02-18

### Other

- *(release)* add manual dispatch mode for release-pr

## [0.28.5](https://github.com/DecapodLabs/decapod/compare/v0.28.4...v0.28.5) - 2026-02-18

### Other

- *(readme)* add Ko-fi callout, emoji polish, and linked file refs

## [0.28.4](https://github.com/DecapodLabs/decapod/compare/v0.28.3...v0.28.4) - 2026-02-18

### Other

- Enforce constitutional bootstrap and todo-scoped worktrees

## [0.28.3](https://github.com/DecapodLabs/decapod/compare/v0.28.2...v0.28.3) - 2026-02-18

### Added

- enforce strict agent dependency and automated initialization

## [0.28.2](https://github.com/DecapodLabs/decapod/compare/v0.28.1...v0.28.2) - 2026-02-18

### Added

- implement mandatory todo enforcement for agents

## [0.28.1](https://github.com/DecapodLabs/decapod/compare/v0.28.0...v0.28.1) - 2026-02-18

### Added

- enforce worktree path and add to .gitignore

## [0.28.0](https://github.com/DecapodLabs/decapod/compare/v0.27.0...v0.28.0) - 2026-02-18

### Added

- implement on-demand container sandboxing for worktrees
- enable agent-invoked git worktrees and isolation mandates

## [0.27.0](https://github.com/DecapodLabs/decapod/compare/v0.26.3...v0.27.0) - 2026-02-18

### Added

- promote todo to core control plane

## [0.26.3](https://github.com/DecapodLabs/decapod/compare/v0.26.2...v0.26.3) - 2026-02-18

### Added

- automate database normalization and entrypoint blending

## [0.26.2](https://github.com/DecapodLabs/decapod/compare/v0.26.1...v0.26.2) - 2026-02-18

### Added

- consolidate fragmented sqlite databases into 4 core bins

## [0.26.1](https://github.com/DecapodLabs/decapod/compare/v0.26.0...v0.26.1) - 2026-02-18

### Added

- implement local trace sink and binding transparency

## [0.26.0](https://github.com/DecapodLabs/decapod/compare/v0.25.5...v0.26.0) - 2026-02-17

### Added

- implement deterministic agent-facing RPC interface

## [0.25.5](https://github.com/DecapodLabs/decapod/compare/v0.25.4...v0.25.5) - 2026-02-17

### Other

- fresh .decapod init

## [0.25.4](https://github.com/DecapodLabs/decapod/compare/v0.25.3...v0.25.4) - 2026-02-17

### Other

- *(init)* make init instant by deferring DB setup to runtime

## [0.25.3](https://github.com/DecapodLabs/decapod/compare/v0.25.2...v0.25.3) - 2026-02-17

### Other

- *(readme)* clarify platform-agnostic operating model
- *(readme)* add high-level ascii architecture model
- *(readme)* sharpen positioning and differentiate value
- *(readme)* hint assurance model and capability surface
- *(readme)* remove demo gif and tighten public positioning

## [0.25.2](https://github.com/DecapodLabs/decapod/compare/v0.25.1...v0.25.2) - 2026-02-17

### Added

- *(init)* bootstrap schema-only stores and enforce workspace isolation

### Other

- release v0.25.1

## [0.25.1](https://github.com/DecapodLabs/decapod/compare/v0.25.0...v0.25.1) - 2026-02-17

### Added

- *(init)* bootstrap schema-only stores and enforce workspace isolation

## [0.25.0](https://github.com/DecapodLabs/decapod/compare/v0.24.0...v0.25.0) - 2026-02-17

### Added

- *(governance)* add weights and balances enforcement

### Other

- remove health check job
- add DECAPOD_SESSION_PASSWORD env var for health check
- add DECAPOD_CONTAINER=1 for GitHub Actions health check

## [0.24.0](https://github.com/DecapodLabs/decapod/compare/v0.23.10...v0.24.0) - 2026-02-17

### Other

- *(release)* set release-plz allow_dirty to boolean
- *(release)* allow runtime session dirt in release-plz

## [0.23.10](https://github.com/DecapodLabs/decapod/compare/v0.23.9...v0.23.10) - 2026-02-17

### Other

- README.md and lingering file catchup

## [0.23.9](https://github.com/DecapodLabs/decapod/compare/v0.23.8...v0.23.9) - 2026-02-17

### Other

- ignore session files in .decapod/generated/sessions/
- route policy.rs DB access through DbBroker

## [0.23.8](https://github.com/DecapodLabs/decapod/compare/v0.23.7...v0.23.8) - 2026-02-17

### Fixed

- add agent.session.cleanup event handler in todo rebuild

## [0.23.7](https://github.com/DecapodLabs/decapod/compare/v0.23.6...v0.23.7) - 2026-02-17

### Other

- automated container updates

## [0.23.6](https://github.com/DecapodLabs/decapod/compare/v0.23.5...v0.23.6) - 2026-02-17

### Other

- Add demo image to README

## [0.23.5](https://github.com/DecapodLabs/decapod/compare/v0.23.4...v0.23.5) - 2026-02-17

### Other

- verify docker workspace

## [0.23.4](https://github.com/DecapodLabs/decapod/compare/v0.23.3...v0.23.4) - 2026-02-17

### Added

- persist worktrees, auto-push branch, create PR after container

### Fixed

- remove needless as_deref calls

## [0.23.3](https://github.com/DecapodLabs/decapod/compare/v0.23.2...v0.23.3) - 2026-02-17

### Added

- code factory
- code factory

## [0.23.2](https://github.com/DecapodLabs/decapod/compare/v0.23.1...v0.23.2) - 2026-02-17

### Added

- code factory
- code factory
- code factory
- code factory
- code factory
- code factory
- code factory
- code factory
- code factory
- code factory

## [0.23.1](https://github.com/DecapodLabs/decapod/compare/v0.23.0...v0.23.1) - 2026-02-17

### Added

- workspace enhancements

## [0.23.0](https://github.com/DecapodLabs/decapod/compare/v0.22.0...v0.23.0) - 2026-02-16

### Other

- Change output filename and clean up commands
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- *(demo)* refresh decapod VHS GIF with local build
- README uplift
- rebake vhs demo in /tmp/studio
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift
- README uplift

## [0.22.0](https://github.com/DecapodLabs/decapod/compare/v0.21.0...v0.22.0) - 2026-02-16

### Added

- gitainers fixes
- gitainers fixes
- gitainers fixes
- gitainers fixes
- gitainers fixes
- gitainers fixes
- gitainers fixes

### Other

- sync container plugin state
- move readiness docs to dev/ (force track) and remove docs dir
- *(readiness)* record final ship decision with timestamp and provenance
- *(readiness)* finalize production-readiness package and proof gate

## [0.21.0](https://github.com/DecapodLabs/decapod/compare/v0.20.0...v0.21.0) - 2026-02-16

### Added

- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- gitainers
- require gh auth for automated PR creation
- gitainers
- gitainers
- gitainers

## [0.20.0](https://github.com/DecapodLabs/decapod/compare/v0.19.5...v0.20.0) - 2026-02-16

### Added

- x
- x
- x
- x
- x
- x
- x
- x
- x
- x
- x
- x
- x

### Other

- Merge branch 'master' into ahr/auto-schema-migrate

## [0.19.5](https://github.com/DecapodLabs/decapod/compare/v0.19.4...v0.19.5) - 2026-02-16

### Added

- gitainer envs

## [0.19.4](https://github.com/DecapodLabs/decapod/compare/v0.19.3...v0.19.4) - 2026-02-16

### Added

- gitainer envs
- gitainer envs
- gitainer envs

## [0.19.3](https://github.com/DecapodLabs/decapod/compare/v0.19.2...v0.19.3) - 2026-02-16

### Added

- task dependencies

## [0.19.2](https://github.com/DecapodLabs/decapod/compare/v0.19.1...v0.19.2) - 2026-02-16

### Added

- task dependencies
- task dependencies
- task dependencies
- task dependencies

## [0.19.1](https://github.com/DecapodLabs/decapod/compare/v0.19.0...v0.19.1) - 2026-02-16

### Added

- x

## [0.19.0](https://github.com/DecapodLabs/decapod/compare/v0.18.0...v0.19.0) - 2026-02-16

### Added

- x
- x
- x

## [0.18.0](https://github.com/DecapodLabs/decapod/compare/v0.17.0...v0.18.0) - 2026-02-16

### Added

- autonomy lineage loop
- autonomy lineage loop

## [0.17.0](https://github.com/DecapodLabs/decapod/compare/v0.16.1...v0.17.0) - 2026-02-16

### Added

- reflex
- reflex
- reflex
- reflex
- reflex

## [0.16.1](https://github.com/DecapodLabs/decapod/compare/v0.16.0...v0.16.1) - 2026-02-16

### Added

- broker enhancements
- broker enhancements
- broker enhancements
- broker enhancements
- broker enhancements
- broker enhancements

### Other

- 60+ validation checks
- validation improvement
- Merge branch 'master' into ahr/control-plane-broker-risk-lineage

## [0.16.0](https://github.com/DecapodLabs/decapod/compare/v0.15.2...v0.16.0) - 2026-02-16

### Added

- *(control-plane)* stabilize broker envelope and add chaos replay gate

## [0.15.2](https://github.com/DecapodLabs/decapod/compare/v0.15.1...v0.15.2) - 2026-02-16

### Added

- human-in-the-loop
- human-in-the-loop

## [0.15.1](https://github.com/DecapodLabs/decapod/compare/v0.15.0...v0.15.1) - 2026-02-16

### Added

- todo trust grants
- todo trust grants

## [0.15.0](https://github.com/DecapodLabs/decapod/compare/v0.14.1...v0.15.0) - 2026-02-16

### Added

- better todo verification
- better todo verification
- better todo verification

## [0.14.1](https://github.com/DecapodLabs/decapod/compare/v0.14.0...v0.14.1) - 2026-02-16

### Added

- better testing

## [0.14.0](https://github.com/DecapodLabs/decapod/compare/v0.13.0...v0.14.0) - 2026-02-16

### Added

- constitutional control surface optimizations
- constitutional control surface optimizations
- constitutional control surface optimizations
- constitutional control surface optimizations
- constitutional control surface optimizations

## [0.13.0](https://github.com/DecapodLabs/decapod/compare/v0.12.1...v0.13.0) - 2026-02-15

### Added

- better updates

## [0.12.1](https://github.com/DecapodLabs/decapod/compare/v0.12.0...v0.12.1) - 2026-02-15

### Added

- decision queries
- decision queries
- decision queries
- decision queries
- decision queries
- decision queries
- decision queries
- decision queries

## [0.12.0](https://github.com/DecapodLabs/decapod/compare/v0.11.2...v0.12.0) - 2026-02-15

### Added

- additional fixes
- additional fixes
- additional fixes
- additional fixes
- additional fixes
- additional fixes
- additional fixes

### Other

- fix 429 crates.io backoff

## [0.11.2](https://github.com/DecapodLabs/decapod/compare/v0.11.1...v0.11.2) - 2026-02-15

### Other

- init clarification

## [0.11.1](https://github.com/DecapodLabs/decapod/compare/v0.11.0...v0.11.1) - 2026-02-15

### Other

- fmt

## [0.11.0](https://github.com/DecapodLabs/decapod/compare/v0.10.0...v0.11.0) - 2026-02-15

### Added

- *(todo)* implement multi-agent task ownership system (v0.10.0)

## [0.10.0](https://github.com/DecapodLabs/decapod/compare/v0.9.0...v0.10.0) - 2026-02-15

### Added

- todo and federation determinism

## [0.9.0](https://github.com/DecapodLabs/decapod/compare/v0.8.1...v0.9.0) - 2026-02-15

### Added

- federation
- federation

## [0.8.1](https://github.com/DecapodLabs/decapod/compare/v0.8.0...v0.8.1) - 2026-02-15

### Added

- knowledge graph
- knowledge graph

## [0.8.0](https://github.com/DecapodLabs/decapod/compare/v0.7.0...v0.8.0) - 2026-02-15

### Added

- multi-agent todo
- multi-agent todo schema

### Fixed

- *(schemas)* satisfy clippy doc comment spacing
- *(todo)* resolve CI fmt and duplicate type errors

### Other

- Merge branch 'master' into ahr/work

## [0.7.0](https://github.com/DecapodLabs/decapod/compare/v0.6.9...v0.7.0) - 2026-02-15

### Added

- multi-user schema
- mult-agent

## [0.6.9](https://github.com/DecapodLabs/decapod/compare/v0.6.8...v0.6.9) - 2026-02-15

### Other

- Contributing doc

## [0.6.8](https://github.com/DecapodLabs/decapod/compare/v0.6.7...v0.6.8) - 2026-02-15

### Added

- MEMORY + KNOWLEDGE refinement

## [0.6.7](https://github.com/DecapodLabs/decapod/compare/v0.6.6...v0.6.7) - 2026-02-15

### Added

- control surface opacity

## [0.6.6](https://github.com/DecapodLabs/decapod/compare/v0.6.5...v0.6.6) - 2026-02-15

### Added

- validation override for updates
- validation override for updates

## [0.6.5](https://github.com/DecapodLabs/decapod/compare/v0.6.4...v0.6.5) - 2026-02-15

### Added

- source code restructure
- constitution cleanup

### Other

- apply rustfmt module ordering

## [0.6.4](https://github.com/DecapodLabs/decapod/compare/v0.6.3...v0.6.4) - 2026-02-15

### Other

- entrypoint
- fix formatting in validate.rs
- entrypoint
- entrypoint

## [0.6.3](https://github.com/DecapodLabs/decapod/compare/v0.6.2...v0.6.3) - 2026-02-15

### Other

- improve release workflow to sync version file with Cargo.toml

## [0.6.2](https://github.com/DecapodLabs/decapod/compare/v0.6.1...v0.6.2) - 2026-02-14

### Other

- improve release workflow to sync version file with Cargo.toml

## [0.6.1](https://github.com/DecapodLabs/decapod/compare/v0.6.0...v0.6.1) - 2026-02-14

### Fixed

- clippy redundant closure warning

### Other

- fix import ordering for cargo fmt
- fix import ordering for CI formatting
- fix formatting and update release workflow
- finalizing versioning
- finalizing versioning
- finalizing versioning

## [0.6.0](https://github.com/DecapodLabs/decapod/compare/v0.5.2...v0.6.0) - 2026-02-14

### Other

- fixing versioning
- fixing versioning
- README
- README

## [0.5.2](https://github.com/DecapodLabs/decapod/compare/v0.5.1...v0.5.2) - 2026-02-14

### Other

- README
- REAME

## [0.5.1](https://github.com/DecapodLabs/decapod/compare/v0.5.0...v0.5.1) - 2026-02-14

### Added

- enhancements
- enhancements

## [0.5.0](https://github.com/DecapodLabs/decapod/compare/v0.4.0...v0.5.0) - 2026-02-14

### Added

- restructure constitution with proper architectural layers

### Fixed

- update validation to check for methodology/ARCHITECTURE.md instead of specs/ARCHITECTURE.md

## [0.4.0](https://github.com/DecapodLabs/decapod/compare/v0.3.3...v0.4.0) - 2026-02-14

### Added

- add `decapod qa gatling` command with native Rust test harness

### Other

- Merge pull request #110 from DecapodLabs/ahr/work
- fix rustfmt formatting in lib.rs
- add CLI gatling test and full regression audit

## [0.3.3](https://github.com/DecapodLabs/decapod/compare/v0.3.2...v0.3.3) - 2026-02-14

### Other

- stop managing CODEX.md in init cleanup lists

## [0.3.2](https://github.com/DecapodLabs/decapod/compare/v0.3.1...v0.3.2) - 2026-02-14

### Added

- **Task claiming and release**: New `decapod todo claim` and `decapod todo release` commands enable agents to claim tasks for active work, preventing coordination conflicts
- **Smart auto-assignment by category**: When creating tasks, system automatically assigns them to agents already working in the same category (inferred from title/tags)
- **Task assignment tracking**: Added `assigned_to` and `assigned_at` fields to task schema for visibility into who's working on what
- **Category-based agent routing**: Tasks are intelligently routed to the appropriate agent based on category affinity and existing work allocation

### Changed

- Bumped TODO schema version to 7 with automatic migration
- Enhanced task.add events with category inference and auto-assignment metadata
- Updated Task struct and all SQL queries to include assignment fields

## [0.3.1](https://github.com/DecapodLabs/decapod/compare/v0.3.0...v0.3.1) - 2026-02-14

### Other

- Merge pull request #101 from DecapodLabs/ahr/work

## [0.3.0](https://github.com/DecapodLabs/decapod/compare/v0.2.2...v0.3.0) - 2026-02-14

### Added

- consolidate CLI migration into grouped command architecture
- add summary and autonomy subcommands to health module

### Fixed

- update CI workflow to use new health summary command
- resolve CI regressions for verify fmt, schema test, and watcher command
- resolve clippy manual_map warning in verify.rs

### Other

- update README subsystems section with new CLI structure
- run cargo fmt for formatting consistency
- update constitution with new CLI command structure

## [0.2.2](https://github.com/DecapodLabs/decapod/compare/v0.2.1...v0.2.2) - 2026-02-14

### Added

- lock down entrypoint correctness and add verification subsystem

### Other

- run cargo fmt for formatting consistency

## [0.2.1](https://github.com/DecapodLabs/decapod/compare/v0.2.0...v0.2.1) - 2026-02-13

### Added

- deploy all 5 agent entrypoints and enforce 4 invariants
- rewrite agent entrypoints as engineering organization metaphor

### Fixed

- rewrite agent entrypoints as thin routing shims

### Other

- untrack generated entrypoint files
- run cargo fmt for consistent formatting

## [0.2.0](https://github.com/DecapodLabs/decapod/compare/v0.1.18...v0.2.0) - 2026-02-13

### Added

- [**breaking**] migrate to release-plz for automated releases
- autotag
- autotag
- autotag
- autotag
- autotag
- autotag
- autotag
- readme screenshots
- autotag
- autotag
- autotag
- autotag
- readme video
- readme video
- readme video
- readme video
- autotag
- ui, todo, etc
- readme video

### Fixed

- use GitHub App token for PR creation
- correct auto-tag and push-tag workflow sequence
- add push-tag for bump PR merge
- remove auto-PR creation, push branch+tag for manual merge

### Other

- reset version to 0.1.18 (last published on crates.io)
- reset version to 0.1.19 for release-plz
- update Cargo.lock for v0.1.19
- bump version to v0.1.19
- add GitHub App setup instructions for auto-tag workflow
- Update Cargo.toml
- bump version to v0.1.19
- Update Cargo.toml
- bump version to v0.1.19
- bump version to v0.1.19
- bump version to v0.1.19
- bump version to v0.1.19
