# OVERRIDE.md - Project-Specific Decapod Overrides

> **IMPORTANT:** For detailed usage instructions and examples, see [README.md](README.md).

**Canonical:** OVERRIDE.md
**Authority:** override
**Layer:** Project
**Binding:** Yes (overrides embedded constitution)

<!-- ═══════════════════════════════════════════════════════════════════════ -->
<!-- ⚠️  CHANGES ARE NOT PERMITTED ABOVE THIS LINE                           -->
<!-- ═══════════════════════════════════════════════════════════════════════ -->

## Core Overrides (Routers and Indices)

### core/ENGINEERING_EXCELLENCE

### core/DECAPOD

### core/INTERFACES

### core/METHODOLOGY

### core/PLUGINS

### core/GAPS

### core/DEMANDS

### core/DEPRECATION

---

## Specs Overrides (System Contracts)

### specs/INTENT

### specs/SYSTEM

### specs/AMENDMENTS

### specs/SECURITY

### specs/GIT

---

## Interfaces Overrides (Binding Contracts)

### interfaces/CLAIMS

### interfaces/CONTROL_PLANE

### interfaces/DOC_RULES

### interfaces/GLOSSARY

### interfaces/STORE_MODEL

---

## Methodology Overrides (Practice Guides)

### methodology/ARCHITECTURE

### methodology/SOUL

### methodology/KNOWLEDGE

### methodology/MEMORY

### methodology/METRICS

### methodology/INCIDENT_RESPONSE

### methodology/RELEASE_MANAGEMENT

---

## Architecture Overrides (Domain Patterns)

### architecture/DATA

### architecture/CACHING

### architecture/MEMORY

### architecture/WEB

### architecture/CLOUD

### architecture/FRONTEND

### architecture/ALGORITHMS

### architecture/SECURITY

### architecture/OBSERVABILITY

### architecture/CONCURRENCY

### architecture/API_DESIGN

### architecture/COST_OPTIMIZATION

---

## Plugins Overrides (Operational Subsystems)

### plugins/TODO

### plugins/MANIFEST

### plugins/EMERGENCY_PROTOCOL

### plugins/DB_BROKER

### plugins/CRON

### plugins/REFLEX

### plugins/HEALTH

### plugins/POLICY

### plugins/WATCHER

### plugins/KNOWLEDGE

### plugins/ARCHIVE

### plugins/FEDERATION

### plugins/FEEDBACK

### plugins/TRUST

### plugins/CONTEXT

### plugins/HEARTBEAT

### plugins/APTITUDE

### plugins/VERIFY

### plugins/DECIDE

### plugins/AUTOUPDATE


### Blended from Legacy AGENTS Entrypoint

# AGENTS.md — Universal Agent Contract

This is a Decapod-managed repository. **Strict Dependency: You are strictly bound to the Decapod control plane.**

This contract applies equally to Claude, Codex, OpenCode, Kilo, Pi, Cursor, Windsurf, Gemini, Aider, Devin, and future agents.

## Mandatory Initialization

```bash
cargo install decapod

decapod validate
decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'
decapod session acquire
decapod rpc --op agent.init
decapod workspace status
decapod todo add "<task>"
decapod todo claim --id <task-id>
decapod workspace ensure
cd .decapod/workspaces/<your-worktree>
decapod rpc --op context.resolve
```

## Control-Plane First Loop

```bash
# Discover what this binary actually supports in this repo
decapod capabilities --format json
decapod data schema --deterministic

# Resolve scoped governance context before implementation
decapod rpc --op context.scope --params '{"query":"<problem>","op":"<op>"}' --path <path> --tag <tag>
decapod rpc --op context.scope --params '{"query":"<problem>","limit":8}'

# Convergence/proof surfaces (call when relevant)
decapod govern workunit init --task-id <task-id> --intent-ref <intent>
decapod govern capsule query --topic "<topic>" --scope interfaces --task-id <task-id>
decapod eval plan --task-set-id <id> --task-ref <task-id> --model-id <model> --prompt-hash <hash> --judge-model-id <judge> --judge-prompt-hash <hash>
```

## Golden Rules (Non-Negotiable)

