# Team Skills: Procedural Memory Examples

This file provides concrete examples of procedural norms (team skills) that agents must follow. Each entry is machine-readable JSON with provenance.

---

## Commit Norms

```json
{
  "id": "norm.commit.atomic",
  "type": "commit_norm",
  "schema_version": "1.0.0",
  "title": "Atomic commits",
  "rule": "Each commit must represent a single, complete change. Split feature branches into logical units.",
  "examples": {
    "good": "feat: add user authentication\n\n- Add login endpoint\n- Add password hashing\n- Add session management",
    "bad": "feat: various improvements\n\n- fixed bug\n- added feature\n- changed styling"
  },
  "enforcement": "PR review checks for atomicity",
  "provenance": [
    {
      "evidence_type": "doc",
      "evidence_ref": "constitution/methodology/COMMIT_CONVENTIONS.md",
      "cited_by": "agent-arx",
      "cited_at": 1700000000
    }
  ]
}
```

```json
{
  "id": "norm.commit.conventional",
  "type": "commit_norm",
  "schema_version": "1.0.0",
  "title": "Conventional commits",
  "rule": "Use Conventional Commits format: <type>(<scope>): <description>",
  "types": ["feat", "fix", "docs", "style", "refactor", "test", "chore", "revert"],
  "enforcement": "CI lint gate rejects non-conventional",
  "provenance": [
    {
      "evidence_type": "commit",
      "evidence_ref": "abc123def",
      "cited_by": "agent-arx",
      "cited_at": 1700000001
    }
  ]
}
```

```json
{
  "id": "norm.commit.tests_required",
  "type": "commit_norm",
  "schema_version": "1.0.0",
  "title": "Tests required",
  "rule": "Every feature/fix commit must include corresponding tests. No test = no merge.",
  "exceptions": ["docs-only", "refactor-no-behavior-change"],
  "enforcement": "CI gate checks test coverage delta",
  "provenance": [
    {
      "evidence_type": "pr",
      "evidence_ref": "42",
      "cited_by": "agent-arx",
      "cited_at": 1700000002
    }
  ]
}
```

---

## PR Expectations

```json
{
  "id": "norm.pr.checklist",
  "type": "pr_expectation",
  "schema_version": "1.0.0",
  "title": "PR checklist",
  "rule": "All items must be checked before merge",
  "checklist": [
    "Tests pass (CI green)",
    "No merge conflicts",
    "Documentation updated if needed",
    "Breaking changes documented in CHANGELOG.md",
    "Risk tier assigned and approved",
    "At least one reviewer approval"
  ],
  "enforcement": "PR cannot be merged without checklist verification",
  "provenance": [
    {
      "evidence_type": "doc",
      "evidence_ref": "constitution/methodology/PR_PROCESS.md",
      "cited_by": "agent-arx",
      "cited_at": 1700000003
    }
  ]
}
```

```json
{
  "id": "norm.pr.risk_tier",
  "type": "pr_expectation",
  "schema_version": "1.0.0",
  "title": "Risk tier classification",
  "rule": "Every PR must declare risk tier. Higher tiers require more scrutiny.",
  "tiers": {
    "trivial": { "reviewers": 0, "tests": "unit", "examples": "typos, formatting" },
    "low": { "reviewers": 1, "tests": "unit+integration", "examples": "small bug fixes" },
    "medium": { "reviewers": 2, "tests": "full", "examples": "new features" },
    "high": { "reviewers": 3, "tests": "full+chaos", "examples": "security, core logic" },
    "critical": { "reviewers": 5, "tests": "full+chaos+manual", "examples": "auth, payment" }
  },
  "enforcement": "PR blocked if tier not assigned or insufficient review",
  "provenance": [
    {
      "evidence_type": "doc",
      "evidence_ref": "constitution/specs/RISK_CLASSIFICATION.md",
      "cited_by": "agent-arx",
      "cited_at": 1700000004
    }
  ]
}
```

---

## User Expectations

