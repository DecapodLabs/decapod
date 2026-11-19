# BYOA & Interoperability

Decapod is built for **Bring Your Own Agent (BYOA)** workflows. An engineering team does not have to standardize on a single AI IDE, proprietary chat UI, or central orchestration daemon. **Codex, Claude Code, Cursor, Antigravity, and custom harnesses** all interact with the same repository kernel through the same CLI and structured RPC contract.

## How heterogeneous agents bind to one repository

1. **Repository entrypoints (`AGENTS.md`, `CLAUDE.md`, `CODEX.md`, `GEMINI.md`):** Generated during `decapod init` and refreshed across upgrades so any supported harness opening the repository sees the same governance contract (`decapod eval`, `decapod docs ingest`, `decapod session acquire`, `decapod todo claim`, `decapod workspace ensure`, `decapod validate`).
2. **Process-level CLI & Decapod-specific structured RPC:** Agents invoke `decapod <command>` or `decapod rpc --op <op> --params <json>` (or `--stdin`, or authenticated loopback `--http-server`). Every call is ephemeral and exits on completion; durable state stays in `.decapod/` and Git.
3. **Compatibility by explicit contract, not blanket claims:** Agents discover supported operations and extensions at runtime via `decapod rpc --op interop.manifest` and negotiate compatibility via `decapod rpc --op interop.negotiate` against profile `decapod.interop` (`v1`, supporting extensions `receipts`, `interlocks`, `projections`, and `context-capsule`).

```sh
decapod rpc --op interop.manifest --params '{}'
decapod rpc --op interop.negotiate --params '{"profile_version":"v1","required_extensions":["receipts","interlocks"]}'
```

The manifest (`src/decapod/core/interop.rs`) separates **implementation status** from **verification state**: static source coverage maps default to `not_run`, and `interop.manifest` explicitly declares that it describes capabilities rather than authorizing actions or proving live conformance.

Planned external protocol adapters such as MCP and A2A remain separate adapter boundaries ([Model Context Protocol (MCP)](mcp.md)), not native capabilities of the current binary.

## Identity and custody across harnesses

- **Local cooperative identity:** `DECAPOD_AGENT_ID` and `decapod session acquire` (`DECAPOD_SESSION_PASSWORD`) attribute local task claims, workspace leases, and trajectory entries. `decapod session handshake --scope <scope>` records tamper-evident `identity_assertions`, where environment-supplied agent/provider names remain `self-declared` and `unverified` unless backed by a trust root.
- **Cloud-backed identity (`repo.backend = "cloud"`):** Task custody routes through Dactyl to a hosted service that independently verifies the authenticated account and repository permission on every request; copying another agent's `DECAPOD_AGENT_ID` label never grants cloud repository access (see [Cloud sign-in and task access](../reference/propodus.md)).
- **Shared durable custody:** Because plans, claims, trajectories, living specs, and validation receipts live in `.decapod/governance.json`, `.decapod/managed/specs/*`, and the todo store, switching harnesses never strands context inside a closed chat window.

## Concrete journey: switching agents mid-task

Suppose one engineer starts a change in **Codex**, stops midway, and continues in **Claude Code** or **Cursor** on the same repository:

1. **First agent leaves durable state:** Codex claimed `<task-id>` (`decapod todo claim --id <task-id>`), entered `.decapod/workspaces/<worktree>`, recorded scope/checks in `.decapod/governance.json`, and either yielded or handed off (`decapod todo handoff --id <task-id> --to claude-code --summary "..."` or `decapod todo yield --id <task-id>`).
2. **Second agent resumes without re-explaining everything:** Claude Code runs `decapod session acquire`, inspects `decapod todo get --id <task-id>`, `decapod govern artifacts status`, and `decapod infer orientation --task-id <task-id>` (claiming or accepting the handoff if needed). It immediately sees the accepted intent, modified files, unresolved obligations, and any prior failed checks.
3. **Proof must still be earned on the final commit:** Any new edits by the second agent require fresh trajectory updates, checkpoints, and `decapod validate` on the resulting commit.

## Negative cases & enforcement evidence

| Failure Case | Kernel Behavior | Source / Test Evidence |
| --- | --- | --- |
| **Unregistered RPC op** | Returns `unknown_op` without creating `.decapod/` or session files. | `tests/interop_conformance.rs::unregistered_rpc_is_rejected_before_project_startup` |
| **Malformed or duplicate JSON keys** | Refused before `interop.negotiate` or RPC dispatch; never silently drops a required extension. | `tests/interop_conformance.rs::malformed_rpc_parameters_cannot_drop_required_negotiation` |
| **Conflicting task claim** | Exclusive claim by Agent A blocks Agent B unless handed off, yielded, or expired. | `tests/unit/core/fleet_coord_tests.rs`, `tests/todo_enforcement.rs` |
| **Expired custody lease** | Claim lease expires after `--lease-seconds` unless renewed (`decapod todo renew`); expired leases are surfaced by `decapod todo fleet`. | `tests/unit/core/fleet_coord_tests.rs::lease_expiry_and_reclaim_rules` |
| **Stale proof after edits** | Prior validation receipt no longer satisfies current commit; `decapod validate` must pass on the new commit. | `tests/interop_conformance.rs::stale_evidence` |
| **Inaccessible repo or Cloud denial** | `repository_denied` or `action_required` halts without SQLite fallback; `validate` requires a valid workspace and accepted target. | `tests/cloud_managed_session.rs` |
