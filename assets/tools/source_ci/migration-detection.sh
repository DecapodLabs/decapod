#!/usr/bin/env bash
# Fixed source-owned CI recipe. The measured profile binds these exact bytes.
set -euo pipefail
umask 077
export CHANGE_EVENT="${SOURCE_CI_EVENT:?}"
export CHANGE_BASE="${SOURCE_CI_BASE?}"
export CHANGE_HEAD="${SOURCE_CI_HEAD:?}"
set -euo pipefail
"${SOURCE_CI_PROFILE_EXECUTABLE:-$RUNNER_TEMP/source-ci-profile}" migration-changed --base "$CHANGE_BASE" --head "$CHANGE_HEAD" >> "$GITHUB_OUTPUT"
