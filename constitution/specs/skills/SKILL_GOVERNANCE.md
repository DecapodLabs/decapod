# SKILL_GOVERNANCE.md

**Authority:** constitution
**Layer:** Specs
**Binding:** Yes

## Purpose

Decapod treats external "skills" as optional input material, not runtime authority.
To be promotion-relevant, skills must be translated into deterministic, repo-native artifacts.

## Artifact Contract

### SKILL_CARD
- Path: `<repo>/.decapod/governance/skills/<skill_name>.json`
- Kind: `skill_card`
- Fields: `skill_name`, `source_path`, `source_sha256`, `workflow_outline`, `dependencies`, `tags`, `card_hash`
- Determinism rule: identical SKILL.md content produces identical `card_hash`.

### SKILL_RESOLUTION
- Path: `<repo>/.decapod/generated/skills/<query_hash>.json` (optional write)
- Kind: `skill_resolution`
- Fields: `query`, `resolved[]`, `resolution_hash`
- Determinism rule: identical query + identical skill store state produces identical `resolution_hash`.

## Multi-Agent Boundary

1. Skills are shared repo primitives, not per-agent hidden memory.
2. Skill ingestion is append/update via Decapod CLI only.
3. Agents MUST NOT claim a skill capability unless it exists in the control-plane artifact/store.

## Promotion Discipline

1. Promotion-relevant skill usage MUST reference a `skill_card` artifact or explicit aptitude skill entry.
2. Free-form skill prose cannot bypass proof gates.
3. Hash mismatch in skill artifacts is a validation failure.

## Non-Goals

- No orchestrator behavior.
- No provider-specific skill runtime.
- No remote registry as canonical source of truth.
