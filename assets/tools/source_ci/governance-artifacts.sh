#!/usr/bin/env bash
# Fixed source-owned CI recipe. The measured profile binds these exact bytes.
set -euo pipefail
umask 077
# Bind Git inspection and the private clone to this checkout, never inherited
# worktree/index/config overrides or replacement objects. Bash 3 supports this
# variable-name expansion too (the recipe also runs on macOS).
for source_git_var in ${!GIT_@}; do
  unset "$source_git_var"
done
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null
export GIT_NO_REPLACE_OBJECTS=1 GIT_NO_LAZY_FETCH=1 GIT_TERMINAL_PROMPT=0
source_root="$(git rev-parse --show-toplevel)"
source "${SOURCE_CI_EXECUTOR_ROOT:-$source_root}/assets/tools/source_ci/private-checkout.sh"
source_private_checkout
cd "$SOURCE_PRIVATE_CHECKOUT"
export GOVERNANCE_BASE="${SOURCE_CI_BASE?}" GOVERNANCE_HEAD_REF="${SOURCE_CI_HEAD:?}"
# Custody has verified the provider checkout. Publication checks must inspect
# authored H, not GitHub's synthetic B/H merge. Normalize only this private
# clone, only for the exact ordered PR merge, and only with whole-tree parity.
# The ordinary publication HEAD/race and full checkpoint-history gates remain
# unchanged; equality of trees alone never authorizes an unrelated commit.
for governance_commit in "$GOVERNANCE_BASE" "$GOVERNANCE_HEAD_REF"; do
  [[ "$governance_commit" =~ ^[0-9a-f]{40}([0-9a-f]{24})?$ ]]
  test "$(git cat-file -t "$governance_commit")" = commit
done
test "${#GOVERNANCE_BASE}" = "${#GOVERNANCE_HEAD_REF}"
test "$GOVERNANCE_BASE" != "$GOVERNANCE_HEAD_REF"
test "$(git rev-parse --verify HEAD^{commit})" = "$source_head"
if [ "$source_head" != "$GOVERNANCE_HEAD_REF" ]; then
  test "${SOURCE_CI_EVENT:?}" = pull_request
  test "$(git rev-list --parents -n 1 "$source_head")" = \
    "$source_head $GOVERNANCE_BASE $GOVERNANCE_HEAD_REF"
  test "$(git rev-parse "$source_head^{tree}")" = \
    "$(git rev-parse "$GOVERNANCE_HEAD_REF^{tree}")"
  git checkout --quiet --detach "$GOVERNANCE_HEAD_REF"
fi
test "$(git rev-parse --verify HEAD^{commit})" = "$GOVERNANCE_HEAD_REF"
test "$(git -C "$source_root" rev-parse --verify HEAD^{commit})" = "$source_head"
set -euo pipefail
chmod +x "$source_root/target/debug/decapod"
"$source_root/target/debug/decapod" govern artifacts verify-checkpoints \
  --base-branch "$GOVERNANCE_BASE" \
  --head-ref "$GOVERNANCE_HEAD_REF" --target-base master --publication-material
