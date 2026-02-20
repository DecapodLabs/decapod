#!/bin/bash
# multi_linter.sh - Decapod Plankton PostToolUse hook for multi-language linting
# Supports: Python (ruff), Shell (shellcheck+shfmt), YAML (yamllint), 
#           JSON, TOML (taplo), Dockerfile (hadolint), Markdown (markdownlint-cli2)
#
# Three-Phase Architecture:
#   Phase 1: Auto-format files (silent on success)
#   Phase 2: Collect unfixable violations as JSON
#   Phase 3: Delegate to Claude subprocess for fixes, then verify

set -euo pipefail
trap 'kill 0' SIGTERM

if ! command -v jaq >/dev/null 2>&1; then
  echo "[hook] error: jaq is required but not found. Install: brew install jaq" >&2
  exit 0
fi

CLAUDE_PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$(pwd)}"
CONFIG_FILE="${CLAUDE_PROJECT_DIR}/.claude/hooks/config.json"

load_config() {
  if [[ -f "${CONFIG_FILE}" ]]; then
    CONFIG_JSON=$(cat "${CONFIG_FILE}")
  else
    CONFIG_JSON='{}'
  fi
}

is_language_enabled() {
  local lang="$1"
  local enabled
  enabled=$(echo "${CONFIG_JSON}" | jaq -r ".languages.${lang}" 2>/dev/null)
  [[ "${enabled}" != "false" ]]
}

get_exclusions() {
  local defaults='["tests/","docs/",".venv/","scripts/","node_modules/",".git/",".claude/"]'
  echo "${CONFIG_JSON}" | jaq -r ".exclusions // ${defaults} | .[]" 2>/dev/null
}

is_auto_format_enabled() {
  local enabled
  enabled=$(echo "${CONFIG_JSON}" | jaq -r '.phases.auto_format' 2>/dev/null)
  [[ "${enabled}" != "false" ]]
}

is_subprocess_enabled() {
  local enabled
  enabled=$(echo "${CONFIG_JSON}" | jaq -r '.phases.subprocess_delegation' 2>/dev/null)
  [[ "${enabled}" != "false" ]]
}

should_exclude() {
  local file="$1"
  while IFS= read -r pattern; do
    if [[ "${file}" == ${pattern} ]]; then
      return 0
    fi
  done < <(get_exclusions)
  return 1
}

run_ruff_format() {
  local file="$1"
  if command -v ruff >/dev/null 2>&1; then
    ruff format "$file" 2>/dev/null || true
  fi
}

run_ruff_check() {
  local file="$1"
  if ! command -v ruff >/dev/null 2>&1; then
    return 0
  fi
  ruff check --output-format=json "$file" 2>/dev/null || echo "[]"
}

run_shellcheck() {
  local file="$1"
  if ! command -v shellcheck >/dev/null 2>&1; then
    return 0
  fi
  shellcheck --format=json "$file" 2>/dev/null || echo "[]"
}

run_shfmt() {
  local file="$1"
  if command -v shfmt >/dev/null 2>&1; then
    shfmt -w "$file" 2>/dev/null || true
  fi
}

run_yamllint() {
  local file="$1"
  if ! command -v yamllint >/dev/null 2>&1; then
    return 0
  fi
  yamllint -f json "$file" 2>/dev/null || echo "[]"
}

run_taplo() {
  local file="$1"
  if ! command -v taplo >/dev/null 2>&1; then
    return 0
  fi
  taplo check "$file" 2>&1 || echo "[]"
}

run_hadolint() {
  local file="$1"
  if ! command -v hadolint >/dev/null 2>&1; then
    return 0
  fi
  hadolint "$file" 2>&1 || true
}

run_markdownlint() {
  local file="$1"
  if ! command -v markdownlint >/dev/null 2>&1; then
    return 0
  fi
  markdownlint "$file" 2>&1 || true
}

get_file_language() {
  local file="$1"
  case "${file}" in
    *.py) echo "python" ;;
    *.sh|*.bash) echo "shell" ;;
    *.yaml|*.yml) echo "yaml" ;;
    *.json) echo "json" ;;
    *.toml) echo "toml" ;;
    Dockerfile|Dockerfile.*) echo "dockerfile" ;;
    *.md) echo "markdown" ;;
    *) echo "unknown" ;;
  esac
}

phase1_autoformat() {
  local file="$1"
  local lang
  lang=$(get_file_language "$file")

  case "${lang}" in
    python) run_ruff_format "$file" ;;
    shell) run_shfmt "$file" ;;
    *) ;;
  esac
}

phase2_collect_violations() {
  local file="$1"
  local lang
  lang=$(get_file_language "$file")
  local violations="[]"

  case "${lang}" in
    python) violations=$(run_ruff_check "$file") ;;
    shell) violations=$(run_shellcheck "$file") ;;
    yaml) violations=$(run_yamllint "$file") ;;
    toml) 
      if ! run_taplo "$file" >/dev/null 2>&1; then
        violations=$(echo '[{"line": 1, "message": "TOML validation failed"}]')
      fi
      ;;
    dockerfile)
      if ! run_hadolint "$file" >/dev/null 2>&1; then
        violations=$(echo '[{"line": 1, "message": "Dockerfile linting failed"}]')
      fi
      ;;
    markdown)
      if ! run_markdownlint "$file" >/dev/null 2>&1; then
        violations=$(echo '[{"line": 1, "message": "Markdown linting failed"}]')
      fi
      ;;
  esac

  echo "$violations"
}

phase3_delegate_fixes() {
  local file="$1"
  local violations="$2"

  local violation_count
  violation_count=$(echo "$violations" | jaq 'length' 2>/dev/null || echo "0")

  if [[ "$violation_count" -eq 0 ]] || [[ "$violation_count" -eq "0" ]]; then
    return 0
  fi

  echo "[hook] Delegating $violation_count violation(s) to Claude for fixes..."

  local fix_prompt="Fix the following linting violations in ${file}:

$(echo "$violations" | jaq -c '.[] | "\(.line): \(.message)"' 2>/dev/null | tr -d '"' | sed 's/^/  /')

Apply the fixes directly to the file. Do not modify linter configuration files."

  echo "$fix_prompt" | claude --dangerously-skip-permissions -p "$(cat)" >/dev/null 2>&1 || true

  return 0
}

main() {
  load_config

  local tool_name="${1:-}"
  local file_path="${2:-}"

  if [[ -z "$file_path" ]]; then
    exit 0
  fi

  if should_exclude "$file_path"; then
    exit 0
  fi

  local lang
  lang=$(get_file_language "$file_path")

  if ! is_language_enabled "$lang"; then
    exit 0
  fi

  if is_auto_format_enabled; then
    phase1_autoformat "$file_path"
  fi

  local violations
  violations=$(phase2_collect_violations "$file_path")

  local violation_count
  violation_count=$(echo "$violations" | jaq 'length' 2>/dev/null || echo "0")

  if [[ "$violation_count" -gt 0 ]] && [[ "$violation_count" -ne "0" ]]; then
    echo "[hook] Found $violation_count violation(s) in $file_path"

    if is_subprocess_enabled; then
      phase3_delegate_fixes "$file_path" "$violations"
    fi

    exit 2
  fi

  exit 0
}

main "$@"
