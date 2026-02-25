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

---

## Meta-Skills (Agent Training)

Decapod includes meta-skills that train external agents how to interface with the control plane. These live in `metadata/skills/` and are Constitution-native.

### Classification

| Type | Purpose | Location |
|------|---------|----------|
| **Interface** | How to call Decapod RPC | `metadata/skills/agent-decapod-interface/` |
| **UX** | How to interact with humans | `metadata/skills/human-agent-ux/` |
| **Refinement** | How to turn intent into specs | `metadata/skills/intent-refinement/` |

### Activation

Meta-skills activate when:
- Agent initializes (`agent.init` triggers interface skill)
- Human gives vague intent (triggers refinement)
- Agent needs to communicate with human (triggers UX)

### Agent Onboarding

For new agents, ensure these meta-skills are loaded:
1. `agent-decapod-interface` - Required for any Decapod interaction
2. `human-agent-ux` - Required for human-facing work
3. `intent-refinement` - Required for any task involving intent

### Custom Skills

To add domain-specific skills:
1. Create `metadata/skills/<skill-name>/SKILL.md`
2. Add YAML frontmatter with `name`, `description`, `allowed-tools`
3. Run `decapod docs ingest` to register
4. Skills become available via `decapod context.capsule.query`
