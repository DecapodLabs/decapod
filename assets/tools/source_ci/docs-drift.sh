#!/usr/bin/env bash
# Fixed source-owned CI recipe. The measured profile binds these exact bytes.
set -euo pipefail
umask 077
export BASE_SHA="${SOURCE_CI_BASE?}" HEAD_SHA="${SOURCE_CI_HEAD:?}"

# Select the captured documentation source head
set -euo pipefail
rustc --edition=2024 assets/tools/docs_sync.rs -o "$RUNNER_TEMP/decapod-docs-sync"
"$RUNNER_TEMP/decapod-docs-sync" select --repo . --base "$BASE_SHA" --head "$HEAD_SHA"


# Build Decapod
(
export CARGO_NET_GIT_FETCH_WITH_CLI=true GIT_TERMINAL_PROMPT=0 CARGO_TARGET_DIR=target
cargo build --all-features
)

# Verify generated documentation has no drift
set -euo pipefail
"$RUNNER_TEMP/decapod-docs-sync" verify --repo . --base "$BASE_SHA" --head "$HEAD_SHA" --decapod target/debug/decapod
