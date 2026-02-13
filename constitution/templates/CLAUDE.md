# CLAUDE.md - Decapod Entrypoint for Claude

**Canonical:** CLAUDE.md
**Authority:** entrypoint
**Layer:** Guides
**Binding:** No

This is a Decapod-managed repository. **You (Claude) are contractually obligated to use the Decapod toolchain FIRST and ALWAYS.**

## 🚨 CRITICAL: RUN THESE COMMANDS BEFORE ANY WORK

**STOP.** Before you write a single line of code or answer any question about this codebase, you MUST execute this sequence:

### 1. Read the Constitution Router (REQUIRED - RUN THIS NOW)
```bash
decapod docs show core/DECAPOD.md
```

### 2. Check System State (REQUIRED - RUN THIS NOW)
```bash
decapod validate
```

### 3. See Active Work (REQUIRED - RUN THIS NOW)
```bash
decapod todo list
```

### 4. Review Project Contracts (REQUIRED - RUN THESE NOW)
```bash
decapod docs show specs/INTENT.md
decapod docs show specs/ARCHITECTURE.md
```

**IF YOU SKIP THIS SEQUENCE, YOUR WORK IS INVALID AND UNSAFE.**

This entrypoint does not define methodology. All authority defers to the embedded constitution.

## Hard Rules (Non-Negotiable)

1. **BEFORE any multi-step work**: Run `decapod todo add "<task description>" --priority high` to track it
2. **NEVER bypass the CLI**: Use `decapod` commands only, never direct DB/file manipulation
3. **BEFORE claiming completion**: Run `decapod validate` and ensure all 29 checks pass
4. **ALWAYS route through DECAPOD.md**: All methodology lives in the embedded constitution, not in your assumptions

**Violation of these rules = invalid work. No exceptions.**

## Project-Specific Overrides (OVERRIDE.md)

This project can customize Decapod's embedded constitution via `.decapod/OVERRIDE.md`:

- **What it is:** Project-specific overrides for embedded constitution components
- **Where:** `.decapod/OVERRIDE.md` (created during `decapod init`)
- **How to use:** Edit component sections (e.g., `### plugins/TODO.md`) to override behavior
- **Validation:** Run `decapod docs override` after making changes
- **Reading:** `decapod docs show <path>` automatically merges overrides with embedded docs

**Example:** If this project has custom TODO priority levels, they'll be defined under `### plugins/TODO.md` in OVERRIDE.md and will appear when you run `decapod docs show plugins/TODO.md`.

**Always check for project overrides** - the merged view (embedded + overrides) is what you should follow.

## Common Decapod Commands (USE THESE)

```bash
# Discovery and state
decapod validate          # Check system health (REQUIRED before completion)
decapod todo list        # See active work (REQUIRED at startup)
decapod docs list        # Browse constitution
decapod docs show <path> # Read specific doc

# Work tracking (USE TODO SUBSYSTEM - see PLUGINS.md)
decapod todo add "<task title>" --priority high|medium|low  # Create task
decapod todo done <id>    # Mark task complete
decapod todo archive <id> # Archive completed task (REQUIRED after done)
decapod proof record     # Document completed work
decapod feedback propose # Suggest changes

# System operations
decapod init            # Initialize workspace
decapod build           # Build the project
decapod test            # Run tests
decapod proof run       # Run proof surface
```

## TODO Subsystem Quick Reference

**Always use the todo subsystem for tracking work:**

| Command | Purpose |
|---------|---------|
| `decapod todo add "title" --priority high` | Create new task |
| `decapod todo list` | List all tasks |
| `decapod todo done <id>` | Mark complete |
| `decapod todo archive <id>` | Archive (REQUIRED after done) |

**See also:** `decapod docs show plugins/TODO.md` for full subsystem docs.

## File Manipulation Rules

**NEVER edit files directly when a Decapod command exists:**

❌ BAD: `echo "todo item" >> .decapod/todo.md`
✅ GOOD: `decapod todo create "todo item"`

❌ BAD: Manually editing proof files
✅ GOOD: `decapod proof record --type=completion --note="implemented feature"`

❌ BAD: Changing config files directly
✅ GOOD: Look for `decapod config set` equivalent

## Decision Protocol (MUST FOLLOW)

### Before Implementation:
1. **State the Intent** in one sentence
2. **Identify Proof Surface** that will fail if wrong
3. **Check Architecture** for constraints and boundaries
4. **Propose Contract Changes** if intent changes
5. **Get User Confirmation** on approach

### During Implementation:
1. **Use Decapod commands** when available
2. **Create ADRs** for irreversible decisions
3. **Write tests FIRST** (mandatory per ARCHITECTURE.md §7)
4. **Update system maps** if boundaries change

### After Implementation:
1. **Run `decapod validate`** - must pass
2. **Create proof events** for significant changes
3. **Update relevant docs** if architecture drifted

## Architecture First, Implementation Second

Per ARCHITECTURE.md, all changes must consider:

- **System Boundary:** What's in/out of scope?
- **Interface Contracts:** Do schemas/protocols change?
- **State Model:** Is data ownership affected?
- **Concurrency:** Are there serialization points?
- **Failure Modes:** What breaks and how to recover?
- **Proof Surface:** What tests must pass?

**If you cannot answer these questions, you do not understand the change well enough to implement it safely.**

## Token Efficiency Protocol

To reduce token usage and errors:

1. **Use `decapod docs ingest`** once at start (optional but recommended)
2. **Reference doc sections** rather than asking to re-read
3. **Use `decapod context pack`** for history management
4. **Ask targeted questions** about specific sections

## This is Non-Authoritative

All contracts, patterns, and behavioral rules live in the embedded constitution accessed via `decapod docs`. This entrypoint routes to authoritative documents.

## Emergency Protocol

If you encounter:
- **Missing proofs:** Create them first
- **Architecture drift:** Enter recovery mode explicitly
- **Bypass requirements:** Document why and get approval

See: `decapod docs show plugins/EMERGENCY_PROTOCOL.md`

## Links

- `embedded/core/DECAPOD.md` — **Authoritative router. READ THIS FIRST.**
- `embedded/core/CONTROL_PLANE.md` — Your operational contract (binding)
- `embedded/specs/SYSTEM.md` — Authority and proof doctrine
- `embedded/specs/INTENT.md` — Authority contracts
- `embedded/specs/ARCHITECTURE.md` — System boundaries and tradeoffs
- `embedded/core/PLUGINS.md` — Subsystem registry
- `embedded/plugins/EMERGENCY_PROTOCOL.md` — Critical procedures
- `.decapod/constitutions/specs/INTENT.md` — Project-specific contracts
