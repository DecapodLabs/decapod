# Introduction

Decapod is a daemonless, local-first, repo-native governance kernel and control plane for AI coding agents. It provides the technical substrate required for agents to operate safely and effectively within complex, multi-agent software environments.

That substrate is `.decapod/`: the project-local state layer where intent, selected context, workspace custody, protected boundaries, validation evidence, and completion status become durable and inspectable. Agents can come and go; the repository keeps the work bounded, attributable, resumable, and provable.

## The Governance Gap

While modern LLMs are exceptionally capable at generating code, they often struggle with the "last mile" of software engineering: maintaining intent across long horizons, respecting subtle architectural boundaries, and providing verifiable proof of completion. Decapod bridges this gap by embedding governance directly into the repository.

Chat transcripts and tool logs are useful during a run, but they are not a durable source of project truth. A governed Decapod run should be recoverable without trusting the original conversation: another agent, human reviewer, or CI system can inspect repository state to understand what was requested, what was understood, which boundaries applied, which workspace was used, what validation ran, and whether the work is complete.

## Core Pillars

Decapod is built on four foundational pillars:

1.  **Repo-Native State:** Governed execution state, from task tracking to architectural specifications and proof artifacts, lives directly in your repository under the `.decapod/` directory. No required external databases, no proprietary clouds, no dependence on a single agent transcript: just your repo, your rules (see [Configuration](configuration.md)).
2.  **[Isolated Execution](workflows/workspace-isolation.md):** Decapod automates the creation of isolated git worktrees and (optionally) Docker containers for every task. This prevents environment corruption and race conditions, especially in concurrent multi-agent workflows.
3.  **[Explicit Intent](concepts/intent.md):** We move beyond vague prompts. Decapod forces the formalization of human intent into versioned specifications (`specs/INTENT.md`) before implementation begins.
4.  **[Proof-Backed Completion](concepts/proof.md):** "Done" is not a claim an agent makes; it is a state Decapod verifies. Mandatory validation gates and proof artifacts ensure that every change satisfies project-wide policy.

## Who is it for?

### For Humans: The Safety Net
Decapod gives you total oversight. You define the project's **[Constitution](concepts/constitution.md)** and local **[Overrides](concepts/overrides.md)**. Decapod ensures that every agent—regardless of provider or model—adheres to these rules. You receive auditable proof for every PR, ensuring your main branch remains stable and high-quality.

### For Agents: The Orientation System
Decapod removes the guesswork from agentic work. Instead of hallucinating directory structures or inventing CLI arguments, agents use Decapod to orient themselves within the repository. By calling Decapod at key **Pressure Points**, agents gain the context and boundaries they need to deliver correct, first-pass implementations.
