# Payload Examples

This document provides grounded examples of correct Decapod command invocations and JSON-RPC payloads.

## JSON-RPC Operations (`decapod rpc`)

The `rpc` command is the primary interface for structured agent interaction.

### Retrieve Constitution Directive
```bash
decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'
```

### Resolve Scoped Context
```bash
decapod rpc --op context.scope --params '{"query":"how to handle sqlite migrations","limit":8}'
```

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
