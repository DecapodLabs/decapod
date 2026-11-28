#!/usr/bin/env bash
# Execute one compiled selective core/CLI shard from the authenticated tarball.
set -euo pipefail
umask 077
[[ $# == 0 ]]
: "${RUNNER_TEMP:?}" "${DECAPOD_SELECTIVE_TARGETS:?}"

archive="target/selective-download/selective-tests.tar"
[[ -f "$archive" ]]
out="$RUNNER_TEMP/selective-tests"
rm -rf "$out"
mkdir -p "$out"
tar -xf "$archive" -C "$out"
exec "$out/integration-shards" run-selective --artifact "$out" --targets "$DECAPOD_SELECTIVE_TARGETS"
