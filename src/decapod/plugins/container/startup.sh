set -eu
cd "${DECAPOD_WORKSPACE:-$PWD}"
mkdir -p "${HOME:-/tmp/decapod-home}"
git_safe() {
git -c safe.directory="${DECAPOD_WORKSPACE:-$PWD}" "$@"
}
unset SSH_AUTH_SOCK || true
if [ "${DECAPOD_CONTAINER_DEBUG:-0}" = "1" ]; then
echo "debug: workspace=${DECAPOD_WORKSPACE:-$PWD}" >&2
echo "debug: uid=$(id -u) gid=$(id -g)" >&2
git_safe remote -v >&2 || true
fi
unset GIT_DIR GIT_WORK_TREE
git config --global user.name "${DECAPOD_GIT_USER_NAME:-Decapod Agent}"
git config --global user.email "${DECAPOD_GIT_USER_EMAIL:-agent@decapod.local}"
if ! command -v decapod >/dev/null 2>&1 && [ -f Cargo.toml ] && command -v cargo >/dev/null 2>&1; then
decapod() { cargo run --quiet --bin decapod -- "$@"; }
fi
if command -v decapod >/dev/null 2>&1; then
decapod version >/dev/null 2>&1 || true
if decapod --help 2>/dev/null | grep -qE "(^|[[:space:]])update([[:space:]]|$)"; then
decapod update
fi
fi
