# Introduction

Decapod is a daemonless, local-first, repo-native governance kernel and control plane for AI coding agents. It provides the technical substrate required for agents to operate safely and effectively within complex, multi-agent software environments.

## The Governance Gap

While modern LLMs are exceptionally capable at generating code, they often struggle with the "last mile" of software engineering: maintaining intent across long horizons, respecting subtle architectural boundaries, and providing verifiable proof of completion. Decapod bridges this gap by embedding governance directly into the repository.

## Core Pillars

Decapod is built on four foundational pillars:

1.  **Repo-Native State:** All governance data, from task tracking to architectural specifications, lives directly in your repository under the `.decapod/` directory. No external databases, no proprietary clouds—just your repo, your rules.
2.  **Isolated Execution:** Decapod automates the creation of isolated git worktrees and (optionally) Docker containers for every task. This prevents environment corruption and race conditions, especially in concurrent multi-agent workflows.
3.  **Explicit Intent:** We move beyond vague prompts. Decapod forces the formalization of human intent into versioned specifications (`specs/INTENT.md`) before implementation begins.
4.  **Proof-Backed Completion:** "Done" is not a claim an agent makes; it is a state Decapod verifies. Mandatory validation gates and proof artifacts ensure that every change satisfies project-wide policy.

## Who is it for?

### For Humans: The Safety Net
Decapod gives you total oversight. You define the project's **Constitution** and local **Overrides**. Decapod ensures that every agent—regardless of provider or model—adheres to these rules. You receive auditable proof for every PR, ensuring your main branch remains stable and high-quality.

### For Agents: The Orientation System
Decapod removes the guesswork from agentic work. Instead of hallucinating directory structures or inventing CLI arguments, agents use Decapod to orient themselves within the repository. By calling Decapod at key **Pressure Points**, agents gain the context and boundaries they need to deliver correct, first-pass implementations.
