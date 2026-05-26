# External Trackers

Decapod is built to be a "good citizen" in your existing developer ecosystem. It does not replace your high-level project management tools (GitHub Issues, Linear, Jira); it provides the **operational bridge** to the repository.

## The Integration Pattern

1.  **Management Layer (External):** A human creates an issue in Linear (e.g., `DEV-456`).
2.  **Operational Layer (Decapod):** An agent adds a Decapod todo that references the external issue.
    ```bash
    decapod todo add "Fix regression in auth" --ref "DEV-456"
    ```
3.  **Execution (Isolated):** The agent claims the todo and enters its isolated workspace.
4.  **Proof (Verification):** The agent marks the task as done, satisfying the Decapod proof gates.
5.  **Sync (Closure):** The passing Decapod state provides the "green light" to close the external Linear issue.

## Why This Bridge Matters

External trackers are "blind" to the repository state. They don't know if an agent is currently corrupting a worktree or if the implementation violates a security policy. Decapod provides the **technical proof** that the work associated with an external issue is actually correct and compliant.