1. **MUST** refine intent with the user before inference-heavy work.
2. **MUST NOT** work on main/master. **MUST** use `.decapod/workspaces/*`.
3. **MUST** read `.decapod/config.toml` as user-editable project context and may update it when user intent changes.
4. **MUST NOT** claim done without `decapod validate` passing.
5. **MUST NOT** invent capabilities that are not exposed by the binary.
6. **MUST** stop if requirements conflict, intent is ambiguous, or policy boundaries are unclear.
7. **MUST** respect the Interface abstraction boundary.

## Invariants (Normative)

These invariants are directly enforced by tests. Violations will cause CI failure.

- **INV-DAEMONLESS**: Decapod MUST NOT leave background processes running. (enforced by `tests/daemonless_lifecycle.rs`)
- **INV-BOUNDED-VALIDATE**: `decapod validate` MUST terminate within bounded time. (enforced by `tests/validate_termination.rs`)
- **INV-STORE-BOUNDARY**: Agents MUST NOT directly mutate `.decapod/*`; all access MUST use CLI. (enforced by validation gates)
- **INV-SESSION-AUTH**: Mutations require active session with valid credentials. (enforced by session commands)
- **INV-PROOF-GATED**: Workunit status `VERIFIED` MUST have passed proof-plan gates. (enforced by `tests/workunit_publish_gate.rs`)
- **INV-WORKSPACE-ISOLATION**: Protected branches (main/master) MUST NOT be directly mutated. (enforced by workspace validation)

## Safety Invariants
- ✅ Router pointer: `core/DECAPOD`
- ✅ Validation gate: `decapod validate`
- ✅ Constitution RPC gate: `decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'`
- ✅ Workspace status gate: `decapod workspace status`
- ✅ Claim-before-work gate: `decapod todo claim --id <task-id>`
- ✅ Session auth gate: `DECAPOD_SESSION_PASSWORD`
- ✅ Workspace gate: Docker git workspaces
- ✅ Privilege gate: request elevated permissions before Docker/container workspace commands

## Universal Agent Operating Contract

**Doctrine:** Agents should establish intent, shape context, bound mutation, and define proof before implementation.

**Before:** Determine what's asked; identify files/commands/modules/artifacts; define in/out scope; surface assumptions/clarifications; create dependency-aware todos.

**During:** Avoid opportunistic rewrites; preserve behavior unless required; stop before crossing subsystem boundaries; run the strongest practical verification.

**After:** Report what changed, tested, not tested, and uncertainty. Ensure `decapod validate` passes.

## Decapod Governance

Decapod is the repo-native control plane agents call on demand. It shapes context before inference, prevents scope drift, enforces boundaries, coordinates shared state, and requires proof-backed completion.

- **The agent performs the work.** Decapod does not implement or decide.
- **Decapod governs the work.** It validates, tracks, and surfaces convergence proof.
- **Decapod does not replace agents.** It makes Claude, Codex, OpenCode, Kilo, Pi, Cursor, Windsurf, Gemini, Aider, Devin, and others more reliable by absorbing common deficiencies.
- **Authority is hierarchical.** Constitution, project/spec intent, task boundaries, proof requirements, and generated artifacts outrank agent-local execution.
Call Decapod before editing. Let Decapod validate after editing.

## Operating Notes

- Read `.decapod/config.toml` before planning; it captures project name, summary, architecture, primary languages, and agent entrypoint preferences.
- Treat `.decapod/config.toml` as human-editable project context. You may update it when user intent or project direction changes.
- Read `.decapod/OVERRIDE.md` when present; it is the repo-local place for constitution overrides.
- Do not mutate Decapod-owned state under `.decapod/` directly; generated specs, data, workspaces, and sessions stay via decapod CLI.
- Use `decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'` for binding contracts; `decapod capabilities --format json` for available ops.
- Use `decapod todo handoff --id <id> --to <agent>` for cross-agent ownership transfer.
- Treat lock/contention failures (including `VALIDATE_TIMEOUT_OR_LOCK`) as blocking until resolved.


### Blended from Legacy AGENTS Entrypoint

# AGENTS.md — Universal Agent Contract

This is a Decapod-managed repository. **Strict Dependency: You are strictly bound to the Decapod control plane.**

This contract applies equally to Claude, Codex, Gemini, Cursor, Kilo, and any other agent operating here.

## Mandatory Initialization

