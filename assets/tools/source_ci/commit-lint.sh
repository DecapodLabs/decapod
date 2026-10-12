#!/usr/bin/env bash
# Fixed source-owned CI recipe. The measured profile binds these exact bytes.
set -euo pipefail
umask 077
export CHANGE_EVENT="${SOURCE_CI_EVENT:?}"
export CHANGE_BASE="${SOURCE_CI_MERGE_BASE:?}"
export CHANGE_HEAD="${SOURCE_CI_HEAD:?}"
set -euo pipefail
git --no-replace-objects log --format=%B "$CHANGE_BASE..$CHANGE_HEAD" | while read -r msg; do
  [ -z "$msg" ] && continue
  if echo "$msg" | grep -qiE "^(wip|fixup|squash)"; then
    echo "❌ ERROR: WIP commit found: $msg"
    exit 1
  fi
done
echo "✅ Commit messages look good"
