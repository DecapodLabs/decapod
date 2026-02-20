#!/bin/bash
# protect_linter_configs.sh - PreToolUse hook to block modifications to linter configs
# Prevents agents from "fixing" violations by changing the rules instead of the code

set -euo pipefail

CLAUDE_PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$(pwd)}"
CONFIG_FILE="${CLAUDE_PROJECT_DIR}/.claude/hooks/config.json"

load_protected_files() {
  if [[ -f "${CONFIG_FILE}" ]]; then
    jaq -r '.protected_files[]' "${CONFIG_FILE}" 2>/dev/null || echo ""
  else
    cat << 'EOF'
.ruff.toml
ty.toml
biome.json
.oxlintrc.json
.semgrep.yml
knip.json
.flake8
.yamllint
.shellcheckrc
.hadolint.yaml
taplo.toml
.markdownlint.jsonc
.markdownlint-cli2.jsonc
.jscpd.json
.claude/hooks/*
.claude/settings.json
EOF
  fi
}

is_protected() {
  local file="$1"
  local protected_files
  protected_files=$(load_protected_files)

  while IFS= read -r pattern; do
    if [[ -z "$pattern" ]]; then
      continue
    fi
    if [[ "$file" == $pattern ]] || [[ "$file" == *"$pattern" ]]; then
      return 0
    fi
  done <<< "$protected_files"

  return 1
}

main() {
  local tool_name="${1:-}"
  local file_path="${2:-}"

  if [[ -z "$file_path" ]]; then
    exit 0
  fi

  case "$tool_name" in
    Write|Edit)
      if is_protected "$file_path"; then
        echo "[hook] ERROR: Cannot modify protected linter configuration file: $file_path" >&2
        echo "[hook] Fix the code, not the rules. Violations should be resolved by changing the code." >&2
        exit 1
      fi
      ;;
  esac

  exit 0
}

main "$@"
