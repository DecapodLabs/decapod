# Contributing to Decapod

Decapod is a governed agent control plane. Contributions are accepted when they increase enforcement value with minimal surface area.

## Non-Negotiable PR Rules

Every PR MUST include:

- Intent: what invariant or behavior is being changed.
- Invariants affected: explicit list.
- Proof added: test/gate/command that enforces the change.

"No vibes PRs": assertion-only changes with no enforcement path are rejectable.

## Project Tooling Standard

Project tooling is Rust. Runtime code lives under `src/` (`src/decapod/`, `src/main.rs`); non-runtime Rust lives under `assets/`:

- `assets/build/` — build-time Rust (build scripts, codegen).
- `assets/benches/` — benchmark Rust.
- `assets/tools/` — maintenance and release-ops Rust, exposed as additional `[[bin]]` targets in `Cargo.toml` and invoked via `cargo run --bin <name>`. Keep each tool deterministic: fixed inputs to fixed outputs so a workflow or future validate extension can call it without surprises.

Python/Bash one-offs are not the contribution path. If a one-off became necessary, raise it in an issue first so it can be folded into a `assets/tools/` Rust tool rather than committed as a stray script.

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

Canonical Bazel and Rust toolchain configuration lives under `.config/build/`.
The root `BUILD.bazel`, `MODULE.bazel`, and `.bazelrc` files are intentionally
small Bazel discovery shims; `MODULE.bazel.lock` remains at the repository root
because Bazel generates and discovers that lockfile there. The root
`rust-toolchain.toml` is a rustup-compatible symlink to `.config/build/`.

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

### Build and test cleanup

The normal Bazel commands above clean their action-owned sandbox trees before
returning. Sandbox-directory reuse is disabled; compiled outputs, action caches,
download caches, and declared test logs remain available. `--sandbox_debug`
intentionally retains Bazel sandboxes when investigating a failure.

Bazel tests automatically run through the Rust `//:build_lifecycle` supervisor.
Each test process gets a unique temporary directory under Bazel's `TEST_TMPDIR`.
The supervisor waits for the child and removes only that invocation's scratch on
success, failure, or handled Unix SIGHUP/SIGINT/SIGTERM. Tests must write evidence
that should survive cleanup to Bazel's declared test-output locations, not TMPDIR.
Source files, governance evidence, sibling runs, and build output/cache trees are
never cleanup targets. If child shutdown cannot be established or the scratch
root was replaced, cleanup fails closed and reports the retained path. SIGKILL
and machine crashes cannot run cleanup; no later invocation sweeps old or
unowned directories.
Build/test children must join their descendants before returning. Cancellation
is forwarded to the immediate child's process group; a descendant that creates
another group must handle its own shutdown and join. Detached descendants are
outside this portable lifecycle contract. This helper is not a general
process/container reaper and cannot guarantee cleanup after a runner's hard kill.

For Cargo, use the repository lifecycle aliases:

```bash
cargo build-main --locked
cargo test-main --locked --lib
cargo test-main --locked --test entrypoint_correctness

# Verify the cleanup supervisor itself
cargo test --locked --features build-tools --bin decapod-build
bazelisk test //:build_lifecycle_tests
```

These aliases run the Rust tool in `assets/tools/build_lifecycle.rs`, then invoke
Cargo with owned scratch beneath the checkout's `target/`. Plain `cargo build`
and `cargo test` remain supported but do not use the scratch supervisor. The
optional `build-tools` feature exposes `decapod-build`; normal installation still
installs only the runtime binary.

Cargo development, test, and install profiles disable incremental compilation
by default to avoid accumulating per-worktree compiler scratch. Compiled
dependencies remain cached. For a deliberate incremental edit loop, opt in with
`CARGO_INCREMENTAL=1 cargo test-main ...`. Existing Cargo incremental directories
are not deleted by a build: they may belong to another active process. No build
or test action invokes `bazel clean`, `cargo clean`, or an output-base expunge.

### Nix Development Shell

If you are using Nix, you can enter a fully reproducible development shell containing all the required tooling by running:

```bash
nix develop
```

You can also build Decapod using Nix:

```bash
nix build
```

For more details, see the **[Contributing Guidelines](docs/book/src/contributing.md)** section in the mdBook.

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
cargo deny --config .config/deny.toml check

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

### 4. Interactive Consistency Lints (`.config/clippy.toml`)
We configure Clippy via `.config/clippy.toml` to disallow raw standard library APIs (like blocking sockets) in favor of async libraries or project-wide connection pools. `.cargo/config.toml` exports `CLIPPY_CONF_DIR=.config` so standard `cargo clippy` checks this policy automatically.

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

## Before You Open a PR

> [!IMPORTANT]
> **GitHub Workflow Permissions (`workflow` scope)**
> 
> If you are contributing changes that trigger GitHub Actions workflows or edit files under `.github/workflows/`, your Personal Access Token (PAT) must have the `workflow` scope.
> 
> To verify if your token has the appropriate permission level, run the following API call:
> ```bash
> curl -I -H "Authorization: Bearer YOUR_GITHUB_TOKEN" https://api.github.com/user
> ```
> Inspect the **`X-OAuth-Scopes`** header in the response. It must contain the `workflow` scope (for example: `X-OAuth-Scopes: repo, workflow`).

Beyond the rules above, every PR must satisfy Decapod's governance gates and CI's artifact expectations:

- Work in an isolated worktree: `decapod workspace ensure` after claiming a todo. Do not push directly to `master`.
- `decapod validate` must pass. If it reports `entrypoint_release_mismatch`, regenerate the governed entrypoints (`AGENTS.md`, `CLAUDE.md`, `CODEX.md`, `GEMINI.md`) with the installed Decapod release and re-run; the entrypoint pin and the binary must agree before validate can pass.
- The `governance-artifacts` CI job verifies `.decapod/governance.json` and each authored commit's exact-material checkpoint, including merges. A document at HEAD alone is insufficient.
- Use `govern artifacts begin-pr --id <change> --base-branch master` for the explicit boundary, then existing plan and trajectory commands to record the work. Before committing, stage authored changes, record a unique `govern artifacts checkpoint --id <id> --summary "..." --proof-ref <evidence>`, stage governance.json, and commit.
- Accepted completed claims remain in Git history; explicit unresolved obligations carry forward until resolved with proof. Runtime workunit manifests live under ignored `.decapod/data/workunits/`.

## Architecture Boundary

- Keep core deterministic and minimal.
- Prefer plugin/local shim extension over core expansion.
- Do not bypass Decapod command surfaces to mutate `.decapod` state.
- Keep project tooling Rust-native; non-Rust one-offs belong in issue discussion, not in the tree.
