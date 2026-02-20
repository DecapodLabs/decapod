#!/bin/bash
# enforce_package_managers.sh - PreToolUse hook to enforce modern package managers
# Blocks legacy tools (pip, npm) in favor of modern alternatives (uv, bun)

set -euo pipefail

CLAUDE_PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$(pwd)}"
CONFIG_FILE="${CLAUDE_PROJECT_DIR}/.claude/hooks/config.json"

load_config() {
  if [[ -f "${CONFIG_FILE}" ]]; then
    CONFIG_JSON=$(cat "${CONFIG_FILE}")
  else
    CONFIG_JSON='{}'
  fi
}

get_allowed_subcommands() {
  local tool="$1"
  echo "${CONFIG_JSON}" | jaq -r ".package_managers.allowed_subcommands.${tool} // [] | .[]" 2>/dev/null
}

is_allowed_subcommand() {
  local tool="$1"
  local subcommand="$2"

  while IFS= read -r allowed; do
    if [[ "$subcommand" == "$allowed" ]]; then
      return 0
    fi
  done < <(get_allowed_subcommands "$tool")

  return 1
}

get_enforced_python() {
  echo "${CONFIG_JSON}" | jaq -r '.package_managers.python // "uv"' 2>/dev/null
}

get_enforced_js() {
  echo "${CONFIG_JSON}" | jaq -r '.package_managers.javascript // "bun"' 2>/dev/null
}

main() {
  load_config

  local tool_name="${1:-}"
  local command_line="${2:-}"

  if [[ -z "$command_line" ]]; then
    exit 0
  fi

  case "$tool_name" in
    Bash)
      local cmd
      cmd=$(echo "$command_line" | awk '{print $1}' | xargs basename 2>/dev/null || echo "")

      case "$cmd" in
        pip|pip3)
          if ! is_allowed_subcommand "pip" "$(echo "$command_line" | awk '{print $2}' 2>/dev/null || echo "")"; then
            echo "[hook] ERROR: pip is not allowed. Use '$(get_enforced_python)' instead." >&2
            echo "[hook] Install: pip install uv && uv pip install <package>" >&2
            exit 1
          fi
          ;;
        npm)
          if ! is_allowed_subcommand "npm" "$(echo "$command_line" | awk '{print $2}' 2>/dev/null || echo "")"; then
            echo "[hook] ERROR: npm is not allowed. Use '$(get_enforced_js)' instead." >&2
            echo "[hook] Install: bun install <package>" >&2
            exit 1
          fi
          ;;
        yarn|pnpm)
          echo "[hook] WARNING: $cmd is not the enforced package manager." >&2
          echo "[hook] Use '$(get_enforced_js)' instead." >&2
          exit 1
          ;;
        poetry|pipenv)
          echo "[hook] ERROR: $cmd is not allowed. Use '$(get_enforced_python)' instead." >&2
          exit 1
          ;;
      esac
      ;;
  esac

  exit 0
}

main "$@"
