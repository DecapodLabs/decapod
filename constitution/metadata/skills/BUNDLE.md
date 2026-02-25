# Agent Skill Bundle

**Authority:** metadata
**Layer:** Skills Index
**Purpose:** Agent onboarding and skill activation guide

This bundle contains meta-skills that train agents how to interface with Decapod and humans.

---

## Core Bundle (Required)

These skills are Constitution-native and MUST be loaded for any agent session.

| Skill | Purpose | Trigger Phrases |
|-------|---------|-----------------|
| `agent-decapod-interface` | How to call Decapod RPC, handle responses, manage workspace | "call decapod", "initialize", "get context", "validate", "store decision" |
| `human-agent-ux` | Elegant human interaction, question patterns, progress updates | "ask human", "clarify", "present options", "iterate", "feedback" |
| `intent-refinement` | Transform vague intent into explicit specs and validation criteria | "make it faster", "add feature", "what's the approach?", scope unclear |

---

## Activation Flow

### 1. Session Start

```bash
decapod rpc --op agent.init
```

This triggers auto-loading of core bundle skills.

### 2. Context Load

Before any significant action:

```bash
decapod context.capsule.query --topic interfaces --skill agent-decapod-interface
decapod context.capsule.query --topic methodology --skill intent-refinement
```

### 3. Human Interaction

When interfacing with human, load:

```bash
decapod context.capsule.query --topic ux --skill human-agent-ux
```

---

## Skill Reference

### agent-decapod-interface

```
Path: metadata/skills/agent-decapod-interface/SKILL.md
Covers:
- RPC calling conventions
- Response envelope parsing
- Decision patterns (init → context → act → store → validate)
- Error handling
- Workspace management
- Capability discovery
```

### human-agent-ux

```
Path: metadata/skills/human-agent-ux/SKILL.md
Covers:
- Intent capture templates
- Question patterns (open-ended, constrained, binary)
- Refusal patterns
- Progress communication
- Feedback iteration
- Anti-patterns
```

### intent-refinement

```
Path: metadata/skills/intent-refinement/SKILL.md
Covers:
- Input classification (Type A/B/C)
- Specification templates
- Context gathering before inference
- "What must be true" check
- Validation mapping
- Refinement questions
```

---

## Usage

To load a skill for current context:

```bash
decapod docs show metadata/skills/<skill-name>/SKILL.md
```

To query skills by topic:

```bash
decapod context.capsule.query --topic <topic> --skill <skill-name>
```

---

## Extending

See `specs/skills/SKILL_GOVERNANCE.md` for how to add custom skills.
