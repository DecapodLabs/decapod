#!/usr/bin/env bash
set -euo pipefail

# The exact target description
EXPECTED_DESC="🦀 Decapod is a Rust-built, repo-native control-plane kernel for AI swarms—safe shared state, enforced truth, and loop-agnostic orchestration."

# Extract the description from the root package using cargo metadata
# We use --no-deps to only look at our own workspace members
ACTUAL_DESC=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name=="decapod") | .description')

echo "Verifying crate description..."
if [ "$ACTUAL_DESC" == "$EXPECTED_DESC" ]; then
    echo "Success: Description matches exactly."
    exit 0
else
    echo "Error: Description mismatch!"
    echo "Expected: $EXPECTED_DESC"
    echo "Found:    $ACTUAL_DESC"
    exit 1
fi
