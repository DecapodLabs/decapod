#!/usr/bin/env bash
# Decapod CLI Gatling Test v2 — exercises every code path with correct signatures
# Runs in an isolated temp directory to avoid polluting project state

set -uo pipefail

DECAPOD="cargo run --manifest-path /home/arx/projects/decapod/Cargo.toml --quiet --"
WORKDIR=$(mktemp -d /tmp/decapod-gatling.XXXXXX)
RESULTS_FILE="/home/arx/projects/decapod/dev/gatling_results.jsonl"

# Create a fake git repo in the workdir
cd "$WORKDIR"
git init -q .
git config user.email "test@test.com"
git config user.name "Test"
touch README.md
git add . && git commit -q -m "init"

> "$RESULTS_FILE"

PASS=0
FAIL=0
TOTAL=0

run_test() {
  local test_id="$1"
  local description="$2"
  local expect_fail="${3:-0}"   # 1 = we expect nonzero exit
  shift 3
  local cmd="$*"

  TOTAL=$((TOTAL + 1))

  local start_ns=$(date +%s%N)
  local output
  output=$(eval "$cmd" 2>&1)
  local exit_code=$?
  local end_ns=$(date +%s%N)
  local duration_ms=$(( (end_ns - start_ns) / 1000000 ))

  local status="PASS"
  if [ "$expect_fail" = "1" ]; then
    # We expect failure — PASS if it did fail, FAIL if it succeeded unexpectedly
    if [ $exit_code -eq 0 ]; then
      status="FAIL"
      FAIL=$((FAIL + 1))
    else
      PASS=$((PASS + 1))
    fi
  else
    if [ $exit_code -ne 0 ]; then
      status="FAIL"
      FAIL=$((FAIL + 1))
    else
      PASS=$((PASS + 1))
    fi
  fi

  # Truncate output for jsonl
  local trunc_output
  trunc_output=$(printf '%s' "$output" | head -c 3000 | tr '\n' '|' | sed 's/\\/\\\\/g' | sed 's/"/\\"/g' | tr -d '\t')

  printf '{"id":"%s","desc":"%s","expect_fail":%s,"exit":%d,"status":"%s","ms":%d,"output":"%s"}\n' \
    "$test_id" "$description" "$expect_fail" "$exit_code" "$status" "$duration_ms" "$trunc_output" >> "$RESULTS_FILE"

  if [ "$status" = "FAIL" ]; then
    echo "  FAIL [$test_id] $description (exit=$exit_code, ${duration_ms}ms)"
    # Print first 5 lines of output for diagnostics
    printf '%s\n' "$output" | head -5 | sed 's/^/       > /'
  else
    echo "  PASS [$test_id] $description (exit=$exit_code, ${duration_ms}ms)"
  fi
}

echo "=========================================="
echo "  DECAPOD CLI GATLING TEST v2"
echo "  Workdir: $WORKDIR"
echo "=========================================="
echo ""

# ==========================================
# 1. TOP-LEVEL / VERSION / HELP
# ==========================================
echo "--- 1. Top-Level Commands ---"
run_test "T001" "decapod --version" 0 $DECAPOD --version
run_test "T002" "decapod --help" 0 $DECAPOD --help
run_test "T003" "decapod (no args) → expect error" 1 $DECAPOD

# ==========================================
# 2. INIT
# ==========================================
echo ""
echo "--- 2. Init ---"
run_test "T010" "init (bootstrap)" 0 $DECAPOD init
run_test "T011" "init --force (re-bootstrap)" 0 $DECAPOD init --force
run_test "T012" "init --dry-run" 0 $DECAPOD init --dry-run
run_test "T013" "init --all" 0 $DECAPOD init --all
run_test "T014" "init --claude" 0 $DECAPOD init --claude
run_test "T015" "init --gemini" 0 $DECAPOD init --gemini
run_test "T016" "init --agents" 0 $DECAPOD init --agents
run_test "T017" "init clean" 0 $DECAPOD init clean
# Re-init after clean
$DECAPOD init --force >/dev/null 2>&1
run_test "T018" "init (alias: i)" 0 $DECAPOD i

