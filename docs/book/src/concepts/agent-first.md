# Agent-First Architecture

Decapod exposes a machine-facing governance contract for **coding agents**. The human supplies engineering intent and judgment through their chosen editor or harness; **the coding agent invokes Decapod** at decision, coordination, and verification boundaries while performing the work.

Because the contract lives at the repository boundary rather than inside a vendor-specific runtime, **Codex, Claude Code, Cursor, Antigravity, and custom CLI harnesses** can operate on the same repository through the same kernel interface (see [BYOA & Interoperability](interoperability.md)).

## The Agentic Lifecycle

Decapod structures agent work into an explicit, resumable decision-time lifecycle:

1. **Prompt Gate & Ingestion:** The agent checks untrusted input (`decapod eval --stdin --format json`), reads embedded command contracts (`decapod docs ingest`), and acquires a local or cloud-backed session (`decapod session acquire`).
2. **Task Claiming & Isolation:** The agent claims a `todo` with an explicit lease (`decapod todo claim --id <id> --lease-seconds 3600`) and creates an isolated worktree (`decapod workspace ensure --branch ...`) so overlapping work stays visible and isolated (see [Single-Agent Workflow](../workflows/single-agent.md) and [Multi-Agent Workflow](../workflows/multi-agent.md)).
3. **Decision-Time Orientation & Reflection:** Inside the worktree, the agent calls `decapod infer orientation --task-id <id>` and `decapod rpc --op context.resolve` to load plan constraints, living specs, and task scope. At consequential boundaries, it may pass `--reflection '<json>'` or invoke `decapod rpc --op mentor.obligations` for bounded three-round challenge checks, and query the embedded [Constitution](constitution.md) for engineering reference context.
4. **Implementation & Trajectory Custody:** The agent edits code and living specifications (`.decapod/managed/specs/*`) inside its isolated workspace, records trajectory observations (`decapod govern trajectory record`), reviews changed contracts (`decapod govern plan review-spec`), and checkpoints staged material (`decapod govern artifacts checkpoint`) before committing.
5. **Validation & Exact-Commit Recovery:** The agent runs `decapod validate` on the committed candidate. Failures return typed blockers and recovery guidance rather than passing silently; the agent repairs the branch, commits a new checkpoint, and re-runs `decapod validate` on the exact revised commit (see [Proof](proof.md)).
6. **Publication & Completion:** Passing gates permit `decapod workspace publish` and `decapod todo done --id <id> --validated` (see [Artifacts Reference](../reference/artifacts.md)).

## Key Agent-First Concepts

### 1. Deterministic Context Capsules
Models degrade when flooded with stale or irrelevant prose. Decapod's **Context Capsules** (`decapod govern capsule query`, `decapod rpc --op context.resolve`) select bounded, provenance-tagged slices of doctrine and project state so different harnesses start from the same constraints.

### 2. Living Specifications
Living specifications (`.decapod/managed/specs/*`) are the acting agent's explicit interpretation of the repository. The agent authors and maintains their semantic prose; Decapod requires and validates them (`decapod rpc --op specs.refresh` or `decapod validate --refresh-specs` refreshes supported fingerprints and manifests without inventing authored claims; see [Intent](intent.md)).

An incorrect specification surfaces a misunderstanding before publication. A stale specification means governed work remains incomplete.

### 3. Aptitude & Federated Knowledge
Agents can preserve learned repository conventions and procedural observations across sessions using `decapod data aptitude` and `decapod data federation`, while task custody and unresolved obligations persist in `todo` and `.decapod/governance.json`.

### 4. Structured RPC and Adapter Boundaries (MCP)
Decapod exposes a CLI and a Decapod-specific structured RPC interface (`decapod rpc --op <op>`). It reserves an adapter boundary for the **Model Context Protocol (MCP)** so future adapters can expose repository operations over MCP; the current binary does not itself implement an MCP server (see [Model Context Protocol (MCP)](mcp.md) and [BYOA & Interoperability](interoperability.md)).

## Decision-Time Negative Cases

Agent-first governance fails closed rather than smoothing over broken preconditions:

- **Unregistered or malformed RPC:** Unknown ops return `unknown_op` without initializing project state; malformed or duplicate-key JSON parameters are rejected before dispatch (`tests/interop_conformance.rs`).
- **Missing session or expired claim:** Mutating commands refuse unauthenticated or expired custody rather than silently reassigning ownership (`decapod todo renew` or `decapod todo handoff` is required).
- **Reflection stagnation or blocked escalation:** Repeating an unchanged forbidden-path proposal or asking a routine discoverable question returns `reflect` / `blocked` rather than manufacturing human approval (`tests/infer_precision.rs`).
- **Stale proof or Cloud denial:** Editing files invalidates prior validation; in `backend = "cloud"` mode, `repository_denied` or `action_required` halts without falling back to local SQLite (`tests/cloud_managed_session.rs`).
