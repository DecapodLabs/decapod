# Structured Logging Baseline

## Log schema
- Required fields: timestamp, level, subsystem, operation, task_id, event_id, actor, outcome
- Optional fields: dependency, retry_count, duration_ms, error_code

## Correlation model
- Tie task lifecycle events to federation nodes and proof records
- Carry intent references through mutation operations

## Redaction rules
- Never log secrets, private keys, raw tokens, or full credential paths
- Hash or truncate sensitive identifiers where needed

## Retention and usage
- Keep logs long enough for incident reconstruction
- Provide canonical diagnostic queries for validate failures and state rebuild mismatches
