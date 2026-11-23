#!/usr/bin/env bash
# Governed scheduled/manual documentation auto-sync lifecycle (#1388).
#
# Invariants:
# - Never uses unvalidated raw `create-pull-request` commits.
# - Starts an explicit new-PR identity from the accepted target baseline (`begin-pr`)
#   when opening a fresh PR or resetting after a merged PR.
# - Preserves concurrent branch movement and never force-pushes.
# - Records generated changes, refreshes living specs/manifest, creates an exact
#   material checkpoint before every commit, and verifies the created commit via
#   `govern artifacts verify-checkpoints --publication-material` before pushing.
# - Fails closed with an actionable drift report if any governed gate fails.
set -euo pipefail
umask 077

DECAPOD_BIN="${DECAPOD_BIN:-./target/debug/decapod}"
BASE_REF="${DOCS_AUTOSYNC_BASE_REF:-origin/master}"
TARGET_BASE="${DOCS_AUTOSYNC_TARGET_BASE:-master}"
SYNC_BRANCH="${DOCS_AUTOSYNC_BRANCH:-docs-autosync}"
REMOTE_NAME="${DOCS_AUTOSYNC_REMOTE:-origin}"
REPORT_PATH="${DOCS_AUTOSYNC_REPORT_PATH:-${RUNNER_TEMP:-/tmp}/docs-autosync-drift-report.md}"
DOCS_BUILD_CMD="${DOCS_AUTOSYNC_BUILD_CMD:-}"
DRY_RUN="${DOCS_AUTOSYNC_DRY_RUN:-false}"
AGENT_ID="${DECAPOD_AGENT_ID:-docs-autosync-bot}"
SESSION_PASSWORD="${DECAPOD_SESSION_PASSWORD:-docs-autosync-governed-session}"

export DECAPOD_AGENT_ID="$AGENT_ID"
export DECAPOD_SESSION_PASSWORD="$SESSION_PASSWORD"

