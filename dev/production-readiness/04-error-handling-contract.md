# Error-Handling Contract

## Taxonomy
- User input errors: invalid flags/arguments, missing IDs
- State integrity errors: DB parity mismatch, lineage violations
- External dependency errors: Docker/Git/network auth failures
- Internal invariant violations: impossible state transitions

## Handling rules
- No silent failures
- Return typed errors with actionable remediation steps
- Propagate context for root-cause diagnostics
- Fail closed on security-relevant uncertainty

## Reliability behavior
- Retry only transient external failures with bounded backoff
- Do not retry deterministic validation failures
- Guarantee idempotent task transitions (`open -> done -> archived`)

## Operator output standard
- Report intent
- Report what action ran
- Report exact failure category and next fix step
