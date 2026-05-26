# Single-Agent Workflow

Even in single-agent environments, Decapod provides a rigorous structure that prevents common agentic errors and ensures high-quality delivery.

## The Standard Loop

1.  **Orientation:** The agent reads `AGENTS.md` and initializes its session.
    - `decapod session acquire`
2.  **Intent Capture:** The agent identifies its task and formalizes the intent.
    - `decapod todo claim --id <id>`
    - `update specs/INTENT.md`
3.  **Workspace Entry:** The agent moves into an isolated environment.
    - `decapod workspace ensure`
4.  **Implementation:** The agent performs the work within the workspace.
5.  **Validation:** The agent verifies the change against project policy.
    - `decapod validate`
6.  **Completion:** The agent generates proof and marks the task as done.
    - `decapod todo done --id <id> --validated`

## Key Benefits

- **Safe Iteration:** The agent works on a dedicated branch, meaning it can't accidentally break the main build while experimenting.
- **Forced Specification:** The agent is forced to think about "Why" and "How" before writing code, leading to more coherent designs.
- **Verifiable Outcome:** The human operator receives a PR with a `decapod validate` pass, providing confidence in the change.
