---
name: agent-decapod-interface
description: Master skill for agent-decapod interaction. Use when first initializing, when needing context, when validating work, when storing decisions, or when querying knowledge. Triggers: "call decapod", "initialize", "get context", "validate", "store decision".
allowed-tools: Bash
---

# Agent-Decapod Interface

This skill teaches you how to properly interface with Decapod as an agent. Decapod is not an agent—it is a deterministic control plane you call to validate, context-gate, and persist your decisions.

## The Golden Rule

**You never act on your own authority. You invoke Decapod to get permission, context, or validation before acting.**

## Initialization (MUST DO FIRST)

Before ANY other operation, initialize:

```bash
decapod rpc --op agent.init
```

This returns:
- Your session receipt
- What operations are allowed next
- Any blockers or prerequisites

**NEVER skip initialization.** Without it, you have no authority to act.

## Response Envelope

Every decapod response follows this structure:

```json
{
  "receipt": {
    "operation": "what happened",
    "hashes": {"artifact": "sha256..."},
    "touched_paths": ["files changed"]
  },
  "context_capsule": {
    "relevant_specs": ["spec/INTENT.md", "specs/SECURITY.md"],
    "authority_fragments": ["interface boundaries"],
    "governance_hints": ["validation rules"]
  },
  "allowed_next_ops": ["what you can do now"],
  "blocked_by": ["what prevents progress", "or empty"]
}
```

**You MUST read and respect `allowed_next_ops` and `blocked_by`.**

## Core Operations

### 1. Get Context (Before Inference)

Before making any significant decision:

```bash
decapod rpc --op context.resolve --params '{"operation": "your_action"}'
```

Or scoped to a query:

```bash
decapod rpc --op context.scope --params '{"query": "security validation", "limit": 5}'
```

This returns relevant constitution fragments so you don't violate authority boundaries.

### 2. Validate (Before Claiming Done)

Never claim done without validation:

```bash
decapod validate
```

If validation fails:
1. Read the specific failure messages
2. Fix the issues
3. Re-validate
4. Only claim done when validation passes

**Validation is the gate for promotion-relevance.**

### 3. Store Decisions (For Audit)

When you make a significant decision:

```bash
decapod store.upsert --kind decision --data '{"reasoning": "...", "choice": "...", "alternatives": [...]}'
```

This creates an auditable artifact. Required for:
- Architecture choices
- Security tradeoffs
- Trade-off decisions

### 4. Query Knowledge (Before Acting)

When you need prior context:

```bash
decapod store.query --kind decision --query "security"
decapod knowledge search --query "previous approach to auth"
```

### 5. Resolve Standards

When you need authoritative guidance:

```bash
decapod rpc --op standards.resolve --params '{"question": "how to handle secrets"}'
```

### 6. Workspace Management

Before modifying files:

```bash
decapod workspace status  # Check current state
decapod workspace ensure  # Create/get isolated worktree
```

**You CANNOT work on main/master.** Decapod enforces this.

## Decision Pattern

For EVERY significant action, follow this sequence:

1. **INIT**: `decapod rpc --op agent.init` (once per session)
2. **CONTEXT**: `decapod rpc --op context.resolve` (before decisions)
3. **ACT**: Make the decision
4. **STORE**: `decapod store.upsert` (persist reasoning)
5. **VALIDATE**: `decapod validate` (before claiming done)
6. **ITERATE**: Fix failures, re-validate

## Error Handling

| Error | Response |
|-------|----------|
| `workspace_required` | Run `decapod workspace ensure` first |
| `verification_required` | Run `decapod validate` and fix failures |
| `store_boundary_violation` | You're writing to wrong location; check paths |
| `decision_required` | Store your decision before proceeding |

## Prohibited Patterns

NEVER:
- Skip `agent.init` and claim authority
- Act without first getting context for significant decisions
- Claim done without `decapod validate` passing
- Write to repo root directly (use workspace)
- Work on main/master
- Store secrets or credentials in decapod store

## Capability Discovery

To learn what's available:

```bash
decapod capabilities --format json
```

Check `stability: stable` operations first. Beta operations may change.

## Reference

- Core contract: `core/DECAPOD.md`
- Interfaces: `core/INTERFACES.md`
- Skill governance: `specs/skills/SKILL_GOVERNANCE.md`
