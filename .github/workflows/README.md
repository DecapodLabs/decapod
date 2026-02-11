# Decapod Workflows

This directory contains the GitHub Actions workflows for Decapod.

- `ci.yml`: Runs on every push and PR to `main`. Performs formatting checks, linting (clippy), and unit tests.
- `release.yml`: Runs on tags matching `v*.*.*`. Builds release binaries for Linux, macOS, and Windows, and creates a GitHub Release with attached artifacts and checksums.
- `publish-cratesio.yml`: Runs on tags matching `v*.*.*`. Publishes the crate to crates.io. Requires `CARGO_REGISTRY_TOKEN` secret.
