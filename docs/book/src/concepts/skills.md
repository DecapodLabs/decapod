# Skills

Skills are specialized, agent-first instruction sets that define how an AI agent should approach specific tasks, subsystems, or workflows within your repository. Unlike the general Decapod Constitution, which defines "What" the rules are, Skills define the "How"—the concrete procedures, tool usage, and verification steps for a domain-specific concern.

## Anatomy of a Skill

A Skill is defined as a Markdown file with a YAML frontmatter.

```markdown
---
name: rust-security-audit
description: Procedure for auditing Rust code for common vulnerabilities and unsafe usage.
tags: [rust, security, audit]
---

# Rust Security Audit Skill

## Workflow
1. Run `cargo audit` to check for known vulnerabilities in dependencies.
2. Search for `unsafe` blocks and verify they have safety comments.
3. Check for potential integer overflows in arithmetic operations.
4. Verify that sensitive data is cleared from memory.

## Tools
- `cargo-audit`
- `grep` / `ripgrep`
```

## Importing Skills

Skills are imported into the Decapod Aptitude memory, making them discoverable by agents.

```bash
decapod data aptitude skill import --path metadata/skills/rust-security-audit.SKILL.md --write-card
```

The `--write-card` flag generates a deterministic JSON "Skill Card" in `.decapod/skills/`, which serves as a version-controlled proof of the skill's presence and state.

## Deep Integration: The `OVERRIDE.md` Substrate

Skills are not just static documents; they are part of the Decapod Constitution. This means they can be targeted for granular overrides in `.decapod/OVERRIDE.md`.

### Targetting a Skill Node
Every skill imported into Decapod is assigned a node ID: `metadata/skills/<skill-name>`. You can use this ID to add project-specific layers to a shared skill.

```markdown
### metadata/skills/rust-security-audit

**Project-Specific Guidance:**
- In addition to the standard audit, verify that no `panic!` macros are used in production paths.
- Ensure all `Result` types are handled without `.unwrap()`.
```

### Why Override Skills?
1. **Context Blending:** Use a global security skill but add project-specific exceptions.
2. **Team Norms:** Adjust a shared "Release" skill to match your team's specific QA sign-off process.
3. **Evolution:** Gradually migrate a global skill to a new version by overriding it locally first.

## Skill Resolution

Agents don't need to know which skill to use. They can query Decapod for the most relevant skills for their current task.

```bash
decapod data aptitude skill resolve --query "auditing security in our rust service"
```

Decapod ranks skills based on lexical match, tags, and usage history, returning a scoped set of instructions.

## Overriding Skills

Just like any other Decapod directive, skills can be customized per-project via `.decapod/OVERRIDE.md`.

```markdown
### metadata/skills/rust-security-audit

In this project, additionally ensure that all `unwrap()` calls are replaced with proper error handling or `expect()` with a descriptive message.
```

## Why use Skills?

- **Standardization:** Ensure all agents follow the same best practices for complex tasks.
- **Knowledge Persistence:** Skills survive session boundaries and different agent providers.
- **Provenance:** Skill cards provide a deterministic audit trail of which instructions were active during a task.
