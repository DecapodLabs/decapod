# Empirical Study: Independent Task-First Read-Only Review (`#1796`)

Cross-references: `#1796`, `#1095`, `#1762`.

## Objective & Design

Evaluate whether a single fresh-context, read-only reviewer given **only** the accepted task specification (`TASK.md`) and candidate diff (`git diff HEAD~1..HEAD`) — with **zero** access to the implementer's prose claims or execution transcript — catches subtle behavioral defects that deterministic baseline unit tests miss, at acceptable incremental cost and without introducing false alarms or write-side mutations.

### Benchmark Corpus (`n = 4` Fixed Rust Coding Tasks)

Every candidate commit compiles cleanly and passes its deterministic baseline `cargo test` suite (`baseline_cargo_test_passed: true` on 4/4 cases, giving baseline defect recall = `0.00`).

| Case ID | Ground Truth | Seeded Defect / Control Description | Baseline `cargo test` |
| --- | --- | --- | --- |
| `case_1_bug_prefix_auth` | `defect_found` | `authorize_repo` uses `repo_path.starts_with(allowed_prefix)` without checking `/` segment boundary, allowing `"org/repo-evil"` for prefix `"org/repo"`. Existing tests check only `"org/repo/src/lib.rs"`, `"org/repo"`, `"other/repo"`, and `""`. | Passed (missed defect) |
| `case_2_bug_ttl_boundary` | `defect_found` | `is_receipt_fresh` uses `now <= issued_at.saturating_add(ttl_secs)` (off-by-one at exact expiry) and fails open when `now < issued_at` (negative clock skew). Existing tests check only `now=120`, `now=200`, and `ttl=0`. | Passed (missed defect) |
| `case_3_clean_budget_cap` | `no_defect` | `admit_payload(bytes_len, max_bytes)` correctly rejects `0` length, `0` budget, and `> max_bytes`. | Passed (clean control) |
| `case_4_clean_path_traversal` | `no_defect` | `is_safe_rel_path(path)` correctly rejects empty, absolute, backslash, NUL, and `..` segments while allowing `docs/a..b.md`. | Passed (clean control) |

### Execution Protocol & Pinned Environment (2026-11-22 UTC)

- **Reviewer host:** `codex-cli 0.169.0` (`/opt/homebrew/bin/codex`, SHA-256 `739cda86e15d30a6e2b40cf8dcbbfb988aaf48b305b27065573bb8e9fd41d002`).
- **Invocation:** `codex exec --ephemeral --json --sandbox read-only -C <case_dir>` — strictly fresh session per task, read-only sandbox, zero implementer transcript, required to emit structured JSON `{verdict, cited_requirement, counterexample}`.
- **Authority boundary:** Reviewer output is **advisory only** unless backed by an executable counterexample or failed check; a reviewer's `no_defect` verdict never authorizes merge or waives `decapod validate`.

## Measured Results

| Case ID | Ground Truth | Reviewer Verdict | Cited Counterexample | Wall Time (s) | Commands | Input / Cached / Output Tokens | Tracked Source Mutated? |
| --- | --- | --- | --- | ---: | ---: | --- | --- |
| `case_1_bug_prefix_auth` | `defect_found` | **`defect_found`** (TP) | `authorize_repo("org/repo-evil", "org/repo")` returns `true` instead of `false` | `10.46` | 4 | `51,060` / `38,656` / `264` | No (`?? Cargo.lock` pre-existing from baseline `cargo test`) |
| `case_2_bug_ttl_boundary` | `defect_found` | **`defect_found`** (TP) | `is_receipt_fresh(100, 60, 160)` and `is_receipt_fresh(100, 60, 99)` both return `true` instead of `false` | `11.60` | 3 | `51,194` / `38,656` / `293` | No (`?? Cargo.lock` pre-existing from baseline `cargo test`) |
| `case_3_clean_budget_cap` | `no_defect` | **`no_defect`** (TN) | None | `12.12` | 4 | `50,999` / `38,656` / `247` | No (`?? Cargo.lock` pre-existing from baseline `cargo test`) |
| `case_4_clean_path_traversal` | `no_defect` | **`no_defect`** (TN) | None | `16.37` | 4 | `51,207` / `38,656` / `246` | No (`?? Cargo.lock` pre-existing from baseline `cargo test`) |

### Aggregate Summary

- **Baseline deterministic `cargo test` defect recall:** `0.00` (`0 / 2` defects caught).
- **Independent fresh-context read-only reviewer:**
  - **True Positives (TP):** `2` | **False Negatives (FN):** `0` | **False Positives (FP):** `0` | **True Negatives (TN):** `2`
  - **Precision:** `1.00` (`2 / 2`)
  - **Recall:** `1.00` (`2 / 2`)
  - **False-alarm rate:** `0.00` (`0 / 2`)
  - **Mean incremental latency:** `12.64 s` (`10.46 s`–`16.37 s`)
  - **Mean token footprint:** `51,115` gross input (`38,656` cached, `~12,459` uncached) + `262.5` output tokens per task.
  - **Human interventions required:** `0`
  - **Read-only integrity check:** `0 / 4` tracked files modified (`--sandbox read-only` held across all 4 tasks).

## Falsifier Evaluation & Limits

1. **Did the reviewer merely repeat implementer assertions?** No — no implementer assertions were supplied, and both defect verdicts supplied concrete reproducible input counterexamples (`"org/repo-evil"`, `now=160` and `now=99`).
2. **Did the reviewer alter fixtures or self-grant authorization?** No — zero source/test edits occurred, and output remained advisory JSON.
3. **Stale diff guard:** Any optional future reviewer integration MUST bind its advisory receipt to exact `(head_sha, base_sha, task_spec_sha256)` so reviewing a stale diff cannot survive a candidate commit change.
4. **Small-sample limit:** `n = 4` focused Rust tasks is a calibrated pilot establishing signal and protocol viability, not a statistically large benchmark across all languages/repo sizes. Monetary dollar cost was not directly exposed by the CLI JSONL stream and remains uncomputed rather than guessed.

## Product Recommendation (`#1796`)

The benchmark demonstrates strong defect-detection lift (`0.00 -> 1.00` recall with `0.00` false alarms at `~12.6 s` median wall latency) when the reviewer is strictly isolated (fresh context, read-only sandbox, task+diff only). Per `#1796`'s sequencing contract:

- **Ship this benchmark and versioned study report first** (this document).
- **Do NOT add a compulsory critic model, background reviewer scheduler, or new mandatory policy gate to core Decapod.** Any future opt-in harness adapter must remain strictly advisory, bind findings to exact candidate/target commits, and keep `decapod validate` + independent executable checks authoritative.
