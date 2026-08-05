# External Trackers

Decapod does not replace project-management systems such as GitHub Issues, Linear, Jira, or Beads. Those systems may remain the organizational system of record. Decapod governs accepted work at the repository execution layer.

## The Integration Pattern

1.  **Organizational Layer (External):** A human creates an issue in Linear (e.g., `DEV-456`).
2.  **Execution Layer (Decapod):** An agent adds a Decapod todo that references the external issue (see [CLI Reference](../reference/cli.md#task-tracking)).
    ```bash
    decapod todo add "Fix regression in auth" --ref "DEV-456"
    ```
3.  **Execution (Isolated):** The agent claims the todo and enters its isolated workspace (see [Workspace Isolation](workspace-isolation.md)).
4.  **Proof (Verification):** The agent marks the task as done, satisfying the Decapod proof gates (see [Proof & Validation](../concepts/proof.md)).
5.  **Sync (Closure):** The passing Decapod state provides the "green light" to close the external Linear issue.


## Why This Bridge Matters

External trackers organize work but do not enforce Decapod's repository invariants. Decapod contributes execution-layer custody, validation, and evidence. The external item is closed according to the team's own approval policy after the governed work reaches its required publication state.
