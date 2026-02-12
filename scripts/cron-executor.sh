#!/bin/bash
# Decapod Cron Executor
# Runs due cron jobs based on their schedule
# Usage: ./scripts/cron-executor.sh [decapod-root]

set -e

DECAPOD_ROOT="${1:-.}"
cd "$DECAPOD_ROOT"

# Check if decapod is available
if ! command -v decapod &> /dev/null; then
    if [ -f "./target/release/decapod" ]; then
        DECAPOD_CMD="./target/release/decapod"
    elif [ -f "./target/debug/decapod" ]; then
        DECAPOD_CMD="./target/debug/decapod"
    else
        echo "Error: decapod binary not found"
        exit 1
    fi
else
    DECAPOD_CMD="decapod"
fi

echo "Decapod Cron Executor - $(date)"
echo "================================"

# Get list of active cron jobs
# For now, we just run the watcher job specifically
# In a full implementation, this would parse schedules and run due jobs

# Run watcher if it exists
if $DECAPOD_CMD cron list 2>/dev/null | grep -q "watcher_periodic"; then
    echo "Running watcher..."
    $DECAPOD_CMD watcher run > /dev/null 2>&1
    echo "✓ Watcher executed"
fi

# Run proof validation periodically (every 6th run - approximately every 30 min)
# Use a simple counter file
COUNTER_FILE=".decapod/.cron_counter"
COUNTER=0
if [ -f "$COUNTER_FILE" ]; then
    COUNTER=$(cat "$COUNTER_FILE")
fi
COUNTER=$(( (COUNTER + 1) % 6 ))
echo "$COUNTER" > "$COUNTER_FILE"

if [ "$COUNTER" -eq 0 ]; then
    echo "Running proof validation..."
    if $DECAPOD_CMD proof run > /dev/null 2>&1; then
        echo "✓ All proofs passed"
    else
        echo "⚠ Some proofs failed (check proof.events.jsonl)"
    fi
fi

echo "================================"
echo "Done at $(date)"