#!/usr/bin/env bash
# One real selected shard and its receipt are a single failure-propagating
# invocation. There is no independently callable success-emission recipe.
set -euo pipefail
umask 077
[[ $# == 0 ]]
: "${RUNNER_TEMP:?}" "${CUSTODY_SHARD:?}"
case "$CUSTODY_SHARD" in
  gatling-[0-3]|release-policies|contract_conformance|constitution_claims|spec_conformance|substrate_integrity) ;;
  *) echo 'Unknown integration shard' >&2; exit 1 ;;
esac
# Authenticate bytes before extracting or executing the packed helper.
[[ "$CUSTODY_ARCHIVE_SHA256" =~ ^[0-9a-f]{64}$ ]]
printf '%s  %s\n' "$CUSTODY_ARCHIVE_SHA256" target/integration-download/integration-tests.tar | sha256sum --check --status
mkdir "$RUNNER_TEMP/integration-tests"
tar -xf target/integration-download/integration-tests.tar -C "$RUNNER_TEMP/integration-tests"
[[ "$CUSTODY_HELPER_SHA256" =~ ^[0-9a-f]{64}$ ]]
printf '%s  %s\n' "$CUSTODY_HELPER_SHA256" "$RUNNER_TEMP/integration-tests/integration-custody" | sha256sum --check --status
"$RUNNER_TEMP/integration-tests/integration-custody" verify \
  --artifact "$RUNNER_TEMP/integration-tests" \
  --archive target/integration-download/integration-tests.tar

umask 077
# Restore and receipt work have their own workflow overhead budget. Preserve
# the original five-minute budget around the actual selected test command.
# timeout failure, cancellation, or signal exits before any receipt is written.
# GNU timeout owns a separate process group. Relay workflow/executor cancellation
# to it while waiting, so cancelled recipes do not leave detached test work.
shard_pid=''
stop_shard() {
  if [[ -n "$shard_pid" ]]; then
    kill -TERM "$shard_pid" 2>/dev/null || true
    wait "$shard_pid" 2>/dev/null || true
  fi
}
trap stop_shard EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
timeout --signal=TERM --kill-after=30s 300s \
  "$RUNNER_TEMP/integration-tests/integration-shards" run \
  --artifact "$RUNNER_TEMP/integration-tests" \
  --shard "$CUSTODY_SHARD" &
shard_pid=$!
wait "$shard_pid"
shard_pid=''
trap - EXIT TERM INT
"$RUNNER_TEMP/integration-tests/integration-custody" record \
  --artifact "$RUNNER_TEMP/integration-tests" \
  --archive target/integration-download/integration-tests.tar \
  --shard "$CUSTODY_SHARD" --receipts target/integration-receipts
