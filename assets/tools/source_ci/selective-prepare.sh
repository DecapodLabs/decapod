#!/usr/bin/env bash
# Reviewed compile-once selective core/CLI test preparation.
# Outputs a bounded 80-byte archive descriptor (`selective-archive.v1`) so
# custody ZIPs bind the exact tarball without duplicating ~400 MB per worker.
set -euo pipefail
umask 077
[[ $# == 0 ]]
: "${RUNNER_TEMP:?}"

rustc --edition=2024 "${SOURCE_CI_EXECUTOR_ROOT:-.}/assets/tools/integration_shards.rs" -o "$RUNNER_TEMP/integration-shards"
rm -rf "$RUNNER_TEMP/selective-tests"
"$RUNNER_TEMP/integration-shards" prepare-selective --output "$RUNNER_TEMP/selective-tests"

mkdir -p target/selective-artifacts target/selective-custody-artifacts
tar -C "$RUNNER_TEMP/selective-tests" -cf target/selective-artifacts/selective-tests.tar .
sha256_line="$(sha256sum target/selective-artifacts/selective-tests.tar)"
digest="${sha256_line%% *}"
bytes="$(wc -c < target/selective-artifacts/selective-tests.tar | tr -d ' ')"
[[ "$digest" =~ ^[0-9a-f]{64}$ ]]
[[ "$bytes" =~ ^[0-9]+$ && "$bytes" -gt 0 ]]
printf '%s  %s\n' "$digest" "$bytes" > target/selective-custody-artifacts/selective-archive.v1
