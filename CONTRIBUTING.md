# Contributing to Decapod

Decapod is a minimal kernel with an expansive plugin ecosystem. We prioritize contributions that extend the system via plugins while keeping the core stable and lightweight.

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

## Local Development

### Git Hooks Setup
Install git hooks to enforce code quality:
```bash
./hooks/install.sh
```

**Pre-push hook:** Verifies `Cargo.lock` is up to date before pushing.

### Cargo.lock Management
Decapod uses `--locked` builds for reproducibility:
- ✅ **Always commit `Cargo.lock`** after `cargo update`
- ✅ **Run `cargo update`** after changing dependencies
- ✅ **CI fails** if lock file is stale
- ✅ **Pre-push hook blocks** stale lock files

**After changing dependencies:**
```bash
cargo update
git add Cargo.lock
git commit -m "chore: update Cargo.lock"
```

### Build and Test
```bash
cargo build --locked
cargo test --locked
cargo clippy --all-targets --all-features --locked
decapod validate  # Must pass 29/29 checks
export DECAPOD_LOG=debug  # For debugging
```

## PR Expectations
- Small, atomic PRs preferred (<300 LOC).
- Plugins must include a minimal smoke test or proof surface.
- Core changes require updated unit/integration tests.
