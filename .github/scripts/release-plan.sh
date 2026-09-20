#!/usr/bin/env bash
set -euo pipefail

# Keep runtime shell expansion out of the workflow command embedded in the
# Buildkite upload. The imported workflow command is interpolated while the
# pipeline is uploaded; this script is read by the agent only when the step
# runs.
event_name="${BUILDKITE_GITHUB_EVENT:-${GITHUB_EVENT_NAME:-}}"
if [ -z "$event_name" ] && [ -n "${BUILDKITE_PULL_REQUEST:-}" ] && [ "${BUILDKITE_PULL_REQUEST}" != "false" ]; then
  event_name="pull_request"
fi
: "${event_name:?Unable to determine the GitHub event for cargo-dist planning}"

if [ -n "${BUILDKITE_TAG:-}" ]; then
  ref_name="$BUILDKITE_TAG"
else
  ref_name="${GITHUB_REF_NAME:-${BUILDKITE_BRANCH:-}}"
fi

if [ "$event_name" = "pull_request" ]; then
  cargo dist plan --output-format=json > plan-dist-manifest.json
else
  : "${ref_name:?Unable to determine the ref for cargo-dist hosting}"
  cargo dist host --steps=create --tag="$ref_name" --output-format=json > plan-dist-manifest.json
fi

echo "cargo dist ran successfully"
cat plan-dist-manifest.json

if [ "$event_name" = "pull_request" ]; then
  tag=""
  tag_flag=""
  publishing="false"
else
  tag="$ref_name"
  tag_flag="--tag=$ref_name"
  publishing="true"
fi

: "${GITHUB_OUTPUT:?GITHUB_OUTPUT must be provided by the workflow runtime}"
{
  echo "manifest=$(jq -c . plan-dist-manifest.json)"
  echo "tag=${tag}"
  echo "tag_flag=${tag_flag}"
  echo "publishing=${publishing}"
} >> "$GITHUB_OUTPUT"
