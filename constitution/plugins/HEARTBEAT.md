# HEARTBEAT.md

## ⚠️ DEPRECATED - Use Health Summary Instead

**This subsystem has been consolidated into HEALTH.md.**

### Migration

**Old command:**
```bash
decapod heartbeat
```

**New command:**
```bash
decapod govern health summary
```

### What Changed

The `heartbeat` functionality provided a system health overview by aggregating:
- Health claim states (VERIFIED, STALE, CONTRADICTED, ASSERTED)
- Pending policy approvals
- Watcher staleness status
- System alerts

This functionality is now available as the `summary` subcommand under `decapod govern health`.

### Why It Was Moved

Heartbeat was a thin aggregator over health, policy, and watcher data. Moving it under the `govern` group:
1. Reduces top-level CLI clutter (22 → 9 commands)
2. Groups governance/monitoring commands together
3. Makes the relationship to health explicit
4. Maintains all functionality without changes

### See Also

- `HEALTH.md` - Complete health subsystem documentation
- `MIGRATION.md` (project root) - Full CLI migration guide
- `TRUST.md` - Also deprecated, use `decapod govern health autonomy`

---

**This file is kept for historical reference and will be removed in a future version.**
