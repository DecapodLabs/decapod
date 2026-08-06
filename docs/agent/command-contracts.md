# Command Contracts

This document defines the normative operational contracts for the Decapod CLI.

## `decapod eval`
- **Intent:** Evaluate an untrusted agent prompt before any repository or tool action

## `decapod activate`
- **Intent:** Activate local control plane state and run startup migrations

## `decapod init`
- **Intent:** Bootstrap system and manage lifecycle

## `decapod setup`
- **Intent:** Configure repository (hooks, settings)

## `decapod session`
- **Intent:** Session token management (required for agent operation)

## `decapod cloud`
- **Intent:** Optional cloud credential and Propodus integration commands

## `decapod constitution`
- **Intent:** Embedded Constitution Graph queries and lookups

## `decapod docs`
- **Intent:** Access agent-facing methodology documentation (restricted to docs/agent/)
- **Restriction:** Only handles documents under `docs/agent/`.

## `decapod todo`
- **Intent:** Track tasks and work items
- **Preconditions:** Agent must have an active session.
- **State Transition:** Managed via `todo.db`.

## `decapod obligation`
- **Intent:** Governance-native obligation graph

## `decapod validate`
- **Intent:** Validate methodology compliance
- **Intent:** Verify methodology compliance.
- **Outcome:** Exit code 0 on success, 1 on failure.
- **Failure State:** A failed gate leaves the task incomplete; it is not a completion signal.
- **Recovery:** Follow supported remediation, update the violated artifact or state, and re-run validation. Escalate decision gates or unsupported recovery to the human.

## `decapod govern`
- **Intent:** Governance: policy, health, proofs, audits

## `decapod data`
- **Intent:** Data: archives, knowledge, context, schemas

## `decapod auto`
- **Intent:** Automation: scheduled and event-driven

## `decapod qa`
- **Intent:** Quality assurance: verification and checks

## `decapod decide`
- **Intent:** Architecture decision prompting

## `decapod workspace`
- **Intent:** Agent workspace management
- **Preconditions:** Task must be claimed.
- **State Transition:** Creates git worktrees/containers.
- **Publication:** Publication is a governed transition and remains blocked while required validation or evidence is unsatisfied.

## `decapod rpc`
- **Intent:** Decapod-specific structured RPC interface for agents

## `decapod release`
- **Intent:** Release lifecycle checks and guards

## `decapod capabilities`
- **Intent:** Show Decapod capabilities (for agent discovery)

## `decapod infer`
- **Intent:** Inference governance: shape context before model, validate after

## `decapod trace`
- **Intent:** Local trace management

## `decapod system`
- **Intent:** System: capabilities, version, doctor

## `decapod context`
- **Intent:** Context: infer, lcm, internalize, preflight, impact

# RPC Operations (Auto-generated)

### Operation: `AgentInit`
### Operation: `WorkspaceStatus`
### Operation: `WorkspaceEnsure`
### Operation: `WorkspacePublish`

Publication uses an ordinary fast-forward Git push. Decapod never force-pushes a
governed workspace branch. If the remote rejects the push as non-fast-forward,
inspect and reconcile the divergence in the workspace, rerun validation, and
retry publication. Do not use `git push --force` or `git push --force-with-lease`;
stop for human judgment if shared history would need to be rewritten.
### Operation: `ContextResolve`
### Operation: `ContextCapsuleQuery`
### Operation: `ContextBundleExport`
### Operation: `ContextBundleImport`
### Operation: `ContextBindings`
### Operation: `ConstitutionGet`
### Operation: `ConstitutionLinksQuery`
### Operation: `ConstitutionLinksNavigate`
### Operation: `SpecsRefresh`
### Operation: `ConstitutionMigrate`
### Operation: `AgentRegistryQuery`
### Operation: `SchemaGet`
### Operation: `StoreUpsert`
### Operation: `StoreQuery`
### Operation: `ValidateRun`
### Operation: `ScaffoldNextQuestion`
### Operation: `ScaffoldApplyAnswer`
### Operation: `ScaffoldGenerateArtifacts`
### Operation: `StandardsResolve`
### Operation: `MentorObligations`
### Operation: `AssuranceEvaluate`
