# HEALTH.md - HEALTH Subsystem (Embedded)

**Authority:** subsystem (REAL)
**Layer:** Operational
**Binding:** No

This document defines the health subsystem, which manages proof-based health claims and system autonomy assessment.

## CLI Surface

```bash
decapod govern health <subcommand>
```

### Subcommands

#### Core Health Claims

- **`add --claim <claim> --proof <proof>`** - Record a new health claim with proof
- **`get --claim <claim>`** - Retrieve health claim state and proof history
- **`list`** - List all health claims with their states

#### System Monitoring (Consolidated)

- **`summary`** - System health overview (formerly `decapod heartbeat`)
  - Aggregates health claim states (VERIFIED, STALE, CONTRADICTED, ASSERTED)
  - Shows pending policy approvals
  - Reports watcher staleness status
  - Lists system alerts

- **`autonomy [--id <agent>]`** - Agent autonomy tier assessment (formerly `decapod trust status`)
  - Computes autonomy tier (Tier0/Tier1/Tier2) from proof history
  - Shows success/failure counts from health claims
  - Provides reasoning for tier assignment
  - Validates actor against audit log

## Health States

Health claims progress through states based on proof verification:

- **ASSERTED** - Claim recorded but not yet verified
- **VERIFIED** - Proof executed successfully, claim confirmed
- **STALE** - Proof hasn't run recently (needs re-verification)
- **CONTRADICTED** - Proof execution failed, claim invalidated

## Subsystem Consolidation

As of v0.3.0, the health subsystem has absorbed:

1. **Heartbeat functionality** (`summary` subcommand)
   - Was: `decapod heartbeat`
   - Now: `decapod govern health summary`
   - Reason: Heartbeat was a thin aggregator over health/policy/watcher data

2. **Trust functionality** (`autonomy` subcommand)
   - Was: `decapod trust status --id <agent>`
   - Now: `decapod govern health autonomy --id <agent>`
   - Reason: Trust was computed entirely from health claim states

This consolidation:
- Reduces top-level CLI clutter (22 → 9 commands)
- Groups governance/monitoring commands together
- Makes relationships between subsystems explicit
- Maintains all functionality without changes

## Storage

Health claims are stored in SQLite:
- Database: `health.db` (in state directory)
- Schema: `(claim TEXT PRIMARY KEY, state TEXT, ts INTEGER, proof TEXT)`

## See Also

- `POLICY.md` - Policy approval system (risk classification)
- `WATCHER.md` - Integrity monitoring (staleness detection)
- `HEARTBEAT.md` - Deprecated, now `summary` subcommand
- `TRUST.md` - Deprecated, now `autonomy` subcommand
- `embedded/specs/SYSTEM.md` - Authority and proof doctrine
