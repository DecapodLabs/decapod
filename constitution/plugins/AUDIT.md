# Decapod CLI Gatling Audit

**Date:** 2026-02-13
**Version:** 0.3.2
**Test Harness:** `dev/gatling_test.sh` (v2)
**Environment:** Isolated temp git repo, `cargo run --quiet`

---

## Executive Summary

| Metric | Value |
|--------|-------|
| Total tests | 155 |
| Pass | 139 |
| Fail | 16 |
| Pass rate | 89% |
| Critical bugs | 2 |
| Stubs / not-implemented | 1 |
| Environmental (test-env only) | 9 |
| Undocumented validation rules | 1 |
| Edge case behavior questions | 3 |

**Bottom line:** Two critical bugs exist. The `todo rebuild` event replay handler is missing support for 3 event types that the CLI actively emits (`task.edit`, `task.claim`, `task.release`). This cascades into `decapod validate` (which internally calls rebuild for determinism checks), making validation universally broken on any repo that has ever used `todo edit`, `todo claim`, or `todo release`.

---

## Critical Bugs

### BUG-1: `todo rebuild` missing event handlers (CRITICAL)

**Severity:** Critical — breaks rebuild, validate, and determinism guarantees
**Test IDs:** T053, T060, T062, T063, T064, T065
**File:** `src/plugins/todo.rs:1483-1488`

**Root cause:** The `rebuild_db_from_events()` function has a match arm for replaying events from `todo.events.jsonl`. It handles:
- `task.add` (line 1334)
- `task.done` (line 1409)
- `task.archive` (line 1416)
- `task.comment` (line 1423) — no-op, correct
- `task.verify.capture` | `task.verify.result` (line 1424)

But the CLI emits three additional event types that the handler does **not** recognize:
- **`task.edit`** (emitted by `todo edit`, line 986)
- **`task.claim`** (emitted by `todo claim`, line 1062)
- **`task.release`** (emitted by `todo release`, line 1122)

Any `todo.events.jsonl` containing these events causes rebuild to fail with:
```
Error: ValidationError("Unknown event_type 'task.edit'")
```

**Cascade:** `decapod validate` calls `todo::rebuild_db_from_events()` internally (at `src/core/validate.rs:330`) to verify deterministic rebuild. Since any real-world repo will contain these events, **validation is broken for all repos that use edit/claim/release**.

**Fix:** Add match arms in the rebuild handler for `task.edit` (apply partial updates to task fields), `task.claim` (update `assigned_to`, `assigned_at`), and `task.release` (clear `assigned_to`, `assigned_at`).

---

### BUG-2: `todo --format json list` ID extraction is non-obvious

**Severity:** Medium — functional but affects tooling interoperability
**Test IDs:** T051 (indirect — caused task ID to be `UNKNOWN`)

**Root cause:** The JSON output from `todo --format json list` wraps tasks in a `{"items": [...]}` envelope. The task IDs use the format `R_01KHDB64XNXW2ZA81AF8C6MX4H`. Simple `grep -o '"id":"[^"]*"'` extraction can fail depending on JSON formatting. In the test, the second task ID extraction returned empty, causing `todo done --id --validated` to fail with "a value is required for '--id'".

This is not a CLI bug per se, but the JSON format makes programmatic extraction fragile. Consider adding a `--quiet` or `--ids-only` mode for scripting.

---

## Stubs / Not Implemented

### STUB-1: `govern proof test --name` is a stub

**Test ID:** T091
**File:** `src/lib.rs` (ProofSubCommand::Test)
**Error:** `NotImplemented("Individual proof testing not yet implemented")`

The `govern proof test --name <NAME>` subcommand exists in the CLI (Clap accepts it) but the handler immediately returns a `NotImplemented` error. This is a documented stub. Either implement it or remove the subcommand to avoid confusion.

---

## Environmental / Test-Context Failures

These failures are **not bugs** — they fail because the test runs in an isolated temp directory without real project context.

### ENV-1: `validate` fails in temp repo (T060, T062-T065)

Validation performs methodology compliance checks (AGENTS.md exists, entrypoints present, event log determinism, etc.). A minimal temp repo with only `README.md` naturally fails most checks. The exit code 1 here is correct behavior — it means "validation found issues", not "the tool crashed".

