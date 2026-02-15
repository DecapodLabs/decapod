# Contributing to Decapod

Decapod is a minimal kernel with an expansive plugin ecosystem. We prioritize contributions that extend the system via plugins while keeping the core stable and lightweight.

## New Contributor Start Here

Run this sequence before making changes:

```bash
decapod --version
decapod update
decapod --help
decapod docs show core/DECAPOD.md
decapod validate
decapod todo list
```

If any step fails, stop and fix that before editing code.

## Core Architecture and Command Structure

### Runtime Layers

- `src/lib.rs`: CLI entrypoint and command routing.
- `src/core/`: shared runtime services (store, schemas, docs loader, validation, broker, scaffold).
- `src/plugins/`: subsystem implementations (todo, health, watcher, knowledge, teammate, etc.).
- `constitution/`: embedded constitutional docs (specs, interfaces, methodology, plugins).
- `.decapod/data/`: runtime state (SQLite + event logs), always mutated through CLI/broker.

### How Commands Are Organized

Top-level command groups:

- `init`, `setup`
- `docs`
- `todo`
- `validate`
- `update`, `version`
- `govern`
- `data`
- `auto`
- `qa`

Subsystem behavior should be exposed as `decapod <group> <subcommand>`, not ad hoc top-level commands.

### Where to Add What

- New governance/verification behavior: `src/plugins/health.rs`, `src/plugins/watcher.rs`, or `src/plugins/policy.rs`
- New work-tracking behavior: `src/plugins/todo.rs`
- New persisted state: `src/core/schemas.rs` plus migration path
- New constitutional contract/guidance: `constitution/interfaces/*` or `constitution/methodology/*`
- New command wiring: `src/lib.rs`

## Core vs Plugins
- **Core:** The central Rust kernel. Changes here must be minimal, highly optimized, and focused on state integrity. We rarely accept new features in core unless they are fundamental to orchestration.
- **Plugins:** Connectors, adapters, and harnesses. This is where 90% of development happens. We want you shipping these.

## Plugin Contribution Guidelines
To contribute a new plugin:
1. **Rust Code:** Implement the plugin functionality in Rust code within the `crates/` directory.
2. **Constitution File:** Create a dedicated constitution file for your plugin in the `constitution/plugins/` directory, detailing its purpose, authority, and scope.
3. **Registry Listing:** Add your plugin to the Subsystem Registry in `constitution/core/PLUGINS.md`, including its `Name`, `Status`, `Truth`, `Owner Doc`, `Store`, `Mutability`, `Proof Surface`, and `Safety Gates`.

## Submission Rules
1. **Use Templates:** Every issue and PR must use the provided templates. Missing fields result in immediate closure.
2. **Triage Signature:** By submitting, you acknowledge that incomplete or non-reproducible reports will be closed without discussion.
3. **Logs are Required:** No screenshots. Paste 30-200 lines of logs showing failure context.
4. **Idempotency:** All plugins must be idempotent. Re-running a plugin against the same state should be safe and predictable.

## Integration Simplicity Guidelines

When integrating external systems, prefer the simplest reliable shape first.

1. Start with local files or static artifacts (`.json`, `.csv`, `.md`) before network APIs.
2. Prefer API keys or environment variables over OAuth flows unless delegated user identity is required.
3. Use deterministic, replayable ingestion paths (same input should produce same stored state).
4. Provide an offline/dev-mode path where possible for testing.
5. Keep connector scope narrow: one source, one transformation, one write path.
6. Avoid introducing background daemons when a scheduled CLI command is sufficient.
7. Treat OAuth as opt-in complexity, not the default.

Decision rule:
- If a feature can be shipped with local files or a simple authenticated API, do that first.
- Escalate to OAuth/multi-service orchestration only when requirements explicitly demand it.

## Local Development

### Cargo.lock Management
Decapod uses `--locked` builds for reproducibility. **CI auto-updates Cargo.lock** if it's stale and pushes it back to your branch.

**After changing dependencies in Cargo.toml:**
```bash
cargo update
git add Cargo.lock
git commit -m "chore: update Cargo.lock"
```

**You can skip this** - CI will handle it automatically. But doing it locally is faster.

### Build and Test
```bash
cargo build --locked
cargo test --locked
cargo clippy --all-targets --all-features --locked
decapod validate  # Must pass all checks
export DECAPOD_LOG=debug  # For debugging
```

## PR Expectations
- Small, atomic PRs preferred (<500 LOC).
- Plugins must include a minimal smoke test or proof surface.
- Core changes require updated unit/integration tests.
