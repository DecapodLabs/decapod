#!/usr/bin/env bash
# Selective test automation: runs tests against changed files/components only
# Uses decapod impact analysis to determine affected tests
#
# Usage:
#   ./selective-test.sh                    # Auto-detect changed files
#   ./selective-test.sh src/core/todo.rs   # Run tests for specific files
#   ./selective-test.sh --all             # Run all tests

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo "$SCRIPT_DIR")"

# Parse arguments
if [ $# -eq 0 ]; then
    # Auto-detect from git status
    CHANGED_FILES=$(git status --porcelain 2>/dev/null | awk '{if ($1 ~ /^[MADRC]/) print $2}' | sort -u)
    if [ -z "$CHANGED_FILES" ]; then
        echo "No changed files detected. Use --all to run all tests."
        exit 0
    fi
elif [ "$1" = "--all" ]; then
    CHANGED_FILES="--all"
else
    CHANGED_FILES="$*"
fi

echo "Changed files: $CHANGED_FILES"

# Use decapod impact to predict validation outcomes
echo ""
echo "=== Running impact analysis ==="
cd "$REPO_ROOT"
IMPACT_OUTPUT=$(decapod impact --changed-files "$CHANGED_FILES" --predict --format json 2>/dev/null || echo '{"error": "impact failed", "predicted_affected_gates": []}')
echo "$IMPACT_OUTPUT" | jq -r '.predicted_affected_gates[]? // .gates[]? // .impacted?: "none"' 2>/dev/null || true

# Map changed files to test targets (file pattern -> test name associations)
declare -A FILE_TO_TESTS=(
    # Core modules -> corresponding tests
    ["src/core/todo.rs"]="todo_enforcement todo_rebuild_compat"
    ["src/core/validate.rs"]="validate_termination validate_optional_artifact_gates"
    ["src/core/gatekeeper.rs"]="validate_termination validate_optional_artifact_gates"
    ["src/core/workspace.rs"]="workspace_interlock"
    ["src/core/workunit.rs"]="workunit_cli workunit_publish_gate"
    ["src/core/obligation.rs"]="obligation"
    ["src/core/docs.rs"]="context_capsule_cli context_capsule_rpc lcm_determinism"
    ["src/core/context_capsule.rs"]="context_capsule_cli context_capsule_rpc context_capsule_schema"
    ["src/core/rpc.rs"]="agent_rpc_suite"
    ["src/migration.rs"]="core_tests"
    ["src/lib.rs"]="entrypoint_correctness init_config_behavior init_validate_green_field"
    ["src/cli.rs"]="cli_contract_enforcement"

    
    # Plugins -> corresponding tests
    ["src/plugins/todo.rs"]="plugins_todo_tests"
    ["src/plugins/policy.rs"]="plugins_policy_tests"
    ["src/plugins/health.rs"]="plugins_health_tests"
    ["src/plugins/aptitude.rs"]="plugins_aptitude_tests"
    ["src/plugins/internalize.rs"]="plugins_internalize_tests"
    ["src/plugins/federation.rs"]="plugins_federation_tests"
    ["src/plugins/decide.rs"]="plugins_decide_tests"
    ["src/plugins/obligation.rs"]="plugins_obligation_tests"
    
    # Test infrastructure changes
    ["tests/"]="entrypoint_correctness"
)

# Heuristic mapping: module name -> related tests (when exact match not found)
declare -A MODULE_HEURISTICS=(
    ["todo"]="todo_enforcement todo_rebuild_compat"
    ["validate"]="validate_termination validate_optional_artifact_gates"
    ["gatekeeper"]="validate_termination"
    ["workspace"]="workspace_interlock"
    ["workunit"]="workunit_cli workunit_publish_gate"
    ["obligation"]="obligation"
    ["docs"]="context_capsule_cli lcm_determinism"
    ["capsule"]="context_capsule_cli context_capsule_rpc"
    ["rpc"]="agent_rpc_suite"
    ["migration"]="core_tests"
    ["schema"]="context_capsule_schema schema_markdown"
    ["knowledge"]="knowledge_promotion_cli"
    ["eval"]="eval_kernel plan_governed_execution"
    ["federation"]="plugins_federation_tests"
)

declare -A TESTS_TO_RUN

if [ "$CHANGED_FILES" = "--all" ]; then
    echo "Mode: all tests"
    # Add all known test targets
    for test in todo_enforcement validate_termination workspace_interlock workunit_cli \
        context_capsule_cli agent_rpc_suite entrypoint_correctness cli_contract_enforcement \
        init_config_behavior init_validate_green_field \
        plugins_todo_tests plugins_policy_tests plugins_health_tests plugins_aptitude_tests \
        plugins_internalize_tests plugins_federation_tests plugins_decide_tests plugins_obligation_tests; do
        TESTS_TO_RUN[$test]=1
    done
else
    # Parse individual files
    IFS=',' read -ra CHANGED_ARRAY <<< "$CHANGED_FILES"
    
    for changed_file in "${CHANGED_ARRAY[@]}"; do
        changed_file="${changed_file#"${changed_file%%[![:space:]]*}"}"  # trim
        
        # Skip non-source files
        [[ "$changed_file" =~ ^(target|build|\.git|Cargo.lock|flake\.lock|docs/|constitution_embed) ]] && continue
        
        # Check exact matches
        for pattern in "${!FILE_TO_TESTS[@]}"; do
            if [[ "$changed_file" == "$pattern" ]] || [[ "$changed_file" == *"$pattern"* ]]; then
                for test in ${FILE_TO_TESTS[$pattern]}; do
                    TESTS_TO_RUN[$test]=1
                done
            fi
        done
        
        # Heuristic: src/core/X.rs -> tests related to X
        if [[ "$changed_file" =~ ^src/core/([^/]+)\.rs$ ]]; then
            module="${BASH_REMATCH[1]}"
            if [[ -v MODULE_HEURISTICS[$module] ]]; then
                for test in ${MODULE_HEURISTICS[$module]}; do
                    TESTS_TO_RUN[$test]=1
                done
            else
                # Default: run entrypoint test for unknown core modules
                TESTS_TO_RUN[entrypoint_correctness]=1
            fi
        fi
        
        # Heuristic: src/plugins/X.rs -> plugins_X_tests
        if [[ "$changed_file" =~ ^src/plugins/([^/]+)\.rs$ ]]; then
            plugin="${BASH_REMATCH[1]}"
            TESTS_TO_RUN["plugins_${plugin}_tests"]=1
        fi
        
        # Heuristic: tests/X.rs -> itself
        if [[ "$changed_file" =~ ^tests/([^/]+)\.rs$ ]]; then
            test="${BASH_REMATCH[1]}"
            TESTS_TO_RUN["$test"]=1
        fi
        
        # Config changes: run core conformance tests
        if [[ "$changed_file" =~ ^(Cargo\.toml|AGENTS\.md|CLAUDE\.md|CODEX\.md|GEMINI\.md|constitution\.json) ]]; then
            TESTS_TO_RUN[entrypoint_correctness]=1
            TESTS_TO_RUN[cli_contract_enforcement]=1
        fi
        
        # SQL migrations: run core tests
        if [[ "$changed_file" =~ \.sql$ ]]; then
            TESTS_TO_RUN[core_tests]=1
        fi
    done
fi

echo ""
echo "=== Tests to run ==="
if [ ${#TESTS_TO_RUN[@]} -eq 0 ]; then
    echo "(none determined - defaulting to entrypoint_correctness)"
    TESTS_TO_RUN[entrypoint_correctness]=1
fi

TEST_TARGETS=("${!TESTS_TO_RUN[@]}")
echo "Target: ${TEST_TARGETS[*]}"

echo ""
echo "=== Running selective tests ==="

FAILED=0
for test in "${TEST_TARGETS[@]}"; do
    echo ""
    echo ">>> Running: $test"
    if cargo test --all-features --test "$test" -- --test-threads=4 2>&1; then
        echo "✓ $test passed"
    else
        echo "✗ $test FAILED"
        FAILED=1
    fi
done

echo ""
if [ $FAILED -eq 0 ]; then
    echo "=== All selective tests passed ==="
else
    echo "=== Some selective tests failed ==="
fi

exit $FAILED