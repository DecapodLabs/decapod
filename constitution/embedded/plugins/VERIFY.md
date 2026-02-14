# VERIFY.md - Verification Subsystem

**Canonical:** plugins/VERIFY.md
**Authority:** constitution
**Layer:** Plugins
**Binding:** Yes
**Version:** v0.1.0

## Purpose

This document defines the verification subsystem for Decapod: proof-plan replay and drift detection for completed work over time.

## Verification vs Validation

**Validation (`decapod validate`):** Repo is consistent with constitution RIGHT NOW.
- Checks: provenance present, schema integrity, state machine compliance
- Scope: Current repo state
- Frequency: On-demand, pre-commit, CI

**Verification (`decapod verify`):** Completed work is still true OVER TIME.
- Checks: Proof-plan replay, artifact drift detection, claim staleness
- Scope: Historical completed work (TODOs, claims, decisions)
- Frequency: Periodic (daily/weekly), on-demand, post-deploy

**Separation:** Validation and verification are distinct gates. Passing validation does NOT imply verification is current.

## Contracts

### 1. Verification Targets (MVP)

**Primary:** Completed/validated TODOs with proof_plan.

A TODO marked `done` or `validated` MUST have:
- `proof_plan`: List of proofs that were satisfied at completion time
- `verification_artifacts`: Captured state (file paths, hashes, commands, results)

Verification re-executes the proof_plan and compares results against captured artifacts.

**Future:** Verifiable repo claims, knowledge records, architectural decisions.

### 2. TODO Model Extensions for Verification

Required fields (add to TODO schema v3):

```
last_verified_at: Timestamp (ISO8601, nullable)
last_verified_status: Enum (pass|fail|stale|unknown, nullable)
last_verified_notes: String (what failed or changed, nullable)
verification_policy: String (staleness threshold in days, default 90)
verification_artifacts: JSON (captured at completion time)
```

**verification_artifacts schema:**

```json
{
  "completed_at": "2026-02-13T12:00:00Z",
  "proof_plan_results": [
    {
      "proof_gate": "validate_passes",
      "status": "pass",
      "command": "decapod validate",
      "output_hash": "sha256:abc123..."
    },
    {
      "proof_gate": "tests_pass",
      "status": "pass",
      "command": "cargo test",
      "output_hash": "sha256:def456..."
    }
  ],
  "file_artifacts": [
    {
      "path": "src/core/validate.rs",
      "hash": "sha256:ghi789...",
      "size": 12345
    }
  ],
  "commit_hash": "a1b2c3d4",
  "repo_state_hash": "sha256:repo123..."
}
```

### 3. Verification Mechanics (Proof-Plan Replay)

**On TODO completion** (`decapod todo done <id>`):
1. Execute each proof in `proof_plan`
2. Capture results (status, command, output hash)
3. Capture file artifacts (paths, hashes, sizes)
4. Store in `verification_artifacts`
5. Set `last_verified_at` = now, `last_verified_status` = pass

**On verification** (`decapod verify todo <id>`):
1. Re-execute each proof in `proof_plan`
2. Compare results against `verification_artifacts.proof_plan_results`
3. Check file artifacts for drift (hash mismatch, missing files)
4. Update `last_verified_at`, `last_verified_status`, `last_verified_notes`

**Drift Detection:**
- File hash changed → FAIL (drift detected)
- File missing → FAIL (artifact deleted)
- Proof command output changed → FAIL (behavior changed)
- Proof command failed (was pass) → FAIL (regression)

### 4. Staleness Threshold

**Default:** 90 days for normal TODOs, 30 days for critical TODOs.

A TODO is considered **stale** if:
- `last_verified_at` is NULL (never verified since completion)
- OR `now - last_verified_at > verification_policy` (re-verification overdue)

Stale TODOs are flagged but do not fail verification (warning only).

### 5. CLI Surface (MVP)

```bash
# Verify all due items (stale or never verified)
decapod verify

# Verify specific TODO
decapod verify todo <id>

# List items due for re-verification
decapod verify --stale

# Machine-readable output for CI
decapod verify --json

# Force verification even if not stale
decapod verify --force

# Show verification history for TODO
decapod verify todo <id> --history
```

### 6. Output Format

**Human-readable:**

