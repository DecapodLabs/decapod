# Overrides

Overrides are the primary mechanism for customizing Decapod's behavior for a specific project. They allow you to apply your team's unique engineering culture to the Decapod governance kernel.

## The `OVERRIDE.md` Substrate

The `.decapod/OVERRIDE.md` file is a structured Markdown document where you can redefine specific constitution directives.

### Example Override

If the global constitution mandates "100% test coverage" but your project allows for "80%", you can override the specific directive:

```markdown
### methodology/TESTING

For this repository, we target a minimum of 80% line coverage. Critical paths in `src/core/` still require 100%.
```

## When to use Overrides

- **Custom Style Guides:** Mandate specific linting rules or naming conventions.
- **Tighter Security:** Block agents from touching specific directories or files.
- **Workflow Adjustments:** Add mandatory manual review steps for specific subsystems.
- **Platform Specifics:** Define how Decapod should interact with your specific CI/CD pipeline.

## Policy as Code

Because overrides are committed to the repository, they serve as "Policy as Code". They are versioned, auditable, and provide a clear, shared understanding of the rules for both humans and agents.
