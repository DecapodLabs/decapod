# Command Contracts

This document defines the normative operational contracts for the Decapod CLI.

## `decapod activate`
- **Intent:** Activate local control plane state and run startup migrations

## `decapod init`
- **Intent:** Bootstrap system and manage lifecycle

## `decapod setup`
- **Intent:** Configure repository (hooks, settings)

## `decapod session`
- **Intent:** Session token management (required for agent operation)
- **Subcommands:** `acquire`, `status`, `release`, `init`, `handshake`.

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
- **Outcome:** Exit code 0 on success, 1 on failure.

## `decapod system`
- **Intent:** System-level metadata and health
- **Subcommands:** `version`, `doctor`, `capabilities`.

## `decapod govern`
- **Intent:** Governance: policy, health, proofs, audits
- **Subcommands:** `policy`, `health`, `proof`, `watcher`, `feedback`, `gatekeeper`, `plan`, `workunit`, `capsule`, `state-commit`.

## `decapod data`
- **Intent:** Data: archives, knowledge, context, schemas
- **Subcommands:** `archive`, `knowledge`, `context`, `schema`, `repo`, `broker`, `aptitude`, `federation`, `primitives`, `map`.

## `decapod auto`
- **Intent:** Automation: scheduled and event-driven

## `decapod qa`
- **Intent:** Quality assurance: verification and checks
- **Subcommands:** `verify`, `check`, `gatling`, `eval`, `demo`.

## `decapod context`
- **Intent:** Inference context management and prediction
- **Subcommands:** `infer`, `lcm`, `internalize`, `preflight`, `impact`.

## `decapod decide`
- **Intent:** Architecture decision prompting

## `decapod workspace`
- **Intent:** Agent workspace management
- **Preconditions:** Task must be claimed.
- **State Transition:** Creates git worktrees/containers.

## `decapod rpc`
- **Intent:** Structured JSON-RPC interface for agents

## `decapod release`
- **Intent:** Release lifecycle checks and guards

## `decapod capabilities`
- **Intent:** Show Decapod capabilities (for agent discovery)
- **Note:** Also available via `decapod system capabilities`.

## `decapod infer`
- **Intent:** Inference governance: shape context before model, validate after
- **Note:** Also available via `decapod context infer`.

## `decapod trace`
- **Intent:** Local trace management
- **Subcommands:** `export`, `flight-recorder`.

# RPC Operations (Auto-generated)

### Operation: `AgentInit`
### Operation: `WorkspaceStatus`
### Operation: `WorkspaceEnsure`
### Operation: `WorkspacePublish`
### Operation: `ContextResolve`
### Operation: `ContextCapsuleQuery`
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
