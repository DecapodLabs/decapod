# AUTOUPDATE.md - Auto-Update Protocol

**Authority:** subsystem (REAL)
**Layer:** Operational
**Binding:** Yes (for agents)
**Scope:** Binary version management and constitution refresh

This document defines the mandatory auto-update protocol that all agents must follow.

## Purpose

Agents operate on evolving infrastructure. The decapod binary ships updates. The constitution changes. Commands are renamed or moved. Without a refresh protocol, agents will:

1. Call commands that no longer exist
2. Use outdated syntax
3. Violate updated contracts they haven't read
4. Operate on stale assumptions about available features

**The auto-update protocol prevents this drift.**

## The Protocol (Mandatory)

Every agent session MUST begin with:

```bash
decapod --version                   # 1. Check installed version
decapod --help                      # 2. Verify command structure
decapod docs show core/DECAPOD.md  # 3. Refresh constitution
decapod validate                    # 4. Verify system state
decapod todo list                   # 5. Check active work
```

**This is not optional.** This is a proof gate. Agents that skip this will produce unverified work.

## Why Each Step Matters

### 1. `decapod --version`
**Check:** What version of the binary is installed?

- Prevents calling commands from newer versions that don't exist yet
- Alerts to version mismatches between documentation and binary
- Required for debugging and support

### 2. `decapod --help`
**Check:** What commands are currently available?

- Shows the current command structure (grouped vs flat)
- Reveals new commands that weren't in the last version
- Identifies deprecated/removed commands before you call them

**Example:** You remember `decapod heartbeat`. Running `--help` shows it's now `decapod govern health summary`. You adjust before calling the wrong command.

### 3. `decapod docs show core/DECAPOD.md`
**Check:** What's the current contract?

- Refreshes your understanding of the constitution
- Shows updated routing, authority, and binding rules
- Reveals new invariants or changed workflows

**Example:** The constitution may have added a new mandatory validation gate. Refreshing ensures you see it.

### 4. `decapod validate`
**Check:** Is the system currently healthy?

- Runs all proof gates to verify repo state
- Surfaces any pre-existing validation failures
- Establishes a baseline before you make changes

**Example:** If validation already fails, you know not to assume your changes broke it.

### 5. `decapod todo list`
**Check:** What work is currently active?

- Shows tasks other agents may be working on
- Reveals claimed tasks (prevents duplicate work)
- Identifies your next assignment

**Example:** Another agent claimed the task you were planning to work on. You see this and pick a different one.

## Enforcement

This protocol is enforced through:

1. **Agent entrypoints**: All templates (`CLAUDE.md`, `AGENTS.md`, etc.) mandate this sequence
2. **Constitution**: `DECAPOD.md` declares this as an absolute requirement
3. **Validation gates**: Future validation may check for evidence of protocol compliance
4. **Agent contracts**: Skipping this protocol is a contract violation

## Failure Modes

**What happens if you skip this protocol:**

| Skipped Step | Failure Mode | Example |
|--------------|--------------|---------|
| `--version` | Call commands that don't exist in this version | You call `decapod data schema` but binary is v0.2.x (doesn't have grouped commands yet) |
| `--help` | Use renamed/moved commands | You call `decapod heartbeat` (removed) instead of `decapod govern health summary` |
| `docs show` | Violate updated constitution | New contract requires approval for `task.archive` but you didn't refresh and bypass it |
| `validate` | Assume clean state when broken | Validation already failing, you make changes and claim you "broke it" |
| `todo list` | Duplicate work or claim conflicts | Another agent already claimed the task, you work on it anyway |

## CLI Surface

This is not a standalone command - it's a protocol. The commands are:

```bash
decapod --version
decapod --help
decapod docs show core/DECAPOD.md
decapod validate
decapod todo list
```

## See Also

- `embedded/core/DECAPOD.md` — Router (mandates this protocol in §1.1)
- `AGENTS.md` — Universal agent contract (includes mandatory start sequence)
- `CLAUDE.md`, `GEMINI.md`, `CODEX.md`, `OPENCODE.md` — Agent entrypoints (all mandate this)

---

**This protocol is binding. Skipping it is a contract violation.**
