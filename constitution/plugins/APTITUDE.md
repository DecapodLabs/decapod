# APTITUDE.md - APTITUDE Subsystem (Embedded)

**Authority:** subsystem (REAL)
**Layer:** Operational
**Binding:** No

**Quick Reference:**
| Command | Purpose |
|---------|---------|
| `decapod data aptitude add --category git --key ssh --value "mine"` | Record a preference |
| `decapod data aptitude get --category git --key ssh` | Retrieve a preference |
| `decapod data aptitude list` | List all preferences by category |

**Related:** `core/PLUGINS.md` (subsystem registry) | `AGENTS.md` (entrypoint)

---

## CLI Surface

```bash
decapod data aptitude add --category <cat> --key <key> --value <val> [--context <ctx>] [--source <src>]
decapod data aptitude get --category <cat> --key <key>
decapod data aptitude list [--category <cat>] [--format text|json]
decapod data aptitude schema  # JSON schema for programmatic use
# Aliases: decapod data memory ..., decapod data skills ...
```

## Purpose

The memory/skills subsystem catalogs distinct user expectations that persist across sessions, helping AI agents work more effectively with their human collaborators. It transforms one-off instructions into remembered behaviors.

### Why This Matters

Without the memory/skills subsystem:
- User has to repeat "use my SSH key" on every commit
- Agent forgets preferred branch naming conventions
- Code style preferences must be re-explained each session
- Workflow requirements are lost between contexts

With the memory/skills subsystem:
- Preferences are recorded once, remembered always
- Agents check before acting
- Consistent behavior across all interactions
- Builds a profile of how the user likes to work

### Example Use Cases

**Git Preferences:**
```bash
# User says: "always use my SSH key, don't add yourself as a contributor"
decapod data memory add --category git --key ssh_key --value "use_mine" \
  --context "Use user's SSH key for git operations, don't add self as contributor" \
  --source "user_request"

# User says: "keep commit messages concise and imperative"
decapod data memory add --category style --key commit_messages --value "concise_imperative" \
  --context "Keep commit messages under 72 chars, use imperative mood" \
  --source "user_request"
```

**Workflow Conventions:**
```bash
# User says: "use feature/ prefix for branches"
decapod data memory add --category workflow --key branch_naming --value "feature/descriptive-name" \
  --context "Prefix feature branches with feature/ followed by kebab-case description" \
  --source "user_request"
```

## Categories

Standard categories for organizing preferences:

| Category | Description | Example Keys |
|----------|-------------|--------------|
| `git` | Version control preferences | `ssh_key`, `commit_style`, `branch_naming`, `merge_strategy` |
| `style` | Code and documentation style | `commit_messages`, `comment_style`, `naming_conventions` |
| `workflow` | Development workflow | `pr_process`, `testing_requirements`, `review_style` |
| `communication` | Interaction preferences | `verbosity`, `technical_depth`, `update_frequency` |
| `tooling` | Tool-specific preferences | `formatter`, `linter`, `editor_settings` |

### Choosing Categories

- Use existing categories when possible
- Create new categories only for distinct domains
- Keys should be specific within a category
- Values should be actionable by agents

## Preference Lifecycle

### Recording Preferences

When a user expresses a preference:

1. **Capture immediately**: Record while context is fresh
2. **Be specific**: `commit_message_format` not just `style`
3. **Provide context**: Include the "why" not just the "what"
4. **Note the source**: User requests override observed behaviors

```bash
# Good: Specific, contextual, actionable
decapod data memory add --category git --key ssh_contributor --value "user_only" \
  --context "Use user's SSH credentials, never add self as commit contributor" \
  --source "user_request"

# Bad: Vague, no context
# decapod data memory add --category style --key prefs --value "good"
```

### Retrieving Preferences

Agents MUST check preferences before acting:

```bash
# Before committing, check SSH preference
decapod data memory get --category git --key ssh_contributor

# Before creating a branch, check naming convention
decapod data memory get --category workflow --key branch_naming
```

### Updating Preferences

Preferences can be updated by recording again with the same category/key:

```bash
# User changes their mind about commit style
decapod data memory add --category style --key commit_messages --value "detailed_explanatory" \
  --context "Now prefer detailed commit messages with full context" \
  --source "user_request"
```

## Storage Model

Preferences are stored in `aptitude.db` with full audit trail:

| Field | Description |
|-------|-------------|
| `id` | Unique ULID identifier |
| `category` | Preference category |
| `key` | Preference name (unique within category) |
| `value` | Preference value |
| `context` | Optional explanation |
| `source` | How learned: `user_request`, `observed_behavior`, etc. |
| `created_at` | When first recorded |
| `updated_at` | When last modified |

The `(category, key)` combination is unique - recording again updates the existing preference.

## Agent Guidelines

### Do

- **Check before acting**: Always query relevant preferences before operations
- **Record when learned**: When user expresses a preference, record it immediately
- **Be specific**: Use clear, descriptive keys
- **Provide context**: Explain why the preference matters
- **Respect the source**: User requests take precedence over observed behaviors

### Don't

- **Don't assume**: Never assume preferences without checking
- **Don't ignore**: When user states a preference, don't ignore it
- **Be vague**: Avoid generic keys like `prefs` or `settings`
- **Skip context**: Context helps future agents understand the preference

### Example Workflow

```bash
# User asks to commit something
# 1. Check for git preferences
decapod data memory get --category git --key ssh_contributor
# Returns: use user's SSH, don't add self as contributor

# 2. Check commit style
decapod data memory get --category style --key commit_messages
# Returns: concise and imperative

# 3. Perform action respecting preferences
git commit -m "feat: add aptitude plugin"  # Using user's SSH

# 4. User expresses new preference
# User: "always push to ahr/work branch"
decapod data memory add --category git --key default_push_branch --value "ahr/work" \
  --context "Default branch for pushing work" \
  --source "user_request"
```

---

**See also:** `core/PLUGINS.md` for subsystem registry and truth labels.
