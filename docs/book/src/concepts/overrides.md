# Overrides

Overrides are the primary mechanism for customizing Decapod's behavior for a specific project. They allow you to apply your team's unique engineering culture to the Decapod governance kernel (see [Configuration](../configuration.md)).

## The `OVERRIDE.md` Substrate

The `.decapod/OVERRIDE.md` file is a human-authored Markdown document where you can redefine specific constitution directives (see [Repository Constitution](constitution.md)). Each generated directive subsection owns a four-backtick source block. Find the appropriate subsection and replace the visible instruction inside that block with Markdown or whatever documentation style best expresses the policy.

The outer fence prevents headings in the policy from rendering as structure in `OVERRIDE.md`. Decapod removes that wrapper and loads its contents as binding authority. Four backticks allow ordinary triple-backtick examples inside the body. A duplicate registered directive, an unclosed body fence, or an unknown ID in a Decapod namespace outside a body block is ambiguous and fails closed; Decapod never applies only part of an ambiguous override file.

Decapod derives the machine evidence. Resolved context reports the directive ID, `.decapod/OVERRIDE.md` source path, whole-file source hash, directive-body hash and byte count, and repository-project precedence. Users do not author those fields.


### Example Override

If the global constitution mandates "100% test coverage" but your project allows for "80%", you can override the specific directive:

`````markdown
### methodology/TESTING
````markdown
For this repository, we target a minimum of 80% line coverage. Critical paths in `src/decapod/core/` still require 100%.
````
`````

## When to use Overrides

- **Custom Style Guides:** Mandate specific linting rules or naming conventions.
- **Tighter Security:** Block agents from touching specific directories or files.
- **Workflow Adjustments:** Add mandatory manual review steps for specific subsystems.
- **Platform Specifics:** Define how Decapod should interact with your specific CI/CD pipeline.
## Policy as Code

Because overrides are committed to the repository, they serve as "Policy as Code". They are versioned, auditable, and provide a clear, shared understanding of the rules for both humans and agents.

On upgrade, `decapod init` renders the current scaffold and moves each valid legacy body byte-for-byte inside its fenced source area. If an authored body already contains a four-backtick run, Decapod chooses a longer outer fence. Empty retired generated sections are ignored, while a non-empty retired or unknown Decapod directive fails visibly so policy is not discarded. `decapod init` and `decapod validate` reject ambiguous binding authority rather than applying only part of it.
