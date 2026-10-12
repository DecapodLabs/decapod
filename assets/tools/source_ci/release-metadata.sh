#!/usr/bin/env bash
# Fixed source-owned CI recipe. The measured profile binds these exact bytes.
set -euo pipefail
umask 077
export CHANGE_EVENT="${SOURCE_CI_EVENT:?}"
export CHANGE_BASE="${SOURCE_CI_BASE?}"
export CHANGE_HEAD="${SOURCE_CI_HEAD:?}"
set -euo pipefail
rustc --edition=2024 assets/tools/release_metadata.rs -o "$RUNNER_TEMP/decapod-release-metadata"
"$RUNNER_TEMP/decapod-release-metadata" \
  --repo . --event "$CHANGE_EVENT" --base "$CHANGE_BASE" --head "$CHANGE_HEAD" \
  | tee -a "$GITHUB_OUTPUT"
