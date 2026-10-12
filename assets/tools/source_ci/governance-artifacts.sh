#!/usr/bin/env bash
# Fixed source-owned CI recipe. The measured profile binds these exact bytes.
set -euo pipefail
umask 077
source_root="$(git rev-parse --show-toplevel)"
source "$source_root/assets/tools/source_ci/private-checkout.sh"
source_private_checkout
cd "$SOURCE_PRIVATE_CHECKOUT"
export GOVERNANCE_BASE="${SOURCE_CI_BASE?}" GOVERNANCE_HEAD_REF="${SOURCE_CI_HEAD:?}"
set -euo pipefail
chmod +x "$source_root/target/debug/decapod"
"$source_root/target/debug/decapod" govern artifacts verify-checkpoints \
  --base-branch "$GOVERNANCE_BASE" \
  --head-ref "$GOVERNANCE_HEAD_REF" --target-base master --publication-material
