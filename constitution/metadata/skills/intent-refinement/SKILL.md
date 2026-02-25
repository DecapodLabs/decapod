---
name: intent-refinement
description: Transform raw human intent into explicit specifications before inference. Use when the human gives a vague request, when specs are missing, or when scope is unclear. Triggers: "make it faster", "add feature", "what's the approach?".
allowed-tools: Bash
---

# Intent Refinement

The human gives you intent. You make it explicit. This is the most important skill—you cannot validate against fuzzy requirements.

## The Refinement Loop

```
Human Input → Explicit Intent → Spec Artifacts → Context → Action → Validation
```

You MUST complete the loop before claiming done.

## Input Classification

### Type A: Complete Intent

The human gave you everything:
- Goal (what)
- Constraints (what must be true)
- Success criteria (how we know we're done)

**Action**: Confirm and proceed.

### Type B: Partial Intent

The human gave you the goal but not constraints or success criteria.

**Action**: Ask focused questions to fill gaps.

### Type C: Vague Intent

The human gave you neither goal nor constraints.

**Action**: Use the interview pattern to elicit:
- Background: "What's the context for this?"
- Goal: "What should the end result look like?"
- Constraints: "What must be true?"
- Scope: "What's in/out of scope?"

## The Specification Template

Turn intent into this structure:

```markdown
## Intent

**Goal**: [One sentence describing what to accomplish]

**Constraints**:
- [Hard requirement that must be satisfied]
- [Hard requirement that must be satisfied]

**Success Criteria**:
- [Measurable outcome that proves completion]
- [Measurable outcome that proves completion]

**Out of Scope**:
- [Explicitly NOT included]
- [Explicitly NOT included]

**Tradeoffs**:
- [Acceptable compromise if constrained]
- [Acceptable compromise if constrained]
```

## When to Generate Artifacts

| Situation | Action |
|-----------|--------|
| New feature | Generate SPEC.md, validate against it |
| Bug fix | Document current vs expected behavior |
| Refactor | Document invariants that must hold |
| Architecture change | Generate ARCHITECTURE.md, get sign-off |
| Security-sensitive | Generate SECURITY.md, run context |

Use `decapod rpc --op scaffold.generate_artifacts` for structured output.

## Context Gathering (BEFORE Inference)

Before you act on ANY intent:

1. **Resolve relevant specs**: `decapod rpc --op context.resolve --params '{"operation": "your_action"}'`
2. **Check existing decisions**: `decapod store.query --kind decision --query "your_topic"`
3. **Validate against standards**: `decapod rpc --op standards.resolve --params '{"question": "your_question"}'`

**Never infer without context. Never assume no specs apply.**

## The "What Must Be True" Check

For each action you take, ask:
- What spec governs this?
- What must be true after my change?
- How do I verify it's true?

If you can't answer these, you don't have enough context.

## Validation Mapping

Map each success criterion to a validation:

```
Success Criterion: "API responds in <100ms"
→ Validation: Run benchmark, assert <100ms

Success Criterion: "No breaking changes"
→ Validation: Run compatibility tests

Success Criterion: "Tests pass"
→ Validation: `decapod validate`
```

**No criterion without validation. No validation without execution.**

## Anti-Patterns

NEVER:
- Act on intent without explicit confirmation on Type B/C inputs
- Skip context resolution "to save time"
- Define success criteria without measurable outcomes
- Leave scope implicit (it will expand)
- Accept tradeoffs without documenting them
- Claim done without validation against stated criteria

## Refinement Questions (When Stuck)

Use these to unstick vague intent:

| Gap | Question |
|-----|----------|
| Goal unclear | "What should the user experience be when this is done?" |
| Scope unclear | "What's the smallest version we could ship first?" |
| Constraints unclear | "What must absolutely NOT break?" |
| Success unclear | "How will we know this is successful?" |
| Tradeoffs unclear | "If we had to choose between X and Y, which matters more?" |

## Reference

- Agent interface: `agent-decapod-interface` skill
- Human UX: `human-agent-ux` skill
- Intent spec: `specs/INTENT.md`
- Testing contract: `interfaces/TESTING.md`
