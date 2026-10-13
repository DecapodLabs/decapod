#!/usr/bin/env bash
set -euo pipefail

# The compatibility runner starts steps in fresh shells. Restore the pinned
# cargo-dist install location just as release-plan.sh does.
cargo_bin="${CARGO_HOME:-${HOME}/.cargo}/bin"
if [ -x "$cargo_bin/cargo-dist" ]; then
  export PATH="$cargo_bin:$PATH"
fi

# Compile a std-only probe with the release toolchain, not the project or its
# dependencies. No persistent artifact or global Cargo configuration is changed.
scratch="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/decapod-release-cpu.XXXXXX")"
trap 'rm -rf -- "$scratch"' EXIT
rustc --edition=2024 assets/tools/release_cpu.rs -o "$scratch/release-cpu"
jobs="$("$scratch/release-cpu")"

# Scope the detected capacity to this artifact build. The repository's jobs=4
# remains in force for ordinary builds/tests; a lower explicit runner budget is
# preserved. This command builds/packages only and never hosts or publishes.
CARGO_BUILD_JOBS="$jobs" cargo dist build "$@"
