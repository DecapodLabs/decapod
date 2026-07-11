# AGENTS.md — Universal Agent Contract

This is a Decapod-managed repository. **Strict Dependency: You are strictly bound to the Decapod governance kernel.**

## Mandatory Orientation

**Before performing any Decapod operations beyond help/version checks, you MUST ingest the Agent API Orientation corpus from the Decapod binary:**

```bash
decapod docs ingest
```

This command outputs the embedded agent documentation (api-index.md, command-contracts.md, payload-examples.md, error-recovery.md, state-model.md, config-schema.md, mcp.md) directly from the binary — no filesystem reads required.

**Note on Constitution vs Docs:** Agents use `decapod constitution` to discover technology, primitives in software engineering and delivery, and for scoping pre-inference context. Agents use `decapod docs` to access the embedded agent-facing methodology documentation (restricted to docs/agent/).

## Mandatory Initialization

```bash
# Smart bootstrap: only install/init if a newer version is available
(decapod capabilities --format json | grep -q '"is_latest":true') || (cargo install decapod && decapod init --proof)
decapod docs ingest && decapod validate && decapod session acquire
decapod rpc --op agent.init
decapod workspace status
decapod todo add "<task>" && decapod todo claim --id <task-id>
decapod infer orientation --task-id <task-id>
decapod workspace ensure
cd .decapod/workspaces/<your-worktree>
decapod constitution get core/DECAPOD
decapod rpc --op context.resolve
```

## Control-Plane First Loop
```bash
decapod capabilities --format json
decapod data schema --deterministic
decapod infer orientation --intent "<your-goal>" --task-id <id>
decapod govern capsule query --topic "<topic>" --scope interfaces --task-id <task-id>
decapod constitution search --query "<problem>"
```

## Pre-Inference Governance
**Decapod is pre-inference faculty, not post-hoc validation.**
- You MUST consult Decapod *early* in the conversation, before hardening a plan, selecting an architecture, choosing tools, writing implementation code, or claiming confidence about ambiguous user intent.
- Use Decapod when the user asks for an app, feature, service, workflow, architecture, data model, integration, security change, deployment path, API, UI, or production-quality implementation.
- Start with `core/DECAPOD` for broad prompts. Use `core/*` nodes as secondary routers. Use non-core nodes as institutional doctrine.
- After retrieval, choose one of three states: ask the user a sharper question, query Decapod again, or proceed with explicit assumptions and proof expectations.
- Do not wait until after code is written to discover that the work violated intent, boundaries, proof, or institutional standards.

## Golden Rules (Non-Negotiable)
1. **MUST** refine intent with the user before inference-heavy work.
2. **MUST** use `decapod infer orientation` before non-trivial implementation.
3. **MUST** stop and ask the human when Decapod emits a **Decision Gate**.
4. **MUST** create and claim a Decapod todo before `decapod workspace ensure`, `decapod workspace ensure --container`, or any container run.
5. **MUST NOT** work on main/master or modify the root repository's active branch. **MUST** use `decapod workspace ensure`.
6. **MUST** read [.decapod/config.toml](.decapod/config.toml) as user-editable project context.
7. **MUST NOT** claim done without `decapod validate` passing.
8. **MUST NOT** invent capabilities that are not exposed by the binary.
9. **MUST** stop if requirements conflict or intent is ambiguous.
10. **MUST** respect the interface abstraction boundary.
11. **MUST** maintain **Living Specs**: treat `.decapod/generated/specs/*` as dynamic documents.
12. **MUST** use the command contracts from `decapod docs` output instead of guessing arguments.

## Decapod Invocation Contract
Agents act. Decapod orients. Call Decapod at decision boundaries: ambiguous requests, public impact, unclear proof, todo lifecycle, scope expansion, context loss, or multi-agent collision risk.

## Living Specs & Governance
The files under `.decapod/generated/specs/` are living contracts. Review and update [INTENT.md](.decapod/generated/specs/INTENT.md), [ARCHITECTURE.md](.decapod/generated/specs/ARCHITECTURE.md), and [INTERFACES.md](.decapod/generated/specs/INTERFACES.md) to align with evolving intent and reality.

## Epistemic Custody
Preserve the chain between intent, context, assumptions, action, and proof.
1. **Preserve Uncertainty**: Summaries must preserve risk instead of compressing it.
2. **Recursive Continuity**: Prior assumptions MUST carry forward until resolved.
3. **Evidence-Based Claims**: Claims of completion must be tied to measured evidence.
4. **Clarification Trigger**: Stop if a critical assumption cannot be proven.

## Invariants (Normative)
- **INV-DAEMONLESS**: Decapod MUST NOT leave background processes running.
- **INV-BOUNDED-VALIDATE**: `decapod validate` MUST terminate within bounded time.
- **INV-STORE-BOUNDARY**: Agents MUST NOT directly mutate `.decapod/*`.
- **INV-SESSION-AUTH**: Mutations require active session.
- **INV-PROOF-GATED**: `VERIFIED` status requires passed proof-plan gates.
- **INV-ROOT-ISOLATION**: Agents MUST NOT mutate files in the main repository checkout.

## Safety Invariants
- ✅ Router: `decapod constitution get core/DECAPOD`
- ✅ Gates: `decapod capabilities` | `decapod docs ingest` | `decapod validate`
- ✅ Claims: `decapod todo claim --id <task-id>` | ✅ Auth: `DECAPOD_SESSION_PASSWORD`
- ✅ Workspace: Docker git workspaces | ✅ Privilege: request elevated permissions before Docker/container workspace commands

## Operating Notes
- Read `.decapod/config.toml` and `.decapod/OVERRIDE.md`.
- External task managers are allowed, but Decapod todos are still mandatory for workspace isolation.
- DO NOT mutate `.decapod/` state directly; all access MUST use CLI. Access to `.decapod/` is strictly via decapod CLI.
- Stop if requirements conflict or intent is ambiguous. Respect the interface abstraction boundary.
<!-- decapod-validator-anchors
Strict Dependency: You are strictly bound to the Decapod control plane
Strict Dependency: You are strictly bound to the Decapod governance kernel
-->


<!-- decapod-validator-anchors
Interface abstraction boundary
-->