```json
{
  "id": "norm.user.dod",
  "type": "user_expectation",
  "schema_version": "1.0.0",
  "title": "Definition of Done",
  "rule": "A task is not complete until all items are verified",
  "criteria": [
    "Code implemented and peer-reviewed",
    "Tests written and passing",
    "Documentation updated",
    "Validation gate passes (decapod validate)",
    "No regression in health checks"
  ],
  "provenance": [
    {
      "evidence_type": "doc",
      "evidence_ref": "constitution/methodology/DOD.md",
      "cited_by": "agent-arx",
      "cited_at": 1700000005
    }
  ]
}
```

```json
{
  "id": "norm.user.no_assume",
  "type": "user_expectation",
  "schema_version": "1.0.0",
  "title": "No assumptions about user intent",
  "rule": "Always clarify requirements before implementing. Ask questions. Confirm understanding.",
  "rationale": "Prevents wasted work on misaligned expectations",
  "provenance": [
    {
      "evidence_type": "commit",
      "evidence_ref": "xyz789",
      "cited_by": "agent-arx",
      "cited_at": 1700000006,
      "note": "Learned from a project where we built the wrong feature"
    }
  ]
}
```

```json
{
  "id": "norm.user.audit_trail",
  "type": "user_expectation",
  "schema_version": "1.0.0",
  "title": "All decisions must be auditable",
  "rule": "Store rationale in ADRs, meeting notes, or decision artifacts. Don't rely on memory.",
  "provenance": [
    {
      "evidence_type": "doc",
      "evidence_ref": "constitution/specs/AUDIT_REQUIREMENTS.md",
      "cited_by": "agent-arx",
      "cited_at": 1700000007
    }
  ]
}
```

---

## Agent Behavior

```json
{
  "id": "norm.agent.validate_first",
  "type": "agent_expectation",
  "schema_version": "1.0.0",
  "title": "Always validate before claiming done",
  "rule": "Never declare 'done' without running 'decapod validate' and fixing failures",
  "provenance": [
    {
      "evidence_type": "transcript",
      "evidence_ref": "transcript.abc123",
      "cited_by": "agent-arx",
      "cited_at": 1700000008,
      "note": "Established after multiple 'done but broken' incidents"
    }
  ]
}
```

```json
{
  "id": "norm.agent.worktree_required",
  "type": "agent_expectation",
  "schema_version": "1.0.0",
  "title": "Never work on main/master directly",
  "rule": "All implementation work must happen in isolated worktrees. Use 'decapod workspace ensure'.",
  "provenance": [
    {
      "evidence_type": "doc",
      "evidence_ref": "constitution/specs/GIT.md",
      "cited_by": "agent-arx",
      "cited_at": 1700000009
    }
  ]
}
```

---

## Schema for Procedural Norms

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["id", "type", "schema_version", "title", "rule", "provenance"],
  "properties": {
    "id": { "type": "string", "pattern": "^norm\\.(commit|pr|user|agent)\\.[a-z0-9-]+$" },
    "type": { "enum": ["commit_norm", "pr_expectation", "user_expectation", "agent_expectation"] },
    "schema_version": { "type": "string", "pattern": "^\\d+\\.\\d+\\.\\d+$" },
    "title": { "type": "string", "minLength": 1 },
    "rule": { "type": "string", "minLength": 10 },
    "examples": { "type": "object" },
    "exceptions": { "type": "array", "items": { "type": "string" } },
    "checklist": { "type": "array", "items": { "type": "string" } },
    "tiers": { "type": "object" },
    "criteria": { "type": "array", "items": { "type": "string" } },
    "rationale": { "type": "string" },
    "enforcement": { "type": "string" },
    "provenance": {
      "type": "array",
      "minItems": 1,
      "items": {
        "type": "object",
        "required": ["evidence_type", "evidence_ref", "cited_by", "cited_at"],
        "properties": {
          "evidence_type": { "enum": ["commit", "pr", "doc", "test", "transcript"] },
          "evidence_ref": { "type": "string" },
          "cited_by": { "type": "string" },
          "cited_at": { "type": "integer" },
          "note": { "type": "string" }
        }
      }
    }
  }
}
```