**However:** The validate failure is *also* hit by BUG-1 (the `task.edit` rebuild crash). In a real repo, validate would crash rather than report failures cleanly. Once BUG-1 is fixed, validate should produce a clean pass/fail report even if some checks fail.

### ENV-2: `qa check --crate-description` fails in temp repo (T221, T222)

The check runs `cargo metadata --no-deps` in the CWD. In the temp directory, there is no `Cargo.toml`, so `cargo metadata` returns empty output and the description match fails. This is correct behavior — the command is designed to run inside the decapod project itself.

### ENV-3: `context restore` with fake archive ID (T142)

`Error: ValidationError("Archive 'ctx-001' not found")` — Expected. The archive ID `ctx-001` doesn't exist. The command correctly validates and rejects.

### ENV-4: `todo archive --id UNKNOWN` requires policy approval (T054)

`Error: ValidationError("Action 'task.archive' on 'UNKNOWN' is high risk and lacks approval.")` — The archive action requires policy approval (`decapod govern policy approve`). The task ID was also `UNKNOWN` due to ENV-related extraction failure. Both the policy gate and the error message are correct.

### ENV-5: `verify todo` with UNKNOWN task ID (T210)

`Error: NotFound("TODO not found")` — Task ID was `UNKNOWN` due to extraction failure (see BUG-2). The error handling is correct.

---

## Undocumented Validation Rules

### RULE-1: Knowledge `--provenance` requires a scheme prefix

**Test IDs:** T130, T131
**File:** `src/plugins/knowledge.rs:36-40`

The `data knowledge add --provenance` flag requires a URI-like scheme prefix. Accepted schemes:
```
file: | url: | cmd: | commit: | event:
```

Example: `--provenance 'file:src/main.rs'` works; `--provenance 'manual'` does not.

**Issue:** This is not documented in `--help` output or error message guidance. The error message tells you the valid schemes, which is good, but `--help` should mention this requirement. Agents calling this command for the first time will waste a round-trip.

**Correct usage:**
```bash
decapod data knowledge add --id kb-001 --title 'Entry' --text 'Content' --provenance 'cmd:manual-entry'
```

---

## Edge Case Behavior Notes

### EDGE-1: `todo add ''` succeeds (T281)

Adding a task with an empty string title succeeds. This may or may not be intentional. Consider validating that titles are non-empty.

### EDGE-2: `todo get --id NONEXISTENT` succeeds (T058)

Getting a nonexistent task returns exit 0 (with presumably empty/null output). Consider returning exit 1 or a clear "not found" message.

### EDGE-3: `context audit --profile main` with no `--files` succeeds (T289)

The `--files` parameter is a `Vec<PathBuf>`, so an empty vec is valid Clap input. The command reports "0 / 32000 tokens" and exits 0. This is arguably correct but could be surprising.

---

## Full Test Results by Subsystem

### 1. Top-Level (3/3 PASS)

| ID | Command | Status |
|----|---------|--------|
| T001 | `--version` | PASS |
| T002 | `--help` | PASS |
| T003 | `(no args)` | PASS (expected error) |

### 2. Init (9/9 PASS)

| ID | Command | Status |
|----|---------|--------|
| T010 | `init` | PASS |
| T011 | `init --force` | PASS |
| T012 | `init --dry-run` | PASS |
| T013 | `init --all` | PASS |
| T014 | `init --claude` | PASS |
| T015 | `init --gemini` | PASS |
| T016 | `init --agents` | PASS |
| T017 | `init clean` | PASS |
| T018 | `i` (alias) | PASS |

### 3. Setup (4/4 PASS)

| ID | Command | Status |
|----|---------|--------|
| T020 | `setup hook --commit-msg` | PASS |
| T021 | `setup hook --pre-commit` | PASS |
| T022 | `setup hook --uninstall` | PASS |
| T023 | `setup --help` | PASS |

### 4. Docs (8/8 PASS)

