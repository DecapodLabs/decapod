#!/usr/bin/env bash
# Fixed source-owned shard status aggregator gate (#1672).
# Requires every prerequisite and matrix shard to have succeeded; never treats
# skipped, cancelled, or failed shards as green.
set -euo pipefail
umask 077

pre_build="${CUSTODY_PREPARE_RESULT:?Missing CUSTODY_PREPARE_RESULT}"
shards="${CUSTODY_WORKER_RESULT:?Missing CUSTODY_WORKER_RESULT}"

if [[ "$pre_build" != "success" ]]; then
  echo "::error::Shard aggregator blocked: pre-build-validate result was '${pre_build}' (expected 'success')." >&2
  exit 1
fi
if [[ "$shards" != "success" ]]; then
  echo "::error::Shard aggregator failed: shard matrix result was '${shards}' (expected 'success')." >&2
  exit 1
fi

echo "PASS: all required shards completed with result=success."
