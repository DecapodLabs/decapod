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

```bash
cargo build --locked
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
decapod validate
```

## Release Discipline

Before release PR merge:

```bash
decapod release check
```

## Architecture Boundary

- Keep core deterministic and minimal.
- Prefer plugin/local shim extension over core expansion.
- Do not bypass Decapod command surfaces to mutate `.decapod` state.
