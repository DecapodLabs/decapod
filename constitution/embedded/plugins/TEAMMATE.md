# TEAMMATE.md - TEAMMATE Subsystem (Embedded)

**Authority:** subsystem (REAL)
**Layer:** Operational
**Binding:** No

This document defines the teammate subsystem for remembering user preferences and behaviors.

## CLI Surface

```bash
decapod teammate add --category <cat> --key <key> --value <val> [--context <ctx>] [--source <src>]
decapod teammate get --category <cat> --key <key>
decapod teammate list [--category <cat>] [--format text|json]
```

## Purpose

The teammate plugin catalogs distinct user expectations that help AI agents work more effectively:

- **Git preferences**: SSH key usage, commit message style, branch naming conventions
- **Code style**: Formatting preferences, naming conventions, documentation standards
- **Workflow conventions**: Review processes, testing requirements, deployment practices
- **Communication style**: Concise vs detailed, technical depth, update frequency

## Categories

Standard categories for organizing preferences:

- `git` - Version control preferences (SSH keys, branch naming, commit style)
- `style` - Code and documentation style preferences
- `workflow` - Development workflow conventions
- `communication` - How the user prefers to interact
- `tooling` - Tool-specific preferences and configurations

## Storage

Preferences are stored in `teammate.db` with full audit trail:
- `id`: Unique identifier (ULID)
- `category`: Preference category
- `key`: Preference name
- `value`: Preference value
- `context`: Optional explanation or context
- `source`: How the preference was learned (user_request, observed_behavior, etc.)
- `created_at`: When recorded
- `updated_at`: When last modified

## Agent Guidelines

1. **Check before acting**: Use `decapod teammate get` to check for relevant preferences
2. **Record when learned**: When user expresses a preference, record it immediately
3. **Be specific**: Use clear keys like `commit_message_format` not `style`
4. **Provide context**: Include why the preference matters
5. **Respect the source**: User-requested preferences override observed behaviors