| ID | Command | Status |
|----|---------|--------|
| T030 | `docs show core/DECAPOD.md` | PASS |
| T031 | `docs show specs/INTENT.md` | PASS |
| T032 | `docs show plugins/TODO.md` | PASS |
| T033 | `docs ingest` | PASS |
| T034 | `docs override` | PASS |
| T035 | `docs --help` | PASS |
| T036 | `d show` (alias) | PASS |
| T037 | `docs show nonexistent.md` | PASS (expected error) |

### 5. Todo (18/20 — 2 FAIL)

| ID | Command | Status | Notes |
|----|---------|--------|-------|
| T040 | `todo add` (basic) | PASS | |
| T041 | `todo add` (minimal) | PASS | |
| T042 | `todo list` | PASS | |
| T043 | `todo --format json list` | PASS | |
| T044 | `todo --format text list` | PASS | |
| T045 | `todo get` | PASS | |
| T046 | `todo claim` | PASS | |
| T047 | `todo comment` | PASS | |
| T048 | `todo edit` | PASS | |
| T049 | `todo release` | PASS | |
| T050 | `todo done` | PASS | |
| T051 | `todo done --validated` | **FAIL** | BUG-2: ID extraction failed |
| T052 | `todo categories` | PASS | |
| T053 | `todo rebuild` | **FAIL** | BUG-1: `task.edit` unhandled |
| T054 | `todo archive` | ENV-4 | Policy gate (correct behavior) |
| T055 | `t list` (alias) | PASS | |
| T056 | `todo --help` | PASS | |
| T057 | `todo add` (all opts) | PASS | |
| T058 | `todo get` (nonexistent) | PASS | See EDGE-2 |
| T059 | `todo add --ref` | PASS | |
| T05A | `todo add --parent` | PASS | |
| T05B | `todo add --depends-on` | PASS | |
| T05C | `todo add --blocks` | PASS | |

### 6. Validate (2/8 — 6 FAIL)

| ID | Command | Status | Notes |
|----|---------|--------|-------|
| T060 | `validate` | **FAIL** | BUG-1 cascade (crash, not clean fail) |
| T061 | `validate --store user` | PASS | |
| T062 | `validate --store repo` | **FAIL** | BUG-1 cascade |
| T063 | `validate --format json` | **FAIL** | BUG-1 cascade |
| T064 | `validate --format text` | **FAIL** | BUG-1 cascade |
| T065 | `v` (alias) | **FAIL** | BUG-1 cascade |
| T066 | `validate --store invalid` | PASS (expected error) | |
| T067 | `validate --format invalid` | PASS (expected error) | |

### 7. Policy (6/6 PASS)

All pass. Full CRUD + riskmap init/verify + approve working correctly.

### 8. Health (7/7 PASS)

All pass. Claim, proof, get, summary, autonomy all working correctly with proper argument signatures.

### 9. Proof (3/4 — 1 FAIL)

| ID | Command | Status | Notes |
|----|---------|--------|-------|
| T090 | `proof run` | PASS | |
| T091 | `proof test --name` | **FAIL** | STUB-1: NotImplemented |
| T092 | `proof list` | PASS | |
| T093 | `proof --help` | PASS | |

### 10. Watcher (2/2 PASS)

### 11. Feedback (4/4 PASS)

### 12. Archive (3/3 PASS)

### 13. Knowledge (2/4 — 2 FAIL)

| ID | Command | Status | Notes |
|----|---------|--------|-------|
| T130 | `knowledge add` | **FAIL** | RULE-1: provenance needs scheme |
| T131 | `knowledge add` (claim-id) | **FAIL** | RULE-1: same |
| T132 | `knowledge search` | PASS | |
| T133 | `knowledge --help` | PASS | |

### 14. Context (3/4 — 1 FAIL)

| ID | Command | Status | Notes |
|----|---------|--------|-------|
| T140 | `context audit` | PASS | |
| T141 | `context pack` | PASS | |
| T142 | `context restore` | **FAIL** | ENV-3: fake archive ID |
| T143 | `context --help` | PASS | |

### 15. Schema (8/8 PASS)

All pass, including invalid subsystem (graceful handling).

### 16. Repo (3/3 PASS)

### 17. Broker (2/2 PASS)

### 18. Aptitude (10/10 PASS)

