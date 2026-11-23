# `@decapodlabs/pi-decapod`

First-class Pi agent harness lifecycle extension for the **Decapod** governance kernel (`#1277`).

## Lifecycle Hooks

- `onPromptReceived` — evaluates raw human intent through `decapod eval --stdin --format json` and preserves prompt provenance separately from agent interpretation (`humanApprovalAuthenticated: false`).
- `onContextResolve` — queries `context.resolve`, visibly reports `cli_process` vs `native_napi` transport state, and escalates unresolved knowledge conflicts or Decision Gates without auto-approving.
- `onToolResult` — records tool outcomes and stops for human input whenever a Decision Gate or missing authority is surfaced.
- `beforeMutation` — enforces cooperative hook custody and honestly classifies direct host writes that bypass hooks as `outside_native_enforcement`.
- `onSettlement` / `beforeValidationAndAdmission` — refuses completion claims whenever protected-branch, workspace, or repository proof checks fail.
- `onResume` / `switchModelOrHarness` — reconstructs prompt custody across model/harness replacement without replaying full conversation transcripts.
