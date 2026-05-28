# Agent Skills

Skills are procedural, domain-specific instruction sets stored in Decapod's Aptitude memory. Use them to follow project-specific best practices for complex engineering tasks.

## Discovery and Usage

Before performing specialized implementation work, you SHOULD query Decapod for relevant skills.

```bash
decapod data aptitude skill resolve --query "how to write integration tests for sqlite"
```

The output will provide a ranked list of procedural steps, tool recommendations, and verification gates.

## Skill Cards

Importing a skill with `--write-card` creates a JSON artifact in `.decapod/skills/`. This card is a **Mandatory Orientation Asset**:
- If you see `.decapod/skills/*.json`, you MUST read the corresponding skill before proceeding with related work.
- Use `decapod data aptitude skill get --name <name>` to retrieve the full procedural content.

## Overriding Skills

Skills can be overridden or extended in `.decapod/OVERRIDE.md` using the `metadata/skills/<name>` directive ID. Always check `decapod constitution get metadata/skills/<name>` to see the merged final instructions.

## Key Commands

| Command | Purpose |
|---|---|
| `skill resolve` | Discover relevant procedures for a task. |
| `skill import` | Onboard a new SKILL.md into the repo. |
| `skill get` | Retrieve full workflow and context for a skill. |
| `skill list` | See all active skills in the repository. |
