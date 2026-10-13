# Contributing to Decapod

Keep each change focused, explain the behavior it should improve, and include
evidence reviewers can inspect. Your coding agent handles the engineering
workflow; you describe the change, then review and merge the PR.

## Non-Negotiable PR Rules

Every PR needs a clear account of:

- What is changing and why
- Which behavior or contract the change affects
- The tests or other evidence supporting it

## Project Tooling Standard

Use Rust for runtime code and new project tooling. Runtime code lives in
`src/decapod/` and `src/main.rs`.

- `assets/build/` contains build-time tools and code generation
- `assets/benches/` contains benchmarks
- `assets/tools/` contains maintenance and release tools declared in `Cargo.toml`

Keep tools deterministic. Discuss a proposed Python or shell tool in an issue
before adding it.

## Invariant-Touching Changes

Changes to contracts, schemas, or publication rules need a test or gate that
checks the changed behavior. Update CLI contract tests for CLI changes, golden
vectors and tests for RPC changes, and publication checks for provenance or
promotion changes.

## Versioning Policy

- Schema changes require a version bump
- Breaking CLI or RPC changes require a major version bump
- Breaking golden-vector changes require a major version bump

## Local Dev

Start with the [README quick start](README.md#quick-start), then describe the
contribution to your coding agent. The [agent contract](AGENTS.md) covers task
ownership, isolated workspaces, project specifications, and validation. The
agent follows that workflow and brings unresolved decisions back to you.

Bazel through Bazelisk is the primary build and test system. Cargo and Nix are
also supported. Build configuration lives in [`.config/build/`](.config/build/);
Cargo's supervised build and test aliases are in
[`.cargo/config.toml`](.cargo/config.toml), and the Nix environment is defined in
[`flake.nix`](flake.nix).

### Build and test cleanup

The build lifecycle supervisor gives each invocation its own temporary directory
and cleans up that directory after a normal exit or handled cancellation. Keep
test evidence in declared output locations. Caches and other runs' files are not
cleanup targets. Hard kills and machine crashes can leave scratch files behind;
the supervisor does not sweep unrelated directories.

### Nix Development Shell

The Nix flake provides a development shell and package build. See the
[Nix reference](docs/book/src/reference/nix.md) for platform and packaging details.

## Recommended Workflow Tooling

The project supports coverage analysis, dependency auditing, unused-dependency
checks, Clippy, and faster test runners. Dependency policy lives in
[`.config/deny.toml`](.config/deny.toml), and lint configuration lives in
[`.config/clippy.toml`](.config/clippy.toml). Avoid introducing a new tool when the
existing checks cover the work.

## Release Discipline

Release changes must pass the release checks before merge. The agent owns that
verification and records its evidence with the PR.

## Before You Open a PR

The agent prepares a reviewable change:

- Work in a claimed, isolated workspace and leave `master` unchanged
- Update the relevant authored specifications along with the implementation
- Complete validation, repair supported failures, and identify anything still blocked
- Include governance records and an exact-material checkpoint for every authored commit, including merges
- Keep the release-bound entrypoints consistent with the installed Decapod release

The agent runs validation once the source and required checkpoint are ready.
Validation runs the configured build prerequisites and tests. It must pass for
the proposed change against its target branch before the agent opens a PR.

The [agent API](docs/agent/api-index.md) and
[governance artifact reference](docs/book/src/reference/governance-artifacts.md)
describe the required records. Do not put access tokens in the PR or expand token
permissions as a routine setup step.

## Architecture Boundary

Keep core behavior deterministic and small. Prefer an existing extension point
over adding another core responsibility. Use Decapod's supported commands to
update governed state, and keep new project tooling Rust-native.
