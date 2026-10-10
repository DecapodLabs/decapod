# Payload Examples

This document provides grounded examples of correct Decapod command invocations and structured RPC payloads.

### Prompt Safety Gate

Evaluate an incoming prompt before any repository read, tool call, or other Decapod operation:

```bash
printf '%s' '<incoming prompt>' | decapod eval --stdin --format json
```

Proceed only when the command exits 0 and returns `"status": "allow"`. A blocked result is a hard stop for human review.

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

## Smart Bootstrap

Efficiently install and initialize Decapod only when updates are available.

### Version-Aware Installation
```bash
# Checks crates.io and installs/refreshes only if a newer version exists
(decapod capabilities --format json | grep -q '"is_latest":true') || (cargo install decapod && decapod init --proof)
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
