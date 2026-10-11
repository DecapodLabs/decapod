#!/usr/bin/env bash
set -euo pipefail

# Keep runtime shell expansion out of the workflow command embedded in the
# Buildkite upload. The imported workflow command is interpolated while the
# pipeline is uploaded; this script is read by the agent only when the step
# runs.
event_name="${GITHUB_EVENT_NAME:-${BUILDKITE_GITHUB_EVENT:-unknown}}"
ref_name="${GITHUB_REF_NAME:-${BUILDKITE_TAG:-${BUILDKITE_BRANCH:-}}}"

# The imported Buildkite job does not reliably expose a usable event or PR
# marker at runtime. A tag is the only context that authorizes cargo-dist
# hosting; every other invocation is a non-publishing plan.
if [ -n "${BUILDKITE_TAG:-}" ] || [ "${GITHUB_REF_TYPE:-}" = "tag" ]; then
  mode="host"
else
  mode="plan"
fi
echo "cargo-dist mode=$mode event=$event_name ref=${ref_name:-unknown}"

# The installer runs in the preceding workflow step. GitHub-hosted runners
# already expose this directory on PATH, but the Buildkite compatibility
# runtime starts each step in a fresh non-login shell. Restore the installer
# location explicitly so the plan step does not depend on shell-profile
# mutation crossing the step boundary.
cargo_dist_bin="${CARGO_HOME:-${HOME}/.cargo}/bin"
if [ -x "$cargo_dist_bin/cargo-dist" ]; then
  PATH="$cargo_dist_bin:$PATH"
  export PATH
fi
if ! command -v cargo-dist >/dev/null 2>&1; then
  echo "cargo-dist is unavailable; expected the pinned installer at $cargo_dist_bin/cargo-dist" >&2
  exit 1
fi

if [ "$mode" = "plan" ]; then
  cargo dist plan --output-format=json > plan-dist-manifest.json
else
  : "${ref_name:?Unable to determine the ref for cargo-dist hosting}"
  cargo dist host --steps=create --tag="$ref_name" --output-format=json > plan-dist-manifest.json
fi

echo "cargo dist ran successfully"
cat plan-dist-manifest.json

if [ "$mode" = "plan" ]; then
  tag=""
  tag_flag=""
  publishing="false"
else
  tag="$ref_name"
  tag_flag="--tag=$ref_name"
  publishing="true"
fi

: "${GITHUB_OUTPUT:?GITHUB_OUTPUT must be provided by the workflow runtime}"
# A job output is limited to 1 KiB in Buildkite. The complete manifest remains
# a file/artifact; only these bounded scheduling decisions cross job boundaries.
# Assign before echoing so malformed/empty output fails instead of hiding jq's
# exit status inside a successful echo command. False decisions are valid.
plan_outputs="$(jq --raw-output --slurp '
  if length != 1 or (.[0] | type) != "object" then
    error("cargo-dist must emit exactly one manifest object")
  else
    .[0] |
    "has_local_artifacts=\(.ci.github.artifacts_matrix.include != null)",
    "pr_upload=\(.ci.github.pr_run_mode == "upload")"
  end
' plan-dist-manifest.json)"
{
  printf '%s\n' "$plan_outputs"
  echo "tag=${tag}"
  echo "tag_flag=${tag_flag}"
  echo "publishing=${publishing}"
} >> "$GITHUB_OUTPUT"
