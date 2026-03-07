# Contributing to Decapod

Decapod is a governed agent control plane. Contributions are accepted when they increase enforcement value with minimal surface area.

## Non-Negotiable PR Rules

Every PR MUST include:

- Intent: what invariant or behavior is being changed.
- Invariants affected: explicit list.
- Proof added: test/gate/command that enforces the change.

"No vibes PRs": assertion-only changes with no enforcement path are rejectable.

## Invariant-Touching Changes

If a change touches invariants, contracts, schema, or promotion logic, the PR MUST add or update at least one gate.

Examples:

- CLI contract changes -> CLI contract test updates.
- RPC envelope changes -> golden vectors update + tests.
- Promotion/provenance changes -> release/publish gate updates.

## Versioning Policy

- Schema changes require a version bump.
- Breaking CLI/RPC changes require a major bump.
- Golden vector breaking changes require a major bump.

## Local Dev

The `decapod` binary must remain independently buildable and runnable on ordinary machines. Nix is supported here only as an optional contributor shell for local testing, formatting, linting, and CI reproduction.

If you do not use Nix, normal Cargo workflows are still valid:

```bash
cargo build --locked
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
decapod validate
```

## Optional Nix Dev Shell

This repo includes a `flake.nix` for contributors who want a reproducible local toolchain. It is meant to remove machine-specific setup friction around Rust, `clippy`, `rustfmt`, `clang`, and `lld`.

What it is for:

- Reproducing CI locally.
- Getting the expected formatter/linter/test toolchain quickly.
- Avoiding host-specific linker/tooling drift.

What it is not for:

- Running the shipped `decapod` binary in production.
- Making Nix a runtime dependency of the project.
- Requiring contributors to adopt Nix just to use Decapod.

### Nix Crash Course

If you have Nix with flakes enabled, enter the shell with:

```bash
nix develop .#ci
```

That drops you into a shell with the repo's expected Rust toolchain and linker setup. From there, the normal commands work as usual:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features --test agent_rpc_suite -- --test-threads=1
decapod validate
```

If you prefer one-off commands without entering an interactive shell:

```bash
nix develop .#ci -c cargo fmt --all -- --check
nix develop .#ci -c cargo clippy --all-targets --all-features -- -D warnings
nix develop .#ci -c cargo test --all-features
```

### When To Use It

Use the flake when:

- `cargo fmt`, `clippy`, or tests fail because your host toolchain/linker differs from CI.
- You want to reproduce the Linux CI environment closely.
- You want a clean, disposable contributor toolchain.

Skip it when:

- Your local Rust setup already works.
- You are only consuming the `decapod` binary.
- You do not want Nix on your machine.

## Release Discipline

Before release PR merge:

```bash
decapod release check
```

## Architecture Boundary

- Keep core deterministic and minimal.
- Prefer plugin/local shim extension over core expansion.
- Do not bypass Decapod command surfaces to mutate `.decapod` state.
