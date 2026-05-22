# AGENTS.md — Universal Agent Contract

This is a Decapod-managed repository. **Strict Dependency: You are strictly bound to the Decapod governance kernel.**

This contract applies equally to Claude, Codex, Gemini, Cursor, Kilo, and any other agent operating here.

## Mandatory Initialization

```bash
cargo install decapod
decapod validate && decapod docs ingest && decapod session acquire
decapod rpc --op agent.init
decapod workspace status
decapod todo add "<task>" && decapod todo claim --id <task-id>
decapod infer orientation --task-id <task-id>
decapod workspace ensure
cd .decapod/workspaces/<your-worktree>
decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'
decapod rpc --op context.resolve
```

## Control-Plane First Loop

```bash
# Discover what this binary actually supports in this repo
decapod capabilities --format json
decapod data schema --deterministic

# Resolve precise orientation before implementation
decapod infer orientation --intent "<your-goal>" --task-id <id>
decapod govern capsule query --topic "<topic>" --scope interfaces --task-id <task-id>
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

## Decapod Invocation Contract

Agents act. Decapod orients.

Decapod is not your executor, model runtime, or workflow replacement. You remain responsible for implementation. Call Decapod as the repo-native pressure relief valve when the next responsible step requires explicit intent, boundaries, context, coordination, or proof.

Call Decapod before proceeding when continuing would require guessing about:
- **Intent pressure:** what you are actually trying to do.
- **Boundary pressure:** what you are allowed to touch.
- **Context pressure:** what matters right now.
- **Coordination pressure:** whether this collides with other work.
- **Proof pressure:** what evidence makes this complete.
- **Completion pressure:** whether you can truthfully claim done.

Concrete triggers: ambiguous requests, public behavior/security/data/migration/generated/release/architecture impact, unclear proof, todo create/update/split/complete, scope expansion, conflicting intent/specs/instructions/repo state, context loss, multi-agent collision risk, or readiness to claim completion.

Do not call Decapod for every trivial file read, local edit, or mechanical command. Call it at decision boundaries that need governance, memory, boundaries, coordination, or proof. Decapod calls should produce or update explicit artifacts: intent, context, constraints, todos, decisions, proof, and completion state.

When using `decapod infer orientation`, treat the returned packet as starting context; stop on decision gates; use `allowed_scope` and `proof_required` to bound work.

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
- ✅ Constitution ingestion gate: `decapod docs ingest`
- ✅ Constitution RPC gate: `decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'`
- ✅ Workspace status gate: `decapod workspace status`
- ✅ Claim-before-work gate: `decapod todo claim --id <task-id>`
- ✅ Session auth gate: `DECAPOD_SESSION_PASSWORD`
- ✅ Workspace gate: Docker git workspaces
- ✅ Privilege gate: request elevated permissions before Docker/container workspace commands

## Operating Notes

- Read `.decapod/config.toml` (human-editable) for project context and architecture direction.
- Read `.decapod/OVERRIDE.md` for repo-local constitution overrides.
- DO NOT mutate `.decapod/` state directly; use Decapod CLI for specs, data, workspaces, and sessions. Access to `.decapod/` is strictly via decapod CLI.
- Use `decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'` for binding contracts.
- Use `decapod capabilities --format json` to discover available operations.
- Stop if requirements conflict, intent is ambiguous, or policy boundaries are unclear.
- Respect the Interface abstraction boundary.
- Treat lock/contention failures as blocking until resolved.