```bash
cargo install decapod
decapod validate && decapod session acquire
decapod rpc --op agent.init
decapod workspace status
decapod todo add "<task>" && decapod todo claim --id <task-id>
decapod infer orientation --task-id <task-id>
decapod workspace ensure
cd .decapod/workspaces/<your-worktree>
decapod rpc --op context.resolve
```

## Control-Plane First Loop

```bash
# Discover what this binary actually supports in this repo
decapod capabilities --format json
decapod data schema --deterministic

# Resolve precise orientation before implementation
decapod infer orientation --intent "<your-goal>" --task-id <id>
decapod rpc --op context.scope --params '{"query":"<problem>","limit":8}'
```

## Golden Rules (Non-Negotiable)

1. **MUST** refine intent with the user before inference-heavy work.
2. **MUST** use `decapod infer orientation` before non-trivial implementation.
3. **MUST** stop and ask the human when Decapod emits a **Decision Gate**.
4. **MUST NOT** work on main/master or modify the root repository's active branch. **MUST** use `decapod workspace ensure` and `cd .decapod/workspaces/<worktree>`.
5. **MUST** read [.decapod/config.toml](.decapod/config.toml) as user-editable project context and may update it when user intent changes.
6. **MUST NOT** claim done without `decapod validate` passing.
7. **MUST NOT** invent capabilities that are not exposed by the binary.
8. **MUST** stop if requirements conflict, intent is ambiguous, or policy boundaries are unclear.
9. **MUST** respect the Interface abstraction boundary.

## Orientation & Precision (Doctrine)

Before starting expensive exploration, broad refactors, or multi-path architecture work:
- Call `decapod infer orientation`.
- Treat the returned `orientation_packet` as the authoritative starting context.
- If the packet contains `decision_gates`, present the options to the human and wait for a choice.
- Do not bypass decision gates; they are placed where ambiguity would otherwise cause expensive waste.
- Use the `allowed_scope` and `proof_required` fields to bound your work.

## Invariants (Normative)

These invariants are directly enforced by tests. Violations will cause CI failure.

- **INV-DAEMONLESS**: Decapod MUST NOT leave background processes running. (enforced by `tests/daemonless_lifecycle.rs`)
- **INV-BOUNDED-VALIDATE**: `decapod validate` MUST terminate within bounded time. (enforced by `tests/validate_termination.rs`)
- **INV-STORE-BOUNDARY**: Agents MUST NOT directly mutate `.decapod/*`; all access MUST use CLI. (enforced by validation gates)
- **INV-SESSION-AUTH**: Mutations require active session with valid credentials. (enforced by session commands)
- **INV-PROOF-GATED**: Workunit status `VERIFIED` MUST have passed proof-plan gates. (enforced by `tests/workunit_publish_gate.rs`)
- **INV-ROOT-ISOLATION**: Agents MUST NOT check out branches or mutate files in the main repository checkout. All work must happen in isolated `.decapod/workspaces/*` worktrees to avoid disrupting the human user's environment. (enforced by workspace validation)

## Safety Invariants
- ✅ Router pointer: `core/DECAPOD` | ✅ Validation gate: `decapod validate`
- ✅ Constitution RPC gate: `decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'`
- ✅ Workspace status gate: `decapod workspace status`
- ✅ Claim-before-work gate: `decapod todo claim --id <task-id>`
- ✅ Session auth gate: `DECAPOD_SESSION_PASSWORD`
- ✅ Workspace gate: Docker git workspaces
- ✅ Privilege gate: request elevated permissions before Docker/container workspace commands

## Universal Agent Operating Contract

Decapod governs AI coding agents to ensure convergence on human intent and proof-backed completion.

- **Doctrine:** Establish intent, shape context, bound mutation, and define proof BEFORE implementation.
- **Rules:** Avoid opportunistic rewrites; preserve behavior; stop at subsystem boundaries; run strong verification.
- **Workflow:** Claim task -> Orient -> Ensure Workspace -> Work in Worktree -> Validate -> Publish.
- **Hierarchy:** Constitution and project intent outrank agent-local execution.

Call Decapod before editing. Let Decapod validate after editing.

## Operating Notes

