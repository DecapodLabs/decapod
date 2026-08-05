# Introduction

Decapod occupies a specific layer in the delivery system:

```text
Models produce intelligence.
Agents perform work.
Repositories preserve state.
Decapod governs the transition from intent to proof.
```

Reliability is designed, not hoped for. As agents make code generation easier,
the differentiator is whether an organization can trust what was generated.
Decapod turns that trust requirement into repository-native intent, boundaries,
durable state, validation, supported recovery, and evidence.

Decapod is a daemonless, repo-native governance kernel for AI coding agents. It turns human intent into bounded, durable, and proof-backed agent work.

That substrate is `.decapod/`: the project-local state layer where intent, selected context, workspace custody, protected boundaries, validation evidence, and completion status become durable and inspectable. Agents can come and go; the repository keeps the work bounded, attributable, resumable, and provable.

## The execution problem

AI agents make software easier to direct, but natural language alone does not make execution reliable. People still have to notice drift, restore lost context, resolve conflicting work, recognize false completion, and prompt an agent to continue after a recoverable failure. Decapod moves those execution burdens into a governed system.

Chat transcripts and tool logs are useful during a run, but they are not a durable source of project truth. A governed Decapod run should be recoverable without trusting the original conversation: another agent, human reviewer, or CI system can inspect repository state to understand what was requested, what was understood, which boundaries applied, which workspace was used, what validation ran, and whether the work is complete.

## How governance produces convergence

Convergence is a concrete execution property. The agent preserves the accepted intent, works within explicit boundaries, maintains durable state, responds to validation, remediates supported failures, and produces evidence before completion.

1.  **Repo-Native State:** Governed execution state, from accepted tasks to architectural specifications and proof artifacts, lives with the repository under `.decapod/`. External trackers may remain the organizational system of record; Decapod governs accepted work at the execution layer (see [Configuration](configuration.md)).
2.  **[Isolated Execution](workflows/workspace-isolation.md):** Decapod automates the creation of isolated git worktrees and (optionally) Docker containers for every task. This prevents environment corruption and race conditions, especially in concurrent multi-agent workflows.
3.  **[Explicit Intent](concepts/intent.md):** The agent records its interpretation in versioned plans, todos, and living specifications before implementation proceeds.
4.  **[Proof-Backed Completion](concepts/proof.md):** "Done" is not merely a claim an agent makes. Publication requires the applicable validation gates and evidence to pass.

## Responsibility boundaries

- **Human:** expresses intent, provides judgment, and approves meaningful outcomes.
- **Agent:** interprets the request and repository, performs the work, authors living specifications, follows validation feedback, and gathers evidence.
- **Decapod:** makes accepted intent and boundaries explicit, maintains governance state, validates invariants, and blocks publication when required conditions are unsatisfied.
- **Repository:** preserves governed state, custody, history, and proof across processes, models, harnesses, and Decapod invocations.

Decapod does not perform the agent's work or author the repository's meaning. It makes the agent's interpretation and evidence visible before publication. See [Common Questions](concepts/faq.md) for the architectural boundaries behind that division.