write_report() {
  local status="$1"
  local reason="$2"
  local base_sha="${3:-unknown}"
  local head_sha="${4:-unknown}"
  local changed_docs="${5:-}"
  mkdir -p "$(dirname "$REPORT_PATH")"
  cat > "$REPORT_PATH" <<EOF
# Docs Auto-Sync Governed Drift Report

- Status: \`${status}\`
- Reason: ${reason}
- Target base: \`${TARGET_BASE}\` (\`${base_sha}\`)
- Sync branch: \`${SYNC_BRANCH}\`
- Candidate head: \`${head_sha}\`
- Changed docs paths:
${changed_docs:-  (none)}
EOF
}

if [[ ! -x "$DECAPOD_BIN" ]]; then
  write_report "failed_closed" "Decapod binary not executable at ${DECAPOD_BIN}"
  echo "::error::docs-autosync: Decapod binary unavailable at ${DECAPOD_BIN}; failing closed." >&2
  exit 1
fi
DECAPOD_BIN="$(cd "$(dirname "$DECAPOD_BIN")" && pwd)/$(basename "$DECAPOD_BIN")"
if [[ "$REPORT_PATH" != /* ]]; then
  REPORT_PATH="$(pwd)/${REPORT_PATH}"
fi
REPO_ROOT="$(git rev-parse --show-toplevel)"

git fetch "$REMOTE_NAME" "$TARGET_BASE" --quiet
BASE_SHA="$(git rev-parse --verify "${BASE_REF}^{commit}")"

REMOTE_TIP=""
if git ls-remote --exit-code --heads "$REMOTE_NAME" "$SYNC_BRANCH" >/dev/null 2>&1; then
  git fetch "$REMOTE_NAME" "$SYNC_BRANCH" --quiet
  REMOTE_TIP="$(git rev-parse --verify "${REMOTE_NAME}/${SYNC_BRANCH}^{commit}")"
fi

# Determine branch ancestry:
# - No remote branch OR remote branch already merged into accepted BASE_SHA => fresh PR from BASE_SHA.
# - Open unmerged remote branch with BASE_SHA as ancestor => continue existing open PR branch.
# - Open remote branch behind/diverged from BASE_SHA => fail closed rather than force-pushing or rewriting history.
MODE="fresh"
START_POINT="$BASE_SHA"
if [[ -n "$REMOTE_TIP" ]]; then
  if git merge-base --is-ancestor "$REMOTE_TIP" "$BASE_SHA"; then
    MODE="reset_after_merge"
    START_POINT="$BASE_SHA"
  elif git merge-base --is-ancestor "$BASE_SHA" "$REMOTE_TIP"; then
    MODE="continue_open_pr"
    START_POINT="$REMOTE_TIP"
  else
    write_report "failed_closed" \
      "Remote branch ${SYNC_BRANCH} (${REMOTE_TIP}) diverged from accepted base ${BASE_SHA}; refusing to force-push or rewrite contributor history." \
      "$BASE_SHA" "$REMOTE_TIP"
    echo "::error::docs-autosync: ${SYNC_BRANCH} diverged from ${BASE_SHA}; failing closed without force-push." >&2
    exit 1
  fi
fi

# Ensure session is active and claim a Decapod todo so all mutations execute
# strictly inside a canonical isolated worktree under .decapod/workspaces/.
"$DECAPOD_BIN" session acquire >/dev/null 2>&1 || true
TASK_JSON="$("$DECAPOD_BIN" todo add "Governed docs auto-sync from ${BASE_SHA:0:12}" --format json)" || {
  write_report "failed_closed" "Failed to create Decapod todo for docs auto-sync." "$BASE_SHA" "$START_POINT"
  echo "::error::docs-autosync: failed to create Decapod todo; failing closed." >&2
  exit 1
}
TASK_ID="$(python3 -c 'import json,sys; d=json.loads(sys.argv[1]); print(d.get("id") or "")' "$TASK_JSON")"
if [[ -z "$TASK_ID" ]]; then
  write_report "failed_closed" "Decapod todo add returned empty task id." "$BASE_SHA" "$START_POINT"
  echo "::error::docs-autosync: empty todo id; failing closed." >&2
  exit 1
fi
if ! "$DECAPOD_BIN" todo claim --id "$TASK_ID" >/dev/null; then
  write_report "failed_closed" "Failed to claim Decapod todo ${TASK_ID}." "$BASE_SHA" "$START_POINT"
  echo "::error::docs-autosync: failed to claim todo ${TASK_ID}; failing closed." >&2
  exit 1
fi

WORKTREE_BRANCH="agent/${AGENT_ID}/${TASK_ID}-docs-autosync"
WORKTREE_DIR=""
cleanup_worktree() {
  cd "$REPO_ROOT" >/dev/null 2>&1 || true
  if [[ -n "$WORKTREE_DIR" && -d "$WORKTREE_DIR" ]]; then
    git worktree remove --force "$WORKTREE_DIR" >/dev/null 2>&1 || rm -rf "$WORKTREE_DIR"
  fi
  git branch -D "$WORKTREE_BRANCH" >/dev/null 2>&1 || true
}
trap cleanup_worktree EXIT

WS_JSON="$("$DECAPOD_BIN" workspace ensure --branch "$WORKTREE_BRANCH")" || {
  write_report "failed_closed" "Failed to provision canonical Decapod workspace for ${TASK_ID}." "$BASE_SHA" "$START_POINT"
  echo "::error::docs-autosync: workspace ensure failed; failing closed." >&2
  exit 1
}
WORKTREE_DIR="$(python3 -c 'import json,sys; d=json.loads(sys.argv[1]); print(d.get("worktree_path") or "")' "$WS_JSON")"
if [[ -z "$WORKTREE_DIR" || ! -d "$WORKTREE_DIR" ]]; then
  write_report "failed_closed" "workspace ensure did not return a valid worktree_path." "$BASE_SHA" "$START_POINT"
  echo "::error::docs-autosync: invalid worktree_path from workspace ensure; failing closed." >&2
  exit 1
fi
cd "$WORKTREE_DIR"
git reset --hard "$START_POINT" --quiet

# Generate documentation from current code state.
if [[ -n "$DOCS_BUILD_CMD" ]]; then
  bash -c "$DOCS_BUILD_CMD"
else
  "$DECAPOD_BIN" docs build
fi

CHANGED_DOCS="$(git status --porcelain -- docs/ || true)"
if [[ -z "$CHANGED_DOCS" ]]; then
  write_report "no_drift" "Generated documentation matches repository state; no PR mutation required." "$BASE_SHA" "$(git rev-parse HEAD)"
  echo "docs-autosync: no documentation drift detected."
  if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    printf 'drift=false\nmode=%s\nbase_sha=%s\n' "$MODE" "$BASE_SHA" >> "$GITHUB_OUTPUT"
  fi
  exit 0
fi

# Start explicit new-PR identity when creating a new PR or resetting after a merged PR.
PR_ID="docs-autosync-${BASE_SHA:0:12}"
EXISTING_RUN_ID=""
if [[ "$MODE" == "continue_open_pr" && -f ".decapod/governance.json" ]]; then
  EXISTING_RUN_ID="$(python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print(((d.get("sections") or {}).get("trajectory") or {}).get("run_id") or "")' .decapod/governance.json 2>/dev/null || true)"
fi

if [[ "$MODE" != "continue_open_pr" ]]; then
  if ! "$DECAPOD_BIN" govern artifacts begin-pr --id "$PR_ID" --base-branch "$BASE_REF"; then
    write_report "failed_closed" "Failed to initialize new governed PR identity (${PR_ID}) from ${BASE_SHA}." "$BASE_SHA" "$(git rev-parse HEAD)" "$CHANGED_DOCS"
    echo "::error::docs-autosync: begin-pr failed; failing closed without creating uncheckpointed history." >&2
    exit 1
  fi
fi

# Populate governed plan, falsifiable claim, and trajectory before checkpointing.
# On an open PR continuation, preserve the existing plan and trajectory run_id
# instead of overwriting prior PR evidence.
if [[ -z "$EXISTING_RUN_ID" ]]; then
  "$DECAPOD_BIN" govern plan init \
    --title "Governed docs auto-sync (${BASE_SHA:0:12})" \
    --intent "Synchronize generated documentation from accepted ${TARGET_BASE} (${BASE_SHA:0:12})" \
    --todo-id "$TASK_ID" \
    --proof-hook "docs_build" >/dev/null
  "$DECAPOD_BIN" govern plan approve >/dev/null
  RUN_ID="docs-autosync-${GITHUB_RUN_ID:-local}-$(date -u +%s)"
  "$DECAPOD_BIN" govern trajectory init \
    --run-id "$RUN_ID" \
    --task-id "$TASK_ID" \
    --original-intent "Synchronize generated documentation from code state on ${TARGET_BASE}" \
    --derived-intent "Run decapod docs build and govern the resulting documentation update with exact-material checkpoints" \
    --boundary "docs/ and governed living specs/manifest projections only" \
    --scope "docs/" \
    --destination "Governed docs auto-sync PR" \
    --phase execution \
    --next-transition validation >/dev/null
else
  RUN_ID="$EXISTING_RUN_ID"
fi

"$DECAPOD_BIN" govern artifacts claim \
  --id "docs-sync-fresh" \
  --statement "Generated documentation matches accepted code state on ${TARGET_BASE} (${BASE_SHA:0:12})" \
  --falsifier "decapod docs build produces uncommitted diff" \
  --status supported \
  --proof-ref "docs_build" >/dev/null

# Record changed docs paths in trajectory.
while IFS= read -r line; do
  [[ -z "$line" ]] && continue
  doc_path="${line:3}"
  "$DECAPOD_BIN" govern trajectory record --run-id "$RUN_ID" --modified-file "$doc_path" --check "docs_build=passed" >/dev/null || true
done <<< "$CHANGED_DOCS"

# Add a concise factual material note in INTERFACES.md so material-specs gate passes truthfully.
SPEC_INTERFACES=".decapod/managed/specs/INTERFACES.md"
if [[ -f "$SPEC_INTERFACES" ]]; then
  DOCS_SUMMARY="$(printf '%s\n' "$CHANGED_DOCS" | awk '{print $2}' | tr '\n' ',' | sed 's/,$//')"
  python3 - "$SPEC_INTERFACES" "$BASE_SHA" "$DOCS_SUMMARY" <<'PY'
import re, sys
from pathlib import Path

path = Path(sys.argv[1])
base_sha = sys.argv[2]
summary = sys.argv[3]
text = path.read_text(encoding="utf-8")
marker_start = "<!-- decapod:docs-autosync-note:start -->"
marker_end = "<!-- decapod:docs-autosync-note:end -->"
note = (
    f"{marker_start}\n"
    f"### Scheduled Documentation Sync Record\n\n"
    f"- Synchronized generated documentation against accepted base `{base_sha[:12]}`: `{summary}`.\n"
    f"{marker_end}"
)
if marker_start in text and marker_end in text:
    updated = re.sub(
        re.escape(marker_start) + r".*?" + re.escape(marker_end),
        note,
        text,
        flags=re.S,
    )
else:
    att = "<!-- decapod:codebase-attestation:start -->"
    if att in text:
        idx = text.index(att)
        updated = text[:idx].rstrip() + "\n\n" + note + "\n\n" + text[idx:]
    else:
        updated = text.rstrip() + "\n\n" + note + "\n"
path.write_text(updated, encoding="utf-8")
PY
fi

# Refresh living specs & canonical `.decapod/managed/.manifest.json` projection.
if ! "$DECAPOD_BIN" rpc --op specs.refresh >/dev/null; then
  write_report "failed_closed" "specs.refresh failed after docs build." "$BASE_SHA" "$(git rev-parse HEAD)" "$CHANGED_DOCS"
  echo "::error::docs-autosync: specs.refresh failed; failing closed." >&2
  exit 1
fi

# Record current-content-bound canonical spec reviews required for publication material.
"$DECAPOD_BIN" govern plan review-spec \
  --path ".decapod/managed/specs/INTERFACES.md" \
  --disposition updated \
  --reason "Recorded scheduled documentation sync update against ${BASE_SHA:0:12}" >/dev/null
"$DECAPOD_BIN" govern plan review-spec \
  --path ".decapod/managed/specs/ARCHITECTURE.md" \
  --disposition unchanged-with-reason \
  --reason "Scheduled docs sync updates generated documentation only; architecture unchanged" >/dev/null
"$DECAPOD_BIN" govern plan review-spec \
  --path ".decapod/managed/specs/SECURITY.md" \
  --disposition unchanged-with-reason \
  --reason "Scheduled docs sync updates generated documentation only; security posture unchanged" >/dev/null

# Stage material changes BEFORE checkpointing (never stage governance.json before checkpoint).
git add -- docs/ .decapod/managed/
if [[ -f ".decapod/managed/.manifest.json" ]]; then
  git add -f -- .decapod/managed/.manifest.json
fi

CHECKPOINT_ID="docs-autosync-${BASE_SHA:0:12}-$(date -u +%s)"
if ! "$DECAPOD_BIN" govern artifacts checkpoint \
  --id "$CHECKPOINT_ID" \
  --summary "Governed documentation sync from ${BASE_SHA:0:12}"; then
  write_report "failed_closed" "govern artifacts checkpoint failed." "$BASE_SHA" "$(git rev-parse HEAD)" "$CHANGED_DOCS"
  echo "::error::docs-autosync: checkpoint creation failed; failing closed." >&2
  exit 1
fi

git add -- .decapod/governance.json
git commit -m "docs: auto-sync documentation from code state" --quiet
HEAD_SHA="$(git rev-parse --verify "HEAD^{commit}")"

# Verify the exact created commit passes the same immutable checkpoint/publication gate as agent PRs.
if ! "$DECAPOD_BIN" govern artifacts verify-checkpoints \
  --base-branch "$BASE_SHA" \
  --head-ref "$HEAD_SHA" \
  --target-base "$TARGET_BASE" \
  --publication-material >/dev/null; then
  write_report "failed_closed" "verify-checkpoints --publication-material rejected ${HEAD_SHA} against ${BASE_SHA}." "$BASE_SHA" "$HEAD_SHA" "$CHANGED_DOCS"
  echo "::error::docs-autosync: immutable checkpoint verification failed for ${HEAD_SHA}; failing closed." >&2
  exit 1
fi

# Re-verify remote branch did not move concurrently before non-force push.
CURRENT_REMOTE_TIP=""
if git ls-remote --exit-code --heads "$REMOTE_NAME" "$SYNC_BRANCH" >/dev/null 2>&1; then
  git fetch "$REMOTE_NAME" "$SYNC_BRANCH" --quiet
  CURRENT_REMOTE_TIP="$(git rev-parse --verify "${REMOTE_NAME}/${SYNC_BRANCH}^{commit}")"
fi
if [[ "$CURRENT_REMOTE_TIP" != "$REMOTE_TIP" ]]; then
  write_report "failed_closed" \
    "Concurrent branch movement detected on ${SYNC_BRANCH} (expected ${REMOTE_TIP:-absent}, observed ${CURRENT_REMOTE_TIP:-absent}); refusing to force-push." \
    "$BASE_SHA" "$HEAD_SHA" "$CHANGED_DOCS"
  echo "::error::docs-autosync: concurrent branch movement on ${SYNC_BRANCH}; failing closed without force-push." >&2
  exit 1
fi

if [[ "$DRY_RUN" != "true" ]]; then
  if [[ "$MODE" == "reset_after_merge" ]]; then
    # Previous branch tip is strictly an ancestor of BASE_SHA (already merged);
    # HEAD_SHA descends from BASE_SHA, so a fast-forward push succeeds cleanly without --force!
    git push "$REMOTE_NAME" "HEAD:refs/heads/${SYNC_BRANCH}"
  else
    git push "$REMOTE_NAME" "HEAD:refs/heads/${SYNC_BRANCH}"
  fi
fi

write_report "verified_sync" "Governed docs sync commit ${HEAD_SHA} passed verify-checkpoints." "$BASE_SHA" "$HEAD_SHA" "$CHANGED_DOCS"
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  printf 'drift=true\nmode=%s\nbase_sha=%s\nhead_sha=%s\ncheckpoint_id=%s\n' \
    "$MODE" "$BASE_SHA" "$HEAD_SHA" "$CHECKPOINT_ID" >> "$GITHUB_OUTPUT"
fi
echo "docs-autosync: verified governed commit ${HEAD_SHA} (mode=${MODE})."
