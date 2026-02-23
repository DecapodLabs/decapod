# JUDGE_CONTRACT.md - Judge Validation Contract

**Authority:** spec (evaluation judge contract)
**Layer:** Specs
**Binding:** Yes

## Purpose

Define strict, bounded, machine-checkable judge semantics for non-deterministic tasks.

## Strict JSON Contract

Judge outputs used for promotion MUST validate against this shape:

```json
{
  "success": true,
  "explanation": "string, non-empty",
  "failure_reason": "optional string",
  "reached_captcha": false,
  "impossible_task": false
}
```

Rules:
1. Unknown or malformed JSON is invalid.
2. `explanation` MUST be non-empty.
3. Contract violations MUST fail with typed marker: `EVAL_JUDGE_JSON_CONTRACT_ERROR`.

## Bounded Execution

1. Judge execution MUST be bounded by timeout.
2. Timeout MUST fail with typed marker: `EVAL_JUDGE_TIMEOUT`.
3. Timed-out judge artifacts MUST block promotion gates.

## Unbiased-When-Wrong Operationalization

1. A single judge verdict is not sufficient evidence for noisy tasks.
2. Promotion relies on repeated judged runs + aggregate statistics, not one judgment.
3. Judge failures/reasons MUST remain inspectable in durable artifacts.

## Failure Flags

Judge verdicts MUST preserve explicit flags when present:
- `impossible_task`
- `reached_captcha`
- `failure_reason`

These fields are first-class evidence inputs for failure bucketing and remediation planning.

