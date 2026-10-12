#!/usr/bin/env bash
set -euo pipefail
version="$(python3 .github/scripts/release-tool-versions.py | sed -n 's/^cargo_dist=//p')"
test -n "$version"
curl --proto '=https' --tlsv1.2 -LsSf \
  "https://github.com/axodotdev/cargo-dist/releases/download/v${version}/cargo-dist-installer.sh" | sh