- Read `.decapod/config.toml` (human-editable) for project context and architecture direction.
- Read `.decapod/OVERRIDE.md` for repo-local constitution overrides.
- DO NOT mutate `.decapod/` state directly; use CLI for specs, data, workspaces, and sessions.
- Use `decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'` for binding contracts.
- Use `decapod capabilities --format json` to discover available operations.
- Stop if requirements conflict, intent is ambiguous, or policy boundaries are unclear.
- Respect the Interface abstraction boundary.
- Treat lock/contention failures as blocking until resolved.


<!-- decapod-validator-anchors
via decapod CLI
-->


### Blended from Legacy CLAUDE Entrypoint

# CLAUDE.md - Agent Entrypoint

You are working in a Decapod-managed repository.
See [AGENTS.md](AGENTS.md) for the universal contract.

## Project Context

- Read [.decapod/config.toml](.decapod/config.toml) before planning; it captures project name, summary, architecture, primary languages, and entrypoint preferences.
- Treat [.decapod/config.toml](.decapod/config.toml) as human-editable project context. You may update it when user intent or project direction changes.
- Read [.decapod/OVERRIDE.md](.decapod/OVERRIDE.md) when present; it is the repo-local place for constitution overrides.
- Do not mutate Decapod-owned state under `.decapod/` directly; use Decapod CLI surfaces for generated specs, data, workspaces, and sessions.

## Quick Start

```bash
cargo install decapod

decapod validate
decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'
decapod session acquire
decapod rpc --op agent.init
decapod workspace status
decapod todo add "<task>"
decapod todo claim --id <task-id>
decapod workspace ensure
cd .decapod/workspaces/<your-worktree>
decapod rpc --op context.resolve
```

## Control-Plane First

```bash
decapod capabilities --format json
decapod rpc --op context.scope --params '{"query":"<problem>","limit":8}'
decapod data schema --deterministic
```

## Operating Mode

- Use Docker git workspaces and execute in `.decapod/workspaces/*`.
- Call `decapod workspace status` at startup and before implementation work.
- request elevated permissions before Docker/container workspace commands.
- `.decapod files are accessed only via decapod CLI`.
- Read and update `.decapod/config.toml` as project context; use Decapod CLI for other `.decapod/` state.
- Read [.decapod/OVERRIDE.md](.decapod/OVERRIDE.md) for repo-local constitution overrides when present.
- `DECAPOD_SESSION_PASSWORD` is required for session-scoped operations.
- Read canonical router: `decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'.
- Use shared aptitude memory for human-taught preferences across sessions/providers: `decapod data memory add|get` (aliases: `decapod data aptitude`).
- Operator reference: `decapod rpc --op constitution.get --params '{"section":"docs/PLAYBOOK"}'`.
- Capability authority: `decapod capabilities --format json`.
- Scoped context feature: `decapod rpc --op context.scope --params '{"query":"<problem>","op":"<op>"}'` or `decapod rpc --op context.scope`.

Stop if requirements are ambiguous or conflicting.


### Blended from Legacy GEMINI Entrypoint

# GEMINI.md - Agent Entrypoint

You are working in a Decapod-managed repository.
See [AGENTS.md](AGENTS.md) for the universal contract.

## Project Context

- Read [.decapod/config.toml](.decapod/config.toml) before planning; it captures project name, summary, architecture, primary languages, and entrypoint preferences.
- Treat [.decapod/config.toml](.decapod/config.toml) as human-editable project context. You may update it when user intent or project direction changes.
- Read [.decapod/OVERRIDE.md](.decapod/OVERRIDE.md) when present; it is the repo-local place for constitution overrides.
- Do not mutate Decapod-owned state under `.decapod/` directly; use Decapod CLI surfaces for generated specs, data, workspaces, and sessions.

## Quick Start

```bash
cargo install decapod

