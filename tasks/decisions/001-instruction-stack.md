---
id: ADR-001
title: Repo Instruction Stack Design
status: accepted
date: 2026-02-20
scope: Agent onboarding and behavior contracts
---

# ADR-001: Repo Instruction Stack Design

## Context

Decapod's agent entrypoints (CLAUDE.md, AGENTS.md, CODEX.md, GEMINI.md) were nearly identical copies of initialization sequences. They lacked:
- Operating mode guidance (plan-first, proof-first)
- Governance invariant documentation with proof pointers
- Multi-agent coordination norms
- Decision frameworks and failure mode playbooks
- Self-improvement loops
- Progressive disclosure ordering

A new engineer or LLM entering the repo had to read constitution docs to understand behavior expectations, with no clear entry path.

## Decision

Create a layered instruction stack:

1. **CLAUDE.md** — Canonical entrypoint with operating mode, invariants, workflow, subagent strategy.
2. **AGENTS.md** — Universal contract for all agents (golden rules, initialization, coordination).
3. **IDENTITY.md** — What Decapod is/isn't, thesis, invariants, vocabulary.
4. **TOOLS.md** — Complete command reference and workflow documentation.
5. **PLAYBOOK.md** — Decision frameworks, triage flows, failure modes, evidence standards.
6. **tasks/lessons.md** — Post-correction rules that prevent repeats.
7. **tasks/todo.md** — High-level initiative tracking with commitments ledger.
8. **tasks/decisions/** — ADRs with scope, alternatives, consequence, proof impact.

Each file has a distinct role. No duplication. CLAUDE.md references AGENTS.md for universal contract; AGENTS.md does not duplicate CLAUDE.md's operating mode.

## Alternatives Considered

- **Single CLAUDE.md with everything**: Too large. Violates progressive disclosure.
- **Constitution-only approach**: Constitution is embedded at compile time. Instruction stack is repo-level and editable.
- **External wiki/docs**: Violates repo-native canonicality.

## Consequences

- Agents entering the repo have a clear reading order.
- Each document is independently useful and referenceable.
- `decapod validate` entrypoint gate can check for required files.
- Operating mode (plan-first, proof-first) is documented, not implied.
- Self-improvement loop (tasks/lessons.md) prevents repeat failures.

## Proof Impact

- `decapod validate` entrypoint gate checks: CLAUDE.md, AGENTS.md, CODEX.md, GEMINI.md exist.
- New files (IDENTITY.md, TOOLS.md, PLAYBOOK.md) are informational; no gate required yet.
- tasks/ directory is conventional, not gated.
