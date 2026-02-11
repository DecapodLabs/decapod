#!/usr/bin/env bash
set -euo pipefail

HOOK_PATH=".git/hooks/commit-msg"

# Only run if we are in a git repo
if [ ! -d ".git" ]; then
    echo "Error: .git directory not found. Are you in the root of the project?"
    exit 1
fi

echo "Installing commit-msg hook..."

cat > "$HOOK_PATH" << 'EOF'
#!/usr/bin/env bash

# Regex for conventional commits
REGEX="^(feat|fix|chore|ci|docs|style|refactor|perf|test)(\(.*\))?!?: .+"
MSG=$(cat "$1")

if [[ ! $MSG =~ $REGEX ]]; then
    echo "❌ Error: Invalid commit message format."
    echo "   Commit messages must follow the Conventional Commits format."
    echo "   Example: 'feat: add login functionality'"
    echo "   Allowed prefixes: feat, fix, chore, ci, docs, style, refactor, perf, test"
    exit 1
fi
EOF

chmod +x "$HOOK_PATH"
echo "✅ Git hooks installed successfully."
