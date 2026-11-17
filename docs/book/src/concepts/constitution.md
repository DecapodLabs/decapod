# Constitution

The **Decapod Constitution** is a shared, curated reference for the disciplines required to **build, ship, and operate software products**. It gives coding agents on-demand access to engineering golden paths, architecture and design tradeoffs, implementation methodology, research, security and operations, and the product and business context that shapes software delivery.

It is **global across Decapod-managed repositories**. A project contributes its own requirements and conventions through separate repository policies, specifications, and scoped overrides. Every project can draw from the same underlying body of knowledge without recreating it in a giant agent instruction file.

## A software-factory knowledge base

The Constitution is organized for agent retrieval, not for a human to read end to end. Its discoverable doctrine covers software architecture, interfaces, data, methodology, specifications, documentation, and technical research. Research entries capture questions, contributions, engineering implications, limitations, and related work so an agent can investigate the reasoning behind a pattern instead of merely imitating it.

The scope is intentional: **software engineering and the business of bringing software to market**. Technical choices, reliable delivery, product decisions, and go-to-market considerations belong when they materially inform that work. Unrelated encyclopedic subjects do not.

Think of it as an agent-queryable engineering canon: a common source of proven paths, cited research and decision context that becomes useful when a real task calls for it.

## Agents query; humans define intent

An agent can discover relevant Constitution material through Decapod's own CLI or structured RPC interface. Context resolution can select applicable fragments alongside the specific repository's state and requirements. This is more precise than pasting tens of thousands of lines into `AGENTS.md`, and does not require a human to select knowledge entries with slash commands.

For example:

```sh
decapod constitution get core/DECAPOD
decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'
```

The agent still needs to actually invoke the supported interface; installing Decapod is not proof that an arbitrary harness has used the Constitution. See [Agent-First Architecture](agent-first.md) and the [CLI Reference](../reference/cli.md).

## How knowledge becomes authority

The Constitution has a **global baseline**, with local policy applied through separate layers:

1. **Shared Constitution.** The versioned common corpus supplies engineering doctrine, reference material, decision context, and directives with explicit authority and binding classifications.
2. **Repository overrides.** A team's `.decapod/OVERRIDE.md` can refine registered directives for its repository. Exact directive IDs, precedence, and source provenance are part of the governed context. See [Overrides](overrides.md).
3. **Task-specific constraints.** Accepted objectives, specifications, custody, and applicable work-unit policies further constrain the current operation.

**Knowledge is not proof.** Research findings, examples, and advisory recommendations inform decisions; they are not themselves executable validation results. Decapod enforces applicable binding requirements at supported governance boundaries, and required validation must execute against the actual candidate and accepted context before admission. No agent statement or retrieved page is sufficient evidence of correctness.

## Contribute to the common canon

The Constitution is open to improvements through **reviewed GitHub pull requests** to [Decapod](https://github.com/DecapodLabs/decapod). See [Contributing](https://github.com/DecapodLabs/decapod/blob/master/CONTRIBUTING.md) for the repository's review and proof expectations.

Useful contributions strengthen a software-engineering or software-business golden path, document a relevant research result and its limitations, clarify an architectural tradeoff, or improve how agents find applicable knowledge. Include sources and explain what decisions the material should improve. Changes affecting binding behavior also need appropriate compatibility and validation evidence.

The focus stays on **building and operating software businesses**. A bigger corpus is valuable only when it makes the next engineering decision clearer, safer, or more verifiable.
