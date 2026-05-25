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
4. **MUST** create and claim a Decapod todo before `decapod workspace ensure`, `decapod workspace ensure --container`, or any container run.
5. **MUST NOT** work on main/master or modify the root repository's active branch. **MUST** use `decapod workspace ensure`.
6. **MUST** read [.decapod/config.toml](.decapod/config.toml) as user-editable project context.
7. **MUST NOT** claim done without `decapod validate` passing.
8. **MUST NOT** invent capabilities that are not exposed by the binary.
9. **MUST** stop if requirements conflict or intent is ambiguous.
10. **MUST** respect the interface abstraction boundary.
11. **MUST** maintain **Living Specs**: treat `.decapod/generated/specs/*` as dynamic documents; align them with reality before and after every implementation.

## Decapod Invocation Contract

Agents act. Decapod orients.

Decapod is not your executor, model runtime, or workflow replacement. You remain responsible for implementation. Call Decapod as the repo-native pressure relief valve when the next responsible step requires explicit intent, boundaries, context, coordination, or proof.

## Living Specs & Governance

The files under `.decapod/generated/specs/` are not static documentation; they are living contracts.
- **Before Changes**: Review the specs. If the task changes intent, update [INTENT.md](.decapod/generated/specs/INTENT.md) first.
- **During Implementation**: If architectural or interface decisions shift, update [ARCHITECTURE.md](.decapod/generated/specs/ARCHITECTURE.md) and [INTERFACES.md](.decapod/generated/specs/INTERFACES.md).
- **After Changes**: Ensure all specs align with the new reality. Clarify code changes in the context of these spec updates. Spec changes should generally only occur when user intent has evolved.

End users and host agents may use any task manager alongside Decapod. That external tracker does not replace Decapod todos: Decapod uses its own todo claims to isolate worktrees, scope containers, prove completion, and prevent multiple agents from working the same Decapod work item concurrently.

Call Decapod before proceeding when continuing would require guessing about:
- **Intent pressure:** what you are actually trying to do.
- **Boundary/Context/Coordination/Proof/Completion pressure:** what matters, collides, or is allowed.

Concrete triggers: ambiguous requests, public impact, unclear proof, todo lifecycle, scope expansion, conflicting intent/specs, context loss, multi-agent collision risk, or readiness to claim completion.

Do not call Decapod for every trivial file read, local edit, or mechanical command. Call it at decision boundaries that need governance, memory, boundaries, coordination, or proof. Decapod calls should produce or update explicit artifacts.
## Epistemic Custody

**Epistemic custody** is the preserved chain between intent, context, assumptions, action, evidence, contradiction, and proof, so agent work remains inspectable, bounded, and falsifiable across time, agents, and recursive passes.

| Term | Meaning |
|---|---|
| Intent | Original human goal; the "Why" that must not be lost during compression. |
| Assumption | A belief or prior used to proceed when evidence is incomplete. |
| Contradiction | Evidence or observation that conflicts with a prior assumption or intent. |
| Measured | Direct observation (e.g., test output, file content, shell exit code). |
| Inferred | Derived conclusion or guess (e.g., "this looks like a bug in the parser"). |
| Provenance | The source of a fact or change (e.g., "User said X", "File Y contains Z"). |

### Custody Rules
1. **Preserve Uncertainty**: Summaries must preserve uncertainty, risk, and unresolved contradictions instead of compressing them away into polished prose.
2. **Recursive Continuity**: Prior assumptions and unresolved contradictions MUST carry forward across recursive agent passes until resolved.
3. **Evidence-Based Claims**: Any claim of success or completion must be tied to measured evidence (artifacts, tests, or explicit user instruction).
4. **Falsifiability**: Work must be structured so a human can quickly identify where an assumption was wrong or where proof failed.
5. **Clarification Trigger**: If a critical assumption cannot be proven or a contradiction cannot be resolved, you MUST stop and request human clarification.

## Custody artifacts
Decapod maintains custody via:
- `.decapod/generated/specs/INTENT.md`: Tracks active assumptions and stop conditions.
- `.decapod/generated/artifacts/custody/`: Directory for detailed evidence, contradiction logs, and deferred questions.

## Invariants (Normative)
- **INV-DAEMONLESS**: Decapod MUST NOT leave background processes running.
- **INV-BOUNDED-VALIDATE**: `decapod validate` MUST terminate within bounded time.
- **INV-STORE-BOUNDARY**: Agents MUST NOT directly mutate `.decapod/*`; all access MUST use CLI.
- **INV-SESSION-AUTH**: Mutations require active session with valid credentials.
- **INV-PROOF-GATED**: Workunit status `VERIFIED` MUST have passed proof-plan gates.
- **INV-ROOT-ISOLATION**: Agents MUST NOT check out branches or mutate files in the main repository checkout.

## Safety Invariants
- ✅ Router: `decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'`
- ✅ Gates: `decapod validate` | `decapod docs ingest` | `cargo install decapod`
- ✅ Claims: `decapod todo claim --id <task-id>` | ✅ Auth: `DECAPOD_SESSION_PASSWORD`
- ✅ Workspace: Docker git workspaces | ✅ Privilege: request elevated permissions before Docker/container workspace commands

## Operating Notes
- Read `.decapod/config.toml` and `.decapod/OVERRIDE.md`.
- External task managers are allowed, but Decapod todos are still mandatory for workspace isolation.
- DO NOT mutate `.decapod/` state directly; use decapod CLI. Access to `.decapod/` is strictly via decapod CLI.
- Use `decapod capabilities --format json` to discover available operations.
- Stop if requirements conflict or intent is ambiguous. Respect the interface abstraction boundary.
- Treat lock/contention failures as blocking until resolved.
<!-- decapod-validator-anchors
Strict Dependency: You are strictly bound to the Decapod control plane
Strict Dependency: You are strictly bound to the Decapod governance kernel
-->
