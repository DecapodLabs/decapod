#!/usr/bin/env bash
# Fixed source-owned CI recipe, preserving source step scope and exit propagation.
set -euo pipefail
umask 077
source_root="$(git rev-parse --show-toplevel)"
source "$source_root/assets/tools/source_ci/private-checkout.sh"
source_private_checkout
cd "$SOURCE_PRIVATE_CHECKOUT"

# Build Decapod from checkout
(
cargo build --locked --quiet --features build-tools --bin decapod --bin decapod-release-projection-drift
)

# Start validation trajectory
(
export DECAPOD_VALIDATE_SKIP_GIT_GATES="1"
set -euo pipefail
run_id="ci-decapod-validate-${GITHUB_RUN_ID:-${BUILDKITE_BUILD_ID:-local}}"
./target/debug/decapod govern trajectory init \
  --run-id "$run_id" \
  --original-intent "Validate the checked-out Decapod repository" \
  --derived-intent "Run the bounded repository validation gates and bind the receipt to this CI run" \
  --boundary "Repository validation and tracked proof artifacts" \
  --scope ".decapod/governance.json" \
  --destination "CI validation" \
  --phase validation \
  --next-transition publish
)

# Check project prerequisites (readiness is aggregated separately)
(
export CHANGE_EVENT="${SOURCE_CI_EVENT:?}"
export CHANGE_BASE="${SOURCE_CI_BASE?}"
export CHANGE_HEAD="${SOURCE_CI_HEAD:?}"
export DECAPOD_VALIDATE_REQUIRE_CHANGE_CONTEXT="1"
export DECAPOD_VALIDATE_SKIP_GIT_GATES="1"
set -euo pipefail
if [ ! -d .decapod ]; then
  ./target/debug/decapod init --proof
fi
./target/debug/decapod govern project-check --refresh-specs
)

# Ensure no spec or entrypoint drift
(
export RELEASE_ONLY="${SOURCE_CI_RELEASE_ONLY:-${RELEASE_ONLY:-false}}"
export CHANGE_EVENT="${SOURCE_CI_EVENT:?}"
export CHANGE_BASE="${SOURCE_CI_BASE?}"
export CHANGE_HEAD="${SOURCE_CI_HEAD:?}"
set -euo pipefail
if [ "$RELEASE_ONLY" = "true" ]; then
  ./target/debug/decapod-release-projection-drift \
    --repo . --event "$CHANGE_EVENT" --base "$CHANGE_BASE" --head "$CHANGE_HEAD"
  exit 0
fi
MANIFEST=".decapod/managed/specs/.manifest.json"
if git cat-file -e "HEAD:${MANIFEST}" 2>/dev/null && [ -f "${MANIFEST}" ]; then
  OLD=$(git show "HEAD:${MANIFEST}" | sed -n 's/.*"generated_at": "\([^"]*\)".*/\1/p' | head -1)
  if [ -n "${OLD}" ]; then
    sed -i "s/\"generated_at\": \"[^\"]*\"/\"generated_at\": \"${OLD}\"/" "${MANIFEST}"
  fi
  OLD_SIGNAL=$(git show "HEAD:${MANIFEST}" | sed -n 's/.*"repo_signal_fingerprint": "\([^"]*\)".*/\1/p' | head -1)
  if [ -n "${OLD_SIGNAL}" ]; then
    sed -i "s/\"repo_signal_fingerprint\": \"[^\"]*\"/\"repo_signal_fingerprint\": \"${OLD_SIGNAL}\"/" "${MANIFEST}"
  fi
fi
if ! git diff --cached --exit-code HEAD -- . || \
   ! git diff --exit-code HEAD -- . ':!.decapod/governance.json'; then
  echo "::error::Release-bound or living-spec drift after validate --refresh-specs."
  echo "Commit regenerated entrypoints and .decapod/managed/specs from the evaluating binary."
  exit 1
fi
)
mkdir -p "$source_root/target/source-ci-standalone"
cp target/debug/decapod target/debug/decapod-release-projection-drift "$source_root/target/source-ci-standalone/"
cp .decapod/governance.json "$source_root/target/source-ci-standalone/governance.json"
