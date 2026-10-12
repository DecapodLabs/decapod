#!/usr/bin/env bash
# Aggregate every exact selected receipt only after authenticating the shared
# verifier. Provider statuses alone cannot prove a shard or empty selection.
set -euo pipefail
umask 077
[[ $# == 0 ]]
[[ "$CUSTODY_HELPER_SHA256" =~ ^[0-9a-f]{64}$ ]]
printf '%s  %s\n' "$CUSTODY_HELPER_SHA256" target/integration-custody/integration-custody | sha256sum --check --status
chmod +x target/integration-custody/integration-custody
target/integration-custody/integration-custody aggregate --receipts target/integration-receipts
