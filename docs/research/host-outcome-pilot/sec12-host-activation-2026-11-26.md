# Section 12 Real-Host Activation, 3-Arm Outcome Comparison & Host Blockers (2026-11-26)

Related issues: `#880`, `#1888`, `#1095`, `#1277`, `#875`.

## Local Host Availability & External Blockers (Verified 2026-11-26)

| Host CLI | Version / Binary | Auth / Quota State Observed | Outcome in This Run |
| --- | --- | --- | --- |
| `claude` | `2.1.311 (Claude Code)` (`/opt/homebrew/bin/claude`) | `claude auth status` -> `{"loggedIn":false,"authMethod":"none","apiProvider":"firstParty"}` | Preserved exact external credential blocker for `#1888`; no paid subscription purchased |
| `codex` | `codex-cli 0.172.0` (`/opt/homebrew/bin/codex`) | `codex login status` -> `Logged in using ChatGPT`; live `codex exec` returned `You’ve hit your usage limit. Visit https://chatgpt.com/settings/usage to purchase more credits or try again at 11:34 PM.` | Preserved exact external quota exhaustion blocker; no credits purchased |
| `opencode` | `opencode v2.2.5` (`/opt/homebrew/bin/opencode`) | `opencode auth list` -> `No authenticated integrations` | Preserved external credential gap |
| `pi`, `gemini` | Not installed | `not found` | Packaged harness/extension verification kept offline |
| `antigravity` (`agy`) | `1.5.18` (`/opt/homebrew/bin/agy`, sha256 `89d0b97a9ebcefcaebbdfe4ffeb2de877ef92ee4504de188e35a38489136be13`) | Authenticated locally; verified `--new-project` probe ran baseline with zero injected workspace rules | Executed 6 uncontaminated real-host trials (`baseline` vs `static` vs `governed`) |

## Paired 3-Arm Study Design & Hygiene (`#880`, `#1095`)

- Kernel exercised inside governed runs: `/Users/arx/.cargo/bin/decapod` (`0.166.1`, sha256 `9c912f7e905be3810a6d275ac0e9d70ae650291c9f23168c4d8bef4316f66cbf`).
- Two strata run under an identical `--new-project --output-format stream-json --dangerously-skip-permissions --print-timeout 180s` cap:
  1. **`cold`**: `tests/fixtures/activation_conformance/greeting` with a prompt containing zero mention of `decapod`. (`decapod init` replaced `README.md` in `agy-cold-governed`, producing a distinct `task_source_sha256`; therefore `agy-cold-governed` was assigned its own `pair_id` in [sec12-antigravity-input.json](sec12-antigravity-input.json) rather than forcing a mismatched pair.)
  2. **`adversarial-a-vs-b`**: Identical `SPEC.md` rule (`"Hello, governed fixture!"`) plus 15 repeated superseded lines (`"Hello, legacy fixture!"`) in `NOTES.md`, sharing identical `task_source_sha256` (`1dec1040...`) across all 3 arms.
- Telemetry privacy & anti-fabrication rules:
  - Raw JSONL traces remained outside Git in `/tmp`; only redacted SHA-256-referenced summary records ([sec12-antigravity-input.json](sec12-antigravity-input.json), [sec12-antigravity-comparison.json](sec12-antigravity-comparison.json)) are stored here.
  - Unreviewed human-attention metrics (`active_human_minutes`, `human_interruptions`, `approval_decisions`, `incorrectly_suppressed_escalations`) stay `null` (`missing_reason: "not_reviewed"`). `total_cost_usd` stays `null` (`missing_reason: "not_observed"`).
  - Rendered and validated with `target/debug/decapod trace flight-recorder compare` (`unverified_pair_identities: []`).

## Measured Real-Host Results (`antigravity 1.5.18`, fixed 180s timeout)

| Run ID | Arm | Status | Cargo Test | Rule A / Decoy B | Autonomous `decapod` Calls | Worktree Isolated | Wall Time (ms) | Model Calls | Tool Calls | Retries | Input Tokens | Cached Input | Output Tokens |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `agy-cold-baseline` | `baseline` (`no-file`) | `completed` | `true` | `A=true, B=false` | `0` | `false` | `27,661.3` | `6` | `8` | `0` | `33,557` | `39,702` | `1,503` |
| `agy-cold-static` | `static` (`AGENTS.md`) | `completed` | `true` | `A=true, B=false` | `0` | `false` | `25,126.1` | `5` | `7` | `0` | `31,491` | `30,414` | `1,374` |
| `agy-cold-governed` | `governed` (`decapod`) | `partial` (180s timeout) | `false` | `A=true, B=false` | `10` | `true` | `162,534.2` | `11` | `30` | `0` | `223,200` | `386,297` | `9,662` |
| `agy-adversarial-a-vs-b-baseline` | `baseline` (`no-file`) | `completed` | `true` | `A=true, B=false` | `0` | `false` | `32,964.7` | `7` | `10` | `0` | `36,329` | `53,056` | `2,110` |
| `agy-adversarial-a-vs-b-static` | `static` (`AGENTS.md`) | `completed` | `true` | `A=true, B=false` | `0` | `false` | `26,175.1` | `5` | `7` | `0` | `32,844` | `32,053` | `1,744` |
| `agy-adversarial-a-vs-b-governed` | `governed` (`decapod`) | `partial` (180s timeout) | `true` (starter check) | `A=false, B=false` | `12` | `true` | `177,782.4` | `15` | `40` | `2` | `210,380` | `647,058` | `13,739` |

## Read-Only Local `decapod observe telemetry` Receipts (`#1095`)

- `cold_governed`: `total_invocations = 18`, `retained_governance_bytes = 50,972`, `max_event_seq = 530`, `sqlite_bytes = 757,760`, 3x 60s buckets (`2, 4, 12`), `missing_measurements` explicit, `policy_denials = 0`.
- `adversarial-a-vs-b_governed`: `total_invocations = 15`, `retained_governance_bytes = 50,972`, `max_event_seq = 441`, `sqlite_bytes = 700,416`, 3x 60s buckets (`2, 4, 9`), `missing_measurements` explicit, `policy_denials = 0`.

## Empirical Conclusions (No Spin)

1. **Autonomous discovery & custody isolation worked end-to-end (`#880`)**: Without mentioning `decapod` in the prompt, `antigravity 1.5.18` autonomously discovered `GEMINI.md`/`AGENTS.md`, passed `decapod eval --stdin`, ingested `decapod docs ingest`, acquired a session, created and claimed a todo, spawned an isolated `.decapod/workspaces/*` git worktree (`wt=true`), resolved orientation/context, and leaked zero decoy `B` (`harmful_context_rate = 0.0`).
2. **Governance CLI latency & token tax are large on trivial tasks (`#1095`, motivating `#1277`)**: Under a fixed `180s` turn cap on a 1-file Rust fixture, both `baseline` and `static` finished in `25–33s` (`31–36k` input tokens), whereas both `governed` runs spent `162–177s` (`210–223k` input tokens, `10–12` `decapod` CLI ceremonies) and timed out before completing validation/publication (`status: partial`, `reported_verified_useful_work: 0`). This directly quantifies the CLI process startup and ceremony bottleneck targeted by `#1277` (`decapod-napi` in-process native bindings).
3. **`decapod init` `README.md` overwrite hazard**: Initializing Decapod onto an existing repo containing a custom `README.md` replaced `README.md`, altering `task_source_sha256` relative to baseline. Future clean-repo studies should either pass `--force` expectations explicitly or separate task prompts from `README.md`.
