#!/usr/bin/env bash
# Fixed source-owned CI recipe. The measured profile binds these exact bytes.
set -euo pipefail
umask 077
source_private_checkout() {
  set -euo pipefail
  # Imported CI checkouts can inherit a permissive provider umask. Never
  # chmod their existing state or relax the shared-datastore safety gate.
  # Clone only immutable Git objects into a new directory owned by this job.
  umask 077
  source_root="$(git rev-parse --show-toplevel)"
  source_head="$(git rev-parse --verify HEAD^{commit})"
  source_origin="$(git remote get-url origin)"
  # Keep job-owned scratch outside the measured source checkout. The custody
  # guard must still reject every undeclared source output, including clones.
  private_parent="$(cd "${RUNNER_TEMP:?RUNNER_TEMP must name job-owned scratch}" && pwd -P)"
  case "$private_parent/" in
    "$source_root/"*) echo 'private checkout scratch must be outside the source checkout' >&2; return 1 ;;
  esac
  private_root="$(mktemp -d "$private_parent/decapod-ci-validation.XXXXXXXX")"
  git -c core.sharedRepository=false clone --quiet --no-local --no-hardlinks \
    --no-checkout -- "$source_root" "$private_root"
  # A detached provider checkout may have no local branches. Ordinary
  # clone does not retain its remote-tracking refs, which validation uses
  # to prove the immutable event's relationship to the accepted base.
  # Transfer only local Git objects/refs; do not fetch external state.
  git -C "$private_root" fetch --quiet --no-tags --no-write-fetch-head -- \
    "$source_root" '+refs/remotes/origin/*:refs/remotes/origin/*'
  git -C "$private_root" checkout --quiet --detach "$source_head"
  git -C "$private_root" remote set-url origin "$source_origin"
  test "$(git -C "$private_root" rev-parse HEAD)" = "$source_head"
  test "$(git -C "$private_root" rev-parse HEAD^{tree})" = "$(git rev-parse HEAD^{tree})"
  SOURCE_PRIVATE_CHECKOUT="$private_root"
  export SOURCE_PRIVATE_CHECKOUT
}