# ==========================================
# 3. SETUP
# ==========================================
echo ""
echo "--- 3. Setup ---"
run_test "T020" "setup hook --commit-msg" 0 $DECAPOD setup hook --commit-msg
run_test "T021" "setup hook --pre-commit" 0 $DECAPOD setup hook --pre-commit
run_test "T022" "setup hook --uninstall" 0 $DECAPOD setup hook --uninstall
run_test "T023" "setup --help" 0 $DECAPOD setup --help

# ==========================================
# 4. DOCS
# ==========================================
echo ""
echo "--- 4. Docs ---"
run_test "T030" "docs show core/DECAPOD.md" 0 $DECAPOD docs show core/DECAPOD.md
run_test "T031" "docs show specs/INTENT.md" 0 $DECAPOD docs show specs/INTENT.md
run_test "T032" "docs show plugins/TODO.md" 0 $DECAPOD docs show plugins/TODO.md
run_test "T033" "docs ingest" 0 $DECAPOD docs ingest
run_test "T034" "docs override" 0 $DECAPOD docs override
run_test "T035" "docs --help" 0 $DECAPOD docs --help
run_test "T036" "docs (alias: d)" 0 $DECAPOD d show core/DECAPOD.md
run_test "T037" "docs show nonexistent.md → expect error" 1 $DECAPOD docs show nonexistent.md

# ==========================================
# 5. TODO
# ==========================================
echo ""
echo "--- 5. Todo ---"
run_test "T040" "todo add basic" 0 "$DECAPOD todo add 'Test task 1' --description 'A test task'"
run_test "T041" "todo add minimal" 0 "$DECAPOD todo add 'Test task 2'"
run_test "T042" "todo list" 0 $DECAPOD todo list
run_test "T043" "todo --format json list" 0 $DECAPOD todo --format json list
run_test "T044" "todo --format text list" 0 $DECAPOD todo --format text list

