# Production Debt Register

## Scoring rubric
- Blast radius: low/medium/high/critical
- Likelihood: rare/possible/likely
- Detectability: easy/moderate/hard
- Priority = blast radius x likelihood, adjusted by detectability

## Current debt entries
1. Validation instability: intermittent SQLite disk I/O during `decapod validate`
- Blast radius: high
- Likelihood: possible
- Detectability: moderate
- Action: isolate storage path assumptions and add resilience checks

2. Task DB parity drift
- Blast radius: high
- Likelihood: likely
- Detectability: easy
- Action: ensure rebuild parity invariants for all task event types

3. Historical lineage gaps in legacy events
- Blast radius: medium
- Likelihood: likely
- Detectability: easy
- Action: backfill commitment/decision lineage nodes for remaining violating task IDs

4. Container workspace GitHub auth brittleness
- Blast radius: medium
- Likelihood: likely
- Detectability: easy
- Action: standardize SSH credential bootstrap for container workflows

## Governance
- Every entry needs owner, due date, and closure proof artifact
- Critical/high debt blocks production-readiness sign-off
