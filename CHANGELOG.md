# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
