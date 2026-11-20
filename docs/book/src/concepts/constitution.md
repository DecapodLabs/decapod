# Constitution

The **Decapod Constitution** is a global, embedded, agent-queryable knowledge corpus embedded in the Decapod binary. It gives coding agents on-demand access to curated engineering golden paths, architecture and design tradeoffs, implementation methodology, technical research, security and operations, and the product and business context required to **build, ship, and operate software**.

## Subordinate role in the architecture

The Constitution is **not** the primary Decapod product headline, and it is **not** a per-project instruction blob:

- **Decapod is the governance kernel** (custody, isolated workspaces, decision-time orientation and reflection, exact-commit validation, and proof).
- **The Constitution is one embedded reference layer inside that loop**, providing shared baseline engineering defaults and research context across repositories.
- **Repository-specific rules live separately** in `.decapod/config.toml` (project policy and proof profile), `.decapod/OVERRIDE.md` (scoped directive overrides), and `.decapod/managed/specs/*` (agent-authored living specifications).

Every project can draw from the same underlying engineering canon without copying tens of thousands of lines into a giant `AGENTS.md` prompt file.

## A software-factory knowledge base

The Constitution is organized for targeted agent retrieval, not end-to-end human reading. Its discoverable nodes cover software architecture, interfaces, data, methodology, specifications, documentation, and cited technical research. Research entries capture questions, contributions, engineering implications, limitations, and related work so an agent can weigh tradeoffs instead of blindly copying patterns.

Its scope is intentional: **software engineering and the business of bringing software to market**. Technical choices, reliable delivery, product decisions, and go-to-market considerations belong when they materially inform engineering work; unrelated encyclopedic topics do not.

## Agents query inside the governed loop

Coding agents query relevant Constitution fragments through Decapod's CLI or structured RPC interface during orientation and planning—without requiring humans to paste giant rulebooks or run slash-command sequences:

```sh
decapod constitution get core/DECAPOD
decapod constitution search --query "validation"
decapod govern capsule query --topic "interfaces" --scope "interfaces" --task-id <task-id>
decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'
```

The agent must still invoke the supported interface; installing Decapod is not evidence that an arbitrary harness consulted the Constitution. See [Agent-First Architecture](agent-first.md) and the [CLI Reference](../reference/cli.md).

## How knowledge becomes authority

The Constitution has a **global baseline**, with local policy layered separately:

1. **Shared Constitution.** The versioned embedded corpus supplies engineering doctrine, reference material, decision context, and directives with explicit authority and binding classifications.
2. **Repository overrides.** A team's `.decapod/OVERRIDE.md` can refine registered directives for its repository. Exact directive IDs, precedence, and source provenance are part of the governed context. See [Overrides](overrides.md).
3. **Task-specific constraints.** Accepted objectives, plans, specifications, custody, and applicable work-unit policies further constrain the current operation.

**Knowledge is not proof.** Research findings, examples, and advisory recommendations inform decisions; they are not executable validation results. Required validation (`decapod validate`) must still run against the actual candidate commit and accepted target before completion or publication. No retrieved Constitution page substitutes for real checks.

## Contribute to the common canon

The Constitution is community-contributable through **reviewed GitHub pull requests** to [Decapod](https://github.com/DecapodLabs/decapod). See [Contributing](https://github.com/DecapodLabs/decapod/blob/master/CONTRIBUTING.md) for the repository's review and proof expectations.

Useful contributions strengthen a software-engineering or software-business golden path, document a relevant research result and its limitations, clarify an architectural tradeoff, or improve how agents retrieve applicable knowledge. Include sources and explain which engineering decisions the material improves. Changes affecting binding behavior also require compatibility and validation evidence.
