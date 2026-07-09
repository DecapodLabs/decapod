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

## Recommended Workflow Tooling

While Bazel is used for CI/CD and large-scale tests, local iteration is supported via standard Cargo tooling. The following tools are recommended for code quality, dependency integrity, and developer productivity:

### 1. Code Coverage (`cargo-llvm-cov`)
Use LLVM source-based instrumentation to generate precise line and region coverage reports:
```bash
cargo install cargo-llvm-cov
cargo llvm-cov --html
```

### 2. Dependency Audit & Safety (`cargo-deny` & `cargo-audit`)
Audit dependencies for duplicate versions, unapproved licenses, and known vulnerabilities:
```bash
# Custom dependency lints (duplicate check, license limits)
cargo install cargo-deny --locked
cargo deny check

# Security vulnerability checks
cargo install cargo-audit --locked
cargo audit
```

### 3. Clean dependencies (`cargo-machete` & `cargo-udeps`)
Find and remove unused dependencies:
```bash
# Fast heuristic checker
cargo install cargo-machete
cargo machete

# Nightly compiler-accurate checker
cargo install cargo-udeps --locked
cargo +nightly udeps
```

### 4. Interactive Consistency Lints (`clippy.toml`)
We configure Clippy via `clippy.toml` to disallow raw standard library APIs (like blocking sockets) in favor of async libraries or project-wide connection pools. Running standard `cargo clippy` will check this automatically.

### 5. Fast Test Loop (`cargo-nextest` & `cargo-watch`)
Speed up testing cycles with a modern test runner and auto-check on file changes:
```bash
# Run tests in parallel in separate processes
cargo install cargo-nextest --locked
cargo nextest run

# Watch code for saves and auto-check
cargo install cargo-watch
cargo watch -x check
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