```
⚡ VERIFICATION REPORT

  ℹ TODO-123: Add staleness tracking
    ● Proof: validate_passes → PASS (no drift)
    ● Proof: tests_pass → FAIL (output changed)
    ● Artifact: src/core/validate.rs → FAIL (hash mismatch)
    ✗ FAILED (1 proof failed, 1 artifact drifted)

  ℹ TODO-124: Update documentation
    ● Proof: docs_build → PASS (no drift)
    ● Artifact: README.md → PASS (no drift)
    ✓ PASSED (all proofs passed, no drift)

Summary:
  2 TODOs verified
  1 passed
  1 failed
  3 stale (not verified in >90 days)
```

**Machine-readable (--json):**

```json
{
  "verified_at": "2026-02-13T12:00:00Z",
  "summary": {
    "total": 2,
    "passed": 1,
    "failed": 1,
    "stale": 3
  },
  "results": [
    {
      "todo_id": "TODO-123",
      "status": "fail",
      "proofs": [
        {"gate": "validate_passes", "status": "pass"},
        {"gate": "tests_pass", "status": "fail", "reason": "output changed"}
      ],
      "artifacts": [
        {
          "path": "src/core/validate.rs",
          "status": "fail",
          "reason": "hash mismatch",
          "expected": "sha256:abc123...",
          "actual": "sha256:xyz789..."
        }
      ]
    }
  ]
}
```

### 7. Integration with Validation (Optional)

**Validation MAY warn/fail** if:
- Critical validated TODOs are stale (>30 days unverified)
- TODOs in `done` state lack verification_artifacts

This is **configurable** (not mandatory) and staged:
- Phase 1: Verification is separate (no validation integration)
- Phase 2: Validation warns on stale verified work
- Phase 3: Validation fails on critical stale work (repo-configurable)

### 8. Storage

Verification data is stored in TODO DB:
- New fields in `tasks` table (see section 2)
- Verification history in `verification_events.jsonl` (audit log)

No separate verification.db (keep it integrated).

### 9. Governance

**Who can mark as verified?**
- Automated: `decapod verify` (re-runs proofs)
- Manual: `decapod verify todo <id> --manual --notes "<reason>"` (with audit trail)

**Who can waive verification failures?**
- `decapod verify todo <id> --waive --reason "<text>"` (sets status=pass despite failures, logged)

**Audit trail:**
- All verification runs logged to `verification_events.jsonl`
- Includes: timestamp, TODO ID, status, proof results, artifacts checked, waiver reason (if any)

### 10. Proof-Plan Contract

A `proof_plan` is a list of proof gates that must pass. Each gate is either:
- A validation gate (e.g., `validate_passes`)
- A test command (e.g., `cargo test`)
- A build command (e.g., `cargo build`)
- A file invariant (e.g., `file_exists:path/to/file`)
- A custom command (e.g., `./scripts/check_invariant.sh`)

**Proof gate format:**

```json
[
  "validate_passes",
  "test:cargo test --all",
  "build:cargo build --release",
  "file_exists:src/core/verify.rs",
  "file_hash:src/core/verify.rs:sha256:abc123...",
  "cmd:./scripts/check.sh"
]
```

Each gate is a string in format `type:details` or just `type` for known gates.

### 11. Failure Modes & Recovery

**Verification fails:**
- TODO `last_verified_status` = fail
- Output shows which proofs/artifacts failed
- Human reviews, fixes issues, re-runs `decapod verify todo <id>`

**Verification blocked (missing artifacts):**
- If `verification_artifacts` is NULL/empty, verification cannot run
- Status = unknown (never verified)
- Must complete TODO with artifact capture first

**Stale verification:**
- Warning only (does not fail)
- Human decides: re-verify now, extend threshold, or waive

### 12. Constitutional Authority

This subsystem defers to:
- `core/CONTROL_PLANE.md` — Operational contract
- `specs/SYSTEM.md` — Authority and proof doctrine
- `plugins/TODO.md` — TODO lifecycle and state model
- `specs/TODO_MODEL.md` — TODO schema definition

### 13. Non-Negotiable

- **Verification is separate from validation** (different gates, different purposes)
- **Proof-plan replay is deterministic** (same inputs → same outputs, or drift detected)
- **Drift detection is mandatory** (cannot ignore artifact changes)
- **Audit trail required** (all verification runs logged)
- **No silent failures** (output must be actionable, pointing to exact TODO/proof/artifact)

### See Also

- `core/CONTROL_PLANE.md` — Operational contract
- `specs/SYSTEM.md` — Authority and proof doctrine
- `plugins/TODO.md` — TODO subsystem
- `specs/TODO_MODEL.md` — TODO schema
