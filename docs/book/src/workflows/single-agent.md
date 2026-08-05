# Single-Agent Workflow

Even in single-agent environments, Decapod provides a rigorous structure that prevents common agentic errors and ensures high-quality delivery (see [Agent-First Architecture](../concepts/agent-first.md)).

## The Standard Loop

1.  **Orientation:** The agent reads `AGENTS.md` and initializes its session (see [CLI Reference](../reference/cli.md#core-operations)).
    - `decapod session acquire`
2.  **Intent Capture:** The agent identifies its task and formalizes the intent (see [Explicit Intent](../concepts/intent.md)).
    - `decapod todo claim --id <id>`
    - `update specs/INTENT.md` (see [Artifacts Reference](../reference/artifacts.md))
3.  **Workspace Entry:** The agent moves into an isolated environment (see [Workspace Sandboxing](../concepts/workspaces.md)).
    - `decapod workspace ensure`
4.  **Implementation:** The agent performs the work within the workspace.
5.  **Validation and Recovery:** The agent verifies the change against project policy. If a gate fails and remediation is supported, the agent corrects the violated artifact or state and re-runs validation.
    - `decapod validate`
6.  **Publication and Completion:** After required gates pass, the agent publishes through the governed workspace path, records proof, and marks the task as done (see [Proof & Validation](../concepts/proof.md)).
    - `decapod workspace publish`
    - `decapod todo done --id <id> --validated`


## Key Benefits

- **Safe Iteration:** The agent works on a dedicated branch, meaning it can't accidentally break the main build while experimenting.
- **Visible Interpretation:** Agent-authored living specifications make the intended outcome and repository understanding reviewable before publication.
- **Verifiable Outcome:** The human operator receives a PR with identified validation and evidence instead of relying on an agent completion claim.
