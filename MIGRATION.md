# CLI Migration Guide (v0.2.x → v0.3.0)

## Breaking Changes

The Decapod CLI has been streamlined from 22 to 9 commands for better organization and discoverability.

## Quick Reference

| Old Command | New Command | Alias |
|-------------|-------------|-------|
| `decapod init` | `decapod init` | `decapod i` |
| `decapod clean` | `decapod init clean` | - |
| `decapod hook` | `decapod setup hook` | - |
| `decapod docs` | `decapod docs` | `decapod d` |
| `decapod todo` | `decapod todo` | `decapod t` |
| `decapod validate` | `decapod validate` | `decapod v` |
| `decapod policy` | `decapod govern policy` | `decapod g policy` |
| `decapod health` | `decapod govern health` | `decapod g health` |
| `decapod proof` | `decapod govern proof` | `decapod g proof` |
| `decapod watcher` | `decapod govern watcher` | `decapod g watcher` |
| `decapod heartbeat` | `decapod govern health summary` | - |
| `decapod trust` | `decapod govern health autonomy` | - |
| `decapod feedback` | `decapod govern feedback` | `decapod g feedback` |
| `decapod archive` | `decapod data archive` | - |
| `decapod knowledge` | `decapod data knowledge` | - |
| `decapod context` | `decapod data context` | - |
| `decapod schema` | `decapod data schema` | - |
| `decapod repo` | `decapod data repo` | - |
| `decapod broker` | `decapod data broker` | - |
| `decapod teammate` | `decapod data teammate` | - |
| `decapod cron` | `decapod auto cron` | `decapod a cron` |
| `decapod reflex` | `decapod auto reflex` | `decapod a reflex` |
| `decapod verify` | `decapod qa verify` | `decapod q verify` |
| `decapod check` | `decapod qa check` | `decapod q check` |

## Command Groups

### Init & Setup
```bash
# Bootstrap (unchanged)
decapod init

# Teardown (now a subcommand)
decapod init clean

# Git hooks (new setup namespace)
decapod setup hook
```

### Core Workflow (Unchanged)
```bash
decapod docs        # or: decapod d
decapod todo        # or: decapod t
decapod validate    # or: decapod v
```

### Govern (Governance & Safety)
```bash
# All governance commands now under 'govern' namespace
decapod govern policy eval --command task.archive
decapod govern health get --id myclaim
decapod govern health summary              # was: decapod heartbeat
decapod govern health autonomy --id agent  # was: decapod trust status
decapod govern proof run
decapod govern watcher run
decapod govern feedback add --source cli --text "..."
```

### Data (Data Management)
```bash
# All data commands now under 'data' namespace
decapod data archive list
decapod data knowledge search --query "..."
decapod data context audit --profile main --files file1.txt
decapod data schema --subsystem health
decapod data repo map
decapod data broker audit
decapod data teammate list
```

### Auto (Automation)
```bash
# Automation commands now under 'auto' namespace
decapod auto cron add --schedule "0 * * * *" --command "..."
decapod auto reflex add --trigger on-push --command "..."
```

### QA (Quality Assurance)
```bash
# CI/QA commands now under 'qa' namespace
decapod qa verify
decapod qa check --all
```

## Notable Changes

### Consolidated Commands

**Heartbeat → Health Summary**
- The `heartbeat` command provided a system health overview
- Now available as `decapod govern health summary`
- All functionality preserved, same JSON output format

**Trust → Health Autonomy**
- The `trust status` command computed agent autonomy tiers
- Now available as `decapod govern health autonomy`
- All functionality preserved, same tier calculation

### New Aliases

Common commands now have single-letter aliases:
- `decapod i` → init
- `decapod d` → docs
- `decapod t` → todo
- `decapod v` → validate
- `decapod g` → govern
- `decapod a` → auto
- `decapod q` → qa

### Help is Better

```bash
# Top-level help now shows only 9 commands (was 22)
decapod --help

# Each group has focused help
decapod govern --help
decapod data --help
```

## Migration Strategy

### For Scripts

1. **Search and replace** old commands with new paths
2. **Test each command** to ensure it works
3. **Update docs** in your project

### For Agents

Agents should read the updated constitution files which will reflect the new CLI structure:
- `CLAUDE.md` - Updated with new command examples
- `constitution/embedded/core/DECAPOD.md` - Router with new structure
- `constitution/embedded/plugins/*.md` - Plugin docs with new paths

### For CI/CD

Update all CI scripts to use new command paths:

```bash
# OLD
decapod verify
decapod check --crate-description

# NEW
decapod qa verify
decapod qa check --crate-description
```

## Breaking Changes Summary

1. **No backward compatibility**: Old command paths will not work
2. **Heartbeat removed**: Use `decapod govern health summary`
3. **Trust removed**: Use `decapod govern health autonomy`
4. **Clean moved**: Use `decapod init clean`
5. **Hook moved**: Use `decapod setup hook`
6. **Check moved**: Use `decapod qa check`

## Benefits

- **Better organization**: Related commands grouped together
- **Easier discovery**: 9 groups vs 22 flat commands
- **Shorter commands**: Use aliases for common operations
- **Clearer purpose**: Each group has a clear responsibility
- **Room to grow**: Can add new commands to groups without cluttering top level

## Need Help?

- Run `decapod --help` to see all available commands
- Run `decapod <group> --help` to see group-specific commands
- Check `CLAUDE.md` for agent-specific guidance
- See updated constitution docs in `constitution/embedded/`
