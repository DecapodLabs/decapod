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

Decapod uses Bazel (coordinated via Bazelisk) as its primary build and test system.

To build, test, and run validation locally, you can use the following commands:

```bash
# Build the decapod binary
bazelisk build //:decapod

# Run all tests
bazelisk test //:core_tests

# Run a specific test
bazelisk test //:entrypoint_correctness

# Initialize and validate decapod locally
bazel run //:decapod -- init --proof
bazel run //:decapod -- validate
```

If you do not have Bazelisk installed, you can install it via your package manager (e.g., `npm install -g @bazel/bazelisk`, `brew install bazelisk`, etc.).


## Release Discipline

Before release PR merge:

```bash
decapod release check
```

## Architecture Boundary

- Keep core deterministic and minimal.
- Prefer plugin/local shim extension over core expansion.
- Do not bypass Decapod command surfaces to mutate `.decapod` state.
