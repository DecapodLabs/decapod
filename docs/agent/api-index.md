# Agent API Orientation

This is the primary orientation surface for AI agents. Before performing implementation work, you MUST ingest this index and its supporting contracts.

**Absolute Rule:** `--help` is a syntax reference for humans. For operational behavior, preconditions, and state assumptions, you **MUST** consult the command contracts in `docs/agent/command-contracts.md`.

## When to Call Decapod

Call Decapod at **Inference Pressure Points**:

1.  **Before Implementation:** Claim the task and ensure the workspace.
    - `decapod todo claim --id <id>`
    - `decapod workspace ensure`
2.  **Before Design:** Query the constitution to orient yourself within the repo's rules.
    - `decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'`
3.  **Before Committing:** Validate that your implementation meets the project's governance gates.
    - `decapod validate`
4.  **Before Completion:** Mark the task done to generate mandatory proof artifacts.
    - `decapod todo done --id <id>`

## The Agentic Protocol

- **Orientation First:** Never start work without a claimed task and an isolated workspace.
- **Follow the Errors:** Decapod error messages are deterministic recovery instructions. If a call fails, do not guess; follow the error's guidance.
- **Respect the Schema:** Consult Decapod configuration to understand the current repo's policy (e.g., if container isolation is required).
- **Maintain Custody:** Update `specs/INTENT.md` if your research reveals that the original task intent needs refinement.

## Documentation Index

| Resource | Purpose |
|---|---|
| [Command Contracts](command-contracts.md) | **Critical:** Mandatory preconditions and state transitions. |
| [Payload Examples](payload-examples.md) | Correct JSON shapes for RPC and complex flags. |
| [Error Recovery](error-recovery.md) | How to respond to Decapod exit codes and messages. |
| [State Model](state-model.md) | Understanding the lifecycle of Tasks and Workspaces. |
| [Config Schema](config-schema.md) | Policy keys that govern your behavior. |
| [Agent Skills](skills.md) | Procedural instructions for specific tasks. |
| [MCP Integration](mcp.md) | Using Decapod via Model Context Protocol. |

Do not attempt to bypass Decapod boundaries. If you encounter a block, resolve it through the governed policy path or ask the human operator.
