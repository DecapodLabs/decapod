#!/bin/sh
MSG_FILE="$1"
SUBJECT="$(head -n1 "$MSG_FILE")"
if printf '%s' "$SUBJECT" | grep -Eq '^(feat|fix|docs|style|refactor|test|chore|ci|build|perf|revert)(\([^)]+\))?: .+'; then
  exit 0
fi
echo "commit-msg hook: expected conventional commit subject"
echo "got: $SUBJECT"
exit 1
