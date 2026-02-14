# TRUST.md

## ⚠️ DEPRECATED - Use Health Autonomy Instead

**This subsystem has been consolidated into HEALTH.md.**

### Migration

**Old command:**
```bash
decapod trust status --id <agent>
```

**New command:**
```bash
decapod govern health autonomy --id <agent>
```

### What Changed

The `trust` functionality provided agent autonomy tier assessment by computing:
- Autonomy tier (Tier0/Tier1/Tier2) based on proof history
- Success/failure counts from health claims
- Reasoning for tier assignment
- Actor validation against audit log

This functionality is now available as the `autonomy` subcommand under `decapod govern health`.

### Why It Was Moved

Trust status was computed entirely from health claim states and proof events. Moving it under the `govern` group:
1. Reduces top-level CLI clutter (22 → 9 commands)
2. Groups governance/monitoring commands together
3. Makes the relationship to health explicit
4. Maintains all functionality without changes

### See Also

- `HEALTH.md` - Complete health subsystem documentation
- `MIGRATION.md` (project root) - Full CLI migration guide
- `HEARTBEAT.md` - Also deprecated, use `decapod govern health summary`

---

**This file is kept for historical reference and will be removed in a future version.**
