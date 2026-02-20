#!/bin/bash
# stop_config_guardian.sh - Claude Code Stop hook to detect config file tampering
# Detects if any linter config files were modified during the session

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

check_config_tampering() {
  local protected_files
  protected_files=$(load_protected_files)

  local tampered=()
  while IFS= read -r pattern; do
    if [[ -z "$pattern" ]]; then
      continue
    fi
    # Check if any protected files were modified
    if git -C "${CLAUDE_PROJECT_DIR}" diff --name-only 2>/dev/null | grep -q "$pattern"; then
      tampered+=("$pattern")
    fi
  done <<< "$protected_files"

  if [[ ${#tampered[@]} -gt 0 ]]; then
    echo "[hook] WARNING: Detected modification to linter config files during session:" >&2
    for f in "${tampered[@]}"; do
      echo "  - $f" >&2
    done
    echo "[hook] Restoring original config files..." >&2
    git -C "${CLAUDE_PROJECT_DIR}" checkout -- "${CLAUDE_PROJECT_DIR}" 2>/dev/null || true
    echo "[hook] Config files restored. Violations should be fixed in code, not config." >&2
  fi
}

main() {
  # Only run if we're in a git repo and there are changes
  if ! git -C "${CLAUDE_PROJECT_DIR}" rev-parse --git-dir >/dev/null 2>&1; then
    exit 0
  fi

  if git -C "${CLAUDE_PROJECT_DIR}" diff --quiet 2>/dev/null; then
    exit 0
  fi

  check_config_tampering
}

main "$@"
