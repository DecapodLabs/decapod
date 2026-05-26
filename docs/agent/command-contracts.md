# Command Contracts

This document defines the normative operational contracts for the Decapod CLI.

## `decapod todo claim`
- **Intent:** Establish exclusive or shared ownership of a work unit.
- **Preconditions:**
  - Agent must have an active session (`session acquire`).
  - Task status must be `open`.
- **Arguments:**
  - `--id <ULID>`: The unique identifier for the task.
  - `--mode <exclusive|shared>`: Exclusive claims prevent any other agent from claiming.
- **State Transition:** Moves task from `open` to `claimed`.
- **Post-Failure:** If `Conflict` is returned, the task is locked by another agent. **STOP** and select a different task.

## `decapod workspace ensure`
- **Intent:** Provide a clean, isolated environment for implementation.
- **Preconditions:**
  - Task must be `claimed` by the calling agent.
- **Arguments:**
  - `--container`: Wraps the worktree in a Docker container. **Required if `repo.container_workspaces = true`.**
- **State Transition:** Creates a git worktree and (optionally) starts a container.
- **Operational Requirement:** You MUST `cd` into the returned directory path before acting on the codebase.

## `decapod validate`
- **Intent:** Verify that the repository state satisfies all governance gates.
- **Arguments:**
  - `--store <repo|user>`: Usually `repo`.
  - `--format json`: Use for deterministic machine parsing of failures.
- **Outcome:** Exit code `0` on success. Exit code `1` on failure.
- **Failure Protocol:** Read the specific gate failure in the output. Remediate the code or state before attempting to mark the task as done.

## `decapod rpc --op constitution.get`
- **Intent:** Retrieve authoritative guidance from the embedded or project-overridden constitution.
- **Arguments:**
  - `--params '{"section": "path/to/directive"}'`: The unique ID of the directive.
- **Orientation:** If the section is unknown, use `decapod docs search` or `decapod capabilities` to discover the doc graph.