decapod validate
decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'
decapod session acquire
decapod rpc --op agent.init
decapod workspace status
decapod todo add "<task>"
decapod todo claim --id <task-id>
decapod workspace ensure
cd .decapod/workspaces/<your-worktree>
decapod rpc --op context.resolve
```

## Control-Plane First

```bash
decapod capabilities --format json
decapod rpc --op context.scope --params '{"query":"<problem>","limit":8}'
decapod data schema --deterministic
```

## Operating Mode

- Use Docker git workspaces and execute in `.decapod/workspaces/*`.
- Call `decapod workspace status` at startup and before implementation work.
- request elevated permissions before Docker/container workspace commands.
- `.decapod files are accessed only via decapod CLI`.
- Read and update `.decapod/config.toml` as project context; use Decapod CLI for other `.decapod/` state.
- Read [.decapod/OVERRIDE.md](.decapod/OVERRIDE.md) for repo-local constitution overrides when present.
- `DECAPOD_SESSION_PASSWORD` is required for session-scoped operations.
- Read canonical router: `decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'.
- Use shared aptitude memory for human-taught preferences across sessions/providers: `decapod data memory add|get` (aliases: `decapod data aptitude`).
- Operator reference: `decapod rpc --op constitution.get --params '{"section":"docs/PLAYBOOK"}'`.
- Capability authority: `decapod capabilities --format json`.
- Scoped context feature: `decapod rpc --op context.scope --params '{"query":"<problem>","op":"<op>"}'` or `decapod rpc --op context.scope`.

Stop if requirements are ambiguous or conflicting.


### Blended from Legacy CODEX Entrypoint

# CODEX.md - Agent Entrypoint

You are working in a Decapod-managed repository.
See [AGENTS.md](AGENTS.md) for the universal contract.

## Project Context

- Read [.decapod/config.toml](.decapod/config.toml) before planning; it captures project name, summary, architecture, primary languages, and entrypoint preferences.
- Treat [.decapod/config.toml](.decapod/config.toml) as human-editable project context. You may update it when user intent or project direction changes.
- Read [.decapod/OVERRIDE.md](.decapod/OVERRIDE.md) when present; it is the repo-local place for constitution overrides.
- Do not mutate Decapod-owned state under `.decapod/` directly; use Decapod CLI surfaces for generated specs, data, workspaces, and sessions.

## Quick Start

```bash
cargo install decapod

decapod validate
decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'
decapod session acquire
decapod rpc --op agent.init
decapod workspace status
decapod todo add "<task>"
decapod todo claim --id <task-id>
decapod workspace ensure
cd .decapod/workspaces/<your-worktree>
decapod rpc --op context.resolve
```

## Control-Plane First

```bash
decapod capabilities --format json
decapod rpc --op context.scope --params '{"query":"<problem>","limit":8}'
decapod data schema --deterministic
```

## Operating Mode

- Use Docker git workspaces and execute in `.decapod/workspaces/*`.
- Call `decapod workspace status` at startup and before implementation work.
- request elevated permissions before Docker/container workspace commands.
- `.decapod files are accessed only via decapod CLI`.
- Read and update `.decapod/config.toml` as project context; use Decapod CLI for other `.decapod/` state.
- Read [.decapod/OVERRIDE.md](.decapod/OVERRIDE.md) for repo-local constitution overrides when present.
- `DECAPOD_SESSION_PASSWORD` is required for session-scoped operations.
- Read canonical router: `decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'.
- Use shared aptitude memory for human-taught preferences across sessions/providers: `decapod data memory add|get` (aliases: `decapod data aptitude`).
- Operator reference: `decapod rpc --op constitution.get --params '{"section":"docs/PLAYBOOK"}'`.
- Capability authority: `decapod capabilities --format json`.
- Scoped context feature: `decapod rpc --op context.scope --params '{"query":"<problem>","op":"<op>"}'` or `decapod rpc --op context.scope`.

Stop if requirements are ambiguous or conflicting.


### Blended from Legacy AGENTS Entrypoint

# AGENTS.md — Universal Agent Contract

This is a Decapod-managed repository. **Strict Dependency: You are strictly bound to the Decapod control plane.**

This contract applies equally to Claude, Codex, Gemini, Cursor, Kilo, and any other agent operating here.

## Mandatory Initialization

```bash
cargo install decapod
decapod validate && decapod session acquire
decapod rpc --op agent.init
decapod workspace status
decapod todo add "<task>" && decapod todo claim --id <task-id>
decapod infer orientation --task-id <task-id>
decapod workspace ensure
cd .decapod/workspaces/<your-worktree>
decapod rpc --op context.resolve
```

## Control-Plane First Loop

```bash
# Discover what this binary actually supports in this repo
decapod capabilities --format json
decapod data schema --deterministic

# Resolve precise orientation before implementation
decapod infer orientation --intent "<your-goal>" --task-id <id>
decapod rpc --op context.scope --params '{"query":"<problem>","limit":8}'
```

## Golden Rules (Non-Negotiable)

1. **MUST** refine intent with the user before inference-heavy work.
2. **MUST** use `decapod infer orientation` before non-trivial implementation.
3. **MUST** stop and ask the human when Decapod emits a **Decision Gate**.
4. **MUST NOT** work on main/master or modify the root repository's active branch. **MUST** use `decapod workspace ensure` and `cd .decapod/workspaces/<worktree>`.
5. **MUST** read [.decapod/config.toml](.decapod/config.toml) as user-editable project context and may update it when user intent changes.
6. **MUST NOT** claim done without `decapod validate` passing.
7. **MUST NOT** invent capabilities that are not exposed by the binary.
8. **MUST** stop if requirements conflict, intent is ambiguous, or policy boundaries are unclear.
9. **MUST** respect the Interface abstraction boundary.

## Orientation & Precision (Doctrine)

Before starting expensive exploration, broad refactors, or multi-path architecture work:
- Call `decapod infer orientation`.
- Treat the returned `orientation_packet` as the authoritative starting context.
- If the packet contains `decision_gates`, present the options to the human and wait for a choice.
- Do not bypass decision gates; they are placed where ambiguity would otherwise cause expensive waste.
- Use the `allowed_scope` and `proof_required` fields to bound your work.

## Invariants (Normative)

These invariants are directly enforced by tests. Violations will cause CI failure.

- **INV-DAEMONLESS**: Decapod MUST NOT leave background processes running. (enforced by `tests/daemonless_lifecycle.rs`)
- **INV-BOUNDED-VALIDATE**: `decapod validate` MUST terminate within bounded time. (enforced by `tests/validate_termination.rs`)
- **INV-STORE-BOUNDARY**: Agents MUST NOT directly mutate `.decapod/*`; all access MUST use CLI. (enforced by validation gates)
- **INV-SESSION-AUTH**: Mutations require active session with valid credentials. (enforced by session commands)
- **INV-PROOF-GATED**: Workunit status `VERIFIED` MUST have passed proof-plan gates. (enforced by `tests/workunit_publish_gate.rs`)
- **INV-ROOT-ISOLATION**: Agents MUST NOT check out branches or mutate files in the main repository checkout. All work must happen in isolated `.decapod/workspaces/*` worktrees to avoid disrupting the human user's environment. (enforced by workspace validation)

## Safety Invariants
- ✅ Router pointer: `core/DECAPOD` | ✅ Validation gate: `decapod validate`
- ✅ Constitution RPC gate: `decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'`
- ✅ Workspace status gate: `decapod workspace status`
- ✅ Claim-before-work gate: `decapod todo claim --id <task-id>`
- ✅ Session auth gate: `DECAPOD_SESSION_PASSWORD`
- ✅ Workspace gate: Docker git workspaces
- ✅ Privilege gate: request elevated permissions before Docker/container workspace commands

## Universal Agent Operating Contract

Decapod governs AI coding agents to ensure convergence on human intent and proof-backed completion.

- **Doctrine:** Establish intent, shape context, bound mutation, and define proof BEFORE implementation.
- **Rules:** Avoid opportunistic rewrites; preserve behavior; stop at subsystem boundaries; run strong verification.
- **Workflow:** Claim task -> Orient -> Ensure Workspace -> Work in Worktree -> Validate -> Publish.
- **Hierarchy:** Constitution and project intent outrank agent-local execution.

Call Decapod before editing. Let Decapod validate after editing.

## Operating Notes

- Read `.decapod/config.toml` (human-editable) for project context and architecture direction.
- Read `.decapod/OVERRIDE.md` for repo-local constitution overrides.
- DO NOT mutate `.decapod/` state directly; use Decapod CLI for specs, data, workspaces, and sessions. Access to `.decapod/` is strictly via decapod CLI.
- Use `decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'` for binding contracts.
- Use `decapod capabilities --format json` to discover available operations.
- Stop if requirements conflict, intent is ambiguous, or policy boundaries are unclear.
- Respect the Interface abstraction boundary.
- Treat lock/contention failures as blocking until resolved.