# Get the first task ID for subsequent tests
TASK_ID=$($DECAPOD todo --format json list 2>/dev/null | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
if [ -z "$TASK_ID" ]; then
  echo "  [WARN] Could not extract task ID, using fallback"
  TASK_ID="UNKNOWN"
fi
echo "  [info] Using task ID: $TASK_ID"

run_test "T045" "todo get" 0 $DECAPOD todo get --id "$TASK_ID"
run_test "T046" "todo claim" 0 "$DECAPOD todo claim --id $TASK_ID --agent test-agent"
run_test "T047" "todo comment (--comment flag)" 0 "$DECAPOD todo comment --id $TASK_ID --comment 'Test comment'"
run_test "T048" "todo edit" 0 "$DECAPOD todo edit --id $TASK_ID --title 'Updated task title'"
run_test "T049" "todo release" 0 $DECAPOD todo release --id "$TASK_ID"
run_test "T050" "todo done" 0 $DECAPOD todo done --id "$TASK_ID"

# Get second task for --validated test
TASK_ID2=$($DECAPOD todo --format json list 2>/dev/null | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
run_test "T051" "todo done --validated" 0 $DECAPOD todo done --id "$TASK_ID2" --validated
run_test "T052" "todo categories" 0 $DECAPOD todo categories
run_test "T053" "todo rebuild" 0 $DECAPOD todo rebuild
run_test "T054" "todo archive --id" 0 $DECAPOD todo archive --id "$TASK_ID"
run_test "T055" "todo (alias: t) list" 0 $DECAPOD t list
run_test "T056" "todo --help" 0 $DECAPOD todo --help
run_test "T057" "todo add with all opts" 0 "$DECAPOD todo add 'Full task' --description 'desc' --priority high --tags 'bug,ux' --owner 'dev1'"
run_test "T058" "todo get nonexistent" 0 $DECAPOD todo get --id NONEXISTENT_ID_12345
run_test "T059" "todo add with --ref" 0 "$DECAPOD todo add 'Ref task' --ref 'issue#42'"
run_test "T05A" "todo add with --parent" 0 "$DECAPOD todo add 'Child task' --parent $TASK_ID"
run_test "T05B" "todo add with --depends-on" 0 "$DECAPOD todo add 'Dep task' --depends-on $TASK_ID"
run_test "T05C" "todo add with --blocks" 0 "$DECAPOD todo add 'Block task' --blocks $TASK_ID"

# ==========================================
# 6. VALIDATE
# ==========================================
echo ""
echo "--- 6. Validate ---"
# Validate may fail with exit 1 if checks don't pass - that's a validation result, not a crash
run_test "T060" "validate (default)" 0 $DECAPOD validate
run_test "T061" "validate --store user" 0 $DECAPOD validate --store user
run_test "T062" "validate --store repo" 0 $DECAPOD validate --store repo
run_test "T063" "validate --format json" 0 $DECAPOD validate --format json
run_test "T064" "validate --format text" 0 $DECAPOD validate --format text
run_test "T065" "validate (alias: v)" 0 $DECAPOD v
run_test "T066" "validate invalid store → expect error" 1 $DECAPOD validate --store invalid
run_test "T067" "validate invalid format → expect error" 1 $DECAPOD validate --format invalid

# ==========================================
# 7. GOVERN - POLICY
# ==========================================
echo ""
echo "--- 7. Govern > Policy ---"
run_test "T070" "policy eval dangerous cmd" 0 "$DECAPOD govern policy eval --command 'rm -rf /' --path '/tmp/test'"
run_test "T071" "policy eval safe cmd" 0 "$DECAPOD govern policy eval --command 'ls' --path '.'"
run_test "T072" "policy riskmap init" 0 $DECAPOD govern policy riskmap init
run_test "T073" "policy riskmap verify" 0 $DECAPOD govern policy riskmap verify
run_test "T074" "policy --help" 0 $DECAPOD govern policy --help
run_test "T075" "policy approve (synthetic id)" 0 $DECAPOD govern policy approve --id TEST_APPROVAL_123

# ==========================================
# 8. GOVERN - HEALTH
# ==========================================
echo ""
echo "--- 8. Govern > Health ---"
run_test "T080" "health claim (correct args)" 0 "$DECAPOD govern health claim --id test-claim-1 --subject 'System is healthy' --kind assertion"
run_test "T081" "health proof (correct args)" 0 "$DECAPOD govern health proof --claim-id test-claim-1 --surface 'manual check' --result pass"
run_test "T082" "health get (correct args)" 0 $DECAPOD govern health get --id test-claim-1
run_test "T083" "health summary" 0 $DECAPOD govern health summary
run_test "T084" "health autonomy" 0 $DECAPOD govern health autonomy
run_test "T085" "health --help" 0 $DECAPOD govern health --help
run_test "T086" "health claim with provenance" 0 "$DECAPOD govern health claim --id test-claim-2 --subject 'Has tests' --kind proof --provenance 'test suite'"

# ==========================================
# 9. GOVERN - PROOF
# ==========================================
echo ""
echo "--- 9. Govern > Proof ---"
run_test "T090" "proof run" 0 $DECAPOD govern proof run
run_test "T091" "proof test --name" 0 $DECAPOD govern proof test --name schema-check
run_test "T092" "proof list" 0 $DECAPOD govern proof list
run_test "T093" "proof --help" 0 $DECAPOD govern proof --help

# ==========================================
# 10. GOVERN - WATCHER
# ==========================================
echo ""
echo "--- 10. Govern > Watcher ---"
run_test "T100" "watcher run" 0 $DECAPOD govern watcher run
run_test "T101" "watcher --help" 0 $DECAPOD govern watcher --help

# ==========================================
# 11. GOVERN - FEEDBACK
# ==========================================
echo ""
echo "--- 11. Govern > Feedback ---"
run_test "T110" "feedback add (correct args)" 0 "$DECAPOD govern feedback add --source 'test-agent' --text 'Test feedback'"
run_test "T111" "feedback add with links" 0 "$DECAPOD govern feedback add --source 'test-agent' --text 'Feedback with link' --links 'specs/INTENT.md'"
run_test "T112" "feedback propose" 0 $DECAPOD govern feedback propose
run_test "T113" "feedback --help" 0 $DECAPOD govern feedback --help

# ==========================================
# 12. DATA - ARCHIVE
# ==========================================
echo ""
echo "--- 12. Data > Archive ---"
run_test "T120" "archive list" 0 $DECAPOD data archive list
run_test "T121" "archive verify" 0 $DECAPOD data archive verify
run_test "T122" "archive --help" 0 $DECAPOD data archive --help

# ==========================================
# 13. DATA - KNOWLEDGE
# ==========================================
echo ""
echo "--- 13. Data > Knowledge ---"
run_test "T130" "knowledge add (correct args)" 0 "$DECAPOD data knowledge add --id kb-001 --title 'Test entry' --text 'Some knowledge text' --provenance 'manual'"
run_test "T131" "knowledge add with claim-id" 0 "$DECAPOD data knowledge add --id kb-002 --title 'Linked entry' --text 'Knowledge with claim' --provenance 'manual' --claim-id test-claim-1"
run_test "T132" "knowledge search" 0 "$DECAPOD data knowledge search --query 'test'"
run_test "T133" "knowledge --help" 0 $DECAPOD data knowledge --help

# ==========================================
# 14. DATA - CONTEXT
# ==========================================
echo ""
echo "--- 14. Data > Context ---"
# Create a test file for context commands
echo "some content" > "$WORKDIR/test_file.txt"
run_test "T140" "context audit (correct args)" 0 "$DECAPOD data context audit --profile main --files $WORKDIR/test_file.txt"
run_test "T141" "context pack (correct args)" 0 "$DECAPOD data context pack --path $WORKDIR/test_file.txt --summary 'Test context pack'"
run_test "T142" "context restore (correct args)" 0 "$DECAPOD data context restore --id ctx-001 --profile main --current-files $WORKDIR/test_file.txt"
run_test "T143" "context --help" 0 $DECAPOD data context --help

# ==========================================
# 15. DATA - SCHEMA
# ==========================================
echo ""
echo "--- 15. Data > Schema ---"
run_test "T150" "schema (default)" 0 $DECAPOD data schema
run_test "T151" "schema --format json" 0 $DECAPOD data schema --format json
run_test "T152" "schema --format md" 0 $DECAPOD data schema --format md
run_test "T153" "schema --deterministic" 0 $DECAPOD data schema --deterministic
run_test "T154" "schema --subsystem todo" 0 $DECAPOD data schema --subsystem todo
run_test "T155" "schema --subsystem health" 0 $DECAPOD data schema --subsystem health
run_test "T156" "schema --subsystem policy" 0 $DECAPOD data schema --subsystem policy
run_test "T157" "schema invalid subsystem" 0 $DECAPOD data schema --subsystem nonexistent

# ==========================================
# 16. DATA - REPO
# ==========================================
echo ""
echo "--- 16. Data > Repo ---"
run_test "T160" "repo map" 0 $DECAPOD data repo map
run_test "T161" "repo graph" 0 $DECAPOD data repo graph
run_test "T162" "repo --help" 0 $DECAPOD data repo --help

# ==========================================
# 17. DATA - BROKER
# ==========================================
echo ""
echo "--- 17. Data > Broker ---"
run_test "T170" "broker audit" 0 $DECAPOD data broker audit
run_test "T171" "broker --help" 0 $DECAPOD data broker --help

# ==========================================
# 18. DATA - TEAMMATE
# ==========================================
echo ""
echo "--- 18. Data > Teammate ---"
run_test "T180" "teammate add (correct args)" 0 "$DECAPOD data teammate add --category style --key 'theme' --value 'dark mode'"
run_test "T181" "teammate add with context" 0 "$DECAPOD data teammate add --category workflow --key 'editor' --value 'neovim' --context 'coding sessions' --source user_request"
run_test "T182" "teammate list" 0 $DECAPOD data teammate list
run_test "T183" "teammate get (correct args)" 0 "$DECAPOD data teammate get --category style --key theme"
run_test "T184" "teammate observe (correct args)" 0 "$DECAPOD data teammate observe --content 'User prefers concise responses'"
run_test "T185" "teammate observe with category" 0 "$DECAPOD data teammate observe --content 'Always uses dark mode' --category style"
run_test "T186" "teammate prompt (no args)" 0 $DECAPOD data teammate prompt
run_test "T187" "teammate prompt with context" 0 "$DECAPOD data teammate prompt --context 'code review'"
run_test "T188" "teammate prompt --format json" 0 "$DECAPOD data teammate prompt --format json"
run_test "T189" "teammate --help" 0 $DECAPOD data teammate --help

# ==========================================
# 19. AUTO - CRON
# ==========================================
echo ""
echo "--- 19. Auto > Cron ---"
run_test "T190" "cron add" 0 "$DECAPOD auto cron add --name 'test-cron' --schedule '0 * * * *' --command 'echo hello'"
run_test "T191" "cron add with opts" 0 "$DECAPOD auto cron add --name 'full-cron' --schedule '*/5 * * * *' --command 'echo world' --description 'A test cron' --tags 'test,dev'"
run_test "T192" "cron list" 0 $DECAPOD auto cron list

# Get cron ID
CRON_ID=$($DECAPOD auto cron list 2>&1 | grep -oE '[0-9A-Z]{26}' | head -1 || echo "")
if [ -z "$CRON_ID" ]; then
  echo "  [WARN] Could not extract cron ID"
  CRON_ID="UNKNOWN"
fi
echo "  [info] Using cron ID: $CRON_ID"

run_test "T193" "cron get" 0 $DECAPOD auto cron get --id "$CRON_ID"
run_test "T194" "cron update" 0 "$DECAPOD auto cron update --id $CRON_ID --schedule '*/10 * * * *'"
run_test "T195" "cron list --status" 0 "$DECAPOD auto cron list --status active"
run_test "T196" "cron list --name-search" 0 "$DECAPOD auto cron list --name-search test"
run_test "T197" "cron delete" 0 $DECAPOD auto cron delete --id "$CRON_ID"
run_test "T198" "cron --help" 0 $DECAPOD auto cron --help

# ==========================================
# 20. AUTO - REFLEX
# ==========================================
echo ""
echo "--- 20. Auto > Reflex ---"
run_test "T200" "reflex add (correct args)" 0 "$DECAPOD auto reflex add --name 'test-reflex' --trigger-type event --action-type command --action-config 'echo done'"
run_test "T201" "reflex add with opts" 0 "$DECAPOD auto reflex add --name 'full-reflex' --trigger-type event --action-type command --action-config 'echo world' --description 'A test reflex' --tags 'test'"
run_test "T202" "reflex list" 0 $DECAPOD auto reflex list

REFLEX_ID=$($DECAPOD auto reflex list 2>&1 | grep -oE '[0-9A-Z]{26}' | head -1 || echo "")
if [ -z "$REFLEX_ID" ]; then
  echo "  [WARN] Could not extract reflex ID"
  REFLEX_ID="UNKNOWN"
fi
echo "  [info] Using reflex ID: $REFLEX_ID"

run_test "T203" "reflex get" 0 $DECAPOD auto reflex get --id "$REFLEX_ID"
run_test "T204" "reflex update" 0 "$DECAPOD auto reflex update --id $REFLEX_ID --name 'updated-reflex'"
run_test "T205" "reflex list --status" 0 "$DECAPOD auto reflex list --status active"
run_test "T206" "reflex delete" 0 $DECAPOD auto reflex delete --id "$REFLEX_ID"
run_test "T207" "reflex --help" 0 $DECAPOD auto reflex --help

# ==========================================
# 21. QA - VERIFY
# ==========================================
echo ""
echo "--- 21. QA > Verify ---"
# Add a fresh task for verification
$DECAPOD todo add 'Verify test task' --description 'For QA verify' >/dev/null 2>&1
VER_TASK_ID=$($DECAPOD todo --format json list 2>/dev/null | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
if [ -z "$VER_TASK_ID" ]; then
  VER_TASK_ID="UNKNOWN"
fi
echo "  [info] Using verify task ID: $VER_TASK_ID"

run_test "T210" "verify todo (positional ID)" 0 "$DECAPOD qa verify todo $VER_TASK_ID"
run_test "T211" "verify --stale" 0 $DECAPOD qa verify --stale
run_test "T212" "verify --json" 0 $DECAPOD qa verify --json
run_test "T213" "verify --help" 0 $DECAPOD qa verify --help

# ==========================================
# 22. QA - CHECK
# ==========================================
echo ""
echo "--- 22. QA > Check ---"
run_test "T220" "check (no flags)" 0 $DECAPOD qa check
run_test "T221" "check --crate-description" 0 $DECAPOD qa check --crate-description
run_test "T222" "check --all" 0 $DECAPOD qa check --all
run_test "T223" "check --help" 0 $DECAPOD qa check --help

# ==========================================
# 23. UPDATE
# ==========================================
echo ""
echo "--- 23. Update ---"
run_test "T230" "update --help" 0 $DECAPOD update --help

# ==========================================
# 24. GROUP HELP COMMANDS
# ==========================================
echo ""
echo "--- 24. Group Help ---"
run_test "T240" "govern --help" 0 $DECAPOD govern --help
run_test "T241" "govern (alias: g) --help" 0 $DECAPOD g --help
run_test "T250" "data --help" 0 $DECAPOD data --help
run_test "T260" "auto --help" 0 $DECAPOD auto --help
run_test "T261" "auto (alias: a) --help" 0 $DECAPOD a --help
run_test "T270" "qa --help" 0 $DECAPOD qa --help
run_test "T271" "qa (alias: q) --help" 0 $DECAPOD q --help

# ==========================================
# 25. EDGE CASES & ERROR PATHS
# ==========================================
echo ""
echo "--- 25. Edge Cases (expected failures) ---"
run_test "T280" "invalid subcommand" 1 $DECAPOD notacommand
run_test "T281" "todo add empty string" 0 "$DECAPOD todo add ''"
run_test "T282" "todo get missing --id" 1 $DECAPOD todo get
run_test "T283" "docs show empty path" 1 "$DECAPOD docs show ''"
run_test "T284" "knowledge add missing required" 1 "$DECAPOD data knowledge add --id kb-only"
run_test "T285" "cron add missing schedule" 1 "$DECAPOD auto cron add --name 'bad' --command 'echo'"
run_test "T286" "reflex add missing trigger-type" 1 "$DECAPOD auto reflex add --name 'bad' --action-type command --action-config 'echo'"
run_test "T287" "teammate get missing key" 1 "$DECAPOD data teammate get --category style"
run_test "T288" "health claim missing required" 1 "$DECAPOD govern health claim --id only-id"
run_test "T289" "context audit missing files" 1 "$DECAPOD data context audit --profile main"

# ==========================================
# SUMMARY
# ==========================================
echo ""
echo "=========================================="
echo "  GATLING TEST v2 COMPLETE"
echo "  Total: $TOTAL | Pass: $PASS | Fail: $FAIL"
echo "  Pass Rate: $(( PASS * 100 / TOTAL ))%"
echo "  Results: $RESULTS_FILE"
echo "  Workdir: $WORKDIR"
echo "=========================================="

# Cleanup
rm -rf "$WORKDIR"
