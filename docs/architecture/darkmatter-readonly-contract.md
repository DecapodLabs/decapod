# Darkmatter Read-Only Consumer Contract (`#1711`)

## Architectural Boundary

Darkmatter (`DecapodLabs/darkmatter`) is a dedicated, strictly read-only web
(default `localhost:3000`) and terminal cockpit application.

- **Decapod (`DecapodLabs/decapod`)** owns the authoritative governance kernel,
  structured records (`.decapod/governance.json`, `.decapod/managed/specs/*`,
  custody receipts, validation artifacts), and read-only projection contracts.
- **Darkmatter (`DecapodLabs/darkmatter`)** owns presentation, navigation,
  graph visualization, and read-only rendering.
- **No UI assets live in `DecapodLabs/decapod`.** Neither the web UI nor the
  TUI may mutate Decapod state, create a second source of truth, or emit
  UI-generated proof.

## Schema Version & Invariants

Contract schema identifier: `decapod.darkmatter-readonly-snapshot.v1`.

Every conforming offline or live read-only snapshot MUST satisfy:

1. **Read-Only Flags**:
   - `read_only == true`
   - `mutation_endpoints_exposed == false`
   - `ui_generated_proof_allowed == false`
2. **Repository-Bounded File Access**:
   - Every referenced file path (`governance_json_path`, `relevant_files[]`,
     `completion_evidence[].path`) MUST be a normalized repository-relative
     path with no leading `/`, no Windows root prefix, and no `..` traversal
     segments.
   - Consumers MUST reject symlinks escaping the repository root.
   - Permitted read surfaces are restricted to repository specs
     (`.decapod/managed/specs/*.md`), `.decapod/governance.json`, source files,
     diffs, commits, and custody/validation receipts.
3. **End-to-End Traceability Chain**:
   - `human_requested_intent`: exact recorded human request (distinct from
     agent interpretation).
   - `current_agent_objective`: active derived objective bound to `task_id`.
   - `task_id`, `task_scope[]`, `dependencies[]`: task custody and dependency
     graph edges.
   - `relevant_files[]`: repository-bounded files touched or scoped by the task.
   - `completion_evidence[]`: one or more typed evidence artifacts carrying
     `evidence_id`, `kind`, `status`, `sha256:` content digest, and
     repository-relative `path`.
   - `outstanding_obligations[]`: explicit open obligations and blockers
     (preserving uncertainty rather than compressing it).

## Offline Fixture & Verification

- Canonical offline snapshot fixture:
  `tests/fixtures/darkmatter_readonly_snapshot.v1.json`.
- Kernel verifier:
  - Rust API: `decapod::core::houseboat::validate_darkmatter_snapshot`
  - CLI command: `decapod houseboat verify-darkmatter-snapshot --file tests/fixtures/darkmatter_readonly_snapshot.v1.json`

An independent read-only Darkmatter client can render the snapshot offline with
zero privileged mutation access and verify that human intent traces end-to-end
through agent objective, task scope, dependencies, touched files, cryptographic
evidence digests, and outstanding obligations.

## Cross-Repository Coordination

- Related Decapod issues: `#1711`, `#1712`, `#1713`, `#1714`, `#1720`, `#1735`.
- Web and TUI implementation belongs solely in `DecapodLabs/darkmatter`.
