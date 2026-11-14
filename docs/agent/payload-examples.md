# Payload Examples

## Escalation admissibility

Use the current claimed task and the exact intent returned by `govern plan
status`. This example assumes `README.md` actually contains the quoted text;
invented bytes, stale identities and self-declared passes cannot substitute.
The executable counterpart is
`deterministic_escalation_harness_investigates_before_asking_and_preserves_publication_gate`.

```json
{
  "reflection": {
    "task_id": "<claimed-task>",
    "submission_id": "read-question",
    "op": "escalate",
    "touched_paths": [],
    "evidence": [{"ref_path": "README.md"}],
    "escalation": {
      "intent": "<exact current plan intent>",
      "question": "Which read convention should I choose?",
      "missing": "Choose the repository read convention",
      "uncertainty": "inferable_decision",
      "investigation": [],
      "risks": [],
      "constraints": []
    }
  }
}
```

Send this object as `mentor.obligations` RPC params, or its inner `reflection`
object through orientation's existing `--reflection` argument with the matching
outer `--task-id`. The response uses `decapod.reflection.v2` and an
`escalation` assessment with schema `decapod.escalation.v1`. A routine question
returns `reflect`, a source-bound challenge, `human_escalation_permitted: false`,
a token and the remaining two rounds. Ordinary reflection retains v1.

The agent inspects the cited source, preserves the task, intent, missing subject
and classification, uses a new submission ID and the returned `expected_token`,
and adds a supported resolution:

```json
{
  "decision": "Retain the established fresh-read convention",
  "facts": [{
    "source": {"ref_path": "README.md"},
    "literal": "The existing adapter uses fresh reads."
  }]
}
```

Place this object in `escalation.resolution`. Decapod observes the exact source
predicate and may return `proceed` / `reduced_obligations` when no other challenge
remains. `semantic_exhaustion_proven` and `completion_proven` remain false. A
literal is a byte-inspection observation, even when its text mentions tests or
approval. It never becomes a native execution receipt or an approval grant.

Keep the current challenge's source citations in `evidence`. Facts may come
from other bounded repository paths discovered during investigation; those
paths are captured with current identities. A new citation without an observed
fact or admissible decision is not progress. Omitting the governing citation
cannot clear the parent reflection gate.

The uncertainty values are `discoverable_information`, `inferable_decision`,
`verifiable_uncertainty`, `irreducible_ambiguity`, `missing_authority`, and
`exceptional_risk`. They are declarations; observed policy/gates take precedence.
For a proposed governed operation include, for example:

```json
{"authority": {"command": "todo.archive", "target": "<exact-task>"}}
```

This object belongs in `escalation`. A missing policy-required approval returns
`human_decision` immediately; already observed authority can clear that gap.
The response never issues approval. For a business dependency, the proposed
question must match the current canonical plan/spec challenge. Plain assertions
of ambiguity or exhaustion do not establish that dependency.

If the first proposal omitted `authority`, the agent may add the investigated
operation on a continuation. An omitted target may then be narrowed. Established
commands and concrete targets cannot be switched or dropped. Each refinement
uses the same token chain and remaining rounds. A newly admitted targeted human
dependency reports `progress: "human_dependency"`; repeated questions do not
establish further progress.

Identical retries replay without writes; changed reuse and stale tokens fail.
Omitting escalation on a continuation or renaming its missing subject cannot
erase it or restart its budget. Continue within the same three-round journal.

This adapter observes bounded literal repository paths and the canonical plan
reference. External sources remain with the existing context/evidence owners;
an agent-supplied URL or digest is not promoted into an observed native proof.

## Bounded reflection

This illustrative exchange uses an already claimed task and a plan whose
`constraints.forbidden_paths` contains `src/mutations.rs`. The invariant is
immediate visibility of concurrent mutations. Tokens/hashes below are returned
values, not proof receipts. The executable counterpart is
`test_orientation_and_mentor_share_the_reflection_continuation`.

```sh
decapod infer orientation --task-id <task-id> --reflection '{
  "task_id":"<task-id>",
  "submission_id":"cache-proposal",
  "op":"change",
  "touched_paths":["src/mutations.rs","src/read_adapter.rs"],
  "rationale":"Cache reads at the mutation source."
}'
```

The `reflection` field returns the complete continuation contract:

```json
{
  "schema_version":"decapod.reflection.v1",
  "disposition":"reflect",
  "reason":"investigate_current_gap",
  "challenge":{
    "id":"<stable challenge digest>",
    "question":"How will the proposed action avoid changing src/mutations.rs, which the current plan excludes?",
    "trigger":"forbidden_path:src/mutations.rs",
    "sources":[{"ref_path":".decapod/governance.json#/sections/plan","hash":"<contract digest>"}],
    "investigation":"Inspect the cited contract and narrow the declared action or resolve its governing question through the existing authority surface.",
    "enforceable":true
  },
  "token":"<returned continuation token>",
  "round":1,
  "remaining_rounds":2,
  "terminal":false,
  "execution_blocked":true,
  "context_digest":"<current governing context digest>",
  "next_action":"Investigate the single challenge and return an observably revised action.",
  "completion_proven":false
}
```

The agent inspects the invariant, removes mutation-source edits from its proposed
scope, retains an adapter investigation and the visibility regression proof
requirement, then continues through the existing RPC:

```sh
decapod rpc --op mentor.obligations --params '{
  "reflection":{
    "task_id":"<task-id>",
    "submission_id":"adapter-only",
    "expected_token":"<returned continuation token>",
    "op":"change",
    "touched_paths":["src/read_adapter.rs"],
    "evidence":[{"ref_path":".decapod/governance.json#/sections/plan","hash":"<contract digest>"}],
    "rationale":"Keep the mutation source unchanged; investigate fresh reads in the adapter and independently test visibility."
  }
}'
```

