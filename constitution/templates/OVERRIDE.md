# OVERRIDE.md - Project-Specific Decapod Overrides

**Canonical:** OVERRIDE.md
**Authority:** override
**Layer:** Project
**Binding:** Yes (overrides embedded constitution)

---

## 🎯 Quick Start: How to Use This File

1. **Find the section below** that matches what you want to override (Core, Specs, or Plugins)
2. **Uncomment the HTML comment** (`<!-- ... -->`) under the component you want to customize
3. **Write your override content** in that section
4. **Keep it minimal** - only override what's truly project-specific

### Example: Override TODO Priority Levels

Find the `### plugins/TODO.md` section below, remove the `<!-- -->` comment markers, and add your content:

```markdown
### plugins/TODO.md

## Priority Levels (Project Override)

For this project, we use a 5-level priority system:
- **critical**: Production down, blocking release
- **high**: Sprint commitment, must complete this iteration
- **medium**: Backlog, next sprint candidate
- **low**: Nice-to-have, future consideration
- **idea**: Exploration, needs refinement before actionable
```

---

## How Overrides Work

- **Embedded constitution** (read-only, shipped with Decapod) provides the base methodology
- **This file** allows project-specific customization without forking Decapod
- Overrides are applied at runtime when agents read the constitution via `decapod docs show`
- Keep overrides minimal - only add what's specific to YOUR project
- See available components: `decapod docs list`

---

## Component Reference

Available override paths (use these exact headings below):

| Path | What it controls |
|------|-----------------|
| `core/DECAPOD.md` | Navigation charter and routing |
| `core/CONTROL_PLANE.md` | Agent sequencing patterns |
| `core/STORE_MODEL.md` | Store purity model |
| `core/PLUGINS.md` | Subsystem registry |
| `core/DOC_RULES.md` | Documentation compiler rules |
| `core/CLAIMS.md` | Claims ledger |
| `core/SOUL.md` | Agent persona guidelines |
| `specs/INTENT.md` | Authority contracts and methodology intent |
| `specs/ARCHITECTURE.md` | System boundaries, tradeoffs, decisions |
| `specs/SYSTEM.md` | Authority and proof doctrine |
| `specs/AMENDMENTS.md` | Change control process |
| `plugins/TODO.md` | TODO subsystem behavior |
| `plugins/WORKFLOW.md` | Operating loop |
| `plugins/MANIFEST.md` | Canonical vs derived vs state definitions |
| `plugins/DB_BROKER.md` | Broker interface contract |
| (and more...) | See `decapod docs list` for full list |

---

<!-- ═══════════════════════════════════════════════════════════════════════ -->
<!-- ⚠️  CHANGES ARE NOT PERMITTED ABOVE THIS LINE                           -->
<!-- ═══════════════════════════════════════════════════════════════════════ -->

<!--
  Write your project-specific overrides below this line.

  Use the exact component path as a heading (###) to override that component.

  Example:

  ### plugins/TODO.md

  ## Priority Levels (Project Override)

  For this project, we use a 5-level priority system:
  - **critical**: Production down, blocking release
  - **high**: Sprint commitment, must complete this iteration
  - **medium**: Backlog, next sprint candidate
  - **low**: Nice-to-have, future consideration
  - **idea**: Exploration, needs refinement before actionable

  ---

  ### specs/ARCHITECTURE.md

  ## Performance Budget

  All API endpoints must respond within 200ms p99 latency.

  ## Technology Restrictions

  - **Prohibited**: ORMs (use raw SQL)
  - **Required**: SQLite only
-->

---

## Guidelines

1. **Keep it minimal** - Only override what's necessary for your project
2. **Document why** - Explain the reason for each override
3. **Version control** - Commit this file to your repo
4. **Review regularly** - Remove overrides that are no longer needed
5. **Propose upstream** - If your override would benefit all projects, submit a PR to Decapod

---

## Need Help?

- See embedded docs: `decapod docs list`
- Read a specific doc: `decapod docs show <path>`
- Validate overrides: `decapod validate`
