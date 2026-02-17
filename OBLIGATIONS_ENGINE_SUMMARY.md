# Obligations Engine - Implementation Summary

## What's New

### 1. Mentor Module (`src/core/mentor.rs`)
Deterministic obligations system that guides agents back to:
- **ADRs** (Architecture Decision Records) in `docs/decisions/`
- **Documentation** (spec.md, architecture.md, security.md, ops.md)
- **Knowledge Graph** nodes (decisions, commitments from federation)
- **Active Todos** (claimed, in-progress tasks)
- **Container Requirements** (Docker-first Silicon Valley hygiene)

### 2. Docker-First Workspaces
Containerization is now enforced as a **must** obligation:
- `workspace.ensure` creates git worktrees + Docker containers
- `Dockerfile` existence is checked and enforced
- Agents get clear instructions on how to enter containers
- Multiple agents can work in parallel in isolated containers

### 3. RPC Operation: `mentor.obligations`
Input:
```json
{
  "op": "git.commit",
  "params": {},
  "touched_paths": ["src/auth.rs"],
  "high_risk": true
}
```

Output (added to standard envelope):
```json
{
  "obligations": {
    "must": [
      {
        "kind": "container",
        "ref_path": "Dockerfile",
        "title": "Dockerfile exists - Containerization Required",
        "why_short": "Silicon Valley hygiene: All work must be containerized",
        "relevance_score": 0.95
      }
    ],
    "recommended": [...],
    "contradictions": []
  }
}
```

### 4. Deterministic Scoring
- Same repo state + input = same obligations (always)
- Relevance scoring based on:
  - Path/keyword matching
  - Recency (newer ADRs slightly higher)
  - Node type priority (decisions > commitments > observations)
  - **Container requirements (highest priority)**

### 5. Contradiction Detection
- Detects when operations conflict with prior ADRs
- Returns `blocked_by` entries in RPC response
- Recommends creating new ADR or updating spec

## Usage

```bash
# Get obligations before acting
decapod rpc --op mentor.obligations --params '{
  "op": "code.change",
  "touched_paths": ["src/security/auth.rs"],
  "high_risk": true
}'

# Create containerized workspace
decapod workspace ensure --branch feature/auth-improvements

# Check status (includes container info)
decapod workspace status
```

## Silicon Valley Hygiene Enforced

1. **No work outside containers** - Blocked until in Docker
2. **Dockerfile required** - Must obligation if missing
3. **Reproducible environments** - Container hash verification
4. **Parallel work** - Multiple agents in isolated containers
5. **CI-identical** - Local dev matches CI exactly

## Architecture

```
Agent Query
    ↓
mentor.obligations RPC
    ↓
MentorEngine::compute_obligations()
    ├─→ get_container_candidates() [HIGHEST PRIORITY]
    ├─→ get_adr_candidates()
    ├─→ get_doc_candidates()
    ├─→ get_kg_candidates()
    └─→ get_todo_candidates()
    ↓
score_candidates() [deterministic]
    ↓
detect_contradictions()
    ↓
build_obligations() [capped at 5 each]
    ↓
Return: must[] + recommended[] + contradictions[]
```

## Key Constraints Met

✅ **Deterministic** - Same input + repo state = same output
✅ **Immutable sources** - Never modifies existing docs/KG
✅ **Compact** - Max 5 items per obligations list
✅ **Docker-first** - Containerization enforced programmatically
✅ **No prose validation** - Only programmatic gates

## Next Steps

- Optional tiny LLM for ranking/phrasing (weights in `.decapod/models/`)
- Tests for determinism, capping, contradiction detection
- Integration with `validate` gate for container enforcement
