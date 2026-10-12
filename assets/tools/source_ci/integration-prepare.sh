#!/usr/bin/env bash
# Reviewed compile-once recipe shared by source CI and measured preflight.
# Provider setup/artifact upload stays outside this script; no arbitrary command
# or path inputs are accepted. All outputs describe this actual preparation.
set -euo pipefail
umask 077
[[ $# == 0 ]]
: "${RUNNER_TEMP:?}" "${GITHUB_OUTPUT:?}" "${CHANGE_BASE:?}" "${CHANGE_HEAD:?}"
[[ "$CHANGE_BASE" == "$CUSTODY_BASE" && "$CHANGE_HEAD" == "$CUSTODY_HEAD" ]]

"$RUNNER_TEMP/source-ci-profile" paths --base "$CHANGE_BASE" --head "$CHANGE_HEAD" > "$RUNNER_TEMP/integration-paths.txt"
rustc --edition=2024 assets/tools/integration_shards.rs -o "$RUNNER_TEMP/integration-shards"
"$RUNNER_TEMP/integration-shards" plan --paths "$RUNNER_TEMP/integration-paths.txt" > "$RUNNER_TEMP/integration-matrix.json"
export CUSTODY_SHARDS="$(jq -c '[.include[].shard]' "$RUNNER_TEMP/integration-matrix.json")"
count="$(jq '.include | length' "$RUNNER_TEMP/integration-matrix.json")"
rustc --edition=2024 -O assets/tools/integration_custody.rs -o "$RUNNER_TEMP/integration-custody"
# Validate the complete immutable context before publishing any plan output.
"$RUNNER_TEMP/integration-custody" context > "$RUNNER_TEMP/integration-context-output.txt"
mkdir -p target/integration-custody-artifacts
cp "$RUNNER_TEMP/integration-custody" target/integration-custody-artifacts/integration-custody

archive_sha256=''
if [[ "$count" != 0 ]]; then
  "$RUNNER_TEMP/integration-shards" prepare \
    --paths "$RUNNER_TEMP/integration-paths.txt" \
    --output "$RUNNER_TEMP/integration-tests"
  "$RUNNER_TEMP/integration-custody" prepare --artifact "$RUNNER_TEMP/integration-tests"
  mkdir -p target/integration-artifacts
  tar -C "$RUNNER_TEMP/integration-tests" -cf target/integration-artifacts/integration-tests.tar .
  archive_sha256="$("$RUNNER_TEMP/integration-custody" digest --file target/integration-artifacts/integration-tests.tar)"
fi
# Bootstrap the same measured executor and finite reviewed worker recipes.
# Workers/status authenticate this archive before extracting anything. They do
# not checkout source, install toolchains, or compile test/helper code.
bootstrap="$RUNNER_TEMP/integration-bootstrap-package"
mkdir "$bootstrap"
mkdir -p "$bootstrap/integration-bootstrap" "$bootstrap/assets/tools/source_ci"
cp "$RUNNER_TEMP/source-ci-profile" "$bootstrap/integration-bootstrap/source-ci-profile"
cp assets/tools/source_ci/integration-worker.sh assets/tools/source_ci/integration-status.sh \
  "$bootstrap/assets/tools/source_ci/"
mkdir -p target/integration-bootstrap-artifacts
tar -C "$bootstrap" -cf target/integration-bootstrap-artifacts/integration-bootstrap.tar .
bootstrap_sha256="$("$RUNNER_TEMP/integration-custody" digest --file target/integration-bootstrap-artifacts/integration-bootstrap.tar)"
profile_sha256="$("$RUNNER_TEMP/integration-custody" digest --file "$bootstrap/integration-bootstrap/source-ci-profile")"
worker_script_sha256="$("$RUNNER_TEMP/integration-custody" digest --file "$bootstrap/assets/tools/source_ci/integration-worker.sh")"
status_script_sha256="$("$RUNNER_TEMP/integration-custody" digest --file "$bootstrap/assets/tools/source_ci/integration-status.sh")"
# Bounded descriptor binds the large suite archive without duplicating it in
# every evidence ZIP. Workers independently measure and verify these exact bytes.
"$RUNNER_TEMP/integration-custody" describe-preparation > target/integration-custody-artifacts/preparation.v1
# A failed compile/package emits no completed plan or archive identity.
printf 'shards=%s\ncount=%s\n' "$CUSTODY_SHARDS" "$count" >> "$GITHUB_OUTPUT"
cat "$RUNNER_TEMP/integration-context-output.txt" >> "$GITHUB_OUTPUT"
printf 'archive_sha256=%s\n' "$archive_sha256" >> "$GITHUB_OUTPUT"
printf 'bootstrap_sha256=%s\nprofile_sha256=%s\nworker_script_sha256=%s\nstatus_script_sha256=%s\n' \
  "$bootstrap_sha256" "$profile_sha256" "$worker_script_sha256" "$status_script_sha256" >> "$GITHUB_OUTPUT"