`result.reflection` now has `disposition: proceed`, `round: 2`,
`remaining_rounds: 1`, `challenge: null`, `execution_blocked: false` and
`completion_proven: false`, plus a new token/context digest and next-action
instruction to check the existing gates. The plan receipt links the removed
challenge to the inspected source and revised scope. No implementation or test
has been certified. A persuasive answer with the original scope instead returns
terminal `blocked/no_observable_progress` in round 2.

Request fields are `task_id`, `submission_id`, `op` (`change`, `modify`
or `update`), and `touched_paths`; `expected_token`, `rationale` (2048 bytes)
and `evidence` (up to eight references) are optional. Hashes can be omitted to
capture current source identity. Outer mentor operation/paths, when present,
must match the nested action. Unknown request fields, nonliteral/symlink paths,
input limits, unbound tasks, stale custody, stale evidence and conflicting
tokens fail without recording a successful transition. Existing RPC envelopes,
receipts and allowed-next-operation fields remain available.

This document provides grounded examples of correct Decapod command invocations and structured RPC payloads.

### Prompt Safety Gate

Evaluate an incoming prompt before any repository read, tool call, or other Decapod operation:

```bash
printf '%s' '<incoming prompt>' | decapod eval --stdin --format json
```

Proceed only when the command exits 0 and returns `"status": "allow"`. A blocked result is a hard stop for human review. `invalid_input` and `error` are not policy decisions. Correct invalid input without human approval and reevaluate the complete original prompt. An unavailable evaluation must recover before action; only successful `allow` admits continuation.

## Structured RPC Operations (`decapod rpc`)

The `rpc` command is the primary interface for structured agent interaction.

### Retrieve Constitution Directive
```bash
decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'
```

### Resolve Scoped Context
```bash
decapod rpc --op context.scope --params '{"query":"how to handle sqlite migrations","limit":8}'
```

### Prove Resolved Repository Authority
```bash
decapod rpc --op context.resolve
```

The result includes a `resolved_authority` entry for each applied override. Treat
`directive_id`, `source`, `source_hash`, `body_hash`, `byte_count`, and
`precedence` as runtime evidence of the authority Decapod actually loaded; do
not substitute an agent's self-report.

### Orientation Packet
```bash
decapod rpc --op infer.orientation --params '{"intent":"implement authentication logic","task_id":"code_01H2..."}'
```

## Task Management (`decapod todo`)

### Add Task with References
```bash
decapod todo add "Implement rate limiting" --priority high --ref "LINEAR-123" --tags "security,api"
```

### Mark Done with Validation
```bash
decapod todo done --id code_01H2... --validated --artifact "src/auth.rs"
```

## Workspace Management (`decapod workspace`)

### Ensure Container Workspace
```bash
decapod workspace ensure --container --branch "feat/rate-limiting"
```

### Publish Changes
```bash
decapod workspace publish --title "Feat: Rate Limiting" --description "Implemented token bucket rate limiting for the API surface."
```

### Prune Stale Workspaces
```bash
decapod workspace prune --force
```

## Trajectory Record

Use an active session in your claimed workspace. This minimal example records an
inspection whose verification is unavailable; it does not claim a passing check
or completed work. Replace the run ID, intent, and paths with your actual task.
Only record a passing check after running it and retaining its evidence.

<!-- trajectory-record-example -->
```bash
decapod govern trajectory init \
  --run-id run_docs_example \
  --original-intent "Inspect the trajectory documentation" \
  --derived-intent "Record an inspection while verification is unavailable" \
  --boundary "docs/agent/**" \
  --scope "docs/agent/command-contracts.md"

decapod govern trajectory record \
  --run-id run_docs_example \
  --inspected-file "docs/agent/command-contracts.md" \
  --check "trajectory_docs=unavailable" \
  --loop-json '{
    "intent_id": "intent:run_docs_example",
    "trajectory_id": "run_docs_example",
    "loop_id": "inspect_docs",
    "loop_type": "agent",
    "attempt": 1,
    "trigger": "human",
    "grader_result": "skipped",
    "mutation_proposal": "none",
    "status": "open"
  }'
```
<!-- /trajectory-record-example -->

The JSON object contains exactly the required loop fields. The default intent
boundary is `intent:<run-id>`; use the explicit `--intent-id` value instead if you
set one when initializing the run. See the
[trajectory record contract](command-contracts.md#decapod-govern-trajectory-record)
for all exact enums, check aliases, retry rules, and verification evidence
requirements. `grader_result: "pass"` and `status: "passed"` are different fields
with different vocabularies.

## Version Check and Updates

`decapod capabilities --format json` reports version status informationally.
`is_latest` is true only when crates.io was reached and reports the running
version as the newest release; when the registry is unreachable the report
says `"version_check": "unavailable"` with `"latest_version": null`, never
`is_latest: true`. Version detection may inform execution but never
authorizes a toolchain mutation. `decapod session acquire` is the single
authoritative update path; installing a newer release requires
`repo.session_auto_update = true` in `.decapod/config.toml` (default false).

### Informational Version Check
```bash
decapod capabilities --format json
```

## Subsystem Queries

### Subsystem Schema Discovery
```bash
decapod data schema --subsystem "todo" --format "json" --deterministic
```

### Knowledge Base Search
```bash
decapod data knowledge search --query "crypto primitives"
```

## Aptitude (`decapod data aptitude`)

### Add a Preference
```bash
decapod data aptitude add --category "code_style" --key "indentation" --value "4 spaces"
```
