# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
