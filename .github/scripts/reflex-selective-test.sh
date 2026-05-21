#!/usr/bin/env bash
# Reflex action: runs selective tests based on git-changed files
# Triggered by: git post-commit hook or manual invocation

set -euo pipefail

# Get changed files (staged since last commit)
CHANGED_FILES=$(git status --porcelain 2>/dev/null | awk '{if ($1 ~ /^[MADRC]/) print $2}' | sort -u)

if [ -z "$CHANGED_FILES" ]; then
    echo "No changed files to analyze"
    exit 0
fi

echo "Analyzing: $CHANGED_FILES"

# Use decapod impact
IMPACT=$(decapod impact --changed-files "$CHANGED_FILES" --predict --format json 2>/dev/null || echo '{}')
echo "Impact: $IMPACT"

# Run relevant tests
FAILED=0
for file in $CHANGED_FILES; do
    case "$file" in
        src/core/todo.rs)
            echo "Testing: todo module"
            cargo test --all-features --test todo_enforcement -- --test-threads=2 || FAILED=1
            ;;
        src/core/validate.rs)
            echo "Testing: validate module"
            cargo test --all-features --test validate_termination -- --test-threads=2 || FAILED=1
            cargo test --all-features --test validate_optional_artifact_gates -- --test-threads=2 || FAILED=1
            ;;
        src/plugins/*.rs)
            plugin=$(basename "$file" .rs)
            echo "Testing: plugin $plugin"
            cargo test --all-features --test "plugins_${plugin}_tests" -- --test-threads=2 || FAILED=1
            ;;
        src/cli.rs|src/lib.rs)
            echo "Testing: CLI contracts"
            cargo test --all-features --test cli_contract_enforcement -- --test-threads=2 || FAILED=1
            cargo test --all-features --test entrypoint_correctness -- --test-threads=2 || FAILED=1
            ;;
    esac
done

if [ $FAILED -eq 0 ]; then
    echo "✓ All affected tests passed"
else
    echo "✗ Some tests failed"
fi

exit $FAILED