Full CRUD cycle: add, list, get, observe, prompt all working.

### 19. Cron (9/9 PASS)

Full CRUD cycle: add, list, get, update, delete all working.

### 20. Reflex (8/8 PASS)

Full CRUD cycle: add, list, get, update, delete all working.

### 21. Verify (3/4 — 1 FAIL)

| ID | Command | Status | Notes |
|----|---------|--------|-------|
| T210 | `verify todo` | **FAIL** | ENV-5: UNKNOWN task ID |
| T211 | `verify --stale` | PASS | |
| T212 | `verify --json` | PASS | |
| T213 | `verify --help` | PASS | |

### 22. Check (2/4 — 2 FAIL)

| ID | Command | Status | Notes |
|----|---------|--------|-------|
| T220 | `check` | PASS | |
| T221 | `check --crate-description` | **FAIL** | ENV-2: no Cargo.toml in temp dir |
| T222 | `check --all` | **FAIL** | ENV-2: same |
| T223 | `check --help` | PASS | |

### 23-27. Help/Alias Commands (8/8 PASS)

All group-level help and alias commands work correctly.

### 28. Edge Cases (9/10 — 1 unexpected)

| ID | Command | Status | Notes |
|----|---------|--------|-------|
| T280 | invalid subcommand | PASS (expected error) | |
| T281 | `todo add ''` | PASS | See EDGE-1 |
| T282 | `todo get` (no --id) | PASS (expected error) | |
| T283 | `docs show ''` | PASS (expected error) | |
| T284 | `knowledge add` (missing fields) | PASS (expected error) | |
| T285 | `cron add` (missing schedule) | PASS (expected error) | |
| T286 | `reflex add` (missing trigger) | PASS (expected error) | |
| T287 | `aptitude get` (missing key) | PASS (expected error) | |
| T288 | `health claim` (missing fields) | PASS (expected error) | |
| T289 | `context audit` (no files) | **FAIL** | See EDGE-3: succeeds when error expected |

---

## Subsystem CLI Coverage Map

Shows which subcommands actually exist vs. what the constitution documents suggest.

| Plugin | Documented Commands | Missing from CLI | Extra in CLI |
|--------|-------------------|------------------|--------------|
| cron | add, update, get, list, delete, delete-all, enable, disable | **delete-all, enable, disable** | — |
| reflex | add, update, get, list, delete, delete-all, enable, disable | **delete-all, enable, disable** | — |
| todo | add, list, get, done, claim, release, rebuild, archive, comment, edit, categories | — | — |
| aptitude | add, get, list, observe, prompt, infer | **infer** | — |

The constitution/docs reference `cron disable`, `cron enable`, `cron delete-all`, `reflex disable`, `reflex enable`, `reflex delete-all`, and `aptitude infer` — but these subcommands **do not exist in the CLI**. Either the docs are aspirational or the implementations were dropped.

---

## Recommended Fix Priority

1. **BUG-1** (Critical): Add `task.edit`, `task.claim`, `task.release` to `rebuild_db_from_events()` in `src/plugins/todo.rs`. This unblocks validate and rebuild for all real-world repos.

2. **STUB-1** (Medium): Either implement `govern proof test --name` or remove the subcommand.

3. **RULE-1** (Low): Add provenance format hint to `knowledge add --help` output.

4. **Doc drift** (Low): Reconcile constitution docs with actual CLI for cron/reflex/aptitude missing subcommands.

5. **EDGE-1** (Low): Consider rejecting empty-string task titles.

6. **EDGE-2** (Low): Consider returning exit 1 for `todo get --id <nonexistent>`.

---

## Reproduction

```bash
# Run the full gatling test
bash dev/gatling_test.sh

# Reproduce BUG-1 specifically
cd $(mktemp -d) && git init -q . && git config user.email "t@t" && git config user.name "t"
touch README.md && git add . && git commit -q -m "init"
decapod init
decapod todo add 'Test'
TASK_ID=$(decapod todo --format json list | jq -r '.items[0].id')
decapod todo edit --id $TASK_ID --title 'Edited'
decapod todo rebuild  # CRASH: Unknown event_type 'task.edit'
decapod validate      # CRASH: same root cause
```